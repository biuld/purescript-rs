use super::*;

mod import_aliases;

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
fn an_exported_value_may_use_an_unexported_synonym_of_an_exported_type() {
    let source = "module Main (step, Box) where\ndata Box = Box\ntype Inner = Box\ntype Step = Inner\nstep :: Step\nstep = Box\n";
    resolve_program_sources(&[("Main.purs", source)])
        .expect("a value signature expands local synonyms before the export check");
}

#[test]
fn an_exported_value_synonym_still_requires_its_hidden_type() {
    let source = "module Main (step) where\ndata Hidden = Hidden\ntype Alias = Hidden\nstep :: Alias\nstep = Hidden\n";
    let errors = resolve_program_sources(&[("Main.purs", source)]).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("Hidden")
    }));
    assert!(
        errors
            .iter()
            .all(|error| !error.diagnostic.message.contains("Alias"))
    );
}

#[test]
fn an_exported_synonym_still_requires_the_synonym_it_mentions() {
    let source = "module Main (Y()) where\ntype X = Int\ntype Y = X\n";
    let errors = resolve_program_sources(&[("Main.purs", source)]).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("`X`")
    }));
}

#[test]
fn a_quantified_synonym_body_keeps_its_hidden_type() {
    let source = "module Main (value) where\ndata Pair a b = Pair a b\ntype Step a = forall r. Pair a r -> r\nvalue :: forall a. Step a\nvalue _ = 0\n";
    let errors = resolve_program_sources(&[("Main.purs", source)]).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("Pair")
    }));
    assert!(
        errors
            .iter()
            .all(|error| !error.diagnostic.message.contains("Step"))
    );
}

#[test]
fn checks_an_inferred_public_result_type_after_typechecking() {
    let source = "module Main (value) where\ndata Hidden = Hidden\nidentity x = x\nvalue = identity Hidden\n";
    let errors = crate::check_program(&[("Main.purs", source)])
        .expect_err("the inferred public result exposes Hidden");
    assert!(errors.iter().any(|error| {
        error.diagnostic.stage == "P5 typecheck"
            && error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("Hidden")
    }));
}

#[test]
fn checks_the_official_required_hidden_type_case_after_inference() {
    let source = "module Foo (B(..), a, b) where\ndata A = A\ndata B = B\na = A\nb = B\n";
    // The case is checked twice: on the bare program path, and on the lenient path
    // that carries the standard library. The two report different diagnostic types,
    // because the library path has to say which diagnostics the caller owns, so
    // the assertion is made over the diagnostic each one wraps.
    let bare = crate::check_program(&[("RequiredHiddenType.purs", source)])
        .expect_err("the official case exports a value whose type is hidden");
    let with_library =
        crate::check_program_types_lenient_with_prelude(&[("RequiredHiddenType.purs", source)])
            .expect_err("the official case exports a value whose type is hidden");
    for diagnostics in [
        bare.iter()
            .map(|error| &error.diagnostic)
            .collect::<Vec<_>>(),
        with_library
            .iter()
            .map(|error| &error.diagnostic)
            .collect::<Vec<_>>(),
    ] {
        assert!(
            diagnostics.iter().any(|error| {
                error.stage == "P5 typecheck"
                    && error.code == Some("TransitiveExportError")
                    && error.message.contains('A')
            }),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn checks_an_inferred_function_parameter_hidden_in_a_case() {
    let source = "module Main (value) where\ndata Hidden = Hidden\nvalue = \\x -> case x of\n  Hidden -> x\n";
    let errors = crate::check_program(&[("Main.purs", source)])
        .expect_err("the inferred function parameter exposes Hidden");
    assert!(errors.iter().any(|error| {
        error.diagnostic.stage == "P5 typecheck"
            && error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("Hidden")
    }));
}

#[test]
fn checks_hidden_types_reached_through_an_inferred_record_field() {
    let source = "module Main (value) where\ndata Hidden = Hidden\nvalue = \\record -> case record.secret of\n  Hidden -> record.secret\n";
    let errors = crate::check_program(&[("Main.purs", source)])
        .expect_err("the inferred record and result types expose Hidden");
    assert!(errors.iter().any(|error| {
        error.diagnostic.stage == "P5 typecheck"
            && error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("Hidden")
    }));
}

#[test]
fn a_hidden_constructor_does_not_require_its_field_type() {
    let source = "module Main (Wrap) where\ndata Hidden = Hidden\nnewtype Wrap = Wrap Hidden\n";
    resolve_program_sources(&[("Main.purs", source)])
        .expect("a hidden constructor does not expose its field type");
}

#[test]
fn an_exported_constructor_requires_its_field_type() {
    let source = "module Main (T(A)) where\ndata Shown = Shown\ndata Hidden = Hidden\ndata T = A Shown | B Hidden\n";
    let errors = resolve_program_sources(&[("Main.purs", source)]).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.diagnostic.code == Some("TransitiveExportError")
            && error.diagnostic.message.contains("Shown")
    }));
    assert!(
        errors
            .iter()
            .all(|error| !error.diagnostic.message.contains("Hidden"))
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
