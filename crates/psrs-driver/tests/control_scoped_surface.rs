//! Ordinary language rejection of invalid source/target elimination contracts.
const HEADER: &str = r"module Main where
import Prim.Control as C
data F x = F (Int -> x)
data G x = G (Int -> x)
data U x = UF (F x) | UG (G x)
ret :: Int -> C.Control G Boolean
ret n = C.return true
source :: C.Control U Int
source = C.return 42
";

fn rejects(body: &str) {
    let errors = psrs_driver::check_source("Main.purs", &format!("{HEADER}\n{body}"))
        .expect_err("invalid control contract accepted");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}

#[test]
fn rejects_source_requests_in_a_target_clause() {
    rejects(
        r"
clause :: forall x. U x -> (x -> C.Control U Int) -> C.Control G Boolean
clause request raw = C.bind (C.perform request) (\_ -> C.return true)
bad = C.handle ret clause source
main = 0
",
    );
}

#[test]
fn rejects_unrelated_raw_and_target_answers() {
    rejects(
        r"
clause :: forall x. U x -> (x -> C.Control U Int) -> C.Control G Boolean
clause request raw = C.bind (raw true) (\_ -> C.return true)
bad = C.handle ret clause source
main = 0
",
    );
    rejects(
        r"
clause :: forall x. U x -> (x -> C.Control U Int) -> C.Control G Boolean
clause request raw = C.return 42
bad = C.handle ret clause source
main = 0
",
    );
}

#[test]
fn rejects_the_previous_five_argument_handle_signature() {
    rejects(
        r"
old :: Int -> (forall x. Int -> F x -> (x -> C.Control U Int) -> { _1 :: Int, _2 :: C.Control U Int }) -> (forall x r. U x -> (F x -> r) -> (G x -> r) -> r) -> (forall x. G x -> U x) -> C.Control U Int -> C.Control G { _1 :: Int, _2 :: Int }
old = C.handle
main = 0
",
    );
}

#[test]
fn rejects_shallow_residual_effect_removal() {
    rejects(
        r"
shallow :: C.Control U Int -> C.Control U { value :: Int }
shallow computation = C.handle (\a -> C.return { value: a })
  (\request raw -> C.bind (C.perform request) (\answer -> C.bind (raw answer) (\a -> C.return { value: a }))) computation
bad :: C.Control G { value :: Int }
bad = shallow source
main = 0
",
    );
}
