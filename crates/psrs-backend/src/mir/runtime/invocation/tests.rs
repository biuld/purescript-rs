use super::*;
use psrs_runtime::StorageOperation;

fn fixture(operation: StorageOperation) -> Result<RawInvocation, Vec<BackendError>> {
    let (binding, mut source) = crate::mir::runtime::tests::fixture(operation);
    if operation == StorageOperation::Read {
        let body = &mut source.functions[0];
        body.parameters.insert(2, ValueId(4));
        body.values.insert(
            2,
            cc::ValueDecl {
                id: ValueId(4),
                ty: cc::ValueShape::Integer,
            },
        );
    }
    let projected = crate::mir::state::project(source, std::slice::from_ref(&binding)).unwrap();
    let layout = crate::mir::planner::RepresentationPlanner::plan_module(
        &crate::mir::planner::GcPlanner::default(),
        &projected.physical,
    )
    .unwrap();
    let function = &projected.logical.functions[0];
    let first = function
        .values
        .iter()
        .map(|value| value.id.0)
        .max()
        .unwrap()
        + 1;
    RawInvocation::from_source(
        &binding,
        &projected.logical,
        function.symbol,
        function.result,
        &layout,
        first,
    )
}

#[test]
fn returning_source_calls_emit_and_verify_actual_raw_invocations() {
    for operation in [
        StorageOperation::Fill,
        StorageOperation::Read,
        StorageOperation::Write,
    ] {
        let plan = fixture(operation).unwrap();
        let (values, instructions) = plan.emit().unwrap();
        plan.verify(&values, &instructions).unwrap();
        assert_eq!(
            instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction,
                    Instruction::Call { .. } | Instruction::CallVoid { .. }
                ))
                .count(),
            1
        );
        assert!(
            values
                .iter()
                .all(|value| value.id.0 >= plan.first_temporary)
        );
        assert!(plan.import().runtime.is_some());
        if operation == StorageOperation::Write {
            assert!(matches!(
                instructions[instructions.len() - 2],
                Instruction::CallVoid { .. }
            ));
            assert!(
                matches!(instructions.last(), Some(Instruction::Constant { destination, value: 0, .. })
                if *destination == plan.destination)
            );
        }
    }
}

#[test]
fn raw_operand_witnesses_reject_same_typed_substitution() {
    let plan = fixture(StorageOperation::Read).unwrap();
    let (values, instructions) = plan.emit().unwrap();
    let mut wrong = instructions.clone();
    let Instruction::Call { arguments, .. } = &mut wrong[0] else {
        panic!("read must start with the call");
    };
    arguments[1] = ValueId(4);
    assert!(plan.verify(&values, &wrong).is_err());
    let mut wrong = instructions.clone();
    let Instruction::RefCast { value, .. } = wrong.last_mut().unwrap() else {
        panic!("read must recover its result");
    };
    *value = plan.arguments[0];
    assert!(plan.verify(&values, &wrong).is_err());
    let mut wrong = instructions;
    wrong.push(wrong[0].clone());
    assert!(plan.verify(&values, &wrong).is_err());
}

#[test]
fn unit_cannot_replace_an_omitted_or_retargeted_write() {
    let plan = fixture(StorageOperation::Write).unwrap();
    let (values, instructions) = plan.emit().unwrap();
    for mutation in 0..3 {
        let mut wrong = instructions.clone();
        let call = wrong
            .iter()
            .position(|instruction| matches!(instruction, Instruction::CallVoid { .. }))
            .unwrap();
        match mutation {
            0 => {
                wrong.remove(call);
            }
            1 => {
                if let Instruction::CallVoid { function, .. } = &mut wrong[call] {
                    *function = SymbolId::new(function.module, function.index + 1);
                }
            }
            _ => {
                if let Some(Instruction::Constant { value, .. }) = wrong.last_mut() {
                    *value = 1;
                }
            }
        }
        assert!(plan.verify(&values, &wrong).is_err(), "mutation {mutation}");
    }
}

#[test]
fn nonreturning_calls_emit_an_actual_void_call_and_bottom_without_unit() {
    let plan = fixture(StorageOperation::Trap).unwrap();
    let (values, instructions) = plan.emit().unwrap();
    assert!(values.is_empty());
    assert!(matches!(
        &instructions[..],
        [
            Instruction::CallVoid { .. },
            Instruction::Unreachable { .. }
        ]
    ));
    assert!(plan.verify(&values, &instructions[..1]).is_err());
    let mut wrong = instructions;
    wrong[1] = Instruction::Constant {
        destination: plan.destination(),
        value: 0,
        span: plan.span,
    };
    assert!(plan.verify(&values, &wrong).is_err());
}
