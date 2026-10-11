//! Nominal machine identities must survive helper calls and closure captures.

use super::super::{verify_function, verify_module};
use super::symbol;
use crate::cc::layout::control;
use crate::cc::payload::erased_shape;
use crate::cc::{
    Assignment, AssignmentKind, Function, Module, RefShape, Reference, RepresentationTable,
    Signature, ValueDecl, ValueId, ValueShape,
};
use psrs_span::TextRange;
use std::collections::HashMap;

fn reference(heap: RefShape) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap,
    })
}

fn call_fixture(signature: &Signature) -> Function {
    let mut values = signature
        .parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| ValueDecl {
            id: ValueId(index as u32),
            ty: *ty,
        })
        .collect::<Vec<_>>();
    let parameters = values.iter().map(|value| value.id).collect::<Vec<_>>();
    let result = ValueId(values.len() as u32);
    values.push(ValueDecl {
        id: result,
        ty: signature.result,
    });
    let span = TextRange::new(0, 1);
    Function {
        symbol: symbol(0),
        name: "control_call".into(),
        parameters: parameters.clone(),
        values,
        assignments: vec![Assignment {
            destination: result,
            kind: AssignmentKind::DirectCall {
                function: symbol(1),
                arguments: parameters,
            },
            span,
        }],
        result,
        result_type: signature.result,
        span,
    }
}

#[test]
fn control_helper_calls_reject_erased_and_cross_wired_machine_arguments() {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let layout = control::append(&mut table, continuation);
    for (id, argument, wrong) in [
        (
            layout.executor,
            3,
            reference(RefShape::Repr(layout.boundary)),
        ),
        (
            layout.clone_sequence,
            0,
            reference(RefShape::Repr(layout.boundary)),
        ),
        (
            layout.rebuild_sequence,
            2,
            reference(RefShape::Repr(layout.sequence)),
        ),
        (
            layout.boundary_check,
            0,
            reference(RefShape::Repr(layout.owner)),
        ),
        (layout.once, 0, reference(RefShape::Repr(layout.owner))),
        (layout.cancel, 0, reference(RefShape::Repr(layout.boundary))),
    ] {
        let signature = table.signature(id).unwrap();
        let signatures = HashMap::from([(symbol(1), signature.clone())]);
        let valid = call_fixture(signature);
        verify_function(&valid, &signatures, &table).expect("matching machine call");
        for wrong in [wrong, erased_shape()] {
            let mut invalid = valid.clone();
            invalid.values[argument].ty = wrong;
            let errors = verify_function(&invalid, &signatures, &table).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("call shape mismatch")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn control_replay_result_cannot_be_declared_as_an_erased_user_pair() {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let layout = control::append(&mut table, continuation);
    let signature = table.signature(layout.clone_sequence).unwrap();
    let signatures = HashMap::from([(symbol(1), signature.clone())]);
    let mut function = call_fixture(signature);
    function.values.last_mut().unwrap().ty = erased_shape();
    function.result_type = erased_shape();
    let errors = verify_function(&function, &signatures, &table).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("call result shape mismatch")),
        "{errors:?}"
    );
}

#[test]
fn mir_rejects_machine_call_arguments_with_swapped_queue_and_extent() {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let layout = control::append(&mut table, continuation);
    for (id, left, right) in [
        (layout.executor, 3, 4),
        (layout.clone_sequence, 0, 3),
        (layout.rebuild_sequence, 1, 2),
    ] {
        let caller = call_fixture(table.signature(id).unwrap());
        // The callee body is irrelevant to this call-contract verifier fixture.
        let mut callee = caller.clone();
        callee.symbol = symbol(1);
        callee.name = "machine_callee".into();
        callee.assignments[0].kind = AssignmentKind::Unreachable;
        let module = Module {
            name: "MachineCalls".into(),
            functions: vec![caller, callee],
            externals: vec![],
            representations: table.clone(),
            entry: None,
            span: TextRange::new(0, 1),
        };
        verify_module(&module).expect("valid CC helper call");
        let (mut mir, _) = crate::mir::lower_module(module).expect("valid MIR helper call");
        let terminator = mir.functions[0]
            .blocks
            .iter_mut()
            .find_map(|block| {
                block.terminator.as_mut().filter(|terminator| {
                    matches!(terminator, crate::mir::Terminator::ReturnCall { .. })
                })
            })
            .expect("typed helper tail call");
        let crate::mir::Terminator::ReturnCall { arguments, .. } = terminator else {
            unreachable!()
        };
        arguments.swap(left, right);
        let errors = crate::mir::verify_module(&mir).expect_err("wrong machine argument types");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("argument")),
            "{errors:?}"
        );
    }
}

fn raw_capture_module() -> Module {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let layout = control::append(&mut table, continuation);
    let span = TextRange::new(0, 1);
    let queue = reference(RefShape::Repr(layout.sequence));
    let boundary = reference(RefShape::Repr(layout.boundary));
    let receiver = reference(RefShape::Aggregate);
    let closure = reference(RefShape::Closure(continuation));
    let target = Function {
        symbol: symbol(1),
        name: "raw_capture_target".into(),
        parameters: vec![ValueId(0), ValueId(1)],
        values: [receiver, erased_shape(), queue, boundary]
            .into_iter()
            .enumerate()
            .map(|(index, ty)| ValueDecl {
                id: ValueId(index as u32),
                ty,
            })
            .collect(),
        assignments: (0..2)
            .map(|index| Assignment {
                destination: ValueId(index + 2),
                kind: AssignmentKind::ClosureGetCapture {
                    closure: ValueId(0),
                    index,
                },
                span,
            })
            .collect(),
        result: ValueId(1),
        result_type: erased_shape(),
        span,
    };
    let caller = Function {
        symbol: symbol(0),
        name: "raw_capture".into(),
        parameters: vec![ValueId(0), ValueId(1)],
        values: [queue, boundary, closure]
            .into_iter()
            .enumerate()
            .map(|(index, ty)| ValueDecl {
                id: ValueId(index as u32),
                ty,
            })
            .collect(),
        assignments: vec![Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::FunctionRef {
                function: symbol(1),
                signature: continuation,
                captures: vec![ValueId(0), ValueId(1)],
            },
            span,
        }],
        result: ValueId(2),
        result_type: closure,
        span,
    };
    Module {
        name: "RawCapture".into(),
        functions: vec![caller, target],
        externals: vec![],
        representations: table,
        entry: None,
        span,
    }
}

#[test]
fn raw_resumption_capture_preserves_queue_and_checkpoint_shapes() {
    let module = raw_capture_module();
    verify_module(&module).expect("typed checkpoint capture");
    for captures in [vec![ValueId(1), ValueId(0)], vec![ValueId(0), ValueId(0)]] {
        let mut invalid = module.clone();
        let AssignmentKind::FunctionRef {
            captures: actual, ..
        } = &mut invalid.functions[0].assignments[0].kind
        else {
            unreachable!()
        };
        *actual = captures;
        let errors = verify_module(&invalid).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("captures do not match")),
            "{errors:?}"
        );
    }
}

#[test]
fn mir_raw_resumption_rejects_swapped_duplicate_and_missing_captures() {
    let module = raw_capture_module();
    let (mir, _) = crate::mir::lower_module(module).expect("typed raw captures lower");
    crate::mir::verify_module(&mir).expect("typed MIR captures");
    for slots in [vec![1, 0], vec![0, 0], vec![0]] {
        let mut invalid = mir.clone();
        let instruction = &mut invalid.functions[0].blocks[0].instructions[0];
        let crate::mir::Instruction::ClosureNew { captures, .. } = instruction else {
            panic!("expected raw closure construction");
        };
        *captures = slots.into_iter().map(crate::types::ValueId).collect();
        let errors = crate::mir::verify_module(&invalid).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("captures do not match")),
            "{errors:?}"
        );
    }
}

#[test]
fn mir_capture_contract_survives_sparse_projections_after_optimization() {
    let (mut mir, _) = crate::mir::lower_module(raw_capture_module()).unwrap();
    // Dead projection elimination may leave slot 1 without slot 0. Stored
    // captures are unchanged; MIR must check the surviving slot by its index.
    mir.functions[1].blocks[0].instructions.remove(0);
    crate::mir::verify_module(&mir).expect("sparse live projection contract");
    let crate::mir::Instruction::ClosureNew { captures, .. } =
        &mut mir.functions[0].blocks[0].instructions[0]
    else {
        unreachable!()
    };
    captures.swap(0, 1);
    assert!(
        crate::mir::verify_module(&mir)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("captures do not match"))
    );
}

#[test]
fn mir_capture_contract_rejects_inconsistent_reads_of_one_slot() {
    let (mut mir, _) = crate::mir::lower_module(raw_capture_module()).unwrap();
    let crate::mir::Instruction::ClosureGetCapture { index, .. } =
        &mut mir.functions[1].blocks[0].instructions[1]
    else {
        unreachable!()
    };
    *index = 0;
    let errors = crate::mir::verify_module(&mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("incompatible projection types")),
        "{errors:?}"
    );
}

#[test]
fn mir_capture_contract_tracks_receiver_copies_and_reference_casts() {
    let (mir, _) = crate::mir::lower_module(raw_capture_module()).unwrap();
    for cast in [false, true] {
        let mut module = mir.clone();
        let target = &mut module.functions[1];
        let receiver = target.parameters[0];
        let ty = target
            .values
            .iter()
            .find(|value| value.id == receiver)
            .unwrap()
            .ty;
        let alias = crate::types::ValueId(4);
        target
            .values
            .push(crate::types::ValueDecl { id: alias, ty });
        for instruction in &mut target.blocks[0].instructions {
            if let crate::mir::Instruction::ClosureGetCapture { closure, .. } = instruction {
                *closure = alias;
            }
        }
        let span = target.span;
        let copy = if cast {
            let crate::types::ValueType::Ref(reference) = ty else {
                unreachable!()
            };
            crate::mir::Instruction::RefCast {
                destination: alias,
                value: receiver,
                reference,
                span,
            }
        } else {
            crate::mir::Instruction::Copy {
                destination: alias,
                value: receiver,
                span,
            }
        };
        target.blocks[0].instructions.insert(0, copy);
        crate::mir::verify_module(&module).expect("receiver aliases retain the contract");
        let crate::mir::Instruction::ClosureNew { captures, .. } =
            &mut module.functions[0].blocks[0].instructions[0]
        else {
            unreachable!()
        };
        captures.swap(0, 1);
        let errors = crate::mir::verify_module(&module).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("captures do not match")),
            "{errors:?}"
        );
    }
}
