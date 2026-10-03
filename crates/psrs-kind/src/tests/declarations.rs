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
fn checks_kind_of_a_typed_pattern_annotation() {
    let errors = check("module Main where\nbad (value :: Array) = value\n");
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
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
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}

#[test]
fn reports_a_partially_applied_function_synonym() {
    let errors = check("module Main where\nnewtype N = N ((~>) Array)\n");
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
        match kind {
            Kind::Variable(variable) => out.push(*variable),
            Kind::App(head, argument) | Kind::Function(head, argument) => {
                free_variables(head, out);
                free_variables(argument, out);
            }
            Kind::Builtin(_) | Kind::Named(_) => {}
        }
    }
    let mut unbound = Vec::new();
    free_variables(&phantom.kind, &mut unbound);
    unbound.sort_unstable();
    assert_eq!(unbound, phantom.variables, "{phantom:?}");
}
