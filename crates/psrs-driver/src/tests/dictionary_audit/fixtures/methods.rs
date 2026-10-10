//! Fixtures for default methods and recursive instance contexts.

use super::{binder, declaration, push_arrow, push_record, typed};
use psrs_hir::{ExternalKind, ExternalSymbol, Intrinsic, LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;
use psrs_thir as thir;

/// An instance that stores a class default in one method field and a provided
/// implementation in another.
pub(crate) fn default_method_module() -> (thir::Module, SymbolId) {
    let module_id = ModuleId(0);
    let main = SymbolId::new(module_id, 0);
    let provided = SymbolId::new(module_id, 1);
    let default = SymbolId::new(module_id, 2);
    let dictionary = SymbolId::new(module_id, 3);
    let span = TextRange::new(0, 64);

    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let mut types = vec![
        thir::Type::Constructor(thir::TypeConstructor::Int),
        thir::Type::Constructor(thir::TypeConstructor::Boolean),
    ];
    let method = push_arrow(&mut types, integer, boolean);
    let dict = push_record(
        &mut types,
        vec![("primary".into(), method), ("secondary".into(), method)],
    );
    let main_type = {
        let id = thir::TypeId(types.len() as u32);
        types.push(thir::Type::Constructor(thir::TypeConstructor::Int));
        id
    };

    let condition = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(thir::ExprKind::Global(dictionary), dict, span)),
                    field: "secondary".into(),
                },
                method,
                span,
            )),
            Box::new(typed(thir::ExprKind::Integer(0), integer, span)),
        ),
        boolean,
        span,
    );
    let main_value = typed(
        thir::ExprKind::If {
            condition: Box::new(condition),
            then_branch: Box::new(typed(thir::ExprKind::Integer(42), integer, span)),
            else_branch: Box::new(typed(thir::ExprKind::Integer(1), integer, span)),
        },
        integer,
        span,
    );
    let method_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "value", integer, span),
            body: Box::new(typed(thir::ExprKind::Boolean(true), boolean, span)),
        },
        method,
        span,
    );
    let dictionary_value = typed(
        thir::ExprKind::Record(vec![
            (
                "primary".into(),
                typed(thir::ExprKind::Global(provided), method, span),
            ),
            (
                "secondary".into(),
                typed(thir::ExprKind::Global(default), method, span),
            ),
        ]),
        dict,
        span,
    );

    let module = thir::Module {
        type_names: Vec::new(),
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(main, "main", main_type, main_value, span),
            declaration(provided, "provided", method, method_value.clone(), span),
            declaration(default, "default", method, method_value, span),
            declaration(dictionary, "defaultDict", dict, dictionary_value, span),
        ],
        span,
    };
    (module, main)
}

/// A recursive instance constructor: `makeEq fuel ctx` builds a dictionary
/// whose method recurses through the constructor until the fuel reaches zero
/// and then delegates to the captured context dictionary.
pub(crate) fn recursive_instance_module() -> (thir::Module, SymbolId) {
    let module_id = ModuleId(0);
    let main = SymbolId::new(module_id, 0);
    let make_eq = SymbolId::new(module_id, 1);
    let base = SymbolId::new(module_id, 2);
    let is_positive = SymbolId::new(module_id, 3);
    let int_sub = SymbolId::new(module_id, 4);
    let int_le = SymbolId::new(module_id, 5);
    let span = TextRange::new(0, 64);

    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let mut types = vec![
        thir::Type::Constructor(thir::TypeConstructor::Int),
        thir::Type::Constructor(thir::TypeConstructor::Boolean),
    ];
    let method = push_arrow(&mut types, integer, boolean);
    let dictionary = push_record(&mut types, vec![("isPositive".into(), method)]);
    let tail = push_arrow(&mut types, dictionary, dictionary);
    let make_eq_type = push_arrow(&mut types, integer, tail);
    let main_type = {
        let id = thir::TypeId(types.len() as u32);
        types.push(thir::Type::Constructor(thir::TypeConstructor::Int));
        id
    };
    let sub_partial = push_arrow(&mut types, integer, integer);
    let sub_type = push_arrow(&mut types, integer, sub_partial);
    let le_partial = push_arrow(&mut types, integer, boolean);
    let le_type = push_arrow(&mut types, integer, le_partial);

    let subtract = |value: thir::Expr, amount: i32| {
        typed(
            thir::ExprKind::Application(
                Box::new(typed(
                    thir::ExprKind::Application(
                        Box::new(typed(thir::ExprKind::Global(int_sub), sub_type, span)),
                        Box::new(value),
                    ),
                    sub_partial,
                    span,
                )),
                Box::new(typed(thir::ExprKind::Integer(amount), integer, span)),
            ),
            integer,
            span,
        )
    };
    let at_or_below_zero = |value: thir::Expr| {
        typed(
            thir::ExprKind::Application(
                Box::new(typed(
                    thir::ExprKind::Application(
                        Box::new(typed(thir::ExprKind::Global(int_le), le_type, span)),
                        Box::new(value),
                    ),
                    le_partial,
                    span,
                )),
                Box::new(typed(thir::ExprKind::Integer(0), integer, span)),
            ),
            boolean,
            span,
        )
    };

    // `makeEq = \fuel -> \ctx -> { isPositive = \n -> if fuel <= 0 then ctx.isPositive n
    //   else (makeEq (fuel - 1) ctx).isPositive n }`
    let recurse = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::Application(
                    Box::new(typed(thir::ExprKind::Global(make_eq), make_eq_type, span)),
                    Box::new(subtract(
                        typed(thir::ExprKind::Local(LocalId(0)), integer, span),
                        1,
                    )),
                ),
                tail,
                span,
            )),
            Box::new(typed(thir::ExprKind::Local(LocalId(1)), dictionary, span)),
        ),
        dictionary,
        span,
    );
    let delegate = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(
                        thir::ExprKind::Local(LocalId(1)),
                        dictionary,
                        span,
                    )),
                    field: "isPositive".into(),
                },
                method,
                span,
            )),
            Box::new(typed(thir::ExprKind::Local(LocalId(2)), integer, span)),
        ),
        boolean,
        span,
    );
    let recursive_call = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(recurse),
                    field: "isPositive".into(),
                },
                method,
                span,
            )),
            Box::new(typed(thir::ExprKind::Local(LocalId(2)), integer, span)),
        ),
        boolean,
        span,
    );
    let method_closure = typed(
        thir::ExprKind::Lambda {
            binder: binder(2, "value", integer, span),
            body: Box::new(typed(
                thir::ExprKind::If {
                    condition: Box::new(at_or_below_zero(typed(
                        thir::ExprKind::Local(LocalId(0)),
                        integer,
                        span,
                    ))),
                    then_branch: Box::new(delegate),
                    else_branch: Box::new(recursive_call),
                },
                boolean,
                span,
            )),
        },
        method,
        span,
    );
    let make_eq_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "fuel", integer, span),
            body: Box::new(typed(
                thir::ExprKind::Lambda {
                    binder: binder(1, "ctx", dictionary, span),
                    body: Box::new(typed(
                        thir::ExprKind::Record(vec![("isPositive".into(), method_closure)]),
                        dictionary,
                        span,
                    )),
                },
                tail,
                span,
            )),
        },
        make_eq_type,
        span,
    );
    let is_positive_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "value", integer, span),
            body: Box::new(typed(thir::ExprKind::Boolean(true), boolean, span)),
        },
        method,
        span,
    );
    let base_value = typed(
        thir::ExprKind::Record(vec![(
            "isPositive".into(),
            typed(thir::ExprKind::Global(is_positive), method, span),
        )]),
        dictionary,
        span,
    );
    let condition = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(
                        thir::ExprKind::Application(
                            Box::new(typed(
                                thir::ExprKind::Application(
                                    Box::new(typed(
                                        thir::ExprKind::Global(make_eq),
                                        make_eq_type,
                                        span,
                                    )),
                                    Box::new(typed(thir::ExprKind::Integer(3), integer, span)),
                                ),
                                tail,
                                span,
                            )),
                            Box::new(typed(thir::ExprKind::Global(base), dictionary, span)),
                        ),
                        dictionary,
                        span,
                    )),
                    field: "isPositive".into(),
                },
                method,
                span,
            )),
            Box::new(typed(thir::ExprKind::Integer(0), integer, span)),
        ),
        boolean,
        span,
    );
    let main_value = typed(
        thir::ExprKind::If {
            condition: Box::new(condition),
            then_branch: Box::new(typed(thir::ExprKind::Integer(42), integer, span)),
            else_branch: Box::new(typed(thir::ExprKind::Integer(1), integer, span)),
        },
        integer,
        span,
    );

    let intrinsic = |symbol, name: &str, intrinsic| ExternalSymbol {
        symbol,
        name: name.into(),
        kind: ExternalKind::Intrinsic(intrinsic),
        signature: None,
    };
    let module = thir::Module {
        type_names: Vec::new(),
        id: module_id,
        name: "Main".into(),
        externals: vec![
            intrinsic(int_sub, "intSub", Intrinsic::IntSub),
            intrinsic(int_le, "intLe", Intrinsic::IntLe),
        ],
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(main, "main", main_type, main_value, span),
            declaration(make_eq, "makeEq", make_eq_type, make_eq_value, span),
            declaration(base, "base", dictionary, base_value, span),
            declaration(is_positive, "isPositive", method, is_positive_value, span),
        ],
        span,
    };
    (module, main)
}
