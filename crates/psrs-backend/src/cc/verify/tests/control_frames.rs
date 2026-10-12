//! Machine object identities must survive variant storage and MIR lowering.

use super::super::verify_function;
use super::symbol;
use crate::cc::layout::control;
use crate::cc::payload::erased_shape;
use crate::cc::{
    Assignment, AssignmentKind, Function, Module, RefShape, Reference, ReprId, Representation,
    RepresentationTable, Signature, ValueDecl, ValueId, ValueShape,
};
use psrs_span::TextRange;
use std::collections::HashMap;

fn shape(id: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    })
}

fn constructor(table: &RepresentationTable, representation: ReprId, case: u32) -> Function {
    let Some(Representation::Variant { cases }) = table.representation(representation) else {
        panic!("expected a machine variant");
    };
    let fields = &cases
        .iter()
        .find(|candidate| candidate.tag == case)
        .unwrap()
        .fields;
    let mut values = fields
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
        ty: shape(representation),
    });
    let span = TextRange::new(0, 1);
    Function {
        symbol: symbol(0),
        name: "machine_frame".into(),
        parameters: parameters.clone(),
        values,
        assignments: vec![Assignment {
            destination: result,
            kind: AssignmentKind::VariantNew {
                destination: result,
                representation,
                case,
                fields: parameters,
            },
            span,
        }],
        result,
        result_type: shape(representation),
        span,
    }
}

#[test]
fn control_frames_reject_erased_or_wrong_machine_fields_in_cc() {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let nominal_types = control::reserve(&mut table);
    let layout = control::append(&mut table, continuation, &nominal_types);
    for (representation, case, field, wrong) in [
        (layout.computation, 3, 1, shape(layout.boundary)),
        (layout.computation, 4, 2, shape(layout.sequence)),
        (layout.sequence, 2, 0, shape(layout.context)),
        (layout.sequence, 3, 0, shape(layout.sequence)),
        (layout.context, 1, 4, shape(layout.boundary)),
        (layout.owner, 0, 1, shape(layout.callback)),
        (layout.boundary, 1, 1, shape(layout.owner)),
    ] {
        let valid = constructor(&table, representation, case);
        verify_function(&valid, &HashMap::new(), &table).expect("valid machine fields");
        for wrong in [wrong, erased_shape()] {
            let mut invalid = valid.clone();
            invalid.values[field].ty = wrong;
            let errors = verify_function(&invalid, &HashMap::new(), &table).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("variant fields")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn control_frame_projection_cannot_silently_erase_checkpoint_identity() {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let nominal_types = control::reserve(&mut table);
    let layout = control::append(&mut table, continuation, &nominal_types);
    let mut function = constructor(&table, layout.sequence, 3);
    let result = ValueId(function.values.len() as u32);
    function.values.push(ValueDecl {
        id: result,
        ty: erased_shape(),
    });
    function.assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::VariantGet {
            destination: result,
            representation: layout.sequence,
            case: 3,
            field: 0,
            value: function.result,
        },
        span: function.span,
    });
    function.result = result;
    function.result_type = erased_shape();
    let errors = verify_function(&function, &HashMap::new(), &table).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("variant field has")),
        "{errors:?}"
    );
}

#[test]
fn mir_preserves_control_frame_fields_and_rejects_cross_wired_storage() {
    let mut table = RepresentationTable::default();
    let continuation = table.add_signature(Signature {
        parameters: vec![erased_shape()],
        result: erased_shape(),
    });
    let nominal_types = control::reserve(&mut table);
    let layout = control::append(&mut table, continuation, &nominal_types);
    for (representation, case, left, right) in [
        (layout.computation, 3, 1, 2),
        (layout.sequence, 3, 0, 1),
        (layout.context, 1, 2, 3),
        (layout.owner, 0, 0, 1),
        (layout.boundary, 1, 0, 1),
    ] {
        let function = constructor(&table, representation, case);
        let module = Module {
            name: "ControlFrames".into(),
            functions: vec![function.clone()],
            externals: vec![],
            representations: table.clone(),
            entry: None,
            span: function.span,
        };
        let (mut mir, _) = crate::mir::lower_module(module).expect("valid typed machine frame");
        crate::mir::verify_module(&mir).expect("valid frame in MIR");
        let instruction = mir.functions[0].blocks.iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction, crate::mir::Instruction::StructNew { arguments, .. } if arguments.len() == function.parameters.len() + 1))
            .expect("frame constructor");
        let crate::mir::Instruction::StructNew { arguments, .. } = instruction else {
            unreachable!()
        };
        // Slot zero is the variant tag; mutate only its stored payload fields.
        arguments.swap(left + 1, right + 1);
        let errors =
            crate::mir::verify_module(&mir).expect_err("MIR must reject cross-wired frame storage");
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("struct.new argument has the wrong type")),
            "{errors:?}"
        );
    }
}
