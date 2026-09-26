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
fn compiles_a_polymorphic_user_type_declaration() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nf :: forall a. Maybe a -> Maybe a\nf x = x\nunwrap :: Maybe Int -> Int\nunwrap value = case value of\n  Just number -> number\n  _ -> 0\nmain = unwrap (f (Just 42))\n";
    assert!(compile_source("Main.purs", source).is_ok());
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

#[test]
fn lowers_an_opaque_foreign_type_to_core_without_collapsing_it_to_int() {
    let source = "\
module Main where
foreign import data Handle :: Type
foreign import data Other :: Type
keep :: Handle -> Handle
keep h = h
main = keep
";
    let core = lower_source_to_core("Main.purs", source).expect("opaque types lower to Core");
    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .expect("main");
    let psrs_core::Type::Function { parameter, result } = &core.types[main.ty.0 as usize] else {
        panic!(
            "main should be a function, got {:?}",
            core.types[main.ty.0 as usize]
        );
    };
    let mut handle = None;
    for end in [*parameter, *result] {
        match &core.types[end.0 as usize] {
            psrs_core::Type::Constructor(psrs_core::TypeConstructor::User(id)) => {
                assert!(
                    core.opaque_ids.contains(id),
                    "Handle must be recorded as opaque"
                );
                assert!(
                    core.constructors
                        .iter()
                        .all(|constructor| constructor.type_id != *id),
                    "an opaque type has no constructors"
                );
                match handle {
                    None => handle = Some(*id),
                    Some(previous) => assert_eq!(previous, *id),
                }
            }
            other => panic!("Handle must not become {other:?}"),
        }
    }
    let handle = handle.expect("Handle");
    assert!(
        core.opaque_ids.iter().any(|id| *id != handle),
        "Other must stay a distinct opaque type, got {:?}",
        core.opaque_ids
    );
}
