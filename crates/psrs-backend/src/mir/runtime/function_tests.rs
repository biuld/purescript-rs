use super::*;
use crate::mir::{Instruction, verify_module};
use crate::types::ValueId;
use crate::{ExternalBindings, TargetCapabilities};
use psrs_runtime::StorageOperation;

fn lower(operation: StorageOperation) -> super::super::Module {
    let (binding, mut source) = tests::fixture(operation);
    if operation == StorageOperation::Read {
        source.functions[0].parameters.insert(2, ValueId(4));
        source.functions[0].values.insert(
            2,
            cc::ValueDecl {
                id: ValueId(4),
                ty: cc::ValueShape::Integer,
            },
        );
    }
    super::super::lower_module_with_bindings(
        source,
        ExternalBindings {
            imports: vec![],
            runtime: vec![binding],
        },
        TargetCapabilities::default(),
    )
    .unwrap()
    .0
}

#[test]
fn checked_raw_invocations_publish_whole_function_dependency_evidence() {
    for operation in [
        StorageOperation::Fill,
        StorageOperation::Read,
        StorageOperation::Write,
    ] {
        let module = lower(operation);
        assert_eq!(module.imports.len(), 1);
        assert!(module.functions[0].state.is_some());
        verify_module(&module).unwrap();
        super::super::opt::optimize(module, TargetCapabilities::default()).unwrap();
    }
}

#[test]
fn whole_function_checks_reject_same_typed_operand_and_provider_substitution() {
    let module = lower(StorageOperation::Read);
    let mut wrong = module.clone();
    let call = wrong.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| matches!(instruction, Instruction::Call { .. }))
        .unwrap();
    let Instruction::Call { arguments, .. } = call else {
        unreachable!()
    };
    arguments[1] = ValueId(4);
    assert!(verify_module(&wrong).is_err());
    let mut wrong = module.clone();
    wrong.imports[0].runtime.as_mut().unwrap().function = "array_fill".into();
    assert!(
        verify_module(&wrong).is_err(),
        "immutable call evidence must retain its checked import"
    );
    let mut wrong = module;
    wrong.imports[0].runtime = None;
    assert!(verify_module(&wrong).is_err());
}

#[test]
fn whole_function_checks_reject_unit_in_place_of_an_actual_write() {
    let module = lower(StorageOperation::Write);
    let mut wrong = module.clone();
    for block in &mut wrong.functions[0].blocks {
        block
            .instructions
            .retain(|instruction| !matches!(instruction, Instruction::CallVoid { .. }));
    }
    assert!(verify_module(&wrong).is_err());
    let mut wrong = module;
    let unit = wrong.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| matches!(instruction, Instruction::Constant { .. }))
        .unwrap();
    let Instruction::Constant { value, .. } = unit else {
        unreachable!()
    };
    *value = 1;
    assert!(verify_module(&wrong).is_err());
}

#[test]
fn nonreturning_invocations_cannot_acquire_a_normal_return_or_continuation() {
    let module = lower(StorageOperation::Trap);
    verify_module(&module).unwrap();
    super::super::opt::optimize(module.clone(), TargetCapabilities::default()).unwrap();
    assert!(matches!(
        module.functions[0].blocks[0].terminator,
        Some(crate::mir::Terminator::Trap { .. })
    ));
    let mut wrong = module.clone();
    wrong.functions[0].blocks[0].terminator = Some(crate::mir::Terminator::Return {
        value: wrong.functions[0].result,
        span: wrong.span,
    });
    assert!(verify_module(&wrong).is_err());
    let mut wrong = module;
    wrong.functions[0].values.push(crate::types::ValueDecl {
        id: ValueId(100),
        ty: ValueType::I32,
    });
    let block = &mut wrong.functions[0].blocks[0];
    block.instructions.push(Instruction::Constant {
        destination: ValueId(100),
        value: 0,
        span: wrong.span,
    });
    assert!(verify_module(&wrong).is_err());
}
