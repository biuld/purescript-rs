mod adt;
mod nested;

use super::*;
use psrs_core::{
    Binder, Binding, Declaration, Expr, ExprKind, Module, Type, TypeConstructor, TypeId,
};
use psrs_hir::{LocalId, TypeVariableId};

#[test]
fn generic_parameter_adapts_at_boolean_and_integer_uses() {
    let (types, int, bool_ty, variable, identity_type, int_function, boolean_function, use_type) =
        identity_types();
    let module_id = ModuleId(0);
    let identity_symbol = SymbolId::new(module_id, 0);
    let use_symbol = SymbolId::new(module_id, 1);
    let main_symbol = SymbolId::new(module_id, 2);
    let function_local = LocalId(0);
    let identity_local = LocalId(1);

    let identity = declaration(
        identity_symbol,
        "identity",
        identity_type,
        lambda(
            "value",
            identity_local,
            variable,
            local(identity_local, variable),
            identity_type,
        ),
    );
    let boolean_call = app(
        local(function_local, boolean_function),
        boolean(true, bool_ty),
        bool_ty,
    );
    let integer_call = app(local(function_local, int_function), integer(42, int), int);
    let use_body = expr(
        ExprKind::If {
            condition: Box::new(boolean_call),
            then_branch: Box::new(integer_call),
            else_branch: Box::new(integer(1, int)),
        },
        int,
    );
    let use_value = lambda(
        "function",
        function_local,
        identity_type,
        use_body,
        use_type,
    );
    let use_declaration = declaration(use_symbol, "use", use_type, use_value);
    let main_value = app(
        global(use_symbol, use_type),
        global(identity_symbol, identity_type),
        int,
    );
    let main = declaration(main_symbol, "main", int, main_value);

    execute_core_module(
        module(types, vec![identity, use_declaration, main], main_symbol),
        "a rank-2 parameter must adapt independently at Boolean and Int uses",
        42,
    );
}

#[test]
fn escaping_closure_captures_the_generic_function_before_each_instantiation() {
    let (mut types, int, bool_ty, variable, identity_type, int_function, boolean_function, _) =
        identity_types();
    let bool_to_int = push_arrow(&mut types, bool_ty, int);
    let use_type = push_arrow(&mut types, identity_type, bool_to_int);
    let module_id = ModuleId(0);
    let identity_symbol = SymbolId::new(module_id, 0);
    let use_symbol = SymbolId::new(module_id, 1);
    let main_symbol = SymbolId::new(module_id, 2);
    let identity_local = LocalId(0);
    let function_local = LocalId(1);
    let flag_local = LocalId(2);
    let callback_local = LocalId(3);
    let callback_argument = LocalId(4);

    let identity = identity_declaration(identity_symbol, identity_type, variable, identity_local);
    let callback_boolean_call = app(
        local(function_local, boolean_function),
        local(callback_argument, bool_ty),
        bool_ty,
    );
    let callback_integer_call = app(local(function_local, int_function), integer(42, int), int);
    let callback_body = expr(
        ExprKind::If {
            condition: Box::new(callback_boolean_call),
            then_branch: Box::new(callback_integer_call),
            else_branch: Box::new(integer(1, int)),
        },
        int,
    );
    let callback = lambda(
        "argument",
        callback_argument,
        bool_ty,
        callback_body,
        bool_to_int,
    );
    let callback_binding = Binding {
        binder: Binder {
            id: callback_local,
            name: "callback".into(),
            ty: bool_to_int,
            span: span(),
        },
        quantified: Vec::new(),
        value: callback,
        span: span(),
    };
    let callback_use = app(
        local(callback_local, bool_to_int),
        local(flag_local, bool_ty),
        int,
    );
    let body = expr(
        ExprKind::Let {
            bindings: vec![callback_binding],
            body: Box::new(callback_use),
        },
        int,
    );
    let use_value = lambda(
        "function",
        function_local,
        identity_type,
        lambda("flag", flag_local, bool_ty, body, bool_to_int),
        use_type,
    );
    let use_declaration = declaration(use_symbol, "capture", use_type, use_value);
    let main_value = app(
        global(use_symbol, use_type),
        global(identity_symbol, identity_type),
        bool_to_int,
    );
    let main_value = app(main_value, boolean(true, bool_ty), int);
    let main = declaration(main_symbol, "main", int, main_value);

    execute_core_module(
        module(types, vec![identity, use_declaration, main], main_symbol),
        "a lifted closure must retain a generic callable capture across both instances",
        42,
    );
}

#[test]
fn record_field_preserves_a_polymorphic_closure_for_multiple_uses() {
    let int = TypeId(0);
    let bool_ty = TypeId(1);
    let variable = TypeId(2);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
    ];
    let identity_body = push_arrow(&mut types, variable, variable);
    let identity_type = push_forall(&mut types, TypeVariableId(0), identity_body);
    let int_function = push_arrow(&mut types, int, int);
    let boolean_function = push_arrow(&mut types, bool_ty, bool_ty);
    let row = TypeId(types.len() as u32);
    types.push(Type::RowEmpty);
    let field_row = TypeId(types.len() as u32);
    types.push(Type::RowExtend {
        label: "run".into(),
        ty: identity_type,
        tail: row,
    });
    let record_constructor = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    let record_type = TypeId(types.len() as u32);
    types.push(Type::Application(record_constructor, field_row));
    let module_id = ModuleId(0);
    let box_symbol = SymbolId::new(module_id, 0);
    let main_symbol = SymbolId::new(module_id, 1);
    let identity_local = LocalId(0);

    let boxed_record = declaration(
        box_symbol,
        "box",
        record_type,
        expr(
            ExprKind::Record {
                fields: vec![(
                    "run".into(),
                    lambda(
                        "value",
                        identity_local,
                        variable,
                        local(identity_local, variable),
                        identity_type,
                    ),
                )],
            },
            record_type,
        ),
    );
    let boolean_call = app(
        expr(
            ExprKind::FieldAccess {
                record: Box::new(global(box_symbol, record_type)),
                field: "run".into(),
            },
            boolean_function,
        ),
        boolean(true, bool_ty),
        bool_ty,
    );
    let integer_call = app(
        expr(
            ExprKind::FieldAccess {
                record: Box::new(global(box_symbol, record_type)),
                field: "run".into(),
            },
            int_function,
        ),
        integer(42, int),
        int,
    );
    let main_value = expr(
        ExprKind::If {
            condition: Box::new(boolean_call),
            then_branch: Box::new(integer_call),
            else_branch: Box::new(integer(1, int)),
        },
        int,
    );
    let main = declaration(main_symbol, "main", int, main_value);

    execute_core_module(
        module(types, vec![boxed_record, main], main_symbol),
        "a record field must retain its quantified closure signature",
        42,
    );
}

#[test]
fn returned_polymorphic_closure_keeps_its_own_call_arity() {
    let int = TypeId(0);
    let bool_ty = TypeId(1);
    let variable = TypeId(2);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
    ];
    let identity_body = push_arrow(&mut types, variable, variable);
    let identity_type = push_forall(&mut types, TypeVariableId(0), identity_body);
    let producer_type = push_arrow(&mut types, int, identity_type);
    let module_id = ModuleId(0);
    let producer_symbol = SymbolId::new(module_id, 0);
    let main_symbol = SymbolId::new(module_id, 1);
    let ignored = LocalId(0);
    let value = LocalId(1);

    let returned_identity = lambda(
        "value",
        value,
        variable,
        local(value, variable),
        identity_type,
    );
    let producer = declaration(
        producer_symbol,
        "makeIdentity",
        producer_type,
        lambda("ignored", ignored, int, returned_identity, producer_type),
    );
    let make_boolean_identity = app(
        global(producer_symbol, producer_type),
        integer(0, int),
        identity_type,
    );
    let make_integer_identity = app(
        global(producer_symbol, producer_type),
        integer(0, int),
        identity_type,
    );
    let boolean_call = app(make_boolean_identity, boolean(true, bool_ty), bool_ty);
    let integer_call = app(make_integer_identity, integer(42, int), int);
    let main_value = expr(
        ExprKind::If {
            condition: Box::new(boolean_call),
            then_branch: Box::new(integer_call),
            else_branch: Box::new(integer(1, int)),
        },
        int,
    );
    let main = declaration(main_symbol, "main", int, main_value);

    let backend = crate::cc::lower_module(module(types, vec![producer, main], main_symbol))
        .expect("a function returning a polymorphic value should lower to CC");
    let producer_cc = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name == "makeIdentity")
        .expect("producer CC function");
    assert_eq!(
        producer_cc.parameters.len(),
        1,
        "the returned forall owns a separate closure arity"
    );
    assert!(matches!(
        producer_cc.result_type,
        ValueShape::Reference(crate::cc::Reference {
            heap: crate::cc::RefShape::Closure(_),
            ..
        })
    ));
    let (mir, _) = crate::mir::lower_module(backend.cc)
        .expect("rank-N CC should lower through the GC representation");
    run_gc(&mir, 42);
}

fn identity_types() -> (
    Vec<Type>,
    TypeId,
    TypeId,
    TypeId,
    TypeId,
    TypeId,
    TypeId,
    TypeId,
) {
    let int = TypeId(0);
    let boolean = TypeId(1);
    let variable = TypeId(2);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
    ];
    let identity_body = push_arrow(&mut types, variable, variable);
    let identity_type = push_forall(&mut types, TypeVariableId(0), identity_body);
    let int_function = push_arrow(&mut types, int, int);
    let boolean_function = push_arrow(&mut types, boolean, boolean);
    let use_type = push_arrow(&mut types, identity_type, int);
    (
        types,
        int,
        boolean,
        variable,
        identity_type,
        int_function,
        boolean_function,
        use_type,
    )
}

fn identity_declaration(
    symbol: SymbolId,
    identity_type: TypeId,
    variable: TypeId,
    local_id: LocalId,
) -> Declaration {
    declaration(
        symbol,
        "identity",
        identity_type,
        lambda(
            "value",
            local_id,
            variable,
            local(local_id, variable),
            identity_type,
        ),
    )
}

pub(super) fn execute_core_module(module: Module, context: &str, expected: i32) {
    let backend = crate::cc::lower_module(module)
        .unwrap_or_else(|errors| panic!("{context}: Core-to-CC lowering failed: {errors:?}"));
    let (mir, _) = crate::mir::lower_module(backend.cc)
        .unwrap_or_else(|errors| panic!("{context}: CC-to-MIR lowering failed: {errors:?}"));
    run_gc(&mir, expected);
}

pub(super) fn module(types: Vec<Type>, declarations: Vec<Declaration>, entry: SymbolId) -> Module {
    Module {
        id: entry.module,
        name: "RankNBackendTest".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations,
        type_names: Vec::new(),
        entry: Some(entry),
        span: span(),
    }
}

pub(super) fn declaration(symbol: SymbolId, name: &str, ty: TypeId, value: Expr) -> Declaration {
    Declaration {
        symbol,
        name: name.into(),
        name_span: span(),
        quantified: Vec::new(),
        ty,
        value,
        span: span(),
    }
}

pub(super) fn push_arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let partial = TypeId(types.len() as u32);
    types.push(Type::Application(head, parameter));
    let arrow = TypeId(types.len() as u32);
    types.push(Type::Application(partial, result));
    arrow
}

pub(super) fn push_forall(types: &mut Vec<Type>, variable: TypeVariableId, body: TypeId) -> TypeId {
    let quantified = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable],
        body,
    });
    quantified
}

pub(super) fn lambda(
    name: &str,
    id: LocalId,
    ty: TypeId,
    body: Expr,
    function_type: TypeId,
) -> Expr {
    expr(
        ExprKind::Lambda {
            binder: Binder {
                id,
                name: name.into(),
                ty,
                span: span(),
            },
            body: Box::new(body),
        },
        function_type,
    )
}

pub(super) fn local(id: LocalId, ty: TypeId) -> Expr {
    expr(ExprKind::Local(id), ty)
}

pub(super) fn global(symbol: SymbolId, ty: TypeId) -> Expr {
    expr(ExprKind::Global(symbol), ty)
}

pub(super) fn integer(value: i32, ty: TypeId) -> Expr {
    expr(ExprKind::Integer(value), ty)
}

pub(super) fn boolean(value: bool, ty: TypeId) -> Expr {
    expr(ExprKind::Boolean(value), ty)
}

pub(super) fn app(function: Expr, argument: Expr, ty: TypeId) -> Expr {
    expr(
        ExprKind::Application(Box::new(function), Box::new(argument)),
        ty,
    )
}

pub(super) fn expr(kind: ExprKind, ty: TypeId) -> Expr {
    Expr {
        kind,
        ty,
        span: span(),
    }
}
