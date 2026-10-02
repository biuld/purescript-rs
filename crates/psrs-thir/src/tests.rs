use super::*;

#[test]
fn verifier_rejects_invalid_type_references() {
    let module = Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![Type::Constructor(TypeConstructor::Int)],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            quantified: Vec::new(),
            ty: TypeId(2),
            value: Expr {
                kind: ExprKind::Integer(1),
                ty: TypeId(0),
                span: TextRange::new(7, 8),
            },
            span: TextRange::new(0, 8),
        }],
        span: TextRange::new(0, 8),
    };

    let errors = module.verify().unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].message,
        "type reference is outside the THIR type table"
    );
}

#[test]
fn verifier_checks_instance_context_against_constructor_parameters() {
    let dictionary = TypeId(3);
    let module = Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Record),
            Type::RowEmpty,
            Type::Application(TypeId(1), TypeId(2)),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(4), TypeId(0)),
            Type::Application(TypeId(5), TypeId(3)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            quantified: Vec::new(),
            ty: dictionary,
            value: Expr {
                kind: ExprKind::Evidence(Evidence {
                    kind: EvidenceKind::Instance {
                        constructor: SymbolId::new(ModuleId(0), 1),
                        constructor_type: TypeId(6),
                        context: vec![Evidence {
                            kind: EvidenceKind::Given(LocalId(0)),
                            class_id: psrs_hir::TypeId::new(ModuleId(0), 0),
                            ty: TypeId(3),
                            span: TextRange::new(8, 9),
                        }],
                    },
                    class_id: psrs_hir::TypeId::new(ModuleId(0), 0),
                    ty: dictionary,
                    span: TextRange::new(8, 18),
                }),
                ty: dictionary,
                span: TextRange::new(8, 18),
            },
            span: TextRange::new(0, 18),
        }],
        span: TextRange::new(0, 18),
    };

    let errors = module.verify().unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message == "instance evidence does not match its context parameter"
    }));
}

#[test]
fn verifier_requires_superclass_evidence_to_name_a_well_typed_field() {
    let dictionary = TypeId(4);
    let module = Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::RowEmpty,
            Type::RowExtend {
                label: "super".into(),
                ty: TypeId(0),
                tail: TypeId(1),
            },
            Type::Constructor(TypeConstructor::Record),
            Type::Application(TypeId(3), TypeId(2)),
            Type::Constructor(TypeConstructor::Boolean),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            quantified: Vec::new(),
            ty: TypeId(5),
            value: Expr {
                kind: ExprKind::Evidence(Evidence {
                    kind: EvidenceKind::Superclass {
                        parent: Box::new(Evidence {
                            kind: EvidenceKind::Given(LocalId(0)),
                            class_id: psrs_hir::TypeId::new(ModuleId(0), 1),
                            ty: dictionary,
                            span: TextRange::new(8, 9),
                        }),
                        field: "super".into(),
                    },
                    class_id: psrs_hir::TypeId::new(ModuleId(0), 0),
                    ty: TypeId(5),
                    span: TextRange::new(8, 18),
                }),
                ty: TypeId(5),
                span: TextRange::new(8, 18),
            },
            span: TextRange::new(0, 18),
        }],
        span: TextRange::new(0, 18),
    };

    let errors = module.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message == "superclass evidence field has the wrong type")
    );
}

/// A module carrying `Proxy 1` and `Proxy 2` and one global reference at each.
/// A literal is decided, so the verifier compares the two by value instead of
/// treating both as applications of the same nominal head.
fn literal_reference_module(reference_type: TypeId) -> Module {
    let span = TextRange::new(0, 16);
    let proxy = psrs_hir::TypeId::new(ModuleId(0), 0);
    let reference = SymbolId::new(ModuleId(0), 0);
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(proxy)),
            Type::TypeLevelString("a".into()),
            Type::TypeLevelInt(1),
            Type::TypeLevelInt(2),
            Type::Application(TypeId(0), TypeId(2)),
            Type::Application(TypeId(0), TypeId(3)),
            Type::Application(TypeId(0), TypeId(1)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            // `pick :: Proxy 1`, whose value is another reference at the same
            // literal type, so the module needs no value-level constructor.
            Declaration {
                symbol: reference,
                name: "pick".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: TypeId(4),
                value: Expr {
                    kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
                    ty: TypeId(4),
                    span,
                },
                span,
            },
            Declaration {
                symbol: SymbolId::new(ModuleId(0), 1),
                name: "main".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: TypeId(4),
                value: Expr {
                    kind: ExprKind::Global(reference),
                    ty: TypeId(4),
                    span,
                },
                span,
            },
            Declaration {
                symbol: SymbolId::new(ModuleId(0), 2),
                name: "other".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: reference_type,
                value: Expr {
                    kind: ExprKind::Global(reference),
                    ty: TypeId(4),
                    span,
                },
                span,
            },
        ],
        span,
    }
}

#[test]
fn verifier_compares_type_level_literals_by_value() {
    let errors = literal_reference_module(TypeId(5)).verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message == "THIR expression type is inconsistent with its context"),
        "{errors:?}"
    );
}

#[test]
fn verifier_accepts_a_reference_at_its_own_type_level_literal() {
    literal_reference_module(TypeId(4)).verify().unwrap();
}

#[test]
fn verifier_rejects_coercion_evidence_for_a_different_boundary() {
    let span = TextRange::new(0, 12);
    let integer = TypeId(0);
    let boolean = TypeId(1);
    let empty_dictionary = TypeId(4);
    let mut module = Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Boolean),
            Type::RowEmpty,
            Type::Constructor(TypeConstructor::Record),
            Type::Application(TypeId(3), TypeId(2)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: span,
            quantified: Vec::new(),
            ty: boolean,
            value: Expr {
                kind: ExprKind::Coerce {
                    value: Box::new(Expr {
                        kind: ExprKind::Integer(7),
                        ty: integer,
                        span,
                    }),
                    evidence: Evidence {
                        kind: EvidenceKind::Coercible {
                            source_type: integer,
                            target_type: integer,
                        },
                        class_id: psrs_hir::TypeId::COERCIBLE,
                        ty: empty_dictionary,
                        span,
                    },
                    source_type: integer,
                    target_type: boolean,
                },
                ty: boolean,
                span,
            },
            span,
        }],
        span,
    };

    let errors = module.verify().unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message == "coercion evidence types do not match the cast boundary"
    }));

    if let ExprKind::Coerce { evidence, .. } = &mut module.declarations[0].value.kind {
        evidence.ty = integer;
    }
    let errors = module.verify().unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message == "coercible evidence must have the empty class dictionary type"
    }));

    if let ExprKind::Coerce { evidence, .. } = &mut module.declarations[0].value.kind {
        evidence.kind = EvidenceKind::Given(LocalId(1));
    }
    let errors = module.verify().unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message == "coercion expression requires an explicit Coercible proof boundary"
    }));
}
