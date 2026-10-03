//! `Prim.Symbol.Compare` and `Prim.Int.Compare`: the two relations that decide an
//! `Ordering`.
//!
//! Both bind the ordering they decide through the shared substitution, so a goal
//! whose ordering disagrees with what the rule decided is rejected by ordinary
//! type equality under `TypesDoNotUnify` — the same place `purs` rejects it,
//! because official solving unifies a rule's decided arguments against the goal's
//! arguments rather than reporting a code of its own.
//!
//! What the two rules decide differs, and every case below is pinned against
//! `purs` 0.15.16. `Symbol.Compare` compares two known symbols and declines a
//! goal with one known operand. `Int.Compare` compares two known integers and
//! otherwise *closes a relation* over the orderings in scope, which is what
//! `TypeChecker/Entailment/IntCompare.hs` implements: the transitive chain case
//! is `passing/SolvingCompareInt.purs`'s `transLt` and `transEqLt`, the
//! literal-extension case is its `litTransLT`, and the negative literal case is
//! `failing/CompareInt11.purs`.

use psrs_driver::check_source;

/// A program's diagnostics, as the `errorCode` the driver reports and the first
/// line of each message. The message is part of what a case proves: the decided
/// `Ordering` appears in it, which is what shows the diagnostic came from
/// equality on a decided argument rather than from a rule that inspected the
/// wanted one.
fn rejection(source_name: &str, source: &str) -> Vec<(&'static str, String)> {
    let errors = check_source(source_name, source).expect_err("the case must be rejected");
    errors
        .iter()
        .map(|error| {
            (
                error.code.unwrap_or("<none>"),
                error.message.lines().next().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn accepts(source_name: &str, source: &str) {
    if let Err(errors) = check_source(source_name, source) {
        panic!("{source_name} was rejected: {errors:#?}");
    }
}

/// The case is rejected, this code is among its diagnostics, and one of them
/// names `decided`.
fn rejects_with(source_name: &str, source: &str, code: &'static str, decided: &str) {
    let errors = rejection(source_name, source);
    assert!(
        errors.iter().any(|(found, _)| *found == code),
        "{source_name}: expected a {code} diagnostic, got {errors:?}"
    );
    assert!(
        errors.iter().any(|(_, message)| message.contains(decided)),
        "{source_name}: no diagnostic names {decided:?}: {errors:?}"
    );
}

/// Every diagnostic of a rejected program must carry this code, and the case
/// asserts the decided `Ordering` in one of the messages.
fn only_rejects_with(source_name: &str, source: &str, code: &'static str, decided: &str) {
    let errors = rejection(source_name, source);
    assert!(
        !errors.is_empty(),
        "{source_name} was accepted, so the rule decided an obligation purs rejects"
    );
    for (found, _) in &errors {
        assert_eq!(
            *found, code,
            "{source_name}: expected {code}, got {errors:?}"
        );
    }
    assert!(
        errors.iter().any(|(_, message)| message.contains(decided)),
        "{source_name}: no diagnostic names {decided:?}: {errors:?}"
    );
}

/// The preamble every symbol case shares: a comparison that infers its ordering,
/// and an assertion that demands a specific one. `purs` reads the same two
/// declarations.
const SYMBOL_PREAMBLE: &str = "\
import Prim.Symbol (class Compare)
import Prim.Ordering (EQ, GT, LT)
data Proxy :: forall k. k -> Type
data Proxy n = Proxy
cmp :: forall l r o. Compare l r o => Proxy l -> Proxy r -> Proxy o
cmp _ _ = Proxy
assertLesser :: forall l r. Compare l r LT => Proxy l -> Proxy r -> Int
assertLesser _ _ = 0
";

/// The preamble every integer case shares. It also carries the two-argument
/// assertion helpers the relation cases need, because a goal whose operands are
/// rigid variables is exactly a `Proxy l -> Proxy r` signature.
const INTEGER_PREAMBLE: &str = "\
import Prim.Int (class Compare)
import Prim.Ordering (EQ, GT, LT)
data Proxy :: forall k. k -> Type
data Proxy n = Proxy
cmp :: forall l r o. Compare l r o => Proxy l -> Proxy r -> Proxy o
cmp _ _ = Proxy
assertLesser :: forall l r. Compare l r LT => Proxy l -> Proxy r -> Int
assertLesser _ _ = 0
assertGreater :: forall l r. Compare l r GT => Proxy l -> Proxy r -> Int
assertGreater _ _ = 0
assertEqual :: forall l r. Compare l r EQ => Proxy l -> Proxy r -> Int
assertEqual _ _ = 0
";

/// Two known symbols compare in scalar-value order, which is DEC-16's sequence
/// of Unicode scalar values rather than a locale collation: `"A" < "AB"` and
/// `"z" < "é"` because U+007A is below U+00E9. `purs` accepts the same program.
#[test]
fn two_known_symbols_decide_in_scalar_value_order() {
    accepts(
        "symbol-compare.purs",
        &format!(
            "module Main where\n{SYMBOL_PREAMBLE}\
             lt :: Proxy LT\n\
             lt = cmp (Proxy :: Proxy \"A\") (Proxy :: Proxy \"B\")\n\
             eq :: Proxy EQ\n\
             eq = cmp (Proxy :: Proxy \"A\") (Proxy :: Proxy \"A\")\n\
             gt :: Proxy GT\n\
             gt = cmp (Proxy :: Proxy \"b\") (Proxy :: Proxy \"A\")\n\
             prefix :: Proxy LT\n\
             prefix = cmp (Proxy :: Proxy \"A\") (Proxy :: Proxy \"AB\")\n\
             empty :: Proxy LT\n\
             empty = cmp (Proxy :: Proxy \"\") (Proxy :: Proxy \"A\")\n\
             scalar :: Proxy LT\n\
             scalar = cmp (Proxy :: Proxy \"z\") (Proxy :: Proxy \"\u{e9}\")\n"
        ),
    );
}

/// `Symbol.Compare` has one reading: two known symbols. One known operand and
/// one rigid variable decides nothing, so the obligation continues into instance
/// search and the program is rejected for the absence of the instance. `purs`
/// rejects the same program for the same reason.
#[test]
fn one_known_symbol_declines_and_reaches_instance_search() {
    // The goal is `Compare "A" a LT` with `a` a rigid variable, so it is
    // retained rather than decided and becomes this declaration's own scheme
    // constraint; the rejected program then names it. `purs` rejects the same
    // program, and additionally reports the body against its own signature —
    // that second diagnostic is the checked signature, not the rule.
    rejects_with(
        "symbol-compare-declines.purs",
        &format!(
            "module Main where\n{SYMBOL_PREAMBLE}\
             partial :: forall r. Proxy \"A\" -> Proxy r -> Proxy LT\n\
             partial l r = assertLesser l r\n"
        ),
        "NoInstanceFound",
        "Compare",
    );
}

/// A wanted ordering that disagrees with the decision is rejected by ordinary
/// type equality, under `TypesDoNotUnify`, as `purs` rejects it: the goal is
/// `Compare "B" "A" LT`, the rule decides `GT`, and the decided `GT` does not
/// unify with the wanted `LT`.
#[test]
fn a_wanted_symbol_ordering_that_disagrees_is_rejected_by_ordinary_equality() {
    only_rejects_with(
        "symbol-compare-miss.purs",
        &format!(
            "module Main where\n{SYMBOL_PREAMBLE}\
             wrong :: Proxy LT\n\
             wrong = cmp (Proxy :: Proxy \"B\") (Proxy :: Proxy \"A\")\n"
        ),
        "TypesDoNotUnify",
        "expected GT, found LT",
    );
}

/// Two known integers decide by value, and the ordering the caller wanted is
/// filled in. `purs` accepts the same program.
#[test]
fn two_known_integers_decide_by_value() {
    accepts(
        "int-compare-literal.purs",
        &format!(
            "module Main where\n{INTEGER_PREAMBLE}\
             litLt :: Proxy LT\n\
             litLt = cmp (Proxy :: Proxy 1) (Proxy :: Proxy 2)\n\
             litGt :: Proxy GT\n\
             litGt = cmp (Proxy :: Proxy 2) (Proxy :: Proxy 1)\n\
             litEq :: Proxy EQ\n\
             litEq = cmp (Proxy :: Proxy 3) (Proxy :: Proxy 3)\n"
        ),
    );
}

/// The relation closes, and that is the whole of what distinguishes this rule
/// from a literal comparison. Each program here states the goal through a
/// signature whose operands are rigid variables, so nothing but the orderings in
/// scope can decide it:
///
/// - `transLt` and `transEqLt` need `m < p` from two givens joined by `n`;
/// - `transEq` needs `m == p` from two equalities, which the graph reads as two
///   paths;
/// - `symmLt` needs `n < m` from `m > n`, which is the `GT` reading contributing
///   the reversed edge;
/// - `reflEq` needs `n == n` with nothing in scope at all.
///
/// `purs` accepts every one of them, and they are `passing/SolvingCompareInt.purs`'s
/// own cases.
#[test]
fn the_relation_closes_over_the_orderings_in_scope() {
    accepts(
        "int-compare-relation.purs",
        &format!(
            "module Main where\n{INTEGER_PREAMBLE}\
             transLt :: forall m n p. Compare m n LT => Compare n p LT => Proxy m -> Proxy p -> Int\n\
             transLt a b = assertLesser a b\n\
             transEqLt :: forall m n p. Compare m n EQ => Compare n p LT => Proxy m -> Proxy p -> Int\n\
             transEqLt a b = assertLesser a b\n\
             transGt :: forall m n p. Compare m n GT => Compare n p GT => Proxy m -> Proxy p -> Int\n\
             transGt a b = assertGreater a b\n\
             transEq :: forall m n p. Compare m n EQ => Compare n p EQ => Proxy m -> Proxy p -> Int\n\
             transEq a b = assertEqual a b\n\
             symmLt :: forall m n. Compare m n GT => Proxy n -> Proxy m -> Int\n\
             symmLt n m = assertLesser n m\n\
             reflEq :: forall n. Proxy n -> Int\n\
             reflEq a = assertEqual a a\n"
        ),
    );
}

/// A type-level literal in scope is a fact about its neighbours even when no
/// given orders them: the literals are sorted and every earlier one is less than
/// every later one. `Compare a 10 LT` therefore reaches `20` and not `5`, which
/// is `passing/SolvingCompareInt.purs`'s `litTransLT` and
/// `failing/CompareInt11.purs`. `purs` accepts the first program and reports the
/// second as a missing instance.
#[test]
fn a_literal_in_scope_extends_the_relation_and_still_bounds_it() {
    accepts(
        "int-compare-literal-fact.purs",
        &format!(
            "module Main where\n{INTEGER_PREAMBLE}\
             litTransLT :: forall a. Compare a 10 LT => Proxy a -> Int\n\
             litTransLT a = assertLesser a (Proxy :: Proxy 20)\n"
        ),
    );
    only_rejects_with(
        "int-compare-literal-bound.purs",
        &format!(
            "module Main where\n{INTEGER_PREAMBLE}\
             litDown :: forall a. Compare a 10 LT => Proxy a -> Int\n\
             litDown a = assertLesser a (Proxy :: Proxy 5)\n"
        ),
        "NoInstanceFound",
        "Compare",
    );
}

/// An ordering in scope that disagrees with the decision is rejected by ordinary
/// type equality, under `TypesDoNotUnify`, as `purs` rejects the same program:
/// the goal is `Compare n m LT` with `Compare m n GT` in scope, the rule decides
/// `GT`, and the two do not unify.
#[test]
fn a_wanted_integer_ordering_that_disagrees_is_rejected_by_ordinary_equality() {
    // The class's own fundep `left right -> ordering` also reports the
    // disagreement, and `FundepConflict` deliberately carries no official
    // `errorCode`, so this case asks for the `TypesDoNotUnify` among the
    // diagnostics rather than for every diagnostic to be one.
    rejects_with(
        "int-compare-miss.purs",
        &format!(
            "module Main where\n{INTEGER_PREAMBLE}\
             wrong :: forall m n. Compare m n GT => Proxy m -> Proxy n -> Int\n\
             wrong m n = assertLesser m n\n"
        ),
        "TypesDoNotUnify",
        "expected GT, found LT",
    );
}
