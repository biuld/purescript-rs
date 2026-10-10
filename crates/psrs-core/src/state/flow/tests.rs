use super::*;
use crate::{Binder, Binding, CaseBranch, Declaration, Pattern, PatternKind};
use psrs_hir::{CaseBranchCoverage, LocalId, SymbolId};

fn value(kind: ExprKind, ty: u32) -> Expr {
    Expr {
        kind,
        ty: TypeId(ty),
        span: TextRange::new(10, 20),
    }
}
fn local(id: u32, ty: u32) -> Expr {
    value(ExprKind::Local(LocalId(id)), ty)
}
fn step(state: Expr) -> Expr {
    value(
        ExprKind::Record {
            fields: vec![
                ("state".into(), state),
                ("value".into(), value(ExprKind::Integer(42), 3)),
            ],
        },
        8,
    )
}
fn binder(id: u32, ty: u32) -> Binder {
    Binder {
        id: LocalId(id),
        name: format!("v{id}"),
        ty: TypeId(ty),
        span: TextRange::new(10, 20),
    }
}
fn action(body: Expr) -> Expr {
    value(
        ExprKind::Lambda {
            binder: binder(0, 2),
            body: Box::new(body),
        },
        11,
    )
}
fn invocation(input: Expr) -> Expr {
    value(
        ExprKind::Application(Box::new(local(1, 11)), Box::new(input)),
        8,
    )
}
fn field(id: u32) -> Expr {
    value(
        ExprKind::FieldAccess {
            record: Box::new(local(id, 8)),
            field: "state".into(),
        },
        2,
    )
}
fn let_value(id: u32, expression: Expr, body: Expr) -> Expr {
    let ty = expression.ty.0;
    let result = body.ty.0;
    value(
        ExprKind::Let {
            bindings: vec![Binding {
                binder: binder(id, ty),
                quantified: Vec::new(),
                span: expression.span,
                value: expression,
            }],
            body: Box::new(body),
        },
        result,
    )
}
fn fixture(body: Expr) -> Module {
    let mut module = crate::state::tests::fixture();
    module
        .types
        .push(crate::Type::Constructor(crate::TypeConstructor::Boolean));
    module
        .types
        .push(crate::Type::Application(TypeId(9), TypeId(11)));
    module
        .types
        .push(crate::Type::Application(TypeId(13), TypeId(11)));
    let action = action(body);
    let body = value(
        ExprKind::Lambda {
            binder: binder(1, 11),
            body: Box::new(action),
        },
        14,
    );
    module.declarations.push(Declaration {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "action".into(),
        name_span: body.span,
        quantified: Vec::new(),
        ty: TypeId(14),
        span: body.span,
        value: body,
    });
    module
}

#[test]
fn pure_return_uses_the_lambda_parameter_without_inventing_a_root() {
    let module = fixture(step(local(0, 2)));
    let flows = check(&module).unwrap();
    assert_eq!(flows.len(), 1);
    assert!(flows[0].operations.is_empty());
    assert_eq!(
        flows[0].graph.blocks[0].terminator,
        Terminator::Return(vec![DependencyId(0)])
    );
}

#[test]
fn actual_calls_supply_successors_and_aliases_retain_their_producers() {
    let body = let_value(
        2,
        invocation(local(0, 2)),
        let_value(3, invocation(field(2)), step(field(3))),
    );
    let module = fixture(body);
    let flows = check(&module).unwrap();
    let flow = &flows[0];
    assert_eq!(flow.operations.len(), 2);
    assert_eq!(flow.operations[0].input, DependencyId(0));
    assert_eq!(flow.operations[1].input, flow.operations[0].output);
    assert!(
        flow.operations
            .iter()
            .all(|operation| matches!(operation.expression.kind, ExprKind::Application(..)))
    );
    assert_eq!(flow.graph.blocks[0].transitions.len(), 2);
}

#[test]
fn fixed_arity_state_invocation_retains_its_actual_operation_dependency() {
    let mut module = fixture(invocation(local(0, 2)));
    module.types[11] = crate::Type::Closure {
        parameters: vec![TypeId(2)],
        result: TypeId(8),
    };
    module.verify().unwrap();
    let flows = check(&module).unwrap();
    assert_eq!(flows[0].operations.len(), 1);
    assert_eq!(flows[0].operations[0].input, DependencyId(0));
    assert_eq!(
        flows[0].graph.blocks[0].terminator,
        Terminator::Return(vec![flows[0].operations[0].output])
    );
}

#[test]
fn discarded_payload_does_not_allow_discarding_a_state_operation() {
    let module = fixture(let_value(2, invocation(local(0, 2)), step(local(0, 2))));
    let errors = check(&module).unwrap_err();
    assert_eq!(
        errors[0].message,
        "state return discards an executed operation"
    );
}

#[test]
fn repeated_use_of_an_observable_predecessor_is_rejected() {
    let module = fixture(let_value(
        2,
        invocation(local(0, 2)),
        let_value(3, invocation(local(0, 2)), step(field(3))),
    ));
    assert_eq!(
        check(&module).unwrap_err()[0].message,
        "state call consumes a stale dependency"
    );
}

#[test]
fn branch_exclusive_calls_transfer_the_executed_successor_to_a_join() {
    let body = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(invocation(local(0, 2))),
            else_branch: Box::new(invocation(local(0, 2))),
        },
        8,
    );
    let module = fixture(body);
    let flows = check(&module).unwrap();
    assert_eq!(flows[0].operations.len(), 2);
    assert_eq!(flows[0].graph.blocks.len(), 4);
    flows[0].graph.verify().unwrap();
}

#[test]
fn a_pure_branch_join_preserves_incoming_state_aliases() {
    let choice = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(value(ExprKind::Integer(1), 3)),
            else_branch: Box::new(value(ExprKind::Integer(2), 3)),
        },
        3,
    );
    let module = fixture(let_value(2, choice, step(local(0, 2))));
    module.verify().unwrap();
    assert!(check(&module).unwrap()[0].operations.is_empty());
}

#[test]
fn a_trapping_branch_has_no_normal_successor() {
    let choice = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(invocation(local(0, 2))),
            else_branch: Box::new(value(ExprKind::Trap, 8)),
        },
        8,
    );
    let module = fixture(choice);
    module.verify().unwrap();
    let flows = check(&module).unwrap();
    assert_eq!(flows[0].operations.len(), 1);
    assert!(
        flows[0]
            .graph
            .blocks
            .iter()
            .any(|block| block.terminator == Terminator::Trap)
    );
}

#[test]
fn a_stale_return_on_one_branch_is_not_hidden_by_a_valid_other_branch() {
    let choice = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(let_value(2, invocation(local(0, 2)), step(local(0, 2)))),
            else_branch: Box::new(step(local(0, 2))),
        },
        8,
    );
    assert!(check(&fixture(choice)).is_err());
}

#[test]
fn a_record_pattern_retains_the_called_operations_successor() {
    let module = fixture(value(
        ExprKind::Case {
            scrutinee: Box::new(invocation(local(0, 2))),
            branches: vec![CaseBranch {
                pattern: Pattern {
                    ty: TypeId(8),
                    span: TextRange::new(10, 20),
                    kind: PatternKind::Record {
                        fields: vec![(
                            "state".into(),
                            Pattern {
                                ty: TypeId(2),
                                span: TextRange::new(10, 20),
                                kind: PatternKind::Var {
                                    id: LocalId(2),
                                    ty: TypeId(2),
                                },
                            },
                        )],
                    },
                },
                value: step(local(2, 2)),
                span: TextRange::new(10, 20),
                coverage: CaseBranchCoverage::Source,
            }],
        },
        8,
    ));
    module.verify().unwrap();
    assert_eq!(check(&module).unwrap()[0].operations.len(), 1);
}

#[test]
fn core_verification_rejects_a_well_typed_but_stale_state_return() {
    let module = fixture(let_value(2, invocation(local(0, 2)), step(local(0, 2))));
    let errors = module.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message == "state return discards an executed operation")
    );
    assert!(errors.iter().all(|error| error.module == ModuleId(0)));
}

#[test]
fn state_values_cannot_be_fabricated_by_unknown_payloads() {
    let mut module = fixture(step(local(4, 2)));
    // Producer validation is also callable independently of ordinary typing;
    // a dangling local must not acquire a fresh state root.
    assert_eq!(
        check(&module).unwrap_err()[0].message,
        "state value has no checked producer provenance"
    );
    module.declarations.clear();
    assert!(check(&module).unwrap().is_empty());
}

#[test]
fn a_structurally_valid_graph_cannot_omit_real_source_invocations() {
    let module = fixture(invocation(local(0, 2)));
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

#[test]
fn a_structurally_valid_graph_cannot_reorder_real_source_invocations() {
    let module = fixture(let_value(2, invocation(local(0, 2)), invocation(field(2))));
    let mut flows = check(&module).unwrap();
    let flow = &mut flows[0];
    let first = flow.operations[0].expression;
    flow.operations[0].expression = flow.operations[1].expression;
    flow.operations[1].expression = first;
    flow.graph.verify().unwrap();
    assert_eq!(
        flow.verify(&module).unwrap_err().message,
        "state graph changes executing source invocation order or coverage"
    );
}

#[test]
fn source_evidence_cannot_be_reused_with_a_different_arena() {
    let module = fixture(invocation(local(0, 2)));
    let flows = check(&module).unwrap();
    let other = module.clone();
    assert_eq!(
        flows[0].verify(&other).unwrap_err().message,
        "state flow belongs to a different source arena"
    );
}

#[test]
fn unreachable_calls_after_a_trap_do_not_create_successors() {
    let module = fixture(let_value(
        2,
        value(ExprKind::Trap, 3),
        invocation(local(0, 2)),
    ));
    module.verify().unwrap();
    let flows = check(&module).unwrap();
    assert!(flows[0].operations.is_empty());
    assert_eq!(flows[0].graph.blocks[0].terminator, Terminator::Trap);
}

#[test]
fn a_proven_pure_state_body_may_return_its_incoming_state_unchanged() {
    let pure = value(
        ExprKind::Lambda {
            binder: binder(5, 2),
            body: Box::new(step(local(5, 2))),
        },
        11,
    );
    let call = value(
        ExprKind::Application(Box::new(pure), Box::new(local(0, 2))),
        8,
    );
    let module = fixture(let_value(2, call, step(local(0, 2))));
    module.verify().unwrap();
    assert!(
        check(&module)
            .unwrap()
            .iter()
            .all(|flow| flow.operations.is_empty())
    );
}

#[test]
fn an_invocation_after_a_branch_uses_the_selected_successor() {
    let branch = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(invocation(local(0, 2))),
            else_branch: Box::new(invocation(local(0, 2))),
        },
        8,
    );
    let module = fixture(let_value(2, branch, invocation(field(2))));
    module.verify().unwrap();
    let flows = check(&module).unwrap();
    assert_eq!(flows[0].operations.len(), 3);
    assert_eq!(flows[0].graph.blocks.last().unwrap().transitions.len(), 1);
}

#[test]
fn a_valid_graph_cannot_move_a_call_to_the_other_source_branch() {
    let body = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(invocation(local(0, 2))),
            else_branch: Box::new(invocation(local(0, 2))),
        },
        8,
    );
    let module = fixture(body);
    let mut flows = check(&module).unwrap();
    let flow = &mut flows[0];
    let left = flow.graph.blocks[1].transitions.remove(0);
    let right = flow.graph.blocks[2].transitions.remove(0);
    flow.graph.blocks[1].transitions.push(right);
    flow.graph.blocks[2].transitions.push(left);
    for index in [1, 2] {
        let block = &mut flow.graph.blocks[index];
        let transition = &mut block.transitions[0];
        transition.input = block.parameters[0].id;
        flow.operations[transition.operation as usize].input = transition.input;
        let Terminator::Jump(edge) = &mut block.terminator else {
            unreachable!()
        };
        edge.arguments[0] = transition.output.id;
    }
    flow.graph.verify().unwrap();
    assert_eq!(
        flow.verify(&module).unwrap_err().message,
        "state graph changes source dependency provenance or control flow"
    );
}

#[test]
fn a_pure_payload_branch_keeps_a_previously_evaluated_state_field_valid() {
    let payload = value(
        ExprKind::If {
            condition: Box::new(value(ExprKind::Boolean(true), 12)),
            then_branch: Box::new(value(ExprKind::Integer(1), 3)),
            else_branch: Box::new(value(ExprKind::Integer(2), 3)),
        },
        3,
    );
    let module = fixture(value(
        ExprKind::Record {
            fields: vec![("state".into(), local(0, 2)), ("value".into(), payload)],
        },
        8,
    ));
    module.verify().unwrap();
}
