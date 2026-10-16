use super::*;

#[test]
fn a_runtime_dictionary_cannot_authorize_a_coercion() {
    let span = TextRange::new(0, 1);
    let dictionary = Evidence {
        kind: EvidenceKind::DictionaryValue(Box::new(Expr {
            kind: ExprKind::Record(Vec::new()),
            ty: TypeId(3),
            span,
        })),
        class_id: psrs_hir::TypeId::COERCIBLE,
        ty: TypeId(3),
        span,
    };
    let module = Module {
        id: ModuleId(0),
        name: "Main".into(),
        type_names: Vec::new(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::RowEmpty,
            Type::Constructor(TypeConstructor::Record),
            Type::Application(TypeId(2), TypeId(1)),
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
            ty: TypeId(0),
            span,
            value: Expr {
                kind: ExprKind::Coerce {
                    value: Box::new(Expr {
                        kind: ExprKind::Integer(42),
                        ty: TypeId(0),
                        span,
                    }),
                    evidence: dictionary,
                    source_type: TypeId(0),
                    target_type: TypeId(0),
                },
                ty: TypeId(0),
                span,
            },
        }],
        span,
    };
    let errors = module
        .verify()
        .expect_err("dictionary terms are not proof boundaries");
    assert!(errors.iter().any(|error| error.message
        == "coercion expression requires an explicit Coercible proof boundary"));
}
