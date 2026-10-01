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

use operations::synthesize_operations;
use supplies::LocalSupply;

const EFFECT_INTERFACE: &str = "psrs:effect";

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
pub fn lower_effects(module: &mut Module) -> Result<EffectLowering, Vec<VerifyError>> {
    let Some(effect) = library_effect(module) else {
        return Ok(EffectLowering::default());
    };
    let token = intern(module, Type::Constructor(TypeConstructor::Int));
    let closures = rewrite_effect_applications(module, effect, token);
    let lowering = EffectLowering {
        synthesized: synthesize_operations(module, token),
        closures,
    };
    lowering.verify(module)?;
    Ok(lowering)
}

fn library_effect(module: &Module) -> Option<HirTypeId> {
    module.type_names.iter().find_map(|(id, name)| {
        (*name == "Prelude.Effect" && module.opaque_ids.contains(id)).then_some(*id)
    })
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

/// A suspended import: source parameters, the closure type, and its payload.
pub fn suspended_import(module: &Module, ty: TypeId) -> Option<(Vec<TypeId>, TypeId, TypeId)> {
    let mut current = ty;
    let mut seen = 0;
    while seen <= module.types.len()
        && let Some((_, body)) = crate::forall_parts(&module.types, current)
    {
        current = body;
        seen += 1;
    }
    let mut parameters = Vec::new();
    loop {
        if let Some((closure_parameters, result)) = closure_parts(&module.types, current) {
            if closure_parameters.len() != 1 {
                return None;
            }
            return Some((parameters, current, result));
        }
        let (parameter, result) = crate::arrow_parts(&module.types, current)?;
        parameters.push(parameter);
        current = result;
    }
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
