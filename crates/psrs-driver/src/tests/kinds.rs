use super::*;

fn kind_codes(source: &str) -> Vec<&'static str> {
    check_program_kinds_lenient(&[("Main.purs", source)])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

#[test]
fn reports_a_kinds_do_not_unify_error() {
    let codes = kind_codes("module Main where\ndata KindError f a = One f | Two (f a)\n");
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn reports_a_partially_applied_synonym() {
    let codes = kind_codes("module Main where\ntype F x y = x -> y\ntype G x = F x\n");
    assert!(codes.contains(&"PartiallyAppliedSynonym"), "{codes:?}");
}

#[test]
fn reports_an_infinite_kind() {
    let codes = kind_codes("module Main where\ndata F a = F (a a)\n");
    assert!(codes.contains(&"InfiniteKind"), "{codes:?}");
}

#[test]
fn reports_a_type_synonym_cycle() {
    let codes = kind_codes("module Main where\ntype T = T\n");
    assert!(codes.contains(&"CycleInTypeSynonym"), "{codes:?}");
}

#[test]
fn reports_a_kind_declaration_cycle() {
    let codes = kind_codes(
        "module Main where\ndata Foo :: Bar -> Type\ndata Foo a = Foo\ndata Bar :: Foo -> Type\ndata Bar a = Bar\n",
    );
    assert!(codes.contains(&"CycleInKindDeclaration"), "{codes:?}");
}

#[test]
fn reports_an_undefined_type_variable() {
    let codes = kind_codes("module Main where\nfoo :: Array a\nfoo = 1\n");
    assert!(codes.contains(&"UndefinedTypeVariable"), "{codes:?}");
}

#[test]
fn accepts_well_kinded_higher_kinded_declarations() {
    let codes = kind_codes("module Main where\ndata Compose f g a = Compose (f (g a))\n");
    assert!(codes.is_empty(), "{codes:?}");
}
