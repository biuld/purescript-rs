use super::*;

#[test]
fn resolves_data_constructors_and_user_types() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nmain = Just 1\n";
    let modules = resolve_program_sources(&[("Main.purs", source)]).unwrap();
    assert_eq!(modules[0].types.len(), 1);
    assert_eq!(modules[0].types[0].constructors.len(), 2);
    let just = modules[0].types[0]
        .constructors
        .iter()
        .find(|constructor| constructor.name == "Just")
        .unwrap()
        .symbol;
    let psrs_hir::ExprKind::Application(function, _) = &modules[0].declarations[0].value.kind
    else {
        panic!("expected a constructor application");
    };
    assert_eq!(function.kind, psrs_hir::ExprKind::Global(just));
}

#[test]
fn resolves_a_user_type_in_a_signature() {
    let source = "module Main where\ndata Maybe a = Nothing\nvalue :: Maybe Int\nvalue = Nothing\n";
    let modules = resolve_program_sources(&[("Main.purs", source)]).unwrap();
    let type_id = modules[0].types[0].id;
    let signature = modules[0].declarations[0].signature.as_ref().unwrap();
    let psrs_hir::TypeKind::Application(function, _) = &signature.kind else {
        panic!("expected an applied type constructor");
    };
    assert_eq!(function.kind, psrs_hir::TypeKind::Named(type_id));
}

#[test]
fn reports_decl_conflict_for_duplicate_type_names() {
    let source = "module Main where\ndata Fail\ndata Fail\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DeclConflict"))
    );
}

#[test]
fn reports_decl_conflict_between_a_class_and_a_type() {
    let source = "module Main where\nclass Fail\ndata Fail\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DeclConflict"))
    );
}

#[test]
fn imports_a_type_and_its_constructors_across_modules() {
    let library = "module Library where\ndata Maybe a = Nothing | Just a\n";
    let main = "\
module Main where
import Library (Maybe(..))
value :: Maybe Int
value = Just 1
";
    let modules =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap();
    assert_eq!(modules[1].imports[0].types.len(), 1);
    let type_id = modules[0].types[0].id;
    let signature = modules[1].declarations[0].signature.as_ref().unwrap();
    let psrs_hir::TypeKind::Application(function, _) = &signature.kind else {
        panic!("expected an applied imported type");
    };
    assert_eq!(function.kind, psrs_hir::TypeKind::Named(type_id));
}

#[test]
fn importing_a_type_without_constructors_hides_them() {
    let library = "module Library where\ndata Maybe a = Nothing | Just a\n";
    let main = "module Main where\nimport Library (Maybe)\nvalue = Just 1\n";
    let errors =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("`Just`"))
    );
}

#[test]
fn reports_an_unknown_imported_data_constructor() {
    let library = "module Library where\ndata Maybe a = Nothing | Just a\n";
    let main = "module Main where\nimport Library (Maybe(Nope))\nvalue = 1\n";
    let errors =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownImportDataConstructor"))
    );
}

#[test]
fn reports_a_transitive_export_of_an_unexported_type() {
    let source = "module Main (Y(..)) where\ntype X = Int\ndata Y = Y X\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TransitiveExportError"))
    );
}

#[test]
fn reports_a_partial_constructor_export() {
    let source = "module Main (T(A)) where\ndata T = A | B\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TransitiveDctorExportError"))
    );
}

#[test]
fn reports_an_unknown_exported_data_constructor() {
    let source = "module M1 (X(Y)) where\ndata X = X\ndata Y = Y\n";
    let errors = check_program_lenient(&[("M1.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownExportDataConstructor"))
    );
}

#[test]
fn reports_a_class_member_export_without_its_class() {
    let source = "module Test (bar) where\nclass Foo a where\n  bar :: a -> a\n";
    let errors = check_program_lenient(&[("Test.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TransitiveExportError"))
    );
}

#[test]
fn reexports_a_modules_values() {
    let library = "module A where\nx = 1\n";
    let facade = "module Main (module A) where\nimport A\n";
    let user = "module User where\nimport Main\ny = x\n";
    let modules = resolve_program_sources(&[
        ("A.purs", library),
        ("Main.purs", facade),
        ("User.purs", user),
    ])
    .unwrap();
    assert_eq!(modules[1].name, "Main");
    assert!(
        modules[1]
            .exports
            .as_ref()
            .unwrap()
            .values
            .iter()
            .any(|v| v.name == "x")
    );
}

#[test]
fn reports_export_conflict_between_aliased_reexports() {
    let a = "module A where\nx :: Int\nx = 1\n";
    let b = "module B where\nx :: Int\nx = 2\n";
    let c = "module C (module A, module B) where\nimport A as A\nimport B as B\n";
    let errors =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("C.purs", c)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ExportConflict"))
    );
}

#[test]
fn reports_a_scope_conflict_for_ambiguous_unaliased_reexports() {
    let a = "module A where\nthing = 1\n";
    let b = "module B where\nthing = 2\n";
    let c = "module Main (module A, module B) where\nimport A\nimport B\n";
    let errors =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", c)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ScopeConflict"))
    );
}

#[test]
fn reports_a_scope_conflict_for_a_duplicated_qualifier() {
    let a = "module A where\nthing = 1\n";
    let b = "module B where\nthing = 2\n";
    let c = "module Main (module X) where\nimport A as X\nimport B as X\n";
    let errors =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", c)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ScopeConflict"))
    );
}
