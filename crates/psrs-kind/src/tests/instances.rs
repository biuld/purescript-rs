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
fn accepts_a_partially_applied_function_arrow() {
    // `(->)` is a builtin type constructor, not a synonym, so a partial
    // application read on its own is valid: `purs` accepts `type Reader = (->) r`
    // and `Free ((->) Unit)`. Only a named synonym is `PartiallyAppliedSynonym`.
    check_ok(concat!(
        "module Main where\n",
        "type Reader = (->) Int\n",
        "type ReaderString = (->) Int String\n",
        "newtype B = B ((->) Boolean Int)\n",
    ));
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

#[test]
fn checks_an_instance_head_whose_class_is_declared_in_another_module() {
    // `failing/FoldableInstance1.purs`: `Foldable :: (Type -> Type) ->
    // Constraint` is declared in `Data.Foldable`, and the instance is in
    // `Main`, so only the instance's own module sees `Foldable`'s checked
    // scheme. `Foo` has kind `Type` and the head applies the class to it
    // directly, so the head is refused.
    let modules = resolve_program(&[
        (
            "Main.purs",
            "module Main where\n\
             import Data.Foldable\n\
             data Foo = Bar\n\
             instance foldableFoo :: Foldable Foo\n",
        ),
        (
            "Data.Foldable.purs",
            "module Data.Foldable where\n\
             class Foldable :: (Type -> Type) -> Constraint\n\
             class Foldable f where\n  foldr :: forall a b. (a -> b) -> b -> f a -> b\n",
        ),
    ]);
    let errors = check_modules(&modules);
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
    // The head that applies the class is `Main`'s syntax, so the diagnostic is
    // attributed to the module that declares the instance, not to the module
    // whose scheme was consulted.
    let error = errors
        .iter()
        .find(|error| error.code == "KindsDoNotUnify")
        .expect("a kind conflict");
    assert_eq!(error.origin, ModuleId(0), "{errors:?}");
}

#[test]
fn accepts_two_instance_heads_whose_class_is_declared_in_another_module() {
    // A cross-module class is checked against the scheme its declaring module
    // checked, at every use instantiating that scheme, so two heads at
    // different eligible kinds are both accepted — one head must not fix the
    // kind another is judged by.
    check_modules_ok(&[
        (
            "Main.purs",
            "module Main where\n\
             import Data.Proxy\n\
             instance int :: M Int\n\
             instance label :: M \"hello\"\n",
        ),
        (
            "Data.Proxy.purs",
            "module Data.Proxy where\n\
             class M :: forall k. k -> Constraint\n\
             class M a\n",
        ),
    ]);
}

#[test]
fn leaves_a_cross_module_head_with_the_wrong_arity_to_the_arity_rule() {
    // The arity rule reads a foreign class through its checked kind scheme, so
    // a head that applies a two-parameter class to one argument is still an
    // arity error, not a kind error.
    check_modules_ok(&[
        (
            "Main.purs",
            "module Main where\n\
             import Data.Proxy\n\
             instance tooNarrow :: M Int\n",
        ),
        (
            "Data.Proxy.purs",
            "module Data.Proxy where\n\
             class M a b\n",
        ),
    ]);
}

/// Kind checks a resolved multi-module program and requires no diagnostics.
pub(super) fn check_modules_ok(sources: &[(&str, &str)]) {
    let modules = resolve_program(sources);
    let errors = check_modules(&modules);
    assert!(errors.is_empty(), "unexpected kind errors: {errors:?}");
}
