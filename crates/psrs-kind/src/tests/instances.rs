use super::*;

#[test]
fn checks_an_instance_head_against_its_class_kind_signature() {
    // `purs` reports KindsDoNotUnify for this. The class signature is the only
    // thing that can reject the head: without one the class is inferred as
    // `Type -> Constraint`, which accepts any argument.
    let errors = check(
        "module Main where\n\
         class C :: Constraint -> Constraint\n\
         class C a\n\
         instance cInt :: C Int\n",
    );
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn checks_an_instance_head_against_a_shared_kind_parameter() {
    // `failing/StandaloneKindSignatures4.purs`: `Int` fixes the shared parameter
    // at `Type`, and a string literal denotes a symbol.
    let errors = check(
        "module Main where\n\
         class To :: forall k. k -> k -> Constraint\n\
         class To a b | a -> b\n\
         instance toIntString :: To Int \"foo\"\n",
    );
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn accepts_an_instance_head_whose_class_needs_no_signature() {
    check_ok(
        "module Main where\n\
         class C a\n\
         instance cInt :: C Int\n\
         data Wrap a = Wrap a\n\
         class F f\n\
         instance fWrap :: F Wrap\n",
    );
}

#[test]
fn accepts_the_function_constructor_as_an_instance_argument() {
    // `Functor ((->) r)` is the standard library's own instance and `purs`
    // accepts it. `(->) r` is the type constructor applied once, which in a
    // position read on its own is the partially applied function synonym of
    // `failing/TypeSynonyms9.purs`; as an instance argument it is checked
    // against the class's declared parameter kind instead.
    check_ok(concat!(
        "module Main where\n",
        "class Functor f where\n",
        "  map :: forall a b. (a -> b) -> f a -> f b\n",
        "\n",
        "instance functorFunction :: Functor ((->) r) where\n",
        "  map f g = \\value -> f (g value)\n",
        "\n",
        "class F p\n",
        "instance fFunction :: F (->)\n",
    ));
}

#[test]
fn still_reports_a_partially_applied_function_synonym_in_a_type() {
    // The same shape as above, but standing alone as a newtype field, where it
    // is not checked against a class kind. `purs` reports
    // PartiallyAppliedSynonym for `failing/TypeSynonyms9.purs`.
    let errors = check(
        "module Main where\n\
         newtype A (a :: (Type -> Type) -> Type -> Type) = A String\n\
         newtype B = B (A ((->) Array))\n",
    );
    assert!(
        codes(&errors).contains(&"PartiallyAppliedSynonym"),
        "{errors:?}"
    );
}

#[test]
fn leaves_an_instance_head_with_the_wrong_arity_to_the_arity_rule() {
    // An arity mismatch is `ClassInstanceArityMismatch`, which the class
    // environment reports. Reporting `KindsDoNotUnify` here as well would
    // pre-empt that rule, so the kind check steps aside.
    check_ok(
        "module Main where\n\
         class TooWide a b\n\
         instance tooWide :: TooWide Int Int Int\n\
         class TooNarrow a b\n\
         instance tooNarrow :: TooNarrow Int\n",
    );
}
