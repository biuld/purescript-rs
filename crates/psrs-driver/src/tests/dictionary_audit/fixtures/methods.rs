//! Fixtures for default methods and recursive instance contexts.

use super::{binder, declaration, typed};
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
    let method = thir::TypeId(2);
    let dict = thir::TypeId(3);
    let main_type = thir::TypeId(4);

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
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            thir::Type::I32,
            thir::Type::Boolean,
            thir::Type::Function {
                parameter: integer,
                result: boolean,
            },
            thir::Type::Record(vec![
                ("primary".into(), method),
                ("secondary".into(), method),
            ]),
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
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
    let method = thir::TypeId(2);
    let dictionary = thir::TypeId(3);
    let tail = thir::TypeId(4);
    let make_eq_type = thir::TypeId(5);
    let main_type = thir::TypeId(6);
    let sub_type = thir::TypeId(7);
    let le_type = thir::TypeId(8);

    let subtract = |value: thir::Expr, amount: i32| {
        typed(
            thir::ExprKind::Application(
                Box::new(typed(
                    thir::ExprKind::Application(
                        Box::new(typed(thir::ExprKind::Global(int_sub), sub_type, span)),
                        Box::new(value),
                    ),
                    integer,
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
                    thir::TypeId(9),
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
        id: module_id,
        name: "Main".into(),
        externals: vec![
            intrinsic(int_sub, "intSub", Intrinsic::I32Sub),
            intrinsic(int_le, "intLe", Intrinsic::I32LeS),
        ],
        types: vec![
            thir::Type::I32,
            thir::Type::Boolean,
            thir::Type::Function {
                parameter: integer,
                result: boolean,
            },
            thir::Type::Record(vec![("isPositive".into(), method)]),
            thir::Type::Function {
                parameter: dictionary,
                result: dictionary,
            },
            thir::Type::Function {
                parameter: integer,
                result: tail,
            },
            thir::Type::I32,
            thir::Type::Function {
                parameter: integer,
                result: thir::TypeId(9),
            },
            thir::Type::Function {
                parameter: integer,
                result: thir::TypeId(10),
            },
            thir::Type::Function {
                parameter: integer,
                result: integer,
            },
            thir::Type::Function {
                parameter: integer,
                result: boolean,
            },
        ],
        newtype_ids: Vec::new(),
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
