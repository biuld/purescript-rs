use super::*;
use crate::cc::{RefShape, Reference, SignatureId};
use psrs_core::{Type, TypeConstructor};
use psrs_hir::{ModuleId, TypeId as HirTypeId, TypeVariableId};
use std::collections::{HashMap, HashSet};

fn fixture() -> Module {
    Module {
        id: ModuleId(0),
        name: "OrdinaryStateCallable".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(HirTypeId::PRIM_STATE)),
            Type::Constructor(TypeConstructor::User(HirTypeId::PRIM_REAL_WORLD)),
            Type::Application(TypeId(0), TypeId(1)),
            Type::Constructor(TypeConstructor::Int),
            Type::RowEmpty,
            Type::RowExtend {
                label: "value".into(),
                ty: TypeId(3),
                tail: TypeId(4),
            },
            Type::RowExtend {
                label: "state".into(),
                ty: TypeId(2),
                tail: TypeId(5),
            },
            Type::Constructor(TypeConstructor::Record),
            Type::Application(TypeId(7), TypeId(6)),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(9), TypeId(2)),
            Type::Application(TypeId(10), TypeId(8)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        type_names: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 1),
    }
}

fn physical(
    projection: &CallProjection<'_>,
    functions: &HashMap<TypeId, SignatureId>,
) -> Signature {
    projection
        .physical_signature(|source, ty| {
            crate::cc::layout::scalar_type(
                source,
                ty,
                source.span,
                &HashSet::new(),
                &HashSet::new(),
                &HashSet::new(),
                &HashMap::new(),
                &HashMap::new(),
                functions,
            )
        })
        .unwrap()
}

#[test]
fn state_call_projects_to_nullary_payload_without_state_or_step_layout() {
    let source = fixture();
    let before = source.types.clone();
    let projection = CallProjection::checked(&source, TypeId(11)).unwrap();
    let signature = physical(&projection, &HashMap::new());
    assert!(signature.parameters.is_empty());
    assert_eq!(signature.result, ValueShape::Integer);
    assert_eq!(projection.state_contract().unwrap().region, TypeId(1));
    assert_eq!(source.types, before);
}

#[test]
fn fixed_state_closure_preserves_ordinary_arguments_and_function_payload() {
    let mut source = fixture();
    source.types.push(Type::Closure {
        parameters: vec![TypeId(3)],
        result: TypeId(3),
    });
    let Type::RowExtend { ty, .. } = &mut source.types[5] else {
        unreachable!()
    };
    *ty = TypeId(12);
    source.types.push(Type::Closure {
        parameters: vec![TypeId(3), TypeId(2)],
        result: TypeId(8),
    });
    let projection = CallProjection::checked(&source, TypeId(13)).unwrap();
    let signature = physical(&projection, &HashMap::from([(TypeId(12), SignatureId(7))]));
    assert_eq!(signature.parameters, [ValueShape::Integer]);
    assert_eq!(
        signature.result,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(SignatureId(7)),
        })
    );
}

#[test]
fn returned_state_action_keeps_its_own_call_boundary() {
    let mut source = fixture();
    source.types.push(Type::Closure {
        parameters: Vec::new(),
        result: TypeId(11),
    });
    let projection = CallProjection::checked(&source, TypeId(12)).unwrap();
    assert!(projection.state_contract().is_none());
    let signature = physical(&projection, &HashMap::from([(TypeId(11), SignatureId(7))]));
    assert!(signature.parameters.is_empty());
    assert_eq!(
        signature.result,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(SignatureId(7)),
        })
    );
}

#[test]
fn region_mismatch_and_nonfinal_state_reject_before_physical_layout() {
    let mut source = fixture();
    source.types.push(Type::Variable(TypeVariableId(0)));
    source.types.push(Type::Application(TypeId(0), TypeId(12)));
    let Type::RowExtend { ty, .. } = &mut source.types[6] else {
        unreachable!()
    };
    *ty = TypeId(13);
    assert!(CallProjection::checked(&source, TypeId(11)).is_err());
    let mut source = fixture();
    source.types.push(Type::Closure {
        parameters: vec![TypeId(2), TypeId(3)],
        result: TypeId(8),
    });
    assert!(CallProjection::checked(&source, TypeId(12)).is_err());
}

#[test]
fn cc_state_signature_retains_dependencies_until_instruction_projection() {
    let source = fixture();
    let signature = super::super::function_signature(
        &source,
        TypeId(11),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::from([(TypeId(8), crate::cc::ReprId(0))]),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(signature.parameters, [ValueShape::State]);
    assert_eq!(
        signature.result,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(crate::cc::ReprId(0)),
        })
    );
}
