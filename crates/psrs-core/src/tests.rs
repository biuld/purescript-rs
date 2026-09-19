use super::*;
use psrs_hir::{ModuleId, SymbolId};

#[test]
fn verifier_rejects_out_of_range_types() {
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
            ty: TypeId(1),
            value: Expr {
                kind: ExprKind::Integer(0),
                ty: TypeId(0),
                span: TextRange::new(7, 8),
            },
            span: TextRange::new(0, 8),
        }],
        entry: None,
        span: TextRange::new(0, 8),
    };
    assert_eq!(
        module.verify().unwrap_err()[0].message,
        "type reference is outside the Core type table"
    );
}

#[test]
fn verifier_attributes_declaration_errors_to_their_source_module() {
    let owner = ModuleId(7);
    let module = Module {
        id: ModuleId(0),
        name: "Linked".into(),
        externals: Vec::new(),
        types: vec![Type::I32],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(owner, 0),
            name: "broken".into(),
            name_span: TextRange::new(0, 6),
            quantified: Vec::new(),
            ty: TypeId(0),
            value: Expr {
                kind: ExprKind::Global(SymbolId::new(owner, 99)),
                ty: TypeId(0),
                span: TextRange::new(9, 15),
            },
            span: TextRange::new(0, 15),
        }],
        entry: None,
        span: TextRange::new(0, 15),
    };
    let error = module.verify().unwrap_err().remove(0);
    assert_eq!(error.module, owner);
    assert_eq!(error.message, "global reference is not declared");
}
