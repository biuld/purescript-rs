use super::*;
use crate::cc::{
    AssignmentKind, RefShape, Reference, ReprId, Representation, Signature, ValueShape,
};
use crate::mir::{Module, lower_module_with_bindings, verify_module};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings, TargetCapabilities};

fn fixture() -> Module {
    let (logical, _) = super::tests::fixture();
    let mut logical = (*logical).clone();
    let getter = logical.externals[0].symbol;
    let drop = psrs_hir::SymbolId::new(getter.module, getter.index + 1);
    let step = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(ReprId(1)),
    });
    logical
        .representations
        .representations
        .push(Representation::Product {
            fields: vec![ValueShape::State, ValueShape::Integer],
        });
    logical.externals.push(cc::External {
        symbol: drop,
        signature: Some(Signature {
            parameters: vec![ValueShape::Integer, ValueShape::State],
            result: step,
        }),
        projection: None,
    });
    let body = &mut logical.functions[0];
    body.values[3].ty = step;
    body.values.push(cc::ValueDecl {
        id: ValueId(4),
        ty: ValueShape::Integer,
    });
    body.assignments.insert(
        2,
        cc::Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::ProductGet {
                destination: ValueId(4),
                representation: ReprId(0),
                field: 1,
                value: ValueId(1),
            },
            span: logical.span,
        },
    );
    body.assignments[3].kind = AssignmentKind::DirectCall {
        function: drop,
        arguments: vec![ValueId(4), ValueId(2)],
    };
    body.result_type = step;
    let imports = [
        (getter, "wasi:cli/stdout", "get-stdout"),
        (drop, "wasi:io/streams", "[resource-drop]output-stream"),
    ]
    .into_iter()
    .map(|(symbol, interface, function)| ExternalBinding {
        symbol,
        interface: interface.into(),
        function: function.into(),
        source_module: getter.module,
        type_id: None,
        span: logical.span,
    })
    .collect();
    lower_module_with_bindings(
        logical,
        ExternalBindings {
            imports,
            runtime: Vec::new(),
        },
        TargetCapabilities::default(),
    )
    .unwrap()
    .0
}

#[test]
fn canonical_state_calls_check_target_operands_unit_and_import() {
    let module = fixture();
    verify_module(&module).unwrap();
    let function = &module.functions[0];
    assert!(
        function.blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::CallVoid { .. }))
    );
    for mutation in 0..4 {
        let mut changed = module.clone();
        let instructions = &mut changed.functions[0].blocks[0].instructions;
        match mutation {
            0 => {
                let Instruction::Call { function, .. } = &mut instructions[0] else {
                    panic!()
                };
                *function = module.functions[0].symbol;
            }
            1 => {
                let instruction = instructions
                    .iter_mut()
                    .find(|instruction| matches!(instruction, Instruction::CallVoid { .. }))
                    .unwrap();
                let Instruction::CallVoid { arguments, .. } = instruction else {
                    panic!()
                };
                arguments[0] = ValueId(3);
            }
            2 => {
                let Instruction::Constant { value, .. } = instructions.last_mut().unwrap() else {
                    panic!()
                };
                *value = 7;
            }
            _ => changed.imports[0]
                .parameters
                .push(crate::types::ValueType::I32),
        }
        let errors = verify_module(&changed).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.pass == "P9 canonical call verification"),
            "mutation {mutation} must fail canonical correspondence: {errors:?}"
        );
    }
}

#[test]
fn canonical_state_calls_reject_reordering_and_extra_calls() {
    let module = fixture();
    let mut changed = module.clone();
    changed.functions[0].blocks[0].instructions.swap(0, 2);
    assert!(verify_module(&changed).is_err());
    let mut changed = module.clone();
    let call = changed.functions[0].blocks[0].instructions[0].clone();
    changed.functions[0].blocks[0].instructions.push(call);
    assert!(verify_module(&changed).is_err());
}

#[test]
fn canonical_dependency_evidence_cannot_be_removed_to_bypass_verification() {
    let module = fixture();
    verify_module(&module).unwrap();
    let mut changed = module.clone();
    changed.functions[0].state = None;
    let errors = verify_module(&changed).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.pass == "P9 MIR dependency verification"
            && error
                .message
                .contains("missing required dependency evidence")
    }));
    let errors = crate::mir::opt::optimize(changed, TargetCapabilities::default()).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.pass == "P10 MIR optimization"
            && error
                .message
                .contains("missing required dependency evidence")
    }));

    let mut changed = module;
    changed.dependencies = Default::default();
    changed.functions[0].state = None;
    let errors = verify_module(&changed).unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("planned MIR has no checked dependency inventory")
    }));
}

#[test]
fn changing_a_projected_function_identity_cannot_remove_its_source_requirement() {
    let mut module = fixture();
    module.functions[0].state = None;
    module.functions[0].symbol = psrs_hir::SymbolId::new(psrs_hir::ModuleId(99), 99);
    let errors = verify_module(&module).unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("no source or generated-helper provenance")
    }));
}
