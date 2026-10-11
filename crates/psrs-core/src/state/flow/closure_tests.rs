use super::tests::*;
use super::*;

fn callback(body: Expr) -> Expr {
    value(
        ExprKind::Lambda {
            binder: binder(9, 3),
            body: Box::new(body),
        },
        16,
    )
}
fn call(id: u32, argument: Expr) -> Expr {
    value(
        ExprKind::Application(Box::new(local(id, 16)), Box::new(argument)),
        8,
    )
}
fn module(body: Expr) -> Module {
    let mut module = fixture(body);
    module
        .types
        .push(crate::Type::Application(TypeId(9), TypeId(3)));
    module
        .types
        .push(crate::Type::Application(TypeId(15), TypeId(8)));
    module
}

#[test]
fn lexical_callback_consumes_its_captured_source_without_inventing_a_root() {
    let module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        call(2, value(ExprKind::Integer(42), 3)),
    ));
    let flows = check(&module).unwrap();
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].operations.len(), 1);
    assert_eq!(flows[0].operations[0].input, DependencyId(0));
    assert_eq!(flows[0].operations[0].output, DependencyId(1));
    flows[0].verify(&module).unwrap();
}

#[test]
fn creating_an_unused_callback_does_not_execute_its_body() {
    let module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        step(local(0, 2)),
    ));
    let flows = check(&module).unwrap();
    assert!(flows[0].operations.is_empty());
}

#[test]
fn invoking_a_captured_predecessor_twice_rejects() {
    let module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        let_value(
            3,
            call(2, value(ExprKind::Integer(1), 3)),
            call(2, value(ExprKind::Integer(2), 3)),
        ),
    ));
    assert_eq!(
        check(&module).unwrap_err()[0].message,
        "state call consumes a stale dependency"
    );
}

#[test]
fn a_callback_argument_executes_before_the_captured_body() {
    let argument = let_value(3, invocation(local(0, 2)), value(ExprKind::Integer(42), 3));
    let module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        call(2, argument),
    ));
    assert_eq!(
        check(&module).unwrap_err()[0].message,
        "state call consumes a stale dependency"
    );
}

#[test]
fn callback_aliases_survive_a_pure_choice_with_the_selected_root() {
    let chosen = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(local(2, 16)),
            else_branch: Box::new(local(2, 16)),
        },
        16,
    );
    let module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        let_value(3, chosen, call(3, value(ExprKind::Integer(42), 3))),
    ));
    let flows = check(&module).unwrap();
    assert_eq!(flows[0].operations.len(), 1);
    flows[0].verify(&module).unwrap();
}

#[test]
fn an_unknown_callback_consumer_cannot_hide_a_captured_state_operation() {
    let consume = value(
        ExprKind::Application(
            Box::new(value(
                ExprKind::Global(psrs_hir::SymbolId::new(ModuleId(0), 77)),
                18,
            )),
            Box::new(local(2, 16)),
        ),
        3,
    );
    let mut module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        let_value(3, consume, step(local(0, 2))),
    ));
    module
        .types
        .push(crate::Type::Application(TypeId(9), TypeId(16)));
    module
        .types
        .push(crate::Type::Application(TypeId(17), TypeId(3)));
    assert_eq!(
        check(&module).unwrap_err()[0].message,
        "state-carrying argument requires a checked invocation scope"
    );
}

#[test]
fn callback_invocations_cannot_be_removed_from_a_valid_source_witness() {
    let module = module(let_value(
        2,
        callback(invocation(local(0, 2))),
        call(2, value(ExprKind::Integer(42), 3)),
    ));
    let mut flows = check(&module).unwrap();
    let flow = &mut flows[0];
    flow.operations.clear();
    flow.graph.blocks[0].transitions.clear();
    flow.graph.blocks[0].terminator = Terminator::Return(vec![DependencyId(0)]);
    flow.graph.verify().unwrap();
    assert_eq!(
        flow.verify(&module).unwrap_err().message,
        "state graph changes executing source invocation order or coverage"
    );
}
