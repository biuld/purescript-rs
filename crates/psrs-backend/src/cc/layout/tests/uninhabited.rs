//! A constructorless user `data` declaration is uninhabited. Its values come
//! only from unchecked coercions, so the backend lays it out as an erased
//! reference, like any other abstract type. Foreign data stays a resource
//! handle and is excluded.
use super::{declaration_shape, empty_module};
use crate::cc::{RefShape, Reference, ValueShape};
use psrs_core::{ConstructorInfo, Declaration, Expr, ExprKind, Type, TypeConstructor, TypeId};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use std::collections::{HashMap, HashSet};

fn declaration(module: &mut psrs_core::Module, ty: TypeId) -> Declaration {
    let declaration = Declaration {
        symbol: SymbolId::new(module.id, 0),
        name: "value".to_owned(),
        name_span: psrs_span::TextRange::new(0, 1),
        quantified: Vec::new(),
        ty,
        value: Expr {
            kind: ExprKind::Unit,
            ty,
            span: psrs_span::TextRange::new(0, 1),
        },
        span: psrs_span::TextRange::new(0, 1),
    };
    module.declarations.push(declaration.clone());
    declaration
}

fn shape(module: &psrs_core::Module, declaration: &Declaration) -> ValueShape {
    declaration_shape(
        declaration,
        module,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    )
    .expect("an uninhabited type should have a layout")
    .result
}

fn erased() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

#[test]
fn a_bare_constructorless_data_type_uses_the_erased_reference() {
    let empty = HirTypeId::new(ModuleId(1), 0);
    let mut module = empty_module(vec![Type::Constructor(TypeConstructor::User(empty))]);
    module.ordinary_data_ids.push(empty);
    let declaration = declaration(&mut module, TypeId(0));
    assert_eq!(shape(&module, &declaration), erased());
}

#[test]
fn an_applied_constructorless_data_type_uses_the_erased_reference() {
    let empty = HirTypeId::new(ModuleId(1), 0);
    let mut module = empty_module(vec![
        Type::Constructor(TypeConstructor::User(empty)),
        Type::Constructor(TypeConstructor::Int),
        Type::Application(TypeId(0), TypeId(1)),
    ]);
    module.ordinary_data_ids.push(empty);
    let declaration = declaration(&mut module, TypeId(2));
    assert_eq!(shape(&module, &declaration), erased());
}

#[test]
fn a_user_type_with_a_constructor_is_not_treated_as_uninhabited() {
    // The same nominal type becomes an enum as soon as it has a nullary
    // constructor, so the erased fallback must not swallow it.
    let inhabited = HirTypeId::new(ModuleId(1), 0);
    let mut module = empty_module(vec![Type::Constructor(TypeConstructor::User(inhabited))]);
    module.ordinary_data_ids.push(inhabited);
    module.constructors.push(ConstructorInfo {
        symbol: SymbolId::new(module.id, 1),
        name: "Only".to_owned(),
        type_id: inhabited,
        tag: 0,
        field_count: 0,
        field_types: Vec::new(),
        parameters: Vec::new(),
    });
    assert!(!super::is_uninhabited_data_type(&module, inhabited));
}
