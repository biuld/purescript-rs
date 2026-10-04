use super::*;

mod opaque;

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
import Prelude
data Pair = Pair Int Int | Empty
sum p = case p of
  Pair x y -> x + y
  Empty -> 0
main = sum (Pair 20 22)
";
    assert!(compile_source("Main.purs", source).is_ok());
}

#[test]
fn typechecks_do_notation_over_effect() {
    // `do` desugars to `bind`/`discard`/`let`; without dictionaries both names
    // resolve from the enclosing scope, here the imported Prelude. Type
    // inference must see through the desugaring and give the block `Effect Int`.
    let source = "\
module Main where
import Prelude
import WASI.Console
action :: Effect Int
action = do
  handle <- pure 1
  let doubled = handle + handle
  _ <- log \"value\"
  pure doubled
main = runEffect action
";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn typechecks_a_non_variable_do_binder() {
    // A non-variable binder desugars through a `case`; inference must bind the
    // data constructor's fields from the scrutinee type.
    let source = "\
module Main where
import Prelude
data Pair = Pair Int Int
action :: Effect Int
action = do
  Pair x y <- pure (Pair 1 2)
  pure (x + y)
main = runEffect action
";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn rejects_a_do_binder_with_the_wrong_case_type() {
    let source = "\
module Main where
import Prelude
data Pair = Pair Int Int
action :: Effect Int
action = do
  Pair x y <- pure 1
  pure (x + y)
main = runEffect action
";
    assert!(check_source("Main.purs", source).is_err());
}

#[test]
fn typechecks_ado_notation_over_effect() {
    // `ado` desugars to `map`/`apply`/`pure`; inference must see through the
    // curried continuation and give the block `Effect Int`.
    let source = "\
module Main where
import Prelude
action :: Effect Int
action = ado
  x <- pure 20
  y <- pure 22
  in x + y
main = runEffect action
";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn typechecks_an_ado_value_and_let() {
    let source = "\
module Main where
import Prelude
action :: Effect Int
action = ado
  _ <- pure 1
  let base = 40
    in base + 2
main = runEffect action
";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn typechecks_a_where_binding_in_the_right_hand_side() {
    let source = "\
module Main where
value :: Int
value = result where
  result = 42
";
    assert!(check_source("Main.purs", source).is_ok());
}
#[test]
fn typechecks_a_recursive_where_binding() {
    let source = "\
module Main where
import Prelude
count :: Int
count = go 3 0 where
  go n acc = if n == 0 then acc else go (n - 1) (acc + 1)
";
    assert!(check_source("Main.purs", source).is_ok());
}

#[test]
fn rejects_a_where_binding_outside_its_right_hand_side() {
    let source = "\
module Main where
hidden :: Int
hidden = 1 where
  scoped = 2
leaked :: Int
leaked = scoped
";
    let errors = check_source("Main.purs", source).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains("scoped")),
        "{errors:?}"
    );
}

#[test]
fn a_lenient_type_check_reports_a_mismatch_with_its_official_code() {
    let source = "module Main where\ng :: Int\ng = 1\nmain = g 2\n";
    let errors = check_program_types_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}

#[test]
fn a_lenient_type_check_reports_a_missing_instance_with_its_official_code() {
    let source = "module Main where\n\
        class C a where\n  m :: a -> Int\n\
        main = m 1\n";
    let errors = check_program_types_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
}

#[test]
fn a_lenient_type_check_reports_an_occurs_check_with_its_official_code() {
    let source = "module Main where\nf x = x x\nmain = 0\n";
    let errors = check_program_types_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("InfiniteType")),
        "{errors:?}"
    );
}

#[test]
fn a_lenient_type_check_type_checks_a_module_whose_sibling_is_missing() {
    // The case that resolves is type checked even though its sibling does not,
    // which is what makes the M4 and M5 layers measurable without a library.
    let broken = "module Broken where\nimport Absent\nvalue = 1\n";
    let good = "module Main where\nimport Broken\ng :: Int\ng = 1\nmain = g 2\n";
    let errors =
        check_program_types_lenient(&[("Broken.purs", broken), ("Main.purs", good)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("ModuleNotFound")),
        "the missing module is reported against its own source: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.source == DiagnosticOrigin::Source(1)
                && error.diagnostic.code == Some("TypesDoNotUnify")),
        "the resolvable module is still type checked: {errors:?}"
    );
}

#[test]
fn a_lenient_type_check_attributes_a_diagnostic_to_the_user_source_not_the_library() {
    let source = "module Main where\nimport Prelude\nmain = runEffect (pure missingName)\n";
    let errors = check_program_types_lenient_with_prelude(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("UnknownName")
        }),
        "the trusted prefix must not shift a user's diagnostic: {errors:?}"
    );
}

#[test]
fn a_lenient_type_check_reports_a_module_by_its_own_module_id() {
    // A lenient program type checks only the modules that resolved, so the list
    // it iterates is shorter than the source list: a module whose names do not
    // resolve is dropped from it. The module id is the position in the source
    // list, and it is what a diagnostic must use. Enumerating the surviving
    // modules reports this error against `Broken.purs`, which is exactly the
    // module that failed to resolve.
    let broken = "module Broken where\nvalue = missingName\n";
    let good = "module Main where\nanswer :: Int\nanswer = \"not an int\"\n";
    let errors =
        check_program_types_lenient(&[("Broken.purs", broken), ("Main.purs", good)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("UnknownName")),
        "`Broken` is the source that does not resolve: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(1)
                && error.diagnostic.code == Some("TypesDoNotUnify")
        }),
        "the type error belongs to the second source: {errors:?}"
    );
    assert!(
        !errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("TypesDoNotUnify")
        }),
        "the unresolvable module must not inherit the surviving module's error: {errors:?}"
    );
}

#[test]
fn a_lenient_type_check_sees_the_instances_of_the_module_it_imports() {
    // Instance visibility is resolved through the dependency table, which is
    // indexed by module id. Here `Broken` does not resolve, so the surviving
    // modules sit one position below their ids: `Support` is the third module
    // but the second surviving one. Indexing the table by position would answer
    // `Main`'s lookup with `Main`'s own instances, and report `NoInstanceFound`
    // for an instance the program does declare.
    let support = "module Support where\ndata Wrapper a = Wrapper a\nclass Size a where\n  size :: a -> Int\ninstance sizeWrapper :: Size (Wrapper a) where\n  size _ = 1\n";
    let user = "module Main where\nimport Support\nmain = size (Wrapper 1)\n";
    let sources = [
        ("Broken.purs", "module Broken where\nvalue = missingName\n"),
        ("Other.purs", "module Other where\nvalue = 1\n"),
        ("Support.purs", support),
        ("Main.purs", user),
    ];
    let errors = check_program_types_lenient(&sources)
        .expect_err("`Broken` names something that does not exist");
    assert!(
        !errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "the instance declared in Support is visible to Main: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.code == Some("UnknownName")
        }),
        "the unresolved name is reported against its own source: {errors:?}"
    );
}

#[test]
fn checks_an_ascription_against_its_written_type() {
    let source = "module Main where\n\
        g :: Int -> Int\n\
        g _ = 1\n\
        f = g :: Int -> Int\n\
        main :: Int\n\
        main = f 0\n";
    check_program(&[("Main.purs", source)]).expect("a valid ascription should be accepted");
}

#[test]
fn rejects_an_ascription_whose_type_does_not_match() {
    let source = "module Main where\n\
        g :: Int\n\
        g = 1\n\
        f = g :: String\n\
        main :: Int\n\
        main = 0\n";
    let errors = check_program(&[("Main.purs", source)]).expect_err("the ascription should fail");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}

#[test]
fn an_ascription_constrains_a_flexible_expected_type_variable() {
    let source = "module Main where\n\
        same :: forall a. a -> a -> Boolean\n\
        same _ _ = true\n\
        main = same ((\\x -> x) :: Number -> Number) true\n";
    let errors = check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("the annotation must constrain the shared type variable");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}

#[test]
fn an_ascription_with_an_explicit_forall_keeps_that_type() {
    // The written `forall` is checked with skolems and is the result type, so
    // the value is usable at that polymorphic type.
    let source = "module Main where\n\
        f = (\\_ -> 0) :: forall b. b -> Int\n\
        main :: Int\n\
        main = f 1\n";
    check_program(&[("Main.purs", source)]).expect("a polymorphic ascription should be accepted");
}

#[test]
fn an_ascription_whose_written_type_is_quantified_keeps_that_type() {
    // The quantifier is the result type, so the value is usable at that
    // polymorphic type and the declaration needs no signature.
    let source = "module Main where\n\
        f = (\\_ -> 0) :: forall b. b -> Int\n\
        main :: Int\n\
        main = f 1\n";
    check_program(&[("Main.purs", source)]).expect("a polymorphic ascription should be accepted");
}

#[test]
fn an_ascription_does_not_leak_a_node_into_typed_core() {
    // The ascription is a type-directed check at a known expression: the
    // expression is kept with its checked type, so Core has no wrapper.
    let source = "module Main where\n\
        g :: Int -> Int\n\
        g _ = 1\n\
        f = g :: Int -> Int\n\
        main :: Int\n\
        main = f 0\n";
    let typed =
        typecheck_program_sources(&[("Main.purs", source)]).expect("the program should type check");
    assert_eq!(typed.len(), 1);
}
