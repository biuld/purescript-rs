use crate::check_module;
use crate::kind::KindDiagnostic;
use psrs_hir::ModuleId;

fn check(source: &str) -> Vec<KindDiagnostic> {
    let file = psrs_span::SourceFile::new("Test.purs", source);
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    let module = psrs_ast::lower_module(cst).expect("surface lowering");
    let intrinsics = psrs_resolve::bootstrap_externals();
    let resolved = psrs_resolve::resolve_module_with_externals(module, ModuleId(0), &intrinsics)
        .expect("resolve");
    check_module(&resolved)
}

fn check_ok(source: &str) {
    let errors = check(source);
    assert!(errors.is_empty(), "unexpected kind errors: {errors:?}");
}

fn codes(errors: &[KindDiagnostic]) -> Vec<&'static str> {
    errors.iter().map(|error| error.code).collect()
}

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
