//! Representation lowering for the library effect type.
//!
//! `Effect τ` stays an abstract application through type checking and Typed
//! Core. This pass is the only one that matches that type's identity. It
//! replaces each application with a closure whose parameter list is the
//! runtime token, it supplies bodies for `pure`, `bind`, and `runEffect`, and
//! it records every closure it wrote so the shape can be verified before any
//! later pass sees it.

mod operations;
mod supplies;

use crate::{Module, Type, TypeConstructor, TypeId, VerifyError, closure_parts};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use std::collections::HashSet;

use operations::synthesize_operations;
use supplies::LocalSupply;

const EFFECT_INTERFACE: &str = "psrs:effect";

/// The resolved identity of the trusted effect library interface. The driver
/// creates this only from its trusted library prefix and carries it to P8.
/// Consumers must not reconstruct trust from qualified names or opacity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedEffect {
    pub effect_type: HirTypeId,
    pub operations: Vec<EffectOperationBinding>,
}

/// A source command entry that the frontend has checked to have type
/// `Effect Unit`. P8 wraps it after lowering the abstract effect type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectCommandEntry {
    pub symbol: SymbolId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectCompilation {
    pub trusted: TrustedEffect,
    pub command_entry: Option<EffectCommandEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectOperationBinding {
    pub operation: EffectOperation,
    pub symbol: SymbolId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectOperation {
    Pure,
    Bind,
    Run,
    Trap,
}

impl EffectOperation {
    pub fn wit_function(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Bind => "bind",
            Self::Run => "run",
            Self::Trap => "trap",
        }
    }
}

/// An import signature classified while its result still names the abstract
/// trusted Effect constructor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectImportType {
    pub quantified: Vec<psrs_hir::TypeVariableId>,
    pub parameters: Vec<TypeId>,
    pub application: TypeId,
    pub payload: TypeId,
}

/// One representation closure written for an `Effect` application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectClosure {
    /// The closure type node the lowering wrote.
    pub ty: TypeId,
    /// The runtime token the closure must take as its only parameter.
    pub token: TypeId,
    /// The lowered result of the `Effect` application the closure replaced.
    pub result: TypeId,
}

/// Symbols whose abstract `psrs:effect` imports became ordinary declarations.
#[derive(Clone, Debug, Default)]
pub struct EffectLowering {
    pub synthesized: Vec<SymbolId>,
    /// The representation closures written for `Effect` applications.
    pub closures: Vec<EffectClosure>,
}

impl EffectLowering {
    /// Rejects a representation closure whose parameter list is not
    /// `[token]`, or whose result is not the lowered effect result.
    ///
    /// No later pass recognizes `Effect`, so this Core check is the only place
    /// that can reject such a closure. It runs over the closures this lowering
    /// wrote, before closure conversion and before any encoding.
    pub fn verify(&self, module: &Module) -> Result<(), Vec<VerifyError>> {
        let mut errors = Vec::new();
        for closure in &self.closures {
            let Some((parameters, result)) = closure_parts(&module.types, closure.ty) else {
                errors.push(verification_error(
                    module,
                    "a lowered effect application is not a closure",
                ));
                continue;
            };
            if parameters.len() != 1 || parameters[0] != closure.token {
                errors.push(verification_error(
                    module,
                    "a lowered effect closure must take only the runtime token",
                ));
            }
            if result != closure.result {
                errors.push(verification_error(
                    module,
                    "a lowered effect closure must return the lowered effect result",
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Lowers library effects in a linked Core module.
///
/// Programs that do not contain the opaque `Prelude.Effect` type are unchanged.
pub fn lower_effects(
    module: &mut Module,
    trusted: &TrustedEffect,
) -> Result<EffectLowering, Vec<VerifyError>> {
    let effect = trusted.effect_type;
    if !module.opaque_ids.contains(&effect) {
        return Err(vec![verification_error(
            module,
            "trusted Effect identity is missing or is not an opaque type",
        )]);
    }
    match module.callable_parameters(effect) {
        Some(1) => {}
        Some(_) => {
            return Err(vec![verification_error(
                module,
                "trusted Effect type has an incompatible callable representation",
            )]);
        }
        None => module.callable_types.push((effect, 1)),
    }
    let token = state_token_type(module);
    let closures = rewrite_effect_applications(module, effect, token);
    let synthesized = synthesize_operations(module, token, trusted)?;
    let lowering = EffectLowering {
        synthesized,
        closures,
    };
    lowering.verify(module)?;
    Ok(lowering)
}

/// Classifies a WIT source signature by its abstract result type. The returned
/// structure is evidence for a later wrapper; no closure shape is inspected.
pub fn classify_effect_import(
    module: &Module,
    ty: TypeId,
    effect: HirTypeId,
) -> Result<Option<EffectImportType>, &'static str> {
    let mut current = ty;
    let mut quantified = Vec::new();
    let mut parameters = Vec::new();
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current) {
            return Err("cyclic type spine in effect import signature");
        }
        let Some(node) = module.types.get(current.0 as usize) else {
            return Err("effect import signature references an invalid type id");
        };
        if let Type::ForAll { variables, body } = node {
            quantified.extend_from_slice(variables);
            current = *body;
            continue;
        }
        if let Type::Application(function, argument) = node
            && (module.types.get(function.0 as usize).is_none()
                || module.types.get(argument.0 as usize).is_none())
        {
            return Err("effect import signature contains an invalid application spine");
        }
        if let Some((function, payload)) = effect_application(module, current, effect) {
            return Ok(Some(EffectImportType {
                quantified,
                parameters,
                application: function,
                payload,
            }));
        }
        let Some((parameter, result)) = crate::arrow_parts(&module.types, current) else {
            return Ok(None);
        };
        if module.types.get(parameter.0 as usize).is_none()
            || module.types.get(result.0 as usize).is_none()
        {
            return Err("effect import signature contains an invalid function spine");
        }
        parameters.push(parameter);
        current = result;
    }
}

pub fn effect_application(
    module: &Module,
    id: TypeId,
    effect: HirTypeId,
) -> Option<(TypeId, TypeId)> {
    let Type::Application(function, payload) = module.types.get(id.0 as usize)? else {
        return None;
    };
    is_effect_constructor(module, *function, effect).then_some((id, *payload))
}

/// Writes `Type::Closure` over each `Effect` application and records the shape
/// each written closure must keep.
fn rewrite_effect_applications(
    module: &mut Module,
    effect: HirTypeId,
    token: TypeId,
) -> Vec<EffectClosure> {
    let targets = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| {
            let Type::Application(function, argument) = ty else {
                return None;
            };
            is_effect_constructor(module, *function, effect).then_some((index, *argument))
        })
        .collect::<Vec<_>>();
    let mut closures = Vec::with_capacity(targets.len());
    for (index, result) in targets {
        module.types[index] = Type::Closure {
            parameters: vec![token],
            result,
        };
        closures.push(EffectClosure {
            ty: TypeId(index as u32),
            token,
            result,
        });
    }
    closures
}

fn is_effect_constructor(module: &Module, id: TypeId, effect: HirTypeId) -> bool {
    matches!(
        module.types.get(id.0 as usize),
        Some(Type::Constructor(TypeConstructor::User(id))) if *id == effect
    )
}

/// Interns the compiler-owned opaque state token an `Effect` closure takes.
/// It has no source spelling; marking it opaque gives it a scalar runtime shape
/// without letting any pass treat it as an `Int`
/// ([effects](../../../design/backend/fp/effects.md)).
fn state_token_type(module: &mut Module) -> TypeId {
    let token = psrs_hir::TypeId::STATE_TOKEN;
    if !module.opaque_ids.contains(&token) {
        module.opaque_ids.push(token);
    }
    intern(module, Type::Constructor(TypeConstructor::User(token)))
}

fn verification_error(module: &Module, message: &'static str) -> VerifyError {
    VerifyError {
        module: module.id,
        span: module.span,
        message,
    }
}

fn intern(module: &mut Module, ty: Type) -> TypeId {
    if let Some(index) = module.types.iter().position(|existing| existing == &ty) {
        return TypeId(index as u32);
    }
    module.types.push(ty);
    TypeId((module.types.len() - 1) as u32)
}

/// Builds `parameters -> result` in the module type table.
pub fn function_type(module: &mut Module, parameters: &[TypeId], result: TypeId) -> TypeId {
    let mut ty = result;
    for parameter in parameters.iter().rev() {
        ty = operations::arrow(module, *parameter, ty);
    }
    ty
}

/// The next foreign-symbol index above the symbols already in the module.
pub fn fresh_foreign_symbol(module: &Module) -> SymbolId {
    let mut next = psrs_hir::FOREIGN_SYMBOL_BASE;
    for external in &module.externals {
        if external.symbol.module == ModuleId::INTRINSICS {
            next = next.max(external.symbol.index.saturating_add(1));
        }
    }
    for declaration in &module.declarations {
        if declaration.symbol.module == ModuleId::INTRINSICS {
            next = next.max(declaration.symbol.index.saturating_add(1));
        }
    }
    SymbolId::new(ModuleId::INTRINSICS, next)
}

/// A fresh local id above every binder already in the module.
pub fn fresh_local(module: &Module) -> LocalId {
    LocalSupply::new(module).fresh()
}
