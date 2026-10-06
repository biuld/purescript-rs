use super::{declaration_shape, empty_module};
use crate::cc::{RefShape, Reference, ValueShape};
use psrs_core::{Declaration, Expr, ExprKind, Type, TypeConstructor, TypeId};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use std::collections::{HashMap, HashSet};

#[test]
fn an_applied_opaque_foreign_type_uses_the_erased_reference() {
    let opaque = HirTypeId::new(ModuleId(1), 0);
    let mut module = empty_module(vec![
        Type::Constructor(TypeConstructor::User(opaque)),
        Type::Constructor(TypeConstructor::Int),
        Type::Application(TypeId(0), TypeId(1)),
    ]);
    module.opaque_ids.push(opaque);
    let applied = TypeId(2);
    module.declarations.push(Declaration {
        symbol: SymbolId::new(module.id, 0),
        name: "value".to_owned(),
        name_span: psrs_span::TextRange::new(0, 1),
        quantified: Vec::new(),
        ty: applied,
        value: Expr {
            kind: ExprKind::Unit,
            ty: applied,
            span: psrs_span::TextRange::new(0, 1),
        },
        span: psrs_span::TextRange::new(0, 1),
    });
    let signature = declaration_shape(
        &module.declarations[0],
        &module,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    )
    .expect("an applied opaque type should have a layout");
    assert!(signature.parameters.is_empty());
    assert_eq!(signature.result, erased());
}

fn erased() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}
