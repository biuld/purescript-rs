use super::*;

#[test]
fn accepts_a_well_kinded_data_declaration() {
    check_ok("module Main where\ndata Maybe a = Nothing | Just a\n");
}

#[test]
fn infers_a_higher_kinded_parameter() {
    check_ok("module Main where\ndata Compose f g a = Compose (f (g a))\n");
}

#[test]
fn rejects_an_unconstrained_free_type_variable() {
    check_ok("module Main where\nfoo :: forall a. a -> a\nfoo x = x\n");
}

#[test]
fn reports_a_kind_mismatch_between_constructor_fields() {
    let errors = check("module Main where\ndata KindError f a = One f | Two (f a)\n");
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn reports_a_value_signature_of_a_non_type_kind() {
    // `failing/KindStar.purs`: a value signature's written type must have
    // kind `Type`; the official `checkTypeKind` failure is `ExpectedType`.
    let errors = check("module Main where\ndata List a\ntest :: List\ntest = unit\n");
    assert!(codes(&errors).contains(&"ExpectedType"), "{errors:?}");
}

#[test]
fn checks_kind_of_a_typed_pattern_annotation() {
    // A typed binder's written type must have kind `Type`; official
    // PureScript solves that equation through `checkTypeKind`, whose failure
    // is `ExpectedType` (`failing/1570.purs` names the same position).
    let errors = check("module Main where\nbad (value :: Array) = value\n");
    assert!(codes(&errors).contains(&"ExpectedType"), "{errors:?}");
}

#[test]
fn reports_an_infinite_kind() {
    let errors = check("module Main where\ndata F a = F (a a)\n");
    assert!(codes(&errors).contains(&"InfiniteKind"), "{errors:?}");
}

#[test]
fn reports_a_partially_applied_synonym() {
    let errors = check("module Main where\ntype F x y = x -> y\ntype G x = F x\n");
    assert!(
        codes(&errors).contains(&"PartiallyAppliedSynonym"),
        "{errors:?}"
    );
}

#[test]
fn reports_a_type_synonym_cycle() {
    let errors = check("module Main where\ntype T = T\n");
    assert!(codes(&errors).contains(&"CycleInTypeSynonym"), "{errors:?}");
}

#[test]
fn reports_a_kind_declaration_cycle() {
    let errors = check(
        "module Main where\ndata Foo :: Bar -> Type\ndata Foo a = Foo\ndata Bar :: Foo -> Type\ndata Bar a = Bar\n",
    );
    assert!(
        codes(&errors).contains(&"CycleInKindDeclaration"),
        "{errors:?}"
    );
}

#[test]
fn reports_an_undefined_type_variable_in_a_signature() {
    let errors = check("module Main where\nfoo :: Array a\nfoo = 1\n");
    assert!(
        codes(&errors).contains(&"UndefinedTypeVariable"),
        "{errors:?}"
    );
}

#[test]
fn reports_a_kind_mismatch_for_a_standalone_signature() {
    let errors = check(
        "module Main where\ndata Pair :: forall k. k -> k -> Type\ndata Pair a b = Pair\ntype T = Pair Int \"foo\"\n",
    );
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn accepts_a_nullary_and_higher_kinded_foreign_data_type() {
    check_ok(
        "module Main where\n\
         foreign import data Handle :: Type\n\
         foreign import data Effect :: Type -> Type\n\
         keep :: Handle -> Effect Int\n\
         keep h = h\n",
    );
}

#[test]
fn rejects_an_unsaturated_foreign_data_constructor() {
    let errors = check(
        "module Main where\n\
         foreign import data Effect :: Type -> Type\n\
         bad :: Effect\n\
         bad = 1\n",
    );
    // A value signature's written type must have kind `Type`, and the
    // official `checkTypeKind` failure at an annotation is `ExpectedType`.
    assert!(codes(&errors).contains(&"ExpectedType"), "{errors:?}");
}

#[test]
fn reports_a_partially_applied_function_synonym() {
    // `~>` aliases a type synonym; applying the bare operator to fewer
    // arguments than the synonym's arity is `PartiallyAppliedSynonym`. The
    // plain function arrow is a builtin constructor and is never this error.
    let errors = check(
        "module Main where\n\
         type Natural f g = forall a. f a -> g a\n\
         infixr 4 type Natural as ~>\n\
         newtype N = N ((~>) Array)\n",
    );
    assert!(
        codes(&errors).contains(&"PartiallyAppliedSynonym"),
        "{errors:?}"
    );
}

#[test]
fn a_scheme_quantifies_only_the_kinds_the_definition_leaves_undetermined() {
    use crate::{Kind, type_kind};
    let module = resolve(
        "module Main where\n\
         data Apply f = Apply (f Int)\n\
         data Phantom a = Phantom\n\
         data Branch m = Branch (m Branch)\n",
    );
    let (environment, diagnostics) = check_env(std::slice::from_ref(&module));
    // `Branch m = Branch (m Branch)` would need `m :: (m -> Type) -> Type`,
    // which is infinite, so the declaration is rejected rather than generalized.
    assert!(
        diagnostics.iter().any(|error| error.code == "InfiniteKind"),
        "{diagnostics:?}"
    );
    let scheme = |name: &str| {
        let id = module
            .types
            .iter()
            .find(|declaration| declaration.name == name)
            .expect("type declaration")
            .id;
        environment.kind_scheme(id).expect("a checked scheme")
    };

    // `Apply`'s own field `f Int` determines `f`'s kind, so the scheme keeps it
    // and quantifies nothing.
    let apply = scheme("Apply");
    assert_eq!(apply.variables, Vec::<u32>::new(), "{apply:?}");
    assert_eq!(
        apply.kind,
        Kind::Function(
            Box::new(Kind::Function(Box::new(type_kind()), Box::new(type_kind()))),
            Box::new(type_kind()),
        ),
        "{apply:?}"
    );

    // Nothing determines `Phantom`'s parameter, so the scheme quantifies exactly
    // that one variable and leaves nothing else free.
    let phantom = scheme("Phantom");
    assert_eq!(phantom.variables.len(), 1, "{phantom:?}");
    assert!(
        matches!(&phantom.kind, Kind::Function(parameter, _) if matches!(**parameter, Kind::Variable(_))),
        "{phantom:?}"
    );
    fn free_variables(kind: &Kind, out: &mut Vec<u32>) {
        fn walk(kind: &Kind, bound: &mut Vec<u32>, out: &mut Vec<u32>) {
            match kind {
                Kind::Variable(variable) if !bound.contains(variable) => out.push(*variable),
                Kind::Variable(_) => {}
                Kind::App(head, argument) | Kind::Function(head, argument) => {
                    walk(head, bound, out);
                    walk(argument, bound, out);
                }
                Kind::RowExtend { field, tail, .. } => {
                    walk(field, bound, out);
                    walk(tail, bound, out);
                }
                Kind::ForAll { variable, body } => {
                    bound.push(*variable);
                    walk(body, bound, out);
                    bound.pop();
                }
                Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty | Kind::TypeVariable(_) => {}
            }
        }
        walk(kind, &mut Vec::new(), out);
    }
    let mut unbound = Vec::new();
    free_variables(&phantom.kind, &mut unbound);
    unbound.sort_unstable();
    assert_eq!(unbound, phantom.variables, "{phantom:?}");
}

#[test]
fn a_polykinded_constructor_instantiates_at_each_use() {
    // `failing/PolykindGeneralizationLet.purs`. Nothing in `Proxy`'s
    // declaration determines its parameter, so each signature instantiates
    // a fresh kind. Sharing the raw variable would reject `Int` and `"foo"`.
    let errors = check(
        "module Main where\n\
         data Proxy a = Proxy\n\
         data F f a = F (f a)\n\
         fproxy :: forall f a. Proxy f -> Proxy a -> Proxy (F f a)\n\
         fproxy _ _ = Proxy\n\
         intProxy :: Proxy Int\n\
         intProxy = Proxy\n\
         symProxy :: Proxy \"foo\"\n\
         symProxy = Proxy\n",
    );
    assert!(!codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn a_shared_argument_cannot_have_two_row_kinds() {
    // `failing/3765-kinds.purs`. `{a :: Int | r}` and `{b :: Int | r}` share
    // `r` and differ in the known label, so one argument cannot be both.
    let errors = check(
        "module Main where\n\
         data Tricky :: forall r. {a :: Int | r} -> {b :: Int | r} -> Type\n\
         data Tricky x y = Tricky\n\
         type MkTricky x = Tricky x x\n",
    );
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn a_quantified_kind_does_not_escape_into_a_synonym() {
    // `passing/Rank2Kinds.purs`, without the effect import. The parameter
    // annotation stays a quantified kind, and each use of `f` instantiates it.
    let errors = check(
        "module Main where\n\
         data A (a :: forall k. k -> Type) = A\n\
         data B :: (forall k. k -> Type) -> Type\n\
         data B a = B\n\
         data Pair a b = Pair\n\
         data Proxy a = Proxy\n\
         type Id a = a\n\
         type MkP (f :: forall k. k -> k) = Pair (f Int) (f \"foo\")\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_quantified_kind_escapes_through_a_polykinded_proxy() {
    // `failing/SkolemEscapeKinds.purs`. `A`'s kind contains
    // `forall k. k -> Type` in a negative position, and `Proxy`'s parameter
    // is an inferred kind, so the synonym captures that quantifier.
    let errors = check(
        "module Main where\n\
         data Proxy a = Proxy\n\
         data A (a :: forall k. k -> Type) = A\n\
         type B = Proxy A\n",
    );
    assert!(codes(&errors).contains(&"EscapedSkolem"), "{errors:?}");
}
