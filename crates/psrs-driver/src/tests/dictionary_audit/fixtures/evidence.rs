//! Dictionary fixtures whose expressions carry explicit Typed Core evidence.

use super::{binder, declaration, typed};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use psrs_thir as thir;

/// Builds a contextual `Ord` instance from a `Global` `Eq` dictionary and
/// selects a method through the superclass field (nested projection).
pub(crate) fn dictionary_module() -> (thir::Module, SymbolId) {
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
    let superclass_method = typed(
        thir::ExprKind::FieldAccess {
            expression: Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(
                        thir::ExprKind::Local(LocalId(0)),
                        ord_dictionary,
                        span,
                    )),
                    field: "super".into(),
                },
                eq_dictionary,
                span,
            )),
            field: "isPositive".into(),
        },
        method,
        span,
    );
    let condition = typed(
        thir::ExprKind::Application(
            Box::new(superclass_method),
            Box::new(typed(thir::ExprKind::Integer(0), integer, span)),
        ),
        boolean,
        span,
    );
    let body = typed(
        thir::ExprKind::If {
            condition: Box::new(condition),
            then_branch: Box::new(typed(thir::ExprKind::Integer(42), integer, span)),
            else_branch: Box::new(typed(thir::ExprKind::Integer(1), integer, span)),
        },
        integer,
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
            body: Box::new(body),
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
            binder: binder(1, "eq", eq_dictionary, span),
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
                        typed(thir::ExprKind::Local(LocalId(1)), eq_dictionary, span),
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
            // The declared field order deliberately differs from the canonical
            // label order the product layout uses, so field addressing must be
            // by label rather than by declared position.
            thir::Type::Record(vec![
                ("super".into(), eq_dictionary),
                ("rank".into(), method),
                ("compare".into(), method),
            ]),
            thir::Type::Function {
                parameter: eq_dictionary,
                result: ord_dictionary,
            },
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
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

/// Builds an instance whose method field is a closure that captures the
/// instance context dictionary (`compare = \v -> eq.isPositive v`). Selecting
/// and calling that method exercises an escaping method closure.
pub(crate) fn escaping_method_module() -> (thir::Module, SymbolId) {
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
    let condition = typed(
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
                    condition: Box::new(condition),
                    then_branch: Box::new(typed(thir::ExprKind::Integer(42), integer, span)),
                    else_branch: Box::new(typed(thir::ExprKind::Integer(1), integer, span)),
                },
                integer,
                span,
            )),
        },
        integer,
        span,
    );
    // `compare` captures the context dictionary bound as `LocalId(0)`.
    let compare_closure = typed(
        thir::ExprKind::Lambda {
            binder: binder(1, "value", integer, span),
            body: Box::new(typed(
                thir::ExprKind::Application(
                    Box::new(typed(
                        thir::ExprKind::FieldAccess {
                            expression: Box::new(typed(
                                thir::ExprKind::Local(LocalId(0)),
                                eq_dictionary,
                                span,
                            )),
                            field: "isPositive".into(),
                        },
                        method,
                        span,
                    )),
                    Box::new(typed(thir::ExprKind::Local(LocalId(1)), integer, span)),
                ),
                boolean,
                span,
            )),
        },
        method,
        span,
    );
    let make_ord_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "eq", eq_dictionary, span),
            body: Box::new(typed(
                thir::ExprKind::Record(vec![
                    ("compare".into(), compare_closure),
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
                ("super".into(), eq_dictionary),
            ]),
            thir::Type::Function {
                parameter: eq_dictionary,
                result: ord_dictionary,
            },
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
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
