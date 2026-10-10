use super::*;
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

fn fixture(unrelated: bool, sibling: bool) -> Module {
    let span = TextRange::new(0, 1);
    let variable = TypeVariableId(0);
    let used_variable = TypeVariableId(u32::from(unrelated));
    let mut types = vec![
        Type::Variable(variable),
        Type::Variable(used_variable),
        Type::Constructor(TypeConstructor::Function),
        Type::Constructor(TypeConstructor::Int),
    ];
    let mut arrow = |argument| {
        let head = TypeId(types.len() as u32);
        types.push(Type::Application(TypeId(2), argument));
        let result = TypeId(types.len() as u32);
        types.push(Type::Application(head, argument));
        result
    };
    let value_type = arrow(TypeId(1));
    let field_body = arrow(TypeId(0));
    let int_function = arrow(TypeId(3));
    let field = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable],
        body: field_body,
    });
    let mut row = TypeId(types.len() as u32);
    types.push(Type::RowEmpty);
    if sibling {
        let tail = row;
        row = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: "sibling".into(),
            ty: int_function,
            tail,
        });
    }
    let tail = row;
    row = TypeId(types.len() as u32);
    types.push(Type::RowExtend {
        label: "method".into(),
        ty: field,
        tail,
    });
    let record_head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    let record = TypeId(types.len() as u32);
    types.push(Type::Application(record_head, row));
    let declaration = |index, name: &str, quantified, ty, value| Declaration {
        symbol: SymbolId::new(ModuleId(0), index),
        name: name.into(),
        name_span: span,
        quantified,
        ty,
        value,
        span,
    };
    let identity = declaration(
        0,
        "identity",
        vec![used_variable],
        value_type,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: TypeId(1),
                    span,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: TypeId(1),
                    span,
                }),
            },
            ty: value_type,
            span,
        },
    );
    let value = || Expr {
        kind: ExprKind::Global(identity.symbol),
        ty: value_type,
        span,
    };
    let mut fields = vec![("method".into(), value())];
    if sibling {
        fields.push(("sibling".into(), value()));
    }
    let dictionary = declaration(
        1,
        "dictionary",
        vec![],
        record,
        Expr {
            kind: ExprKind::Record { fields },
            ty: record,
            span,
        },
    );
    Module {
        id: ModuleId(0),
        name: "RecordScope".into(),
        types,
        type_names: vec![],
        externals: vec![],
        external_types: vec![],
        newtype_ids: vec![],
        opaque_ids: vec![],
        callable_types: vec![],
        constructors: vec![],
        declarations: vec![identity, dictionary],
        entry: None,
        span,
    }
}

#[test]
fn quantified_record_field_scopes_its_instantiated_method() {
    fixture(false, false).verify().unwrap();
}

#[test]
fn record_field_quantifier_does_not_bind_an_unrelated_variable() {
    assert_scope_error(fixture(true, false));
}

#[test]
fn record_field_quantifier_does_not_leak_to_sibling_fields() {
    assert_scope_error(fixture(false, true));
}

fn projection(unrelated: bool, polymorphic: bool) -> Module {
    let mut module = fixture(unrelated, false);
    let field = module
        .record_field(module.declarations[1].ty, "method")
        .unwrap();
    let Type::ForAll { body, .. } = module.types[field.0 as usize] else {
        panic!("quantified method expected");
    };
    for ty in &mut module.types {
        if let Type::RowExtend { label, ty, .. } = ty
            && label == "method"
        {
            *ty = body;
        }
    }
    let selected = if polymorphic { field } else { body };
    let declaration = &mut module.declarations[1];
    let record = declaration.value.clone();
    declaration.ty = selected;
    declaration.value = Expr {
        kind: ExprKind::FieldAccess {
            record: Box::new(record),
            field: "method".into(),
        },
        ty: selected,
        span: declaration.span,
    };
    module
}

#[test]
fn quantified_projection_scopes_the_record_instantiation() {
    projection(false, true).verify().unwrap();
}

#[test]
fn projection_quantifier_does_not_bind_an_unrelated_variable() {
    assert_scope_error(projection(true, true));
}

#[test]
fn a_monomorphic_projection_does_not_introduce_type_variables() {
    assert_scope_error(projection(false, false));
}

fn array(unrelated: bool, empty: bool) -> Module {
    let mut module = fixture(unrelated, false);
    let dictionary = &module.declarations[1];
    let field = module.record_field(dictionary.ty, "method").unwrap();
    let Type::ForAll { body, .. } = module.types[field.0 as usize] else {
        panic!("quantified method expected");
    };
    let head = TypeId(module.types.len() as u32);
    module.types.push(Type::Constructor(TypeConstructor::Array));
    let applied = TypeId(module.types.len() as u32);
    module.types.push(Type::Application(head, body));
    let quantified = TypeId(module.types.len() as u32);
    module.types.push(Type::ForAll {
        variables: vec![TypeVariableId(0)],
        body: applied,
    });
    let elements = if empty {
        vec![]
    } else {
        vec![Expr {
            kind: ExprKind::Global(module.declarations[0].symbol),
            ty: module.declarations[0].ty,
            span: module.span,
        }]
    };
    let declaration = &mut module.declarations[1];
    declaration.ty = quantified;
    declaration.value = Expr {
        kind: ExprKind::Array { elements },
        ty: quantified,
        span: module.span,
    };
    module
}

#[test]
fn quantified_array_values_scope_their_elements() {
    array(false, false).verify().unwrap();
    array(false, true).verify().unwrap();
}

#[test]
fn an_array_quantifier_does_not_bind_an_unrelated_variable() {
    assert_scope_error(array(true, false));
}

fn assert_scope_error(module: Module) {
    let errors = module.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message == "type variable is outside its quantifier scope"),
        "{errors:?}"
    );
}
