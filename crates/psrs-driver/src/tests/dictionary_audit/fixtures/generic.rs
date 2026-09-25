//! Fixtures where a dictionary or its method crosses a generic boundary.

use super::{binder, declaration, typed};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;
use psrs_thir as thir;

/// Builds a class dictionary value reached through an erased polymorphic
/// identity, so the dictionary crosses the erased protocol before its method
/// is selected.
pub(crate) fn erased_dictionary_module() -> (thir::Module, SymbolId) {
    let module_id = ModuleId(0);
    let main = SymbolId::new(module_id, 0);
    let identity = SymbolId::new(module_id, 1);
    let is_positive = SymbolId::new(module_id, 2);
    let eq_int = SymbolId::new(module_id, 3);
    let span = TextRange::new(0, 64);

    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let method = thir::TypeId(2);
    let eq_dictionary = thir::TypeId(3);
    let variable = thir::TypeId(4);
    let identity_type = thir::TypeId(5);
    let main_type = thir::TypeId(6);

    let selected_method = typed(
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
    );
    let condition = typed(
        thir::ExprKind::Application(
            Box::new(selected_method),
            Box::new(typed(thir::ExprKind::Integer(0), integer, span)),
        ),
        boolean,
        span,
    );
    let identity_application = typed(
        thir::ExprKind::Application(
            Box::new(typed(thir::ExprKind::Global(identity), identity_type, span)),
            Box::new(typed(thir::ExprKind::Global(eq_int), eq_dictionary, span)),
        ),
        eq_dictionary,
        span,
    );
    let main_value = typed(
        thir::ExprKind::Let {
            bindings: vec![thir::Binding {
                binder: binder(0, "dict", eq_dictionary, span),
                quantified: Vec::new(),
                value: identity_application,
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
    let identity_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "value", variable, span),
            body: Box::new(typed(thir::ExprKind::Local(LocalId(0)), variable, span)),
        },
        identity_type,
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

    let mut identity_declaration =
        declaration(identity, "identity", identity_type, identity_value, span);
    identity_declaration.quantified = vec![TypeVariableId(0)];
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
            thir::Type::Variable(TypeVariableId(0)),
            thir::Type::Function {
                parameter: variable,
                result: variable,
            },
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(main, "main", main_type, main_value, span),
            identity_declaration,
            declaration(is_positive, "isPositive", method, is_positive_value, span),
            declaration(eq_int, "eqInt", eq_dictionary, eq_int_value, span),
        ],
        span,
    };
    (module, main)
}

/// Builds an instance whose method field is a global function with a generic
/// type (`forall a. a -> a`). Storing and projecting that field must use the
/// erased closure protocol and adapt back at the concrete use.
pub(crate) fn polymorphic_method_module() -> (thir::Module, SymbolId) {
    let module_id = ModuleId(0);
    let main = SymbolId::new(module_id, 0);
    let identity = SymbolId::new(module_id, 1);
    let poly_dict = SymbolId::new(module_id, 2);
    let span = TextRange::new(0, 64);

    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let method = thir::TypeId(2);
    let dictionary = thir::TypeId(3);
    let variable = thir::TypeId(4);
    let main_type = thir::TypeId(5);

    let call = typed(
        thir::ExprKind::Application(
            Box::new(typed(
                thir::ExprKind::FieldAccess {
                    expression: Box::new(typed(
                        thir::ExprKind::Local(LocalId(0)),
                        dictionary,
                        span,
                    )),
                    field: "poly".into(),
                },
                method,
                span,
            )),
            Box::new(typed(thir::ExprKind::Boolean(true), boolean, span)),
        ),
        boolean,
        span,
    );
    let main_value = typed(
        thir::ExprKind::Let {
            bindings: vec![thir::Binding {
                binder: binder(0, "dict", dictionary, span),
                quantified: Vec::new(),
                value: typed(thir::ExprKind::Global(poly_dict), dictionary, span),
                span,
            }],
            body: Box::new(typed(
                thir::ExprKind::If {
                    condition: Box::new(call),
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
    let identity_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "value", variable, span),
            body: Box::new(typed(thir::ExprKind::Local(LocalId(0)), variable, span)),
        },
        method,
        span,
    );
    let poly_dict_value = typed(
        thir::ExprKind::Record(vec![(
            "poly".into(),
            typed(thir::ExprKind::Global(identity), method, span),
        )]),
        dictionary,
        span,
    );

    let mut identity_declaration = declaration(identity, "identity", method, identity_value, span);
    identity_declaration.quantified = vec![TypeVariableId(0)];
    let module = thir::Module {
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            thir::Type::I32,
            thir::Type::Boolean,
            thir::Type::Function {
                parameter: variable,
                result: variable,
            },
            thir::Type::Record(vec![("poly".into(), method)]),
            thir::Type::Variable(TypeVariableId(0)),
            thir::Type::I32,
        ],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(main, "main", main_type, main_value, span),
            identity_declaration,
            declaration(poly_dict, "polyDict", dictionary, poly_dict_value, span),
        ],
        span,
    };
    (module, main)
}
