use crate::check_module;
use crate::kind::KindDiagnostic;
use psrs_hir::ModuleId;

fn check(source: &str) -> Vec<KindDiagnostic> {
    let resolved = resolve(source);
    check_module(&resolved)
}

fn resolve(source: &str) -> psrs_hir::Module {
    let file = psrs_span::SourceFile::new("Test.purs", source);
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    let module = psrs_ast::lower_module(cst).expect("surface lowering");
    let intrinsics = psrs_resolve::bootstrap_externals();
    psrs_resolve::resolve_module_with_externals(module, ModuleId(0), &intrinsics).expect("resolve")
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

#[test]
fn infers_representational_phantom_and_nominal_roles() {
    let module = resolve(
        "module Main where\n\
         data Box a = Box a\n\
         data Phantom a = Phantom\n\
         data Nominal f a = Nominal (f a)\n\
         data ArrayField a = ArrayField (Array a)\n\
         data FunctionField a = FunctionField (a -> Int)\n\
         data Outer a = Outer (Inner a)\n\
         data Inner a = Inner a\n",
    );
    let (environment, errors) = crate::check_roles(std::slice::from_ref(&module));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    let roles = |name: &str| {
        let declaration = module
            .types
            .iter()
            .find(|declaration| declaration.name == name)
            .expect("type declaration");
        environment.roles(declaration.id).expect("inferred roles")
    };
    assert_eq!(roles("Box"), [psrs_hir::Role::Representational]);
    assert_eq!(roles("Phantom"), [psrs_hir::Role::Phantom]);
    assert_eq!(
        roles("Nominal"),
        [psrs_hir::Role::Representational, psrs_hir::Role::Nominal]
    );
    assert_eq!(roles("ArrayField"), [psrs_hir::Role::Representational]);
    assert_eq!(roles("FunctionField"), [psrs_hir::Role::Representational]);
    assert_eq!(roles("Outer"), [psrs_hir::Role::Representational]);
    assert_eq!(roles("Inner"), [psrs_hir::Role::Representational]);
}

#[test]
fn explicit_roles_may_restrict_but_not_weaken_inferred_roles() {
    let valid = resolve(
        "module Main where\n\
         data Box a = Box a\n\
         type role Box nominal\n",
    );
    let (environment, errors) = crate::check_roles(std::slice::from_ref(&valid));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    assert_eq!(
        environment.roles(valid.types[0].id),
        Some([psrs_hir::Role::Nominal].as_slice())
    );

    let invalid = resolve(
        "module Main where\n\
         data Box a = Box a\n\
         type role Box phantom\n",
    );
    let (_, errors) = crate::check_roles(&[invalid]);
    assert!(
        errors.iter().any(|(_, error)| error.code == "RoleMismatch"),
        "weakened role annotation must fail: {errors:?}"
    );
}

#[test]
fn checks_foreign_role_arity_from_the_declared_kind() {
    let module = resolve(
        "module Main where\n\
         foreign import data Wrapper :: Type -> Type\n\
         type role Wrapper representational\n",
    );
    let (environment, errors) = crate::check_roles(std::slice::from_ref(&module));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    assert_eq!(
        environment.roles(module.types[0].id),
        Some([psrs_hir::Role::Representational].as_slice())
    );

    let wrong_arity = resolve(
        "module Main where\n\
         foreign import data Wrapper :: Type -> Type\n\
         type role Wrapper nominal phantom\n",
    );
    let (_, errors) = crate::check_roles(&[wrong_arity]);
    assert!(
        errors
            .iter()
            .any(|(_, error)| error.code == "RoleDeclarationArityMismatch"),
        "role arity must follow foreign kind arrows: {errors:?}"
    );
}

#[test]
fn unannotated_foreign_roles_are_conservative() {
    let module = resolve(
        "module Main where\n\
         foreign import data Opaque :: Type -> Type\n",
    );
    let (environment, errors) = crate::check_roles(std::slice::from_ref(&module));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    assert_eq!(
        environment.roles(module.types[0].id),
        Some([psrs_hir::Role::Nominal].as_slice())
    );
}

#[test]
fn expands_type_synonyms_inside_data_fields_before_inferring_roles() {
    let module = resolve(
        "module Main where\n\
         type Alias a = a\n\
         data Box a = Box (Alias a)\n",
    );
    let (environment, errors) = crate::check_roles(std::slice::from_ref(&module));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    let box_type = module
        .types
        .iter()
        .find(|declaration| declaration.name == "Box")
        .expect("Box declaration");
    assert_eq!(
        environment.roles(box_type.id),
        Some([psrs_hir::Role::Representational].as_slice())
    );
}

#[test]
fn synonym_expansion_is_capture_avoiding_and_cycle_safe() {
    let capture = resolve(
        "module Main where\n\
         type Alias a = forall b. a -> b\n\
         data Box b = Box (Alias b)\n",
    );
    let (environment, errors) = crate::check_roles(std::slice::from_ref(&capture));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    let box_type = capture
        .types
        .iter()
        .find(|declaration| declaration.name == "Box")
        .expect("Box declaration");
    assert_eq!(
        environment.roles(box_type.id),
        Some([psrs_hir::Role::Representational].as_slice())
    );

    let cycle = resolve(
        "module Main where\n\
         type First a = Second a\n\
         type Second a = First a\n\
         data Box a = Box (First a)\n",
    );
    let (environment, _) = crate::check_roles(std::slice::from_ref(&cycle));
    let box_type = cycle
        .types
        .iter()
        .find(|declaration| declaration.name == "Box")
        .expect("Box declaration");
    assert_eq!(
        environment.roles(box_type.id),
        Some([psrs_hir::Role::Nominal].as_slice())
    );
}
