use super::*;

#[test]
fn typechecks_a_polymorphic_array_signature() {
    let source = "module Main where\nfoo :: forall a. Array a -> Array a\nfoo x = x\n";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn typechecks_user_type_constructors_in_signatures() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nf :: Maybe Int -> Maybe Int\nf x = x\n";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn reports_user_types_as_a_backend_limitation() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nf :: Maybe Int -> Maybe Int\nf x = x\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("aggregate or user-defined types")),
        "{errors:?}"
    );
}

#[test]
fn typechecks_expanded_type_synonyms() {
    let source = "module Main where\ntype Result = Array Int\nf :: Result -> Result\nf x = x\n";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn reports_a_type_synonym_mismatch_after_expansion() {
    let source = "module Main where\ntype Result = Array Int\nf :: Result -> Result\nf x = 1\n";
    let errors = check_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("Array Int")),
        "{errors:?}"
    );
}

#[test]
fn typechecks_constructor_application() {
    let source =
        "module Main where\ndata Maybe a = Nothing | Just a\nvalue :: Maybe Int\nvalue = Just 1\n";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn rejects_a_constructor_argument_type_mismatch() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nvalue :: Maybe Int\nvalue = Just \"no\"\n";
    let errors = check_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("expected Int, found String")),
        "{errors:?}"
    );
}

#[test]
fn typechecks_a_case_expression() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
f :: Maybe Int -> Int
f m = case m of
  Nothing -> 0
  Just x -> x
";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn rejects_a_case_branch_type_mismatch() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
g :: Maybe Int -> Int
g m = case m of
  Nothing -> \"no\"
  Just x -> x
";
    let errors = check_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("expected Int, found String")),
        "{errors:?}"
    );
}

#[test]
fn reports_case_as_a_core_limitation() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
f :: Maybe Int -> Int
f m = case m of
  Nothing -> 0
  Just x -> x
";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("case expressions are not supported")),
        "{errors:?}"
    );
}
