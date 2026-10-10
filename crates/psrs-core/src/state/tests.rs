use super::*;
use psrs_hir::{ModuleId, TypeVariableId};
use psrs_span::TextRange;

pub(super) fn fixture() -> Module {
    Module {
        id: ModuleId(0),
        name: "UnrelatedStateLibrary".into(),
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
        span: TextRange::new(0, 0),
    }
}

#[test]
fn registered_state_identity_needs_no_effect_or_library_name() {
    let module = fixture();
    let signature = signature(&module, TypeId(11)).unwrap();
    assert_eq!(signature.region, TypeId(1));
    assert_eq!(signature.payload, TypeId(3));
    assert!(signature.parameters.is_empty());
}

#[test]
fn fixed_arity_state_closure_preserves_its_payload_boundary() {
    let mut module = fixture();
    module.types.push(Type::Application(TypeId(9), TypeId(3)));
    module.types.push(Type::Application(TypeId(12), TypeId(3)));
    let Type::RowExtend { ty, .. } = &mut module.types[5] else {
        unreachable!()
    };
    *ty = TypeId(13);
    module.types.push(Type::Closure {
        parameters: vec![TypeId(3), TypeId(2)],
        result: TypeId(8),
    });
    let checked = signature(&module, TypeId(14)).unwrap();
    assert_eq!(checked.parameters, [TypeId(3)]);
    assert_eq!(checked.state, TypeId(2));
    assert_eq!(checked.payload, TypeId(13));
}

#[test]
fn function_payload_is_not_an_extra_action_parameter() {
    let mut module = fixture();
    module.types.push(Type::Application(TypeId(9), TypeId(3)));
    module.types.push(Type::Application(TypeId(12), TypeId(3)));
    let Type::RowExtend { ty, .. } = &mut module.types[5] else {
        unreachable!()
    };
    *ty = TypeId(13);
    let signature = signature(&module, TypeId(11)).unwrap();
    assert_eq!(signature.payload, TypeId(13));
    assert!(signature.parameters.is_empty());
}

#[test]
fn step_requires_a_closed_complete_structural_contract() {
    let mut module = fixture();
    module.types[4] = Type::Variable(TypeVariableId(7));
    assert!(step(&module, TypeId(8)).is_none());
    module.types[4] = Type::RowExtend {
        label: "extra".into(),
        ty: TypeId(3),
        tail: TypeId(4),
    };
    assert!(
        step(&module, TypeId(8)).is_none(),
        "cyclic rows must terminate with rejection"
    );
    module.types[4] = Type::RowEmpty;
    let Type::RowExtend { label, .. } = &mut module.types[5] else {
        unreachable!()
    };
    *label = "state".into();
    assert!(
        step(&module, TypeId(8)).is_none(),
        "duplicate labels cannot supply the payload"
    );
}

#[test]
fn state_result_must_preserve_the_nominal_region() {
    let mut module = fixture();
    module.types.push(Type::Variable(TypeVariableId(8)));
    module.types.push(Type::Application(TypeId(0), TypeId(12)));
    let Type::RowExtend { ty, .. } = &mut module.types[6] else {
        unreachable!()
    };
    *ty = TypeId(13);
    assert_eq!(
        signature(&module, TypeId(11)).unwrap_err(),
        "state function returns a different region"
    );
}

#[test]
fn an_unrelated_opaque_constructor_is_not_state() {
    let mut module = fixture();
    module.types[0] = Type::Constructor(TypeConstructor::User(HirTypeId::new(ModuleId(3), 0)));
    assert!(signature(&module, TypeId(11)).is_err());
}

#[test]
fn a_world_runner_checks_both_the_payload_and_the_region() {
    let mut module = fixture();
    primitive::verify(
        &module,
        psrs_hir::Intrinsic::RunWorld,
        &[TypeId(11)],
        TypeId(3),
    )
    .unwrap();
    assert!(
        primitive::verify(
            &module,
            psrs_hir::Intrinsic::RunWorld,
            &[TypeId(11)],
            TypeId(2)
        )
        .is_err()
    );
    module.types[1] = Type::Variable(TypeVariableId(8));
    assert_eq!(
        primitive::verify(
            &module,
            psrs_hir::Intrinsic::RunWorld,
            &[TypeId(11)],
            TypeId(3)
        )
        .unwrap_err(),
        "world runner requires the RealWorld region"
    );
}

#[test]
fn a_region_runner_requires_rank_n_and_rejects_escape() {
    let mut module = fixture();
    module.types[1] = Type::Variable(TypeVariableId(8));
    assert!(
        primitive::verify(
            &module,
            psrs_hir::Intrinsic::RunRegion,
            &[TypeId(11)],
            TypeId(3)
        )
        .is_err()
    );
    module.types.push(Type::ForAll {
        variables: vec![TypeVariableId(8)],
        body: TypeId(11),
    });
    primitive::verify(
        &module,
        psrs_hir::Intrinsic::RunRegion,
        &[TypeId(12)],
        TypeId(3),
    )
    .unwrap();
    let Type::RowExtend { ty, .. } = &mut module.types[5] else {
        unreachable!()
    };
    *ty = TypeId(2);
    assert_eq!(
        primitive::verify(
            &module,
            psrs_hir::Intrinsic::RunRegion,
            &[TypeId(12)],
            TypeId(2)
        )
        .unwrap_err(),
        "region runner result escapes its region"
    );
}

#[test]
fn core_verification_checks_the_world_runner_contract() {
    use crate::{Binder, Declaration, Expr, ExprKind};
    use psrs_hir::{Intrinsic, LocalId, SymbolId};
    let mut module = fixture();
    let span = TextRange::new(0, 10);
    let action = Expr {
        ty: TypeId(11),
        span,
        kind: ExprKind::Lambda {
            binder: Binder {
                id: LocalId(0),
                name: "state".into(),
                ty: TypeId(2),
                span,
            },
            body: Box::new(Expr {
                ty: TypeId(8),
                span,
                kind: ExprKind::Record {
                    fields: vec![
                        (
                            "state".into(),
                            Expr {
                                kind: ExprKind::Local(LocalId(0)),
                                ty: TypeId(2),
                                span,
                            },
                        ),
                        (
                            "value".into(),
                            Expr {
                                kind: ExprKind::Integer(42),
                                ty: TypeId(3),
                                span,
                            },
                        ),
                    ],
                },
            }),
        },
    };
    module.declarations.push(Declaration {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "main".into(),
        name_span: span,
        quantified: Vec::new(),
        ty: TypeId(3),
        span,
        value: crate::primitive::primitive_value(
            Intrinsic::RunWorld,
            vec![action],
            TypeId(3),
            span,
        ),
    });
    module.verify().unwrap();
    module.types[1] = Type::Constructor(TypeConstructor::Int);
    let errors = module.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message == "world runner requires the RealWorld region"),
        "{errors:?}"
    );
}
