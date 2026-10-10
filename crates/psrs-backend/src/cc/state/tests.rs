use super::*;
use crate::cc::{
    AssignmentKind, External, RefShape, Reference, ReprId, Representation, RepresentationTable,
    ValueDecl,
};
use psrs_hir::ModuleId;
use psrs_span::TextRange;

fn step_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(ReprId(0)),
    })
}
fn assignment(id: u32, kind: AssignmentKind) -> Assignment {
    Assignment {
        destination: ValueId(id),
        kind,
        span: TextRange::new(id, id + 1),
    }
}
fn call(id: u32, input: u32) -> Assignment {
    assignment(
        id,
        AssignmentKind::DirectCall {
            function: SymbolId::new(ModuleId(0), 1),
            arguments: vec![ValueId(input)],
        },
    )
}
fn product(id: u32, state: u32, payload: u32) -> Assignment {
    assignment(
        id,
        AssignmentKind::ProductNew {
            destination: ValueId(id),
            representation: ReprId(0),
            arguments: vec![ValueId(state), ValueId(payload)],
        },
    )
}
fn fixture(assignments: Vec<Assignment>, shapes: &[ValueShape], result: u32) -> Module {
    let mut representations = RepresentationTable::default();
    representations
        .representations
        .push(Representation::Product {
            fields: vec![ValueShape::State, ValueShape::Integer],
        });
    Module {
        name: "CCStateBody".into(),
        representations,
        externals: vec![External {
            symbol: SymbolId::new(ModuleId(0), 1),
            signature: Some(Signature {
                parameters: vec![ValueShape::State],
                result: step_shape(),
            }),
            projection: None,
        }],
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "action".into(),
            parameters: vec![ValueId(0)],
            values: shapes
                .iter()
                .enumerate()
                .map(|(index, ty)| ValueDecl {
                    id: ValueId(index as u32),
                    ty: *ty,
                })
                .collect(),
            assignments,
            result: ValueId(result),
            result_type: shapes[result as usize],
            span: TextRange::new(0, 20),
        }],
        entry: None,
        span: TextRange::new(0, 20),
    }
}

#[test]
fn actual_cc_calls_and_field_projections_supply_successor_dependencies() {
    let get = assignment(
        2,
        AssignmentKind::ProductGet {
            destination: ValueId(2),
            representation: ReprId(0),
            field: 0,
            value: ValueId(1),
        },
    );
    let module = fixture(
        vec![call(1, 0), get, call(3, 2)],
        &[
            ValueShape::State,
            step_shape(),
            ValueShape::State,
            step_shape(),
        ],
        3,
    );
    let flows = check(&module).unwrap();
    let flow = &flows[0];
    assert_eq!(flow.operations.len(), 2);
    assert!(std::ptr::eq(
        flow.operations[0].assignment,
        &module.functions[0].assignments[0]
    ));
    assert_eq!(flow.operations[1].input, flow.operations[0].output);
    flow.verify(&module).unwrap();
    assert!(flow.verify(&module.clone()).is_err());
}

#[test]
fn stale_cc_operands_and_returns_are_rejected() {
    let replay = fixture(
        vec![call(1, 0), call(2, 0)],
        &[ValueShape::State, step_shape(), step_shape()],
        2,
    );
    assert!(
        check(&replay)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("stale dependency"))
    );
    let stale = fixture(
        vec![
            call(1, 0),
            assignment(2, AssignmentKind::Constant(42)),
            product(3, 0, 2),
        ],
        &[
            ValueShape::State,
            step_shape(),
            ValueShape::Integer,
            step_shape(),
        ],
        3,
    );
    assert!(
        check(&stale)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("return loses"))
    );
}

#[test]
fn mir_entry_checks_actual_cc_dependencies_before_physical_planning() {
    let replay = fixture(
        vec![call(1, 0), call(2, 0)],
        &[ValueShape::State, step_shape(), step_shape()],
        2,
    );
    let bindings = crate::ExternalBindings {
        runtime: Vec::new(),
        imports: vec![crate::ExternalBinding {
            symbol: replay.externals[0].symbol,
            source_module: ModuleId(0),
            interface: crate::abi::names::STDOUT.into(),
            function: crate::abi::names::GET_STDOUT.into(),
            type_id: None,
            span: replay.span,
        }],
    };
    let errors = crate::mir::lower_module_with_bindings(
        replay,
        bindings,
        crate::TargetCapabilities::default(),
    )
    .err()
    .expect("MIR must reject a stale CC invocation");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("stale dependency"))
    );
    assert!(
        !errors
            .iter()
            .any(|error| error.message.contains("UnprojectedState"))
    );
}

#[test]
fn dependency_recheck_rejects_missing_operands_and_parameter_metadata() {
    let mut module = fixture(vec![call(1, 0)], &[ValueShape::State, step_shape()], 1);
    if let AssignmentKind::DirectCall { arguments, .. } =
        &mut module.functions[0].assignments[0].kind
    {
        arguments.clear();
    }
    assert!(
        derive_all(&module)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("missing its dependency operand"))
    );
    module.functions[0].values.remove(0);
    assert!(
        derive_all(&module)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("no value declaration"))
    );
}

#[test]
fn branch_calls_join_the_executed_successor() {
    let choice = assignment(
        4,
        AssignmentKind::If {
            condition: ValueId(1),
            then_assignments: vec![call(2, 0)],
            then_value: ValueId(2),
            else_assignments: vec![call(3, 0)],
            else_value: ValueId(3),
        },
    );
    let get = assignment(
        5,
        AssignmentKind::ProductGet {
            destination: ValueId(5),
            representation: ReprId(0),
            field: 0,
            value: ValueId(4),
        },
    );
    let module = fixture(
        vec![
            assignment(1, AssignmentKind::Constant(1)),
            choice,
            get,
            call(6, 5),
        ],
        &[
            ValueShape::State,
            ValueShape::Boolean,
            step_shape(),
            step_shape(),
            step_shape(),
            ValueShape::State,
            step_shape(),
        ],
        6,
    );
    let flows = check(&module).unwrap();
    assert_eq!(flows[0].operations.len(), 3);
    assert_eq!(flows[0].graph.blocks.len(), 4);
    assert_eq!(
        flows[0].operations[2].input,
        flows[0].graph.blocks[3].parameters[0].id
    );
}

#[test]
fn a_pure_cc_join_preserves_previously_evaluated_aliases() {
    let choice = assignment(
        5,
        AssignmentKind::If {
            condition: ValueId(1),
            then_assignments: vec![product(3, 0, 2)],
            then_value: ValueId(3),
            else_assignments: vec![product(4, 0, 2)],
            else_value: ValueId(4),
        },
    );
    let module = fixture(
        vec![
            assignment(1, AssignmentKind::Constant(1)),
            assignment(2, AssignmentKind::Constant(42)),
            choice,
            product(6, 0, 2),
        ],
        &[
            ValueShape::State,
            ValueShape::Boolean,
            ValueShape::Integer,
            step_shape(),
            step_shape(),
            step_shape(),
            step_shape(),
        ],
        6,
    );
    assert!(check(&module).unwrap()[0].operations.is_empty());
}

#[test]
fn pure_call_summaries_come_from_checked_bodies_in_any_declaration_order() {
    let mut module = fixture(
        vec![
            call(1, 0),
            assignment(2, AssignmentKind::Constant(42)),
            product(3, 0, 2),
        ],
        &[
            ValueShape::State,
            step_shape(),
            ValueShape::Integer,
            step_shape(),
        ],
        3,
    );
    let mut pure = fixture(
        vec![
            assignment(1, AssignmentKind::Constant(42)),
            product(2, 0, 1),
        ],
        &[ValueShape::State, ValueShape::Integer, step_shape()],
        2,
    )
    .functions
    .remove(0);
    pure.symbol = SymbolId::new(ModuleId(0), 1);
    module.externals.clear();
    module.functions.push(pure);
    let flows = check(&module).unwrap();
    assert!(flows.iter().all(|flow| flow.operations.is_empty()));
}

#[test]
fn a_trapping_cc_branch_has_no_normal_successor() {
    let choice = assignment(
        4,
        AssignmentKind::If {
            condition: ValueId(1),
            then_assignments: vec![call(2, 0)],
            then_value: ValueId(2),
            else_assignments: vec![assignment(3, AssignmentKind::Unreachable)],
            else_value: ValueId(3),
        },
    );
    let module = fixture(
        vec![assignment(1, AssignmentKind::Constant(1)), choice],
        &[
            ValueShape::State,
            ValueShape::Boolean,
            step_shape(),
            step_shape(),
            step_shape(),
        ],
        4,
    );
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
fn state_is_neither_a_fabricated_constant_nor_boxed_payload() {
    let mut module = fixture(
        vec![assignment(1, AssignmentKind::Constant(0))],
        &[ValueShape::State, ValueShape::State],
        1,
    );
    assert!(check(&module).is_err());
    module
        .representations
        .representations
        .push(Representation::Box {
            value: ValueShape::State,
        });
    assert!(
        check(&module)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("no box or array"))
    );
}

#[test]
fn source_correspondence_rejects_an_edited_graph() {
    let module = fixture(vec![call(1, 0)], &[ValueShape::State, step_shape()], 1);
    let mut flows = check(&module).unwrap();
    flows[0].graph.blocks[0].transitions.clear();
    flows[0].operations.clear();
    flows[0].graph.blocks[0].terminator = Terminator::Return(vec![DependencyId(0)]);
    flows[0].graph.verify().unwrap();
    assert!(flows[0].verify(&module).is_err());
}
