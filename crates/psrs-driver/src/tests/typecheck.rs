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
fn reports_polymorphic_user_type_declarations_as_a_backend_limitation() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nf :: forall a. Maybe a -> Maybe a\nf x = x\nmain = f (Just 0)\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("polymorphic declarations")),
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
fn compiles_a_non_parameterized_field_constructor_case() {
    let source = "\
module Main where
data Pair = Pair Int Int | Empty
sum p = case p of
  Pair x y -> x + y
  Empty -> 0
main = sum (Pair 20 22)
";
    assert!(compile_source("Main.purs", source).is_ok());
}
