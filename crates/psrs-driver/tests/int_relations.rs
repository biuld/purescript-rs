//! The `Prim.Int` relations: `Add`, `Mul`, and `ToString`.
//!
//! All three are relations the compiler solves rather than instances a library
//! provides, and all three decide type-level integers. `Add` decides forwards
//! from two addends and backwards from an addend and the sum; `Mul` and
//! `ToString` decide forwards only. A decision is bound through the shared
//! substitution, so a goal whose decided argument disagrees with what the caller
//! wrote is rejected by ordinary type equality under `TypesDoNotUnify` — the same
//! place `purs` rejects it, because official solving unifies a rule's decided
//! arguments against the goal's arguments rather than reporting a code of its
//! own.
//!
//! Every case below is pinned against `purs` 0.15.16: the accepted programs are
//! accepted there too, and each rejected program reports `TypesDoNotUnify` there
//! as well. `failing/IntToString1.purs` and `failing/IntToString3.purs` are the
//! corpus's own form of the `ToString` mismatch cases below.

use psrs_driver::check_source;

/// The `Proxy` these cases constrain, with the kind signature the corpus files
/// carry so `purs` reads it as `k -> Type`.
const PROXY: &str = "data Proxy :: forall k. k -> Type\ndata Proxy a = Proxy\n";

/// A program's diagnostics, as the offset each points at, the `errorCode` the
/// driver reports, and its first line of text. The message is part of what a
/// case proves: the decided literal appears in it, which is what shows the
/// diagnostic came from equality on a decided argument rather than from a rule
/// that inspected the wanted one.
fn rejection(source_name: &str, source: &str) -> Vec<(u32, &'static str, String)> {
    let errors = check_source(source_name, source).expect_err("the case must be rejected");
    errors
        .iter()
        .map(|error| {
            (
                error.span.start,
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

/// Every diagnostic of a rejected program must carry this code, and the case
/// asserts the decided literal in one of the messages.
fn only_rejects_with(source_name: &str, source: &str, code: &'static str, decided: &str) {
    let errors = rejection(source_name, source);
    assert!(
        !errors.is_empty(),
        "{source_name} was accepted, so the rule decided an obligation purs rejects"
    );
    for (offset, found, _) in &errors {
        assert_eq!(*found, code, "{source_name} at {offset}: expected {code}");
    }
    assert!(
        errors
            .iter()
            .any(|(_, _, message)| message.contains(decided)),
        "{source_name}: no diagnostic names the decided value {decided:?}: {errors:?}"
    );
}

/// `Add` decides forwards from two addends and backwards from an addend and the
/// sum. All three directions reach the solver with the decided argument still
/// free, and `purs` accepts the same program.
#[test]
fn add_decides_forwards_and_backwards() {
    accepts(
        "int-add.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Add)\n\n\
             {PROXY}\n\
             useSum :: forall s. Add 1 2 s => Proxy s\n\
             useSum = Proxy\n\
             useRight :: forall r. Add r 5 9 => Proxy r\n\
             useRight = Proxy\n\
             useLeft :: forall l. Add 2 l 9 => Proxy l\n\
             useLeft = Proxy\n\
             -- Forwards: 1 + 2 is 3.\n\
             three :: Proxy 3\n\
             three = useSum\n\
             -- Backwards from the right addend and the sum: 9 - 5 is 4.\n\
             four :: Proxy 4\n\
             four = useRight\n\
             -- Backwards from the left addend and the sum: 9 - 2 is 7.\n\
             seven :: Proxy 7\n\
             seven = useLeft\n"
        ),
    );
}

/// The backwards directions read the missing addend out of the sum, and a
/// caller that supplies a value whose type no signature pins down leaves that
/// addend a skolem the rule must answer. `purs` reports `TypesDoNotUnify` on the
/// decided literal, and so does this compiler, because the decision goes through
/// the shared binder rather than being assigned to the argument directly.
#[test]
fn add_reads_the_left_addend_backwards_and_refuses_a_skolem() {
    only_rejects_with(
        "int-add-backwards-left.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Add)\n\n\
             {PROXY}\n\
             useLeft :: forall l. Add l 5 9 => Proxy l -> Int\n\
             useLeft _ = 0\n\
             -- The left addend is a skolem of this signature, so the rule reads\n\
             -- it backwards as 9 - 5 and the binder refuses the skolem.\n\
             fromRightAndSum :: forall l. Proxy l -> Int\n\
             fromRightAndSum = useLeft\n"
        ),
        "TypesDoNotUnify",
        "expected 4",
    );
}

#[test]
fn add_reads_the_right_addend_backwards_and_refuses_a_skolem() {
    only_rejects_with(
        "int-add-backwards-right.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Add)\n\n\
             {PROXY}\n\
             useRight :: forall r. Add 2 r 9 => Proxy r -> Int\n\
             useRight _ = 0\n\
             -- The right addend is a skolem of this signature, so the rule reads\n\
             -- it backwards as 9 - 2 and the binder refuses the skolem.\n\
             fromLeftAndSum :: forall r. Proxy r -> Int\n\
             fromLeftAndSum = useRight\n"
        ),
        "TypesDoNotUnify",
        "expected 7",
    );
}

/// A sum the caller wrote down that the addends do not make is rejected under
/// `TypesDoNotUnify` on the two integers, which is what `purs` reports for the
/// same program. Before the rule existed this goal reached instance search and
/// was reported as a missing instance, a code the suite never expects here.
#[test]
fn a_wrong_sum_is_rejected_by_ordinary_equality() {
    only_rejects_with(
        "int-add-miss.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Add)\n\n\
             {PROXY}\n\
             useSum :: forall s. Add 1 2 s => Proxy s\n\
             useSum = Proxy\n\
             -- 1 + 2 is 3, so the program cannot call for 4.\n\
             four :: Proxy 4\n\
             four = useSum\n"
        ),
        "TypesDoNotUnify",
        "expected 3, found 4",
    );
}

/// `Add` with two or fewer literals decides nothing and declines, so the goal
/// continues into the ordinary paths instead of being reported as impossible.
/// `purs` accepts the same program: a constraint that is not discharged here is
/// retained, and the class has no members for it to be retained as.
#[test]
fn add_declines_when_no_pair_of_arguments_is_known() {
    accepts(
        "int-add-declines.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Add)\n\n\
             {PROXY}\n\
             -- Only the sum is a literal, so there is no pair to add or subtract.\n\
             addPartial :: forall l r. Add l r 9 => Proxy l -> Proxy r -> Int\n\
             addPartial _ _ = 0\n"
        ),
    );
}

/// `Mul` decides forwards from the two factors. `purs` accepts the same program.
#[test]
fn mul_decides_forwards_from_two_factors() {
    accepts(
        "int-mul.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Mul)\n\n\
             {PROXY}\n\
             useProduct :: forall p. Mul 2 3 p => Proxy p\n\
             useProduct = Proxy\n\
             -- 2 * 3 is 6.\n\
             six :: Proxy 6\n\
             six = useProduct\n"
        ),
    );
}

/// `Mul` has one fundep and official `solveIntMul` reads only the two factors,
/// so a goal with a known product and an unknown factor declines rather than
/// searching for a factorisation. `purs` accepts the same program for the same
/// reason.
#[test]
fn mul_declines_when_a_factor_is_not_a_literal() {
    accepts(
        "int-mul-declines.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Mul)\n\n\
             {PROXY}\n\
             -- The left factor is a rigid variable, so there is nothing to\n\
             -- multiply.\n\
             notProduct :: forall l. Mul l 3 12 => Proxy l -> Int\n\
             notProduct _ = 0\n"
        ),
    );
}

/// A product the factors do not make is rejected under `TypesDoNotUnify` on the
/// two integers, as `purs` rejects the same program.
#[test]
fn a_wrong_product_is_rejected_by_ordinary_equality() {
    only_rejects_with(
        "int-mul-miss.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class Mul)\n\n\
             {PROXY}\n\
             useProduct :: forall p. Mul 2 3 p => Proxy p\n\
             useProduct = Proxy\n\
             -- 2 * 3 is 6, so the program cannot call for 7.\n\
             seven :: Proxy 7\n\
             seven = useProduct\n"
        ),
        "TypesDoNotUnify",
        "expected 6, found 7",
    );
}

/// A known integer determines its string through its decimal spelling.
/// `purs` accepts the same program.
#[test]
fn a_known_integer_determines_its_string() {
    accepts(
        "int-to-string.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class ToString)\n\n\
             {PROXY}\n\
             testToString :: forall i s. ToString i s => Proxy i -> Proxy s\n\
             testToString _ = Proxy\n\
             posToString :: Proxy \"1\"\n\
             posToString = testToString (Proxy :: Proxy 1)\n\
             fortyTwo :: Proxy \"42\"\n\
             fortyTwo = testToString (Proxy :: Proxy 42)\n"
        ),
    );
}

/// The wanted string is decided by the integer and then compared, which is what
/// `failing/IntToString1.purs` exercises: the goal is `ToString 1 "a"`, the rule
/// decides the string as `"1"`, and ordinary type equality rejects it. `purs`
/// reports `TypesDoNotUnify` on the two type-level strings, which is the case
/// this repository's board tracks.
#[test]
fn a_wanted_string_that_disagrees_is_rejected_by_ordinary_equality() {
    only_rejects_with(
        "int-to-string-miss.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class ToString)\n\n\
             {PROXY}\n\
             testToString :: forall i s. ToString i s => Proxy i -> Proxy s\n\
             testToString _ = Proxy\n\
             posToString :: Proxy \"a\"\n\
             posToString = testToString (Proxy :: Proxy 1)\n"
        ),
        "TypesDoNotUnify",
        "expected \"1\", found \"a\"",
    );
}

/// The same rejection for a different integer, which is what
/// `failing/IntToString3.purs` exercises: `0` decides `"0"`, so the wanted
/// `"a"` is still wrong.
#[test]
fn zero_decides_its_own_string_and_still_rejects_a_different_one() {
    only_rejects_with(
        "int-to-string-zero.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class ToString)\n\n\
             {PROXY}\n\
             testToString :: forall i s. ToString i s => Proxy i -> Proxy s\n\
             testToString _ = Proxy\n\
             zeroToString :: Proxy \"a\"\n\
             zeroToString = testToString (Proxy :: Proxy 0)\n"
        ),
        "TypesDoNotUnify",
        "expected \"0\", found \"a\"",
    );
}

/// `ToString` needs a literal to read. A rigid variable is not one, so the rule
/// declines and the obligation continues into the ordinary paths. `purs`
/// accepts the same program, for the same reason: nothing discharges the goal.
#[test]
fn to_string_declines_when_the_integer_is_not_a_literal() {
    accepts(
        "int-to-string-declines.purs",
        &format!(
            "module Main where\n\n\
             import Prim.Int (class ToString)\n\n\
             {PROXY}\n\
             -- The integer is a rigid variable, so the rule has no spelling to\n\
             -- decide the string from.\n\
             notAnInt :: forall i. ToString i \"a\" => Proxy i -> Int\n\
             notAnInt _ = 0\n"
        ),
    );
}
