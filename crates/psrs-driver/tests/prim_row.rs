//! `Prim.Row.Cons`, `Prim.Row.Nub`, and `Prim.RowList.RowToList` through the whole
//! front end.
//!
//! `purs` 0.15.16 is the oracle for every case below, and the code each rejected
//! case is pinned against is the code `purs` raises for it. All three members are
//! declared by the compiler itself, so a case needs nothing but `Prim` and
//! `Prim.Row` — and no standard library, which matters because a closed row's
//! empty tail is a spine node here while `purs` spells it `Unit` from `Data.Unit`.
//!
//! The cases cover each relation's positive reading, the decline, and the
//! disagreement, and they use the shapes library code uses: `Cons` in the
//! `lookup :: Cons sym v rx r => Proxy sym -> Proxy r -> Proxy v` shape the
//! corpus's `PolykindRowCons.purs` has, and the open rows a `Nub` or a
//! `RowToList` cannot decide.
//!
//! Every rejection here comes from the shared unifier rather than from a rule
//! restating it: a rule states what it decided in the relation's dictionary, and
//! the framework unifies that against the goal, which is the step official
//! solving takes on every dictionary it produces.

use psrs_driver::{Diagnostic, check_source};

fn accepts(source_name: &str, source: &str) {
    if let Err(errors) = check_source(source_name, source) {
        panic!("{source_name} was rejected: {errors:#?}");
    }
}

/// The official code of every diagnostic the driver reports, in order. A case
/// states what `purs` raises, which is the whole claim: the diagnostic a rule
/// reports is the same rejection `purs` makes.
fn rejection(source_name: &str, source: &str) -> &'static str {
    let errors = check_source(source_name, source).expect_err("the case must be rejected");
    let codes: Vec<&'static str> = errors
        .iter()
        .map(|error| error.code.unwrap_or("<none>"))
        .collect();
    assert_eq!(
        codes.len(),
        1,
        "{source_name} reports {codes:?} rather than one diagnostic"
    );
    codes[0]
}

/// The part of the only diagnostic a case reports that says which relation
/// failed, so a case pins the reason and not only the code.
fn rejection_message(source_name: &str, source: &str) -> String {
    let errors: Vec<Diagnostic> =
        check_source(source_name, source).expect_err("the case must be rejected");
    errors[0].message.clone()
}

/// The part of every case that shares it: a kind-polymorphic proxy for a
/// `Symbol` or a value, a row-polymorphic proxy for a row, and the two `Prim`
/// classes. A proxy over `Row Type` rather than over `Type` is what keeps a bare
/// row and a `Record` apart, which is the distinction all three relations read.
const ROW_PROXY: &str = r#"data SProxy :: forall k. k -> Type
data SProxy a = SProxy

data RProxy :: Row Type -> Type
data RProxy r = RProxy
"#;

/// `Cons`'s one reading: a known label builds the extension the relation names.
/// The shape is the corpus's own — a label, a value, and a row the caller
/// supplies — so the rule decides the tail and the extension together.
#[test]
fn cons_builds_the_extension_from_a_known_label() {
    accepts(
        "cons-lookup-positive.purs",
        &format!(
            "module Main where\n\nimport Prim\nimport Prim.Row (class Cons)\n{ROW_PROXY}\
             lookup :: forall s v rx r. Cons s v rx r => SProxy s -> RProxy r -> SProxy v\n\
             lookup _ _ = SProxy\n\
             test :: SProxy Number\n\
             test = lookup (SProxy :: SProxy \"a\") (RProxy :: RProxy ( a :: Number | (b :: Boolean) ))\n"
        ),
    );
}

/// A closed tail is decided as readily as an open one: official `solveRowCons`
/// does not read the tail, so `( b :: Number )` is an ordinary tail rather than a
/// reason to decline. This is the second of `Cons`'s two fundeps — `label row`
/// determine `tail` and `value` — reading its known tail as the extension's.
#[test]
fn cons_builds_an_extension_over_a_closed_tail() {
    accepts(
        "cons-closed-tail.purs",
        &format!(
            "module Main where\n\nimport Prim\nimport Prim.Row (class Cons)\n{ROW_PROXY}\
             cons :: forall r. Cons \"a\" Number (b :: Number) r => RProxy r -> RProxy (a :: Number | (b :: Number))\n\
             cons _ = RProxy\n\
             test :: RProxy (a :: Number | (b :: Number))\n\
             test = cons (RProxy :: RProxy (a :: Number | (b :: Number)))\n"
        ),
    );
}

/// An unknown label is not an obligation this relation can answer, and the rule
/// says so rather than naming a field the program never wrote. `purs` reports
/// `NoInstanceFound` for `Prim.Row.Cons s4 Number t2 (a, b)`.
#[test]
fn cons_declines_an_unknown_label() {
    assert_eq!(
        rejection(
            "cons-decline.purs",
            &format!(
                "module Main where\n\nimport Prim\nimport Prim.Row (class Cons)\n{ROW_PROXY}\
                 lookup :: forall s v rx r. Cons s v rx r => SProxy s -> RProxy r -> SProxy v\n\
                 lookup _ _ = SProxy\n\
                 test :: forall s. SProxy s -> SProxy Number\n\
                 test sym = lookup sym (RProxy :: RProxy ( a :: Number | (b :: Boolean) ))\n"
            ),
        ),
        "NoInstanceFound"
    );
}

/// A decided extension that contradicts a row already known to be something else
/// cannot hold, and the code is `purs`'s: the row the rule built does not unify
/// with the wanted one. The diagnostic is the shared row unifier's own — the
/// framework runs that step, not the rule — so it names the label the wanted row
/// lacks rather than restating the relation.
#[test]
fn cons_rejects_a_decided_extension_that_does_not_unify() {
    let source = format!(
        "module Main where\n\nimport Prim\nimport Prim.Row (class Cons)\n{ROW_PROXY}\
         cons :: forall r. Cons \"a\" Number (b :: Number) r => RProxy r -> RProxy (a :: Number)\n\
         cons _ = RProxy\n\
         test :: RProxy (a :: Number)\n\
         test = cons (RProxy :: RProxy (a :: Number))\n"
    );
    assert_eq!(rejection("cons-mismatch.purs", &source), "TypesDoNotUnify");
    assert_eq!(
        rejection_message("cons-mismatch.purs", &source),
        "record has no field `b`"
    );
}

/// A decided extension that contradicts the wanted row while the tail is still
/// unknown is a contradiction, not an undecided obligation: the label and the
/// wanted row are both determined, so the shared unifier finds the disagreement
/// and reports it. `purs` reports `TypesDoNotUnify` here too, and this case used
/// to be the one place the two differed — the rule reported `Failed`, the
/// framework refused it because the tail was still unknown, and the obligation
/// reached instance search as a missing instance instead.
#[test]
fn cons_reports_a_contradiction_while_the_tail_is_unknown() {
    assert_eq!(
        rejection(
            "cons-unknown-tail-mismatch.purs",
            &format!(
                "module Main where\n\nimport Prim\nimport Prim.Row (class Cons)\n{ROW_PROXY}\
                 lookup :: forall s v rx r. Cons s v rx r => SProxy s -> RProxy r -> SProxy v\n\
                 lookup _ _ = SProxy\n\
                 test :: SProxy Number\n\
                 test = lookup (SProxy :: SProxy \"a\") (RProxy :: RProxy (b :: Boolean))\n"
            ),
        ),
        "TypesDoNotUnify"
    );
}

/// `Nub`'s positive reading: a closed row is canonicalised, so the row's labels
/// come back in ascending order however the caller wrote them. This is the
/// obligation `Type.Data.Record.nub` is defined by, and the reason a caller can
/// nub a row and then index it by label.
#[test]
fn nub_canonicalizes_a_closed_row() {
    accepts(
        "nub-canonical.purs",
        &format!(
            "module Main where\n\nimport Prim\nimport Prim.Row (class Nub)\n{ROW_PROXY}\
             nub :: forall r n. Nub r n => RProxy r -> RProxy n\n\
             nub _ = RProxy\n\
             test :: RProxy (a :: Number | (b :: Boolean))\n\
             test = nub (RProxy :: RProxy (b :: Boolean | (a :: Number)))\n"
        ),
    );
}

/// An open row has no nubbed form the rule can state, because the answer depends
/// on the labels inside the tail. `purs` reports `NoInstanceFound` for
/// `Prim.Row.Nub ( a :: Number | r0 ) ( a :: Number | r0 )`.
#[test]
fn nub_declines_an_open_row() {
    assert_eq!(
        rejection(
            "nub-open.purs",
            &format!(
                "module Main where\n\nimport Prim\nimport Prim.Row (class Nub)\n{ROW_PROXY}\
                 nub :: forall r. Nub r r => RProxy r -> RProxy r\n\
                 nub _ = RProxy\n\
                 test :: forall r. RProxy ( a :: Number | r )\n\
                 test = nub (RProxy :: RProxy ( a :: Number | r ))\n"
            ),
        ),
        "NoInstanceFound"
    );
}

/// A nubbed row that is not the row's own canonical form cannot hold, and the
/// diagnostic is the shared row unifier's: the canonical form the rule decided
/// carries the label the wanted row lacks.
#[test]
fn nub_rejects_a_nubbed_row_that_is_not_the_canonical_form() {
    let source = format!(
        "module Main where\n\nimport Prim\nimport Prim.Row (class Nub)\n{ROW_PROXY}\
         nub :: forall r n. Nub r n => RProxy r -> RProxy n\n\
         nub _ = RProxy\n\
         test :: RProxy (b :: Boolean)\n\
         test = nub (RProxy :: RProxy (a :: Number))\n"
    );
    assert_eq!(rejection("nub-mismatch.purs", &source), "TypesDoNotUnify");
    assert_eq!(
        rejection_message("nub-mismatch.purs", &source),
        "record has no field `a`"
    );
}

/// `RowToList`'s positive reading: a closed row becomes the `RowList` spine of
/// `Cons` over `Nil`, in the same ascending label order `Nub` uses. This is the
/// obligation `Type.Data.Symbol` and `Record` key lookups reach when a key is a
/// type-level `Symbol`.
#[test]
fn a_closed_row_converts_to_a_row_list() {
    accepts(
        "row-to-list.purs",
        "module Main where\n\nimport Prim\nimport Prim.RowList (class RowToList, RowList, Cons, Nil)\n\
         data RProxy :: Row Type -> Type\ndata RProxy r = RProxy\n\
         data LProxy :: RowList Type -> Type\ndata LProxy l = LProxy\n\
         toList :: forall r l. RowToList r l => RProxy r -> LProxy l\n\
         toList _ = LProxy\n\
         test :: LProxy (Cons \"a\" Number (Cons \"b\" Boolean Nil))\n\
         test = toList (RProxy :: RProxy (a :: Number | (b :: Boolean)))\n",
    );
}

/// An open row declines for the same reason `Nub` declines one: the list would
/// have to name the labels inside the tail.
#[test]
fn row_to_list_declines_an_open_row() {
    assert_eq!(
        rejection(
            "row-to-list-open.purs",
            "module Main where\n\nimport Prim\nimport Prim.RowList (class RowToList, RowList, Cons, Nil)\n\
             data RProxy :: Row Type -> Type\ndata RProxy r = RProxy\n\
             data LProxy :: RowList Type -> Type\ndata LProxy l = LProxy\n\
             toList :: forall r l. RowToList r l => RProxy r -> LProxy l\n\
             toList _ = LProxy\n\
             test :: forall r. RProxy ( a :: Number | r ) -> LProxy (Cons \"a\" Number Nil)\n\
             test = toList\n"
        ),
        "NoInstanceFound"
    );
}

/// A list that is not the row's own conversion cannot hold. `purs` rejects this
/// while solving the relation, on the label the conversion decided, and so does
/// the shared unifier the framework runs that step through.
#[test]
fn row_to_list_rejects_a_list_that_is_not_the_conversion() {
    let source = "module Main where\n\nimport Prim\nimport Prim.RowList (class RowToList, RowList, Cons, Nil)\n\
         data RProxy :: Row Type -> Type\ndata RProxy r = RProxy\n\
         data LProxy :: RowList Type -> Type\ndata LProxy l = LProxy\n\
         toList :: forall r l. RowToList r l => RProxy r -> LProxy l\n\
         toList _ = LProxy\n\
         test :: LProxy (Cons \"b\" Boolean (Cons \"a\" Number Nil))\n\
         test = toList (RProxy :: RProxy (a :: Number | (b :: Boolean)))\n";
    assert_eq!(
        rejection("row-to-list-mismatch.purs", source),
        "TypesDoNotUnify"
    );
    assert_eq!(
        rejection_message("row-to-list-mismatch.purs", source),
        "type mismatch: expected \"a\", found \"b\""
    );
}
