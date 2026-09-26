//! Fixtures for dictionary parameter ordering and dictionary sharing.

use super::{binder, declaration, typed};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use psrs_thir as thir;

/// A constrained binding with two dictionary parameters ahead of its ordinary
/// argument: `f :: A -> B -> Int -> Int`. The dictionary order is observable
/// because each dictionary uses a distinct method label.
pub(crate) fn ordered_dictionaries_module() -> (thir::Module, SymbolId) {
    let module_id = ModuleId(0);
    let main = SymbolId::new(module_id, 0);
    let a_dict = SymbolId::new(module_id, 1);
    let b_dict = SymbolId::new(module_id, 2);
    let is_positive = SymbolId::new(module_id, 3);
    let constrained = SymbolId::new(module_id, 4);
    let span = TextRange::new(0, 64);

    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let method = thir::TypeId(2);
    let dict_a = thir::TypeId(3);
    let dict_b = thir::TypeId(4);
    let tail = thir::TypeId(5);
    let tail_after_b = thir::TypeId(6);
    let constrained_type = thir::TypeId(7);
    let main_type = thir::TypeId(8);

    let is_positive_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "value", integer, span),
            body: Box::new(typed(thir::ExprKind::Boolean(true), boolean, span)),
        },
        method,
        span,
    );
    let a_dict_value = typed(
        thir::ExprKind::Record(vec![(
            "a".into(),
            typed(thir::ExprKind::Global(is_positive), method, span),
        )]),
        dict_a,
        span,
    );
    let b_dict_value = typed(
        thir::ExprKind::Record(vec![(
            "b".into(),
            typed(thir::ExprKind::Global(is_positive), method, span),
        )]),
        dict_b,
        span,
    );
    // `\da -> \db -> \x -> if da.a x then x else 0`
    let constrained_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "da", dict_a, span),
            body: Box::new(typed(
                thir::ExprKind::Lambda {
                    binder: binder(1, "db", dict_b, span),
                    body: Box::new(typed(
                        thir::ExprKind::Lambda {
                            binder: binder(2, "x", integer, span),
                            body: Box::new(typed(
                                thir::ExprKind::If {
                                    condition: Box::new(typed(
                                        thir::ExprKind::Application(
                                            Box::new(typed(
                                                thir::ExprKind::FieldAccess {
                                                    expression: Box::new(typed(
                                                        thir::ExprKind::Local(LocalId(0)),
                                                        dict_a,
                                                        span,
                                                    )),
                                                    field: "a".into(),
                                                },
                                                method,
                                                span,
                                            )),
                                            Box::new(typed(
                                                thir::ExprKind::Local(LocalId(2)),
                                                integer,
                                                span,
                                            )),
                                        ),
                                        boolean,
                                        span,
                                    )),
                                    then_branch: Box::new(typed(
                                        thir::ExprKind::Local(LocalId(2)),
                                        integer,
                                        span,
                                    )),
                                    else_branch: Box::new(typed(
                                        thir::ExprKind::Integer(0),
                                        integer,
                                        span,
                                    )),
                                },
                                integer,
                                span,
                            )),
                        },
                        tail,
                        span,
                    )),
                },
                tail_after_b,
                span,
            )),
        },
        constrained_type,
        span,
    );
    // `f aDict bDict 42`
    let main_value = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::Application(
                    Box::new(typed(
                        thir::ExprKind::Application(
                            Box::new(typed(
                                thir::ExprKind::Global(constrained),
                                constrained_type,
                                span,
                            )),
                            Box::new(typed(thir::ExprKind::Global(a_dict), dict_a, span)),
                        ),
                        tail_after_b,
                        span,
                    )),
                    Box::new(typed(thir::ExprKind::Global(b_dict), dict_b, span)),
                ),
                tail,
                span,
            )),
            Box::new(typed(thir::ExprKind::Integer(42), integer, span)),
        ),
        integer,
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
            thir::Type::Record(vec![("a".into(), method)]),
            thir::Type::Record(vec![("b".into(), method)]),
            thir::Type::Function {
                parameter: integer,
                result: integer,
            },
            thir::Type::Function {
                parameter: dict_b,
                result: tail,
            },
            thir::Type::Function {
                parameter: dict_a,
                result: tail_after_b,
            },
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(main, "main", main_type, main_value, span),
            declaration(a_dict, "aDict", dict_a, a_dict_value, span),
            declaration(b_dict, "bDict", dict_b, b_dict_value, span),
            declaration(is_positive, "isPositive", method, is_positive_value, span),
            declaration(
                constrained,
                "constrained",
                constrained_type,
                constrained_value,
                span,
            ),
        ],
        span,
    };
    (module, main)
}

/// Binds one instance dictionary in a `let` and reads two different fields, so
/// dictionary construction must happen once.
pub(crate) fn shared_dictionary_module() -> (thir::Module, SymbolId) {
    let module_id = ModuleId(0);
    let main = SymbolId::new(module_id, 0);
    let make_ord = SymbolId::new(module_id, 1);
    let is_positive = SymbolId::new(module_id, 2);
    let eq_int = SymbolId::new(module_id, 3);
    let eq_class = HirTypeId::new(module_id, 0);
    let ord_class = HirTypeId::new(module_id, 1);
    let span = TextRange::new(0, 64);

    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let method = thir::TypeId(2);
    let eq_dictionary = thir::TypeId(3);
    let ord_dictionary = thir::TypeId(4);
    let make_ord_type = thir::TypeId(5);
    let main_type = thir::TypeId(6);

    let eq_evidence = thir::Evidence {
        kind: thir::EvidenceKind::Global(eq_int),
        class_id: eq_class,
        ty: eq_dictionary,
        span,
    };
    let ord_evidence = thir::Evidence {
        kind: thir::EvidenceKind::Instance {
            constructor: make_ord,
            constructor_type: make_ord_type,
            context: vec![eq_evidence],
        },
        class_id: ord_class,
        ty: ord_dictionary,
        span,
    };
    let compare_call = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(
                        thir::ExprKind::Local(LocalId(0)),
                        ord_dictionary,
                        span,
                    )),
                    field: "compare".into(),
                },
                method,
                span,
            )),
            Box::new(typed(thir::ExprKind::Integer(0), integer, span)),
        ),
        boolean,
        span,
    );
    let rank_call = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(
                        thir::ExprKind::Local(LocalId(0)),
                        ord_dictionary,
                        span,
                    )),
                    field: "rank".into(),
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
        thir::ExprKind::Let {
            bindings: vec![thir::Binding {
                binder: binder(0, "ord", ord_dictionary, span),
                quantified: Vec::new(),
                value: typed(thir::ExprKind::Evidence(ord_evidence), ord_dictionary, span),
                span,
            }],
            body: Box::new(typed(
                thir::ExprKind::If {
                    condition: Box::new(compare_call),
                    then_branch: Box::new(typed(
                        thir::ExprKind::If {
                            condition: Box::new(rank_call),
                            then_branch: Box::new(typed(
                                thir::ExprKind::Integer(42),
                                integer,
                                span,
                            )),
                            else_branch: Box::new(typed(thir::ExprKind::Integer(1), integer, span)),
                        },
                        integer,
                        span,
                    )),
                    else_branch: Box::new(typed(thir::ExprKind::Integer(1), integer, span)),
                },
                integer,
                span,
            )),
        },
        integer,
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
    let eq_int_value = typed(
        thir::ExprKind::Record(vec![(
            "isPositive".into(),
            typed(thir::ExprKind::Global(is_positive), method, span),
        )]),
        eq_dictionary,
        span,
    );
    let make_ord_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "eq", eq_dictionary, span),
            body: Box::new(typed(
                thir::ExprKind::Record(vec![
                    (
                        "compare".into(),
                        typed(thir::ExprKind::Global(is_positive), method, span),
                    ),
                    (
                        "rank".into(),
                        typed(thir::ExprKind::Global(is_positive), method, span),
                    ),
                    (
                        "super".into(),
                        typed(thir::ExprKind::Local(LocalId(0)), eq_dictionary, span),
                    ),
                ]),
                ord_dictionary,
                span,
            )),
        },
        make_ord_type,
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
            thir::Type::Record(vec![("isPositive".into(), method)]),
            thir::Type::Record(vec![
                ("compare".into(), method),
                ("rank".into(), method),
                ("super".into(), eq_dictionary),
            ]),
            thir::Type::Function {
                parameter: eq_dictionary,
                result: ord_dictionary,
            },
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(main, "main", main_type, main_value, span),
            declaration(make_ord, "makeOrd", make_ord_type, make_ord_value, span),
            declaration(is_positive, "isPositive", method, is_positive_value, span),
            declaration(eq_int, "eqInt", eq_dictionary, eq_int_value, span),
        ],
        span,
    };
    (module, main)
}
