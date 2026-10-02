use super::*;

#[test]
fn resolves_a_program_that_imports_a_value_across_modules() {
    let library = "module Library where\nanswer = 42\n";
    let main = "module Main where\nimport Library\nmain = answer\n";
    let modules =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap();
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[1].name, "Main");
    assert_eq!(modules[1].imports.len(), 1);
    assert_eq!(modules[1].imports[0].symbols.len(), 1);
    assert_eq!(modules[1].imports[0].symbols[0].local_name, "answer");
}

#[test]
fn reports_a_missing_imported_module() {
    let main = "module Main where\nimport Missing\nmain = 1\n";
    let errors = check_program(&[("Main.purs", main)]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].source, DiagnosticOrigin::Source(0));
    assert!(errors[0].diagnostic.message.contains("`Missing`"));
}

#[test]
fn reports_an_unknown_export_name() {
    let library = "module Library (answer, missing) where\nanswer = 42\n";
    let errors = resolve_program_sources(&[("Library.purs", library)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("`missing`"))
    );
}

#[test]
fn uses_an_explicit_export_list_as_the_module_interface() {
    let library = "module Library (visible) where\nvisible = 1\nhidden = 2\n";
    let main = "module Main where\nimport Library\nmain = hidden\n";
    let errors =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.source == DiagnosticOrigin::Source(1))
    );
}

#[test]
fn reports_orphan_type_declaration_code() {
    let source = "module Main where\nfn :: Int\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanTypeDeclaration"))
    );
}

#[test]
fn reports_orphan_kind_declaration_code() {
    let source = "module Main where\ntype Foo :: Type\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanKindDeclaration"))
    );
}

#[test]
fn reports_overlapping_argument_names_code() {
    let source = "module Main where\nf x x = x\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OverlappingArgNames"))
    );
}

#[test]
fn reports_duplicate_value_declaration_code() {
    let source = "module Main where\nfoo = 1\nfoo = 2\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DuplicateValueDeclaration"))
    );
}

#[test]
fn reports_duplicate_value_members_across_interleaved_instance_equations() {
    let source = r#"module Main where
class Foo a where
  foo :: a -> a
  bar :: a
instance fooX :: Foo Int where
  foo x = x
  bar = 1
  foo x = x
"#;
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| { error.diagnostic.code == Some("DuplicateValueDeclaration") }),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn reports_an_orphan_signature_inside_an_instance() {
    let source = r#"module Main where
class Foo a where
  foo :: a -> a
instance fooInt :: Foo Int where
  bar :: Int
  foo x = x
"#;
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanTypeDeclaration")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn importing_a_class_does_not_import_its_method_values() {
    let library = "module Library where\nclass Identity a where\n  identity :: a -> a\n";
    let main = "module Main where\nimport Library (class Identity)\nmain = identity 1\n";
    let errors = resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)])
        .expect_err("a class import does not import class methods");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownName")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn rejects_duplicate_named_instances_across_classes() {
    let source = "module Main where\n\
class First a\n\
class Second a\n\
instance shared :: First Int\n\
instance shared :: Second Int\n";
    let errors = resolve_program_sources(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DuplicateInstance")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn rejects_a_named_instance_that_redefines_a_module_value() {
    let source = "module Main where\n\
class Marker a\n\
instance markerInt :: Marker Int\n\
markerInt = 1\n";
    let errors = resolve_program_sources(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("RedefinedIdent")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn resolves_a_program_against_the_on_disk_standard_library() {
    let source = "module Main where\nimport Prelude\nmain = runEffect (pure 1)\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ModuleNotFound")
                && error.diagnostic.message.contains("`Prelude`")),
        "{errors:?}"
    );
    check_program_lenient_with_prelude(&[("Main.purs", source)])
        .expect("the on-disk Prelude should be on the module path");
}

#[test]
fn attributes_a_library_backed_diagnostic_to_the_user_source() {
    let source = "module Main where\nimport Prelude\nmain = runEffect (pure missingName)\n";
    let errors = check_program_lenient_with_prelude(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("UnknownName")
                && error.diagnostic.message.contains("`missingName`")
        }),
        "the trusted prefix must not shift a user's diagnostic: {errors:?}"
    );
}

#[test]
fn kind_checks_a_program_against_the_on_disk_standard_library() {
    let source = "module Main where\nimport Prelude\ndata KindError f a = One f | Two (f a)\n";
    let errors = check_program_kinds_lenient_with_prelude(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("KindsDoNotUnify")),
        "{errors:?}"
    );
}
