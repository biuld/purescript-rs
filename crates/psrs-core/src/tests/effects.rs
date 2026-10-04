//! Fixtures for the effect representation closures written by `lower_effects`.
//!
//! The pass is the only one that recognizes the opaque `Prelude.Effect`
//! application, so it also owns the check that every closure it wrote takes the
//! runtime token alone and returns the lowered effect result.

use super::*;
use crate::effect::{EffectClosure, TrustedEffect, classify_effect_import, lower_effects};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};

const SPAN: TextRange = TextRange::new(0, 8);

const TOKEN: TypeId = TypeId(0);
const A: TypeId = TypeId(1);
const B: TypeId = TypeId(2);
const ARROW: TypeId = TypeId(5);
const EFFECT: TypeId = TypeId(6);
const EFFECT_APPLICATION: TypeId = TypeId(7);
const EFFECT_HIR: HirTypeId = HirTypeId::new(ModuleId(0), 3);

fn trusted_effect() -> TrustedEffect {
    TrustedEffect {
        effect_type: EFFECT_HIR,
        operations: Vec::new(),
    }
}

/// A linked module whose only effect is `Effect (a -> b)`.
///
/// Its type table is
///
/// | id | node |
/// | --- | --- |
/// | 0 | `Int`, the token representation lowering chose |
/// | 1, 2 | `a`, `b` |
/// | 3, 4, 5 | `a -> b` |
/// | 6 | `Prelude.Effect` |
/// | 7 | `Effect (a -> b)`, the node lowering replaces |
fn effect_module() -> Module {
    let effect = EFFECT_HIR;
    Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Variable(TypeVariableId(0)),
            Type::Variable(TypeVariableId(1)),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(3), A),
            Type::Application(TypeId(4), B),
            Type::Constructor(TypeConstructor::User(effect)),
            Type::Application(EFFECT, ARROW),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: vec![effect],
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        type_names: vec![(effect, "Prelude.Effect".into())],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: SPAN,
    }
}

/// `Effect (a -> b)` lowers to one token parameter and keeps `a -> b` whole as
/// its result, so the accepted path stays accepted.
#[test]
fn lowered_effect_closures_are_one_token_closure_over_the_effect_result() {
    let mut module = effect_module();
    let lowering =
        lower_effects(&mut module, &trusted_effect()).expect("the written closure keeps its shape");
    assert_eq!(
        lowering.closures,
        vec![EffectClosure {
            ty: EFFECT_APPLICATION,
            token: TOKEN,
            result: ARROW,
        }]
    );
    assert_eq!(
        closure_parts(&module.types, EFFECT_APPLICATION),
        Some(([TOKEN].as_slice(), ARROW)),
        "Effect (a -> b) takes the token alone and returns the function whole"
    );
    assert!(lowering.verify(&module).is_ok());
    assert!(
        module.verify().is_ok(),
        "{:?}",
        module.verify().unwrap_err()
    );
}

/// The negative fixture the obligation names: the closure for `Effect (a -> b)`
/// flattened to two parameters `[Token, a]` is not a representation closure.
/// The general Core verifier accepts that type table, so the rejection is this
/// pass's own check and it happens before closure conversion.
#[test]
fn a_lowered_effect_closure_flattened_to_arity_two_is_rejected() {
    let mut module = effect_module();
    let lowering = lower_effects(&mut module, &trusted_effect())
        .expect("the written closure starts in its shape");
    module.types[EFFECT_APPLICATION.0 as usize] = Type::Closure {
        parameters: vec![TOKEN, A],
        result: ARROW,
    };
    assert!(
        module.verify().is_ok(),
        "{:?}",
        module.verify().unwrap_err()
    );
    assert_eq!(
        messages(lowering.verify(&module)),
        ["a lowered effect closure must take only the runtime token"]
    );
}

/// A closure that takes the token but returns something other than the lowered
/// effect result is not a representation closure either.
#[test]
fn a_lowered_effect_closure_with_the_wrong_result_is_rejected() {
    let mut module = effect_module();
    let lowering = lower_effects(&mut module, &trusted_effect())
        .expect("the written closure starts in its shape");
    module.types[EFFECT_APPLICATION.0 as usize] = Type::Closure {
        parameters: vec![TOKEN],
        result: B,
    };
    assert_eq!(
        messages(lowering.verify(&module)),
        ["a lowered effect closure must return the lowered effect result"]
    );
}

/// A node that is not a closure at all cannot be the representation of an
/// effect either.
#[test]
fn a_lowered_effect_that_is_not_a_closure_is_rejected() {
    let mut module = effect_module();
    let lowering = lower_effects(&mut module, &trusted_effect())
        .expect("the written closure starts in its shape");
    module.types[EFFECT_APPLICATION.0 as usize] = Type::Application(EFFECT, ARROW);
    let errors = lowering
        .verify(&module)
        .expect_err("an application is not a closure representation");
    assert_eq!(
        errors[0].message,
        "a lowered effect application is not a closure"
    );
    assert_eq!(errors[0].module, module.id);
    assert_eq!(errors[0].span, module.span);
}

/// A module without the opaque library type is left alone and passes the check.
#[test]
fn lowering_rejects_missing_trusted_effect_identity() {
    let mut module = effect_module();
    module.type_names.clear();
    module.opaque_ids.clear();
    let errors = lower_effects(&mut module, &trusted_effect())
        .expect_err("missing trusted identity metadata is an error");
    assert_eq!(
        errors[0].message,
        "trusted Effect identity is missing or is not an opaque type"
    );
}

#[test]
fn effect_import_classification_rejects_a_cyclic_arrow_result() {
    let mut module = effect_module();
    module.types = vec![
        Type::Constructor(TypeConstructor::Function),
        Type::Constructor(TypeConstructor::Int),
        Type::Application(TypeId(0), TypeId(1)),
        Type::Application(TypeId(2), TypeId(3)),
    ];
    let error = classify_effect_import(&module, TypeId(3), EFFECT_HIR)
        .expect_err("a recursive arrow spine is malformed");
    assert_eq!(error, "cyclic type spine in effect import signature");
}

#[test]
fn a_checked_foreign_effect_signature_must_quantify_its_type_variables() {
    let mut module = effect_module();
    let symbol = SymbolId::new(ModuleId(0), 9);
    let free = TypeId(module.types.len() as u32);
    module.types.push(Type::Variable(TypeVariableId(99)));
    module.externals.push(psrs_hir::ExternalSymbol {
        symbol,
        name: "foreignEffect".into(),
        kind: psrs_hir::ExternalKind::Wit {
            interface: "test:effect".into(),
            function: "foreign-effect".into(),
        },
        signature: None,
    });
    module.external_types.push(crate::ExternalType {
        symbol,
        source_module: module.id,
        ty: free,
    });

    let errors = module
        .verify()
        .expect_err("free external variables are invalid");
    assert!(
        errors
            .iter()
            .any(|error| { error.message == "type variable is outside its quantifier scope" })
    );
}

#[test]
fn another_foreign_effect_identity_is_not_the_trusted_effect_constructor() {
    let mut module = effect_module();
    let other_effect = HirTypeId::new(ModuleId(1), 3);
    let other = TypeId(module.types.len() as u32);
    module
        .types
        .push(Type::Constructor(TypeConstructor::User(other_effect)));
    let application = TypeId(module.types.len() as u32);
    module.types.push(Type::Application(other, A));
    module.opaque_ids.push(other_effect);
    module
        .type_names
        .push((other_effect, "Prelude.Effect".into()));

    assert_eq!(
        classify_effect_import(&module, application, EFFECT_HIR).unwrap(),
        None,
        "matching spelling and opacity do not substitute for trusted identity"
    );

    lower_effects(&mut module, &trusted_effect()).unwrap();
    assert!(matches!(
        module.types.get(application.0 as usize),
        Some(Type::Application(_, _))
    ));
    assert!(matches!(
        module.types.get(EFFECT_APPLICATION.0 as usize),
        Some(Type::Closure { .. })
    ));
}

#[test]
fn an_ordinary_closure_shaped_import_is_not_an_effect_import() {
    let mut module = effect_module();
    let ordinary = TypeId(module.types.len() as u32);
    module.types.push(Type::Closure {
        parameters: vec![A],
        result: B,
    });

    assert_eq!(
        classify_effect_import(&module, ordinary, EFFECT_HIR).unwrap(),
        None,
        "suspension is planned from the checked abstract Effect result, not closure shape"
    );
}

fn messages(result: Result<(), Vec<VerifyError>>) -> Vec<&'static str> {
    result
        .expect_err("the malformed closure must be rejected")
        .iter()
        .map(|error| error.message)
        .collect()
}
