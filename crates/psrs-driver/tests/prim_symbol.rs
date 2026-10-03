//! `Prim.Symbol.Append` and `Prim.Symbol.Cons` through the whole front end.
//!
//! `purs` 0.15.16 is the oracle for every case below, and the code each rejected
//! case is pinned against is the code `purs` raises for it. Both relations are
//! declared by the compiler itself, so a case needs nothing but `Prim`.
//!
//! The cases cover the two relations in each reading — forwards, backwards
//! through a known prefix, backwards through a known suffix, and backwards
//! through a known symbol — plus the declines, the prefix check that fails, the
//! head that is not one scalar, and the empty symbol that has no first scalar.

use psrs_driver::{Diagnostic, check_source};

fn accepts(source_name: &str, source: &str) {
    if let Err(errors) = check_source(source_name, source) {
        panic!("{source_name} was rejected: {errors:#?}");
    }
}

/// The official code of every diagnostic the driver reports, in order. A case
/// states what `purs` raises, which is the whole claim: the diagnostic a rule
/// reports is the same rejection `purs` makes.
fn rejections(source_name: &str, source: &str) -> Vec<&'static str> {
    let errors = check_source(source_name, source).expect_err("the case must be rejected");
    errors
        .iter()
        .map(|error| error.code.unwrap_or("<none>"))
        .collect()
}

/// The `errorCode` of the only diagnostic a case reports, which is how a case says
/// "this obligation fails and nothing else does".
fn rejection(source_name: &str, source: &str) -> &'static str {
    let mut codes = rejections(source_name, source);
    assert_eq!(
        codes.len(),
        1,
        "{source_name} reports {codes:?} rather than one diagnostic"
    );
    codes.pop().expect("one diagnostic")
}

/// The part of the only diagnostic a case reports that says which relation
/// failed, so a case pins the reason and not only the code.
fn rejection_message(source_name: &str, source: &str) -> String {
    let errors: Vec<Diagnostic> =
        check_source(source_name, source).expect_err("the case must be rejected");
    errors[0].message.clone()
}

/// The part of a program every case shares: a kind-polymorphic `SProxy`, so a
/// symbol of kind `Symbol` can be named in a value position.
const PROXY: &str = r#"data SProxy :: forall k. k -> Type

data SProxy a = SProxy
"#;

/// `Append`'s forward reading: two known symbols decide the third. This is the
/// obligation `Type.Data.Symbol.append` is defined by.
#[test]
fn append_concatenates_two_known_symbols() {
    accepts(
        "append-forward.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Append)\n{PROXY}\
             append :: forall s. Append \"a\" \"b\" s => SProxy s -> SProxy s\n\
             append _ = SProxy\n\
             appended :: SProxy \"ab\"\n\
             appended = append (SProxy :: SProxy \"ab\")\n"
        ),
    );
}

/// `Append`'s first backward reading: the left symbol and the appended symbol are
/// known, so the right symbol is what follows the left one.
#[test]
fn append_decides_the_right_symbol_from_a_known_prefix() {
    accepts(
        "append-prefix.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Append)\n{PROXY}\
             dropPrefix :: forall s. Append \"ab\" s \"abc\" => SProxy s -> SProxy s\n\
             dropPrefix _ = SProxy\n\
             suffix :: SProxy \"c\"\n\
             suffix = dropPrefix (SProxy :: SProxy \"c\")\n"
        ),
    );
}

/// `Append`'s second backward reading: the right symbol and the appended symbol
/// are known, so the left symbol is what precedes the right one.
#[test]
fn append_decides_the_left_symbol_from_a_known_suffix() {
    accepts(
        "append-suffix.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Append)\n{PROXY}\
             dropSuffix :: forall s. Append s \"c\" \"abc\" => SProxy s -> SProxy s\n\
             dropSuffix _ = SProxy\n\
             prefix :: SProxy \"ab\"\n\
             prefix = dropSuffix (SProxy :: SProxy \"ab\")\n"
        ),
    );
}

/// `Cons`'s forward reading: a one-scalar head and a known tail decide the symbol.
#[test]
fn cons_joins_a_one_scalar_head_and_a_tail() {
    accepts(
        "cons-join.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
             join :: forall s. Cons \"a\" \"bc\" s => SProxy s -> SProxy s\n\
             join _ = SProxy\n\
             joined :: SProxy \"abc\"\n\
             joined = join (SProxy :: SProxy \"abc\")\n"
        ),
    );
}

/// `Cons`'s backward reading: a known symbol splits into its first scalar and the
/// rest, which is what lets a recursive walk over a type-level symbol decide one
/// character at a time.
#[test]
fn cons_splits_a_known_symbol() {
    accepts(
        "cons-split.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
             split :: forall h t. Cons h t \"abc\" => SProxy h -> SProxy t -> SProxy h\n\
             split _ _ = SProxy\n\
             first :: SProxy \"a\"\n\
             first = split (SProxy :: SProxy \"a\") (SProxy :: SProxy \"bc\")\n"
        ),
    );
}

/// A symbol is a sequence of scalar values, not bytes, so one multi-byte scalar is
/// one character in both directions.
#[test]
fn a_multi_byte_scalar_is_one_character() {
    accepts(
        "cons-multibyte-split.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
             split :: forall h t. Cons h t \"a\u{1f600}\" => SProxy h -> SProxy t -> SProxy h\n\
             split _ _ = SProxy\n\
             first :: SProxy \"a\"\n\
             first = split (SProxy :: SProxy \"a\") (SProxy :: SProxy \"\u{1f600}\")\n"
        ),
    );
    accepts(
        "cons-multibyte-join.purs",
        &format!(
            "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
             join :: forall s. Cons \"\u{1f600}\" \"x\" s => SProxy s -> SProxy s\n\
             join _ = SProxy\n\
             joined :: SProxy \"\u{1f600}x\"\n\
             joined = join (SProxy :: SProxy \"\u{1f600}x\")\n"
        ),
    );
}

/// No reading applies when no argument is known, so the obligation is not a claim
/// about impossible types and reaches instance search. `purs` reports
/// `NoInstanceFound` for `Prim.Symbol.Append t0 t1 t2` with the hint that the
/// instance head contains unknown type variables.
#[test]
fn an_append_with_no_known_symbol_reaches_instance_search() {
    assert_eq!(
        rejection(
            "append-unknown.purs",
            &format!(
                "module Main where\n\nimport Prim.Symbol (class Append)\n{PROXY}\
             go :: forall l r s. Append l r s => (Int -> Int) -> SProxy l -> SProxy r -> SProxy s -> Int\n\
             go _ _ _ _ = 0\n\
             main :: (Int -> Int) -> Int\n\
             main f = go f SProxy SProxy SProxy\n"
            ),
        ),
        "NoInstanceFound"
    );
}

/// The reading the arguments' shape selects is the only one. `Append "b" s "abc"`
/// has a known left symbol and a known appended symbol, so it is read as a prefix
/// and not as a suffix, and `"b"` is no prefix of `"abc"` even though `"c"` is a
/// suffix of it. `purs` reports `NoInstanceFound` rather than the `"a"` the suffix
/// reading would have produced.
#[test]
fn append_does_not_fall_through_to_the_next_reading() {
    let source = |constraint: &str| {
        format!(
            "module Main where\n\nimport Prim.Symbol (class Append)\n{PROXY}\
             go :: forall s. {constraint} => SProxy s -> Int\n\
             go _ = 0\n\
             main :: Int\n\
             main = go SProxy\n"
        )
    };
    assert_eq!(
        rejection(
            "append-no-backtrack.purs",
            &source("Append \"b\" s \"abc\"")
        ),
        "NoInstanceFound"
    );
    assert_eq!(
        rejection("append-not-prefix.purs", &source("Append \"a\" s \"ba\"")),
        "NoInstanceFound"
    );
}

/// An `Append` whose decided symbol does not unify with the one it is wanted at
/// is rejected on the decision itself, under the code `purs` raises for the same
/// case. The reason is the shared unifier's: the rule states the concatenation and
/// the framework unifies it against the goal, which is the step official solving
/// takes on every dictionary it produces.
#[test]
fn append_rejects_a_decided_symbol_that_does_not_unify() {
    let source = format!(
        "module Main where\n\nimport Prim.Symbol (class Append)\n{PROXY}\
         go :: forall s. Append \"a\" \"b\" s => SProxy s -> Int\n\
         go _ = 0\n\
         main :: Int\n\
         main = go (SProxy :: SProxy \"ba\")\n"
    );
    assert_eq!(
        rejection("append-mismatch.purs", &source),
        "TypesDoNotUnify"
    );
    assert_eq!(
        rejection_message("append-mismatch.purs", &source),
        "type mismatch: expected \"ab\", found \"ba\""
    );
}

/// A `Cons` whose known symbol splits into halves that disagree with a known head
/// or tail is a mismatch and not a join: the splitting reading comes first,
/// exactly as official `consSymbol` reads it.
#[test]
fn cons_rejects_a_split_that_contradicts_a_known_half() {
    let source = |head: &str, tail: &str, symbol: &str| {
        format!(
            "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
             go :: forall s. Cons \"{head}\" \"{tail}\" s => SProxy s -> Int\n\
             go _ = 0\n\
             main :: Int\n\
             main = go (SProxy :: SProxy \"{symbol}\")\n"
        )
    };
    let wide = source("ab", "c", "abc");
    assert_eq!(
        rejection("cons-head-mismatch.purs", &wide),
        "TypesDoNotUnify"
    );
    assert_eq!(
        rejection_message("cons-head-mismatch.purs", &wide),
        "type mismatch: expected \"a\", found \"ab\""
    );

    let short = source("a", "bc", "a");
    assert_eq!(
        rejection("cons-tail-mismatch.purs", &short),
        "TypesDoNotUnify"
    );
    assert_eq!(
        rejection_message("cons-tail-mismatch.purs", &short),
        "type mismatch: expected \"\", found \"bc\""
    );

    assert_eq!(
        rejection("cons-tail-mismatch-2.purs", &source("a", "bc", "abd")),
        "TypesDoNotUnify"
    );
}

/// A split whose head is still unknown and whose tail contradicts a known one
/// fails on the tail, after the head has been decided. `purs` unifies the halves
/// it computed with the wanted ones in the same order, so it rejects this the
/// same way.
#[test]
fn cons_rejects_a_split_that_binds_the_head_and_contradicts_the_tail() {
    assert_eq!(
        rejection(
            "cons-partial-bind.purs",
            &format!(
                "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
                 go :: forall h. Cons h \"bc\" \"ab\" => SProxy h -> Int\n\
                 go _ = 0\n\
                 main :: Int\n\
                 main = go (SProxy :: SProxy \"a\")\n"
            )
        ),
        "TypesDoNotUnify"
    );
}

/// An empty symbol has no first scalar, so the splitting reading decides nothing
/// and the obligation reaches instance search. `purs` reports `NoInstanceFound`
/// for `Prim.Symbol.Cons "a" "" ""`.
#[test]
fn cons_reports_an_empty_symbol_as_a_missing_instance() {
    assert_eq!(
        rejection(
            "cons-empty-symbol.purs",
            &format!(
                "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
                 go :: forall s. Cons \"a\" \"\" s => SProxy s -> Int\n\
                 go _ = 0\n\
                 main :: Int\n\
                 main = go (SProxy :: SProxy \"\")\n"
            )
        ),
        "NoInstanceFound"
    );
}

/// A head that is empty or longer than one scalar cannot be a `Cons` head. The
/// rule says the obligation cannot hold, and the framework refuses the report
/// while the symbol argument is still unknown — an obligation with an unknown
/// argument is undecided, not impossible — so the obligation reaches instance
/// search, which is where `purs`'s own decline lands.
#[test]
fn cons_reports_a_head_that_is_not_one_scalar_as_a_missing_instance() {
    let source = |head: &str, tail: &str| {
        format!(
            "module Main where\n\nimport Prim.Symbol (class Cons)\n{PROXY}\
             go :: forall s. Cons \"{head}\" \"{tail}\" s => SProxy s -> Int\n\
             go _ = 0\n\
             main :: Int\n\
             main = go SProxy\n"
        )
    };
    assert_eq!(
        rejection("cons-wide-head.purs", &source("ab", "c")),
        "NoInstanceFound"
    );
    assert_eq!(
        rejection("cons-empty-head.purs", &source("", "bc")),
        "NoInstanceFound"
    );
}
