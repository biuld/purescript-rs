use super::*;

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
    let (environment, errors) = check_env(std::slice::from_ref(&module));
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
    let (environment, errors) = check_env(std::slice::from_ref(&valid));
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
    let (_, errors) = check_env(&[invalid]);
    assert!(
        errors.iter().any(|error| error.code == "RoleMismatch"),
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
    let (environment, errors) = check_env(std::slice::from_ref(&module));
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
    let (_, errors) = check_env(&[wrong_arity]);
    assert!(
        errors
            .iter()
            .any(|error| error.code == "RoleDeclarationArityMismatch"),
        "role arity must follow foreign kind arrows: {errors:?}"
    );
}

#[test]
fn unannotated_foreign_roles_are_conservative() {
    let module = resolve(
        "module Main where\n\
         foreign import data Opaque :: Type -> Type\n",
    );
    let (environment, errors) = check_env(std::slice::from_ref(&module));
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
    let (environment, errors) = check_env(std::slice::from_ref(&module));
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
    let (environment, errors) = check_env(std::slice::from_ref(&capture));
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
    let (environment, _) = check_env(std::slice::from_ref(&cycle));
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

#[test]
fn primitive_registry_roles_reach_the_checked_kind_environment() {
    // The registry's foreign types carry trusted role signatures, so this also
    // proves they survive the shared role check: a wrong arity would report
    // `RoleDeclarationArityMismatch` and a weakened role `RoleMismatch`.
    let module = resolve("module Main where\ndata Box a = Box a\n");
    let (environment, errors) = check_env(std::slice::from_ref(&module));
    assert!(errors.is_empty(), "unexpected role errors: {errors:?}");
    let phantom = psrs_hir::Role::Phantom;
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_ROW_LIST),
        Some([phantom].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_ROW_LIST_CONS),
        Some([phantom, phantom, phantom].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_TYPE_ERROR_TEXT),
        Some([phantom].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_TYPE_ERROR_QUOTE),
        Some([phantom].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_TYPE_ERROR_QUOTE_LABEL),
        Some([phantom].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_TYPE_ERROR_BESIDE),
        Some([phantom, phantom].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_TYPE_ERROR_ABOVE),
        Some([phantom, phantom].as_slice())
    );
    // A member with no parameter carries no role at all.
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_ROW_LIST_NIL),
        Some([].as_slice())
    );
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_TYPE_ERROR_DOC),
        Some([].as_slice())
    );
    // `Prim.Boolean`'s members are declarations like any other, so they are
    // present in the environment even though they take no parameter.
    assert_eq!(
        environment.roles(psrs_hir::TypeId::PRIM_BOOLEAN_FALSE),
        Some([].as_slice())
    );
}
