//! Complete helper paths reject extra effects before dispatch and before traps.
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::{
    Assignment, AssignmentKind as A, ControlHelperRole as R, ValueDecl, ValueId,
};

#[test]
fn cc_executor_prefix_and_trapping_default_reject_extra_static_checks() {
    for prefix in [false, true] {
        let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (executor, _) = binding(&input.cc.representations, R::Executor);
        let (check, contract) = binding(&input.cc.representations, R::Check);
        let function = input
            .cc
            .functions
            .iter_mut()
            .find(|f| f.symbol == executor)
            .unwrap();
        let extent = function.parameters[4];
        let ty = function.values.iter().find(|v| v.id == extent).unwrap().ty;
        let destination = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
        function.values.push(ValueDecl {
            id: destination,
            ty,
        });
        let extra = Assignment {
            destination,
            kind: A::DirectCall {
                contract: Some(contract),
                function: check,
                arguments: vec![extent],
            },
            span: function.span,
        };
        if prefix {
            function.assignments.insert(0, extra);
        } else {
            let A::TagSwitch {
                default_assignments,
                ..
            } = &mut function.assignments.last_mut().unwrap().kind
            else {
                panic!("dispatch")
            };
            default_assignments.insert(0, extra);
        }
        let errors =
            psrs_backend::cc::verify_module(&input.cc).expect_err("unaccounted static check");
        assert!(
            errors.iter().any(|e| matches!(
                e.pass,
                "P8 CC root and queue effects" | "P8 CC owned execution"
            )),
            "{prefix}: {errors:?}"
        );
    }
}

#[test]
fn cc_publication_rejects_an_extra_potentially_trapping_check() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (start, _) = binding(&input.cc.representations, R::Start);
    let (check, contract) = binding(&input.cc.representations, R::Check);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == start)
        .unwrap();
    let destination = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    function.values.push(ValueDecl {
        id: destination,
        ty: function.result_type,
    });
    function.assignments.push(Assignment {
        destination,
        kind: A::DirectCall {
            contract: Some(contract),
            function: check,
            arguments: vec![function.result],
        },
        span: function.span,
    });
    let errors =
        psrs_backend::cc::verify_module(&input.cc).expect_err("read-only is not unobservable");
    assert!(
        errors
            .iter()
            .any(|e| e.pass == "P8 CC control initialization"),
        "{errors:?}"
    );
}

#[test]
fn cc_executor_rejects_unused_erased_payload_recovery() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (executor, _) = binding(&input.cc.representations, R::Executor);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == executor)
        .unwrap();
    let ty = function
        .values
        .iter()
        .find(|v| v.id == function.parameters[2])
        .unwrap()
        .ty;
    let psrs_backend::cc::ValueShape::Reference(reference) = ty else {
        panic!("computation")
    };
    let destination = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    function.values.push(ValueDecl {
        id: destination,
        ty,
    });
    function.assignments.insert(
        0,
        Assignment {
            destination,
            kind: A::RepresentationCast {
                destination,
                value: function.parameters[0],
                reference,
            },
            span: function.span,
        },
    );
    let errors =
        psrs_backend::cc::verify_module(&input.cc).expect_err("unaccounted payload recovery");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("reference refinement")),
        "{errors:?}"
    );
}

#[test]
fn cc_no_write_recursive_calls_are_not_total() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (executor, _) = binding(&input.cc.representations, R::Executor);
    let original = input
        .cc
        .functions
        .iter()
        .find(|f| f.symbol == executor)
        .unwrap();
    let extent_ty = original
        .values
        .iter()
        .find(|v| v.id == original.parameters[4])
        .unwrap()
        .ty;
    let symbol = psrs_hir::SymbolId::new(
        executor.module,
        input
            .cc
            .functions
            .iter()
            .filter(|f| f.symbol.module == executor.module)
            .map(|f| f.symbol.index)
            .max()
            .unwrap()
            + 1,
    );
    let span = original.span;
    input.cc.functions.push(psrs_backend::cc::Function {
        symbol,
        name: "recursive_no_write".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: extent_ty,
            },
            ValueDecl {
                id: ValueId(1),
                ty: extent_ty,
            },
        ],
        assignments: vec![Assignment {
            destination: ValueId(1),
            kind: A::DirectCall {
                contract: None,
                function: symbol,
                arguments: vec![ValueId(0)],
            },
            span,
        }],
        result: ValueId(1),
        result_type: extent_ty,
        span,
    });
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == executor)
        .unwrap();
    let destination = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    function.values.push(ValueDecl {
        id: destination,
        ty: extent_ty,
    });
    function.assignments.insert(
        0,
        Assignment {
            destination,
            kind: A::DirectCall {
                contract: None,
                function: symbol,
                arguments: vec![function.parameters[4]],
            },
            span,
        },
    );
    let errors = psrs_backend::cc::verify_module(&input.cc)
        .expect_err("an extra recursive call can diverge");
    assert!(
        errors.iter().any(|e| matches!(
            e.pass,
            "P8 CC owned execution" | "P8 CC root and queue effects"
        )),
        "{errors:?}"
    );
}
