//! Dictionary fixtures whose expressions carry explicit Typed Core evidence.

use super::{binder, declaration, push_arrow, push_record, typed};
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
    let mut types = vec![
        thir::Type::Constructor(thir::TypeConstructor::Int),
        thir::Type::Constructor(thir::TypeConstructor::Boolean),
    ];
    let method = push_arrow(&mut types, integer, boolean);
    let eq_dictionary = push_record(&mut types, vec![("isPositive".into(), method)]);
    let ord_dictionary = push_record(
        &mut types,
        vec![
            ("super".into(), eq_dictionary),
            ("rank".into(), method),
            ("compare".into(), method),
        ],
    );
    let make_ord_type = push_arrow(&mut types, eq_dictionary, ord_dictionary);
    let main_type = {
        let id = thir::TypeId(types.len() as u32);
        types.push(thir::Type::Constructor(thir::TypeConstructor::Int));
        id
    };

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
        type_names: Vec::new(),
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
    let mut types = vec![
        thir::Type::Constructor(thir::TypeConstructor::Int),
        thir::Type::Constructor(thir::TypeConstructor::Boolean),
    ];
    let method = push_arrow(&mut types, integer, boolean);
    let eq_dictionary = push_record(&mut types, vec![("isPositive".into(), method)]);
    let ord_dictionary = push_record(
        &mut types,
        vec![("compare".into(), method), ("super".into(), eq_dictionary)],
    );
    let make_ord_type = push_arrow(&mut types, eq_dictionary, ord_dictionary);
    let main_type = {
        let id = thir::TypeId(types.len() as u32);
        types.push(thir::Type::Constructor(thir::TypeConstructor::Int));
        id
    };

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
        type_names: Vec::new(),
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
