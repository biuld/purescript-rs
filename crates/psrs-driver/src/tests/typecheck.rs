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
