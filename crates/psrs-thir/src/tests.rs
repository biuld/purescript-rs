use super::*;

#[test]
fn verifier_rejects_invalid_type_references() {
    let module = Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![Type::I32],
        newtype_ids: Vec::new(),
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
    let dictionary = TypeId(1);
    let module = Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            Type::I32,
            Type::Record(Vec::new()),
            Type::Function {
                parameter: TypeId(0),
                result: dictionary,
            },
        ],
        newtype_ids: Vec::new(),
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
                        constructor_type: TypeId(2),
                        context: vec![Evidence {
                            kind: EvidenceKind::Given(LocalId(0)),
                            class_id: psrs_hir::TypeId::new(ModuleId(0), 0),
                            ty: TypeId(1),
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
    let dictionary = TypeId(1);
    let module = Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            Type::I32,
            Type::Record(vec![("super".into(), TypeId(0))]),
            Type::Boolean,
        ],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            quantified: Vec::new(),
            ty: TypeId(2),
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
                    ty: TypeId(2),
                    span: TextRange::new(8, 18),
                }),
                ty: TypeId(2),
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
