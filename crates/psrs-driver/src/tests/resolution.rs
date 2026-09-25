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
    assert_eq!(errors[0].source, 0);
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
    assert!(errors.iter().any(|error| error.source == 1));
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
