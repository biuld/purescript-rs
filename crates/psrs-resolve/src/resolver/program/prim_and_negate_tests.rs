use super::super::ResolveErrorKind;
use super::{ProgramError, resolve_program};
use psrs_ast as ast;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn name(text: &str) -> ast::Name {
    ast::Name {
        text: text.into(),
        span: TextRange::new(0, text.len() as u32),
    }
}

fn module(
    module_name: &str,
    imports: Vec<ast::Import>,
    exports: Option<ast::ExportList>,
    declarations: Vec<ast::Declaration>,
) -> ast::Module {
    ast::Module {
        name: name(module_name),
        exports,
        imports,
        declarations,
        foreign_imports: Vec::new(),
        type_declarations: Vec::new(),
        role_declarations: Vec::new(),
        fixities: Vec::new(),
        instances: Vec::new(),
        span: TextRange::new(0, 200),
    }
}

fn value(declaration_name: &str, expression: ast::ExprKind) -> ast::Declaration {
    ast::Declaration {
        name: name(declaration_name),
        value: ast::Expr {
            kind: expression,
            span: TextRange::new(0, 10),
        },
        span: TextRange::new(0, 10),
        annotation: None,
    }
}

fn integer(value: &str) -> ast::ExprKind {
    ast::ExprKind::Integer(value.into())
}

fn import(module_name: &str) -> ast::Import {
    ast::Import {
        module: name(module_name),
        alias: None,
        list: None,
        span: TextRange::new(0, 5),
    }
}

fn import_list(module_name: &str, items: Vec<ast::ImportRef>, hiding: bool) -> ast::Import {
    let span = TextRange::new(0, 20);
    ast::Import {
        module: name(module_name),
        alias: None,
        list: Some(ast::ImportList {
            hiding,
            items,
            span,
        }),
        span,
    }
}

fn has_kind(errors: &[ProgramError], kind: ResolveErrorKind) -> bool {
    errors.iter().any(|error| error.error.kind == kind)
}

#[test]
fn qualified_prim_import_resolves_builtin_types_through_the_existing_spine() {
    let mut prim = import("Prim");
    prim.alias = Some(name("P"));
    let mut declaration = value("value", integer("0"));
    declaration.annotation = Some(ast::Type {
        kind: ast::TypeKind::Name(name("P.Number")),
        span: TextRange::new(10, 18),
    });
    let main = module("Main", vec![prim], None, vec![declaration]);

    let resolved = resolve_program(vec![main]).unwrap();
    assert!(matches!(
        resolved[0].declarations[0]
            .signature
            .as_ref()
            .map(|ty| &ty.kind),
        Some(psrs_hir::TypeKind::Constructor(
            psrs_hir::BuiltinType::Number
        ))
    ));
}

#[test]
fn implicit_prim_qualifier_resolves_builtin_types() {
    let mut declaration = value("value", integer("0"));
    declaration.annotation = Some(ast::Type {
        kind: ast::TypeKind::Name(name("Prim.Number")),
        span: TextRange::new(10, 21),
    });
    let main = module("Main", Vec::new(), None, vec![declaration]);

    let resolved = resolve_program(vec![main]).unwrap();
    assert!(matches!(
        resolved[0].declarations[0]
            .signature
            .as_ref()
            .map(|ty| &ty.kind),
        Some(psrs_hir::TypeKind::Constructor(
            psrs_hir::BuiltinType::Number
        ))
    ));
}

#[test]
fn selective_prim_import_hides_unimported_primitive_names() {
    let prim = import_list(
        "Prim",
        vec![ast::ImportRef::Type {
            name: name("Boolean"),
            members: None,
        }],
        false,
    );
    let mut declaration = value("value", integer("0"));
    declaration.annotation = Some(ast::Type {
        kind: ast::TypeKind::Name(name("Number")),
        span: TextRange::new(10, 16),
    });
    let main = module("Main", vec![prim], None, vec![declaration]);

    let errors = resolve_program(vec![main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownTypeName));
}

#[test]
fn aliased_prim_import_hides_implicit_unqualified_primitive_names() {
    let mut prim = import("Prim");
    prim.alias = Some(name("P"));
    let mut declaration = value("value", integer("0"));
    declaration.annotation = Some(ast::Type {
        kind: ast::TypeKind::Name(name("Number")),
        span: TextRange::new(10, 16),
    });
    let main = module("Main", vec![prim], None, vec![declaration]);

    let errors = resolve_program(vec![main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownTypeName));
}

#[test]
fn builtin_type_reexports_keep_the_same_primitive_identity() {
    let mut prim = import("Prim");
    prim.alias = Some(name("P"));
    let facade = module(
        "Facade",
        vec![prim],
        Some(ast::ExportList {
            items: vec![ast::ExportRef::Module(name("P"))],
            span: TextRange::new(0, 20),
        }),
        Vec::new(),
    );
    let mut facade_import = import("Facade");
    facade_import.alias = Some(name("F"));
    let mut declaration = value("value", integer("0"));
    declaration.annotation = Some(ast::Type {
        kind: ast::TypeKind::Name(name("F.Number")),
        span: TextRange::new(10, 18),
    });
    let main = module("Main", vec![facade_import], None, vec![declaration]);

    let resolved = resolve_program(vec![facade, main]).unwrap();
    assert!(matches!(
        resolved[1].declarations[0]
            .signature
            .as_ref()
            .map(|ty| &ty.kind),
        Some(psrs_hir::TypeKind::Constructor(
            psrs_hir::BuiltinType::Number
        ))
    ));
}

#[test]
fn unary_minus_resolves_negate_as_an_ordinary_imported_value() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("negate", integer("0"))],
    );
    let main = module(
        "Main",
        vec![import("Library")],
        None,
        vec![value(
            "value",
            ast::ExprKind::Negate {
                minus_span: TextRange::new(10, 11),
                expression: Box::new(ast::Expr {
                    kind: integer("1"),
                    span: TextRange::new(11, 12),
                }),
            },
        )],
    );

    let resolved = resolve_program(vec![library, main]).unwrap();
    let psrs_hir::ExprKind::Negate { function, .. } = &resolved[1].declarations[0].value.kind
    else {
        panic!("expected a resolved unary minus node");
    };
    assert!(matches!(
        function.kind,
        psrs_hir::ExprKind::Global(SymbolId {
            module: ModuleId(0),
            index: 0,
        })
    ));
}

#[test]
fn unary_minus_uses_a_lexically_scoped_negate_name() {
    let local_negate = ast::Declaration {
        name: name("negate"),
        value: ast::Expr {
            kind: integer("7"),
            span: TextRange::new(15, 16),
        },
        span: TextRange::new(10, 16),
        annotation: None,
    };
    let body = ast::Expr {
        kind: ast::ExprKind::Negate {
            minus_span: TextRange::new(20, 21),
            expression: Box::new(ast::Expr {
                kind: integer("1"),
                span: TextRange::new(21, 22),
            }),
        },
        span: TextRange::new(20, 22),
    };
    let main = module(
        "Main",
        Vec::new(),
        None,
        vec![value(
            "value",
            ast::ExprKind::Let {
                declarations: vec![local_negate],
                body: Box::new(body),
            },
        )],
    );

    let resolved = resolve_program(vec![main]).unwrap();
    let psrs_hir::ExprKind::Let { bindings, body } = &resolved[0].declarations[0].value.kind else {
        panic!("expected the source let expression");
    };
    assert!(matches!(
        body.kind,
        psrs_hir::ExprKind::Negate { ref function, .. }
            if matches!(function.kind, psrs_hir::ExprKind::Local(id) if id == bindings[0].binder.id)
    ));
}

#[test]
fn unary_minus_reports_an_ordinary_unknown_name_when_negate_is_missing() {
    let main = module(
        "Main",
        Vec::new(),
        None,
        vec![value(
            "value",
            ast::ExprKind::Negate {
                minus_span: TextRange::new(10, 11),
                expression: Box::new(ast::Expr {
                    kind: integer("1"),
                    span: TextRange::new(11, 12),
                }),
            },
        )],
    );

    let errors = resolve_program(vec![main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownName));
}

#[test]
fn rejects_source_modules_in_the_reserved_prim_namespace() {
    let reserved = module("Prim.Foobar", Vec::new(), None, Vec::new());

    let errors = resolve_program(vec![reserved]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::CannotDefinePrimModules));
}

#[test]
fn primitive_registry_does_not_invent_a_prim_number_module() {
    let main = module("Main", vec![import("Prim.Number")], None, Vec::new());
    let errors = resolve_program(vec![main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::ModuleNotFound));
}
