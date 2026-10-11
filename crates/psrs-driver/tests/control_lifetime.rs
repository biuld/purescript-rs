//! Delimiter lifetime checks through escaped, ordinarily typed resumptions.
mod control_support;
use control_support::execute;

const ESCAPED: &str = r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. { saved :: Int -> C.Control Op Int } -> Op x -> (x -> C.Control Op Int) -> { _1 :: { saved :: Int -> C.Control Op Int }, _2 :: C.Control Op Int }
handler s (Op k) cont = { _1: { saved: \n -> cont (k n) }, _2: C.return 42 }
result = C.run { saved: \n -> C.return n } handler (C.perform (Op \x -> x))
main :: Int
main = (C.run result._1 handler (result._1.saved 42))._2
"#;

#[test]
fn rejects_a_resumption_after_its_closed_delimiter_returns() {
    execute("expired-closed", ESCAPED, 134);
}

#[test]
fn rejects_a_resume_node_built_before_the_delimiter_expired() {
    execute(
        "expired-prebuilt",
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. { saved :: C.Control Op Int } -> Op x -> (x -> C.Control Op Int) -> { _1 :: { saved :: C.Control Op Int }, _2 :: C.Control Op Int }
handler s (Op k) cont = { _1: { saved: cont (k 42) }, _2: C.return 0 }
result = C.run { saved: C.return 0 } handler (C.perform (Op \x -> x))
main :: Int
main = (C.run result._1 handler result._1.saved)._2
"#,
        134,
    );
}

#[test]
fn a_live_multi_shot_resumption_can_be_interpreted_reentrantly() {
    execute(
        "live-reentrant",
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler s (Op k) cont = let nested = C.run s handler (cont (k 42)) in { _1: nested._1, _2: C.return nested._2 }
main :: Int
main = (C.run 0 handler (C.perform (Op \x -> x)))._2
"#,
        42,
    );
}

const OPEN: &str = r#"module Main where
import Prim.Control as C
data F next = F (Int -> next)
data G next = G (Int -> next)
data U next = UF (F next) | UG (G next)
type Saved = { saved :: Int -> C.Control U Int }
project :: forall x r. U x -> (F x -> r) -> (G x -> r) -> r
project u l r = case u of
  UF f -> l f
  UG g -> r g
inner :: forall x. Saved -> F x -> (x -> C.Control U Int) -> { _1 :: Saved, _2 :: C.Control U Int }
inner s (F k) cont = { _1: { saved: \n -> cont (k n) }, _2: C.return 42 }
outer :: forall x. Int -> G x -> (x -> C.Control G { _1 :: Saved, _2 :: Int }) -> { _1 :: Int, _2 :: C.Control G { _1 :: Saved, _2 :: Int } }
outer s (G k) cont = { _1: s, _2: cont (k 0) }
handled = foldOpen { saved: \n -> C.return n } inner project UG (C.perform (UF (F \x -> x)))
result = C.run 0 outer handled
main :: Int
main = result._2._2
"#;

#[test]
fn rejects_a_completed_open_delimiter_while_its_outer_delimiter_is_live() {
    let text = OPEN.replace("result = C.run 0 outer handled", r"result = C.run 0 outer (C.bind handled (\pair -> foldOpen pair._1 inner project UG (pair._1.saved 42)))");
    execute("expired-open-live-outer", &text, 134);
}

#[test]
fn rejects_an_open_resumption_after_the_outer_run_returns() {
    let text = OPEN.replace("main = result._2._2", "main = (C.run 0 outer (foldOpen result._2._1 inner project UG (result._2._1.saved 42)))._2._2");
    execute("expired-open", &text, 134);
}

#[test]
fn outer_abandonment_expires_a_suspended_inner_delimiter() {
    execute(
        "expired-abandoned-open",
        r#"module Main where
import Prim.Control as C
data F x = F (Int -> x)
data G x = G (Int -> C.Control U Int) (Int -> x)
data U x = UF (F x) | UG (G x)
type Saved = { saved :: Int -> C.Control U Int }
project :: forall x r. U x -> (F x -> r) -> (G x -> r) -> r
project u l r = case u of
  UF f -> l f
  UG g -> r g
inner :: forall x. Int -> F x -> (x -> C.Control U Int) -> { _1 :: Int, _2 :: C.Control U Int }
inner s (F k) cont = { _1: s, _2: C.perform (UG (G (\n -> cont (k n)) (\x -> x))) }
outer :: forall x. Saved -> G x -> (x -> C.Control G { _1 :: Int, _2 :: Int }) -> { _1 :: Saved, _2 :: C.Control G { _1 :: Int, _2 :: Int } }
outer s (G saved k) cont = { _1: { saved: saved }, _2: C.return { _1: 0, _2: 42 } }
result = C.run { saved: \n -> C.return n } outer (foldOpen 0 inner project UG (C.perform (UF (F \x -> x))))
main :: Int
main = (C.run result._1 outer (foldOpen 0 inner project UG (result._1.saved 42)))._2._2
"#,
        134,
    );
}
