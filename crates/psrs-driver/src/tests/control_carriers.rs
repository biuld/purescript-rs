//! Source carriers retain nominal shapes before control operand lowering.
use psrs_backend::cc::{
    Assignment, AssignmentKind, ControlHelperRole, Function, RefShape, ValueId, ValueShape,
};
use psrs_hir::Intrinsic;

const SOURCE: &str = include_str!("../../tests/control_support/nominal_carriers.purs");

fn shape(function: &Function, value: ValueId) -> ValueShape {
    function
        .values
        .iter()
        .find(|declaration| declaration.id == value)
        .unwrap()
        .ty
}

fn visit(body: &[Assignment], callback: &mut impl FnMut(&Assignment)) {
    for assignment in body {
        callback(assignment);
        match &assignment.kind {
            AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                visit(then_assignments, callback);
                visit(else_assignments, callback);
            }
            AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    visit(&case.assignments, callback);
                }
                visit(default_assignments, callback);
            }
            _ => {}
        }
    }
}

#[test]
fn all_eight_operations_use_nominal_carriers_and_direct_callable_protocols() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let signature = |role| {
        let helper = table
            .control_helpers()
            .iter()
            .find(|helper| helper.role() == role)
            .unwrap();
        table.signature(helper.signature()).unwrap()
    };
    let computation = signature(ControlHelperRole::Executor).parameters[2];
    let owner = signature(ControlHelperRole::Once).result;
    let callable = signature(ControlHelperRole::Once).parameters[0];
    assert_ne!(computation, owner);
    assert!(
        matches!(computation, ValueShape::Reference(reference) if matches!(reference.heap, RefShape::Repr(_)))
    );
    let erased = ValueShape::Reference(psrs_backend::cc::Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    let mut operations = std::collections::HashSet::new();
    for function in &input.cc.functions {
        visit(&function.assignments, &mut |assignment| {
            let AssignmentKind::ControlEvidence {
                contract,
                arguments,
                implementation,
                value,
            } = &assignment.kind
            else {
                return;
            };
            let operation = table.control_contracts()[contract.0 as usize].operation();
            operations.insert(operation);
            let expected = match operation {
                Intrinsic::ControlReturn | Intrinsic::ControlPerform => vec![erased],
                Intrinsic::ControlBind => vec![computation, callable],
                Intrinsic::ControlHandle => vec![callable, callable, computation],
                Intrinsic::ControlRun => vec![erased, callable, computation],
                Intrinsic::ControlOnce => vec![callable],
                Intrinsic::ControlResume => vec![owner, erased],
                Intrinsic::ControlCancel => vec![owner],
                _ => unreachable!(),
            };
            assert_eq!(
                arguments
                    .iter()
                    .map(|value| shape(function, *value))
                    .collect::<Vec<_>>(),
                expected
            );
            // Frames and helpers consume the witnessed operands directly.
            let producer = function
                .assignments
                .iter()
                .find(|candidate| candidate.destination == *implementation)
                .unwrap();
            let actual = match &producer.kind {
                AssignmentKind::VariantNew { fields, .. } => fields,
                AssignmentKind::DirectCall { arguments, .. } => arguments,
                _ => unreachable!(),
            };
            assert_eq!(&actual[..arguments.len()], arguments);
            if operation == Intrinsic::ControlOnce {
                assert_eq!(value, implementation);
                assert_eq!(shape(function, *value), owner);
            }
        });
    }
    assert_eq!(operations.len(), 8);
    psrs_backend::compile_cc(input).unwrap();
}

#[test]
fn ordinary_parameters_results_and_join_values_keep_the_carrier_shape() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let expected = |role, parameter: Option<usize>| {
        let helper = table
            .control_helpers()
            .iter()
            .find(|helper| helper.role() == role)
            .unwrap();
        let signature = table.signature(helper.signature()).unwrap();
        parameter.map_or(signature.result, |index| signature.parameters[index])
    };
    for (name, carrier, count) in [
        ("keep", expected(ControlHelperRole::Executor, Some(2)), 1),
        ("retain", expected(ControlHelperRole::Once, None), 1),
        ("choose", expected(ControlHelperRole::Executor, Some(2)), 3),
    ] {
        let function = input
            .cc
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap();
        assert_eq!(function.parameters.len(), count);
        assert_eq!(function.result_type, carrier);
        assert_eq!(
            shape(function, *function.parameters.last().unwrap()),
            carrier
        );
        if name == "choose" {
            assert!(function.assignments.iter().any(|assignment| matches!(
                assignment.kind,
                AssignmentKind::If { .. }
            ) && shape(
                function,
                assignment.destination
            ) == carrier));
        }
    }
}

#[test]
fn cc_rejects_weakening_a_source_computation_to_an_abstract_aggregate() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let contracts = &input.cc.representations.control_contracts();
    let (function, value, destination) = input
        .cc
        .functions
        .iter_mut()
        .find_map(|function| {
            function
                .assignments
                .iter()
                .find_map(|assignment| match &assignment.kind {
                    AssignmentKind::ControlEvidence {
                        contract, value, ..
                    } if contracts[contract.0 as usize].operation() == Intrinsic::ControlReturn => {
                        Some((*value, assignment.destination))
                    }
                    _ => None,
                })
                .map(|(value, destination)| (function, value, destination))
        })
        .unwrap();
    for declaration in &mut function.values {
        if declaration.id == value || declaration.id == destination {
            declaration.ty = ValueShape::Reference(psrs_backend::cc::Reference {
                nullable: false,
                heap: RefShape::Aggregate,
            });
        }
    }
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("control carrier result must retain its nominal representation")),
        "{errors:?}"
    );
}

#[test]
fn cc_boundary_rejects_a_dropped_machine_dispatch_arm() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = input
        .cc
        .representations
        .control_helpers()
        .iter()
        .find(|helper| helper.role() == ControlHelperRole::Executor)
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let AssignmentKind::TagSwitch { cases, .. } =
        &mut function.assignments.last_mut().unwrap().kind
    else {
        panic!("executor result must be a computation dispatch");
    };
    cases.retain(|case| case.tag != 1);
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC verification"),
        "boundary check must reject a dropped machine arm: {errors:?}"
    );
}

#[test]
fn mir_boundary_rejects_a_dropped_machine_dispatch_arm() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (mut module, _) = psrs_backend::mir::lower_module(input.cc).unwrap();
    let symbol = module
        .control_helpers()
        .iter()
        .find(|helper| helper.role() == ControlHelperRole::Executor)
        .unwrap()
        .function();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let mut dropped = false;
    for block in &mut function.blocks {
        if let Some(psrs_backend::mir::Terminator::Switch { cases, .. }) = &mut block.terminator
            && cases.len() > 2
        {
            cases.pop();
            dropped = true;
            break;
        }
    }
    assert!(dropped, "executor must contain a machine dispatch switch");
    let errors = psrs_backend::mir::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P9 MIR verification"),
        "MIR boundary check must reject a dropped machine arm: {errors:?}"
    );
}

#[test]
fn mir_boundary_rejects_a_changed_external_tail_target() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (mut module, _) = psrs_backend::mir::lower_module(input.cc).unwrap();
    let clone = module
        .control_helpers()
        .iter()
        .find(|helper| helper.role() == ControlHelperRole::CloneSequence)
        .unwrap()
        .function();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.symbol == clone)
        .unwrap();
    let mut changed = false;
    for block in &mut function.blocks {
        if let Some(psrs_backend::mir::Terminator::ReturnCall {
            function: target, ..
        }) = &mut block.terminator
            && *target != clone
        {
            *target = clone;
            changed = true;
            break;
        }
    }
    assert!(changed, "clone must tail-call rebuild");
    let errors = psrs_backend::mir::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P9 MIR verification"),
        "MIR boundary check must reject a changed tail target: {errors:?}"
    );
}
