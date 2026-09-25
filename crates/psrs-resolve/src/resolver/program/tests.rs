use super::super::ResolveErrorKind;
use super::{ProgramError, resolve_program};
use psrs_ast as ast;
use psrs_hir::{ExprKind, ModuleId, SymbolId};
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

fn reference(text: &str) -> ast::ExprKind {
    ast::ExprKind::Name(name(text))
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

fn import_value(text: &str) -> ast::ImportRef {
    ast::ImportRef::Value(name(text))
}

fn export_values(names: &[&str]) -> ast::ExportList {
    ast::ExportList {
        items: names
            .iter()
            .map(|text| ast::ExportRef::Value(name(text)))
            .collect(),
        span: TextRange::new(0, 30),
    }
}

fn has_kind(errors: &[ProgramError], kind: ResolveErrorKind) -> bool {
    errors.iter().any(|error| error.error.kind == kind)
}

#[test]
fn resolves_an_unqualified_imported_value() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("answer", integer("42"))],
    );
    let main = module(
        "Main",
        vec![import("Library")],
        None,
        vec![value("main", reference("answer"))],
    );

    let resolved = resolve_program(vec![library, main]).unwrap();
    assert_eq!(
        resolved[1].declarations[0].value.kind,
        ExprKind::Global(SymbolId::new(ModuleId(0), 0))
    );
    for module in &resolved {
        module.verify().unwrap();
    }
}

#[test]
fn resolves_a_qualified_imported_value() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("answer", integer("42"))],
    );
    let main = module(
        "Main",
        vec![import("Library")],
        None,
        vec![value("main", reference("Library.answer"))],
    );

    let resolved = resolve_program(vec![library, main]).unwrap();
    assert_eq!(
        resolved[1].declarations[0].value.kind,
        ExprKind::Global(SymbolId::new(ModuleId(0), 0))
    );
}

#[test]
fn resolves_a_value_through_an_import_alias() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("answer", integer("42"))],
    );
    let mut alias = import("Library");
    alias.alias = Some(name("L"));
    let main = module(
        "Main",
        vec![alias],
        None,
        vec![value("main", reference("L.answer"))],
    );

    let resolved = resolve_program(vec![library, main]).unwrap();
    assert_eq!(
        resolved[1].declarations[0].value.kind,
        ExprKind::Global(SymbolId::new(ModuleId(0), 0))
    );
}

#[test]
fn explicit_import_list_limits_the_names_in_scope() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("foo", integer("1")), value("bar", integer("2"))],
    );
    let main = module(
        "Main",
        vec![import_list("Library", vec![import_value("foo")], false)],
        None,
        vec![value("main", reference("foo"))],
    );

    let resolved = resolve_program(vec![library, main]).unwrap();
    assert_eq!(
        resolved[1].declarations[0].value.kind,
        ExprKind::Global(SymbolId::new(ModuleId(0), 0))
    );

    let missing = module(
        "Other",
        vec![import_list("Library", vec![import_value("foo")], false)],
        None,
        vec![value("main", reference("bar"))],
    );
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("foo", integer("1")), value("bar", integer("2"))],
    );
    let errors = resolve_program(vec![library, missing]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownName));
}

#[test]
fn hiding_list_removes_named_values() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("foo", integer("1")), value("bar", integer("2"))],
    );
    let main = module(
        "Main",
        vec![import_list("Library", vec![import_value("bar")], true)],
        None,
        vec![value("main", reference("foo"))],
    );

    let resolved = resolve_program(vec![library, main]).unwrap();
    assert_eq!(
        resolved[1].declarations[0].value.kind,
        ExprKind::Global(SymbolId::new(ModuleId(0), 0))
    );
}

#[test]
fn explicit_export_list_restricts_the_interface() {
    let library = module(
        "Library",
        Vec::new(),
        Some(export_values(&["foo"])),
        vec![value("foo", integer("1")), value("bar", integer("2"))],
    );
    let main = module(
        "Main",
        vec![import("Library")],
        None,
        vec![value("main", reference("bar"))],
    );

    let errors = resolve_program(vec![library, main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownName));
}

#[test]
fn reports_an_unknown_export() {
    let library = module(
        "Library",
        Vec::new(),
        Some(export_values(&["foo", "missing"])),
        vec![value("foo", integer("1"))],
    );

    let errors = resolve_program(vec![library]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownExport));
}

#[test]
fn reports_an_unknown_import() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("foo", integer("1"))],
    );
    let main = module(
        "Main",
        vec![import_list("Library", vec![import_value("bar")], false)],
        None,
        vec![value("main", integer("1"))],
    );

    let errors = resolve_program(vec![library, main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::UnknownImport));
}

#[test]
fn reports_a_missing_module() {
    let main = module(
        "Main",
        vec![import("Missing")],
        None,
        vec![value("main", integer("1"))],
    );
    let errors = resolve_program(vec![main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::ModuleNotFound));
}

#[test]
fn reports_a_duplicate_module() {
    let first = module("Main", Vec::new(), None, Vec::new());
    let second = module("Main", Vec::new(), None, Vec::new());
    let errors = resolve_program(vec![first, second]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::DuplicateModule));
}

#[test]
fn reports_a_module_cycle() {
    let main = module(
        "Main",
        vec![import("Main")],
        None,
        vec![value("main", integer("1"))],
    );
    let errors = resolve_program(vec![main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::CycleInModules));
}

#[test]
fn reports_a_scope_conflict_between_imports() {
    let first = module("A", Vec::new(), None, vec![value("thing", integer("1"))]);
    let second = module("B", Vec::new(), None, vec![value("thing", integer("2"))]);
    let main = module(
        "Main",
        vec![import("A"), import("B")],
        None,
        vec![value("main", reference("thing"))],
    );

    let errors = resolve_program(vec![first, second, main]).unwrap_err();
    assert!(has_kind(&errors, ResolveErrorKind::ScopeConflict));
}

#[test]
fn a_module_can_reexport_an_imported_value() {
    let library = module(
        "Library",
        Vec::new(),
        None,
        vec![value("answer", integer("42"))],
    );
    let facade = module(
        "Facade",
        vec![import("Library")],
        Some(export_values(&["answer"])),
        Vec::new(),
    );
    let main = module(
        "Main",
        vec![import("Facade")],
        None,
        vec![value("main", reference("answer"))],
    );

    let resolved = resolve_program(vec![library, facade, main]).unwrap();
    assert_eq!(
        resolved[2].declarations[0].value.kind,
        ExprKind::Global(SymbolId::new(ModuleId(0), 0))
    );
    resolved[1].verify().unwrap();
}
