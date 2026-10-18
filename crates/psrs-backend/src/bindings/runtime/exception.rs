//! Checked source contract for the `psrs:runtime-exception` provider.
//!
//! The provider has no host ABI: a call to one of its exports is discharged by
//! the MIR lowering into an inline try region (`mir/lower/exception.rs`), so
//! `plan_import` plans no import for it. The contract here is what the
//! discharge relies on: exactly one guarded callable operand taking `Unit`,
//! and a `Boolean` result. See the design's "Binding" paragraph and worked
//! example in
//! `docs/design/backend/fp/exception-handling-and-stack-exhaustion.md`.

use crate::mir::layout::PlannedLayout;
use crate::types::ValueType;
use crate::{BackendError, RuntimeBinding, cc};
use psrs_core::{Module, Type, TypeConstructor, TypeId};
use psrs_hir::ModuleId;

pub(crate) const RUNTIME_EXCEPTION_MODULE: &str = "psrs:runtime-exception";
pub(crate) const CHECK_THROWS_EXPORT: &str = "checkThrows";

/// The provider's exports with their checked source contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExceptionOperation {
    /// `checkThrows :: forall a. (Unit -> a) -> Boolean` — true when the
    /// argument throws when applied, including stack exhaustion.
    CheckThrows,
}

impl ExceptionOperation {
    pub(crate) const ALL: [Self; 1] = [Self::CheckThrows];

    pub(crate) fn from_export(export: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.export() == export)
    }

    const fn export(self) -> &'static str {
        match self {
            Self::CheckThrows => CHECK_THROWS_EXPORT,
        }
    }

    /// Source type agreement checked before CC publication, including unused
    /// bindings. The guarded callable is `(Unit -> a)`: its own parameter is
    /// the `Unit` constructor and its result `a` is payload-erased, so any
    /// result type is accepted.
    pub(super) fn validate_source(self, module: &Module, ty: TypeId) -> Result<(), String> {
        let (parameters, result) =
            psrs_core::call_parts(&module.types, ty).map_err(str::to_owned)?;
        if parameters.len() != 1 {
            return Err("runtime-exception signature has the wrong parameter count".into());
        }
        let (callable_parameters, _) =
            psrs_core::call_parts(&module.types, parameters[0]).map_err(str::to_owned)?;
        if callable_parameters.len() != 1
            || module.types.get(callable_parameters[0].0 as usize)
                != Some(&Type::Constructor(TypeConstructor::Unit))
        {
            return Err("runtime-exception callable must take Unit".into());
        }
        if module.types.get(result.0 as usize) != Some(&Type::Constructor(TypeConstructor::Boolean))
        {
            return Err("runtime-exception signature must return Boolean".into());
        }
        Ok(())
    }
}

/// The checked provider facts a function lowerer needs to discharge a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExceptionProvider {
    pub module: String,
    pub function: String,
    pub source_module: ModuleId,
    pub operation: ExceptionOperation,
    /// The guarded callable's checked closure signature.
    pub callable: cc::SignatureId,
    /// The callable's erased `Unit` operand type.
    pub operand: ValueType,
    /// The callable's payload-erased result type.
    pub guarded_result: ValueType,
    pub span: psrs_span::TextRange,
}

/// Rechecks the provider's CC call shape at the physical boundary and collects
/// what the discharge needs. Runs once per live binding; the discharge trusts
/// this result.
pub(crate) fn checked_provider(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
    layout: &PlannedLayout,
) -> Result<ExceptionProvider, Vec<BackendError>> {
    let operation = ExceptionOperation::from_export(&binding.function)
        .ok_or_else(|| error(binding, "unknown runtime-exception export"))?;
    let [parameter] = signature.parameters.as_slice() else {
        return Err(error(
            binding,
            "runtime-exception signature has the wrong parameter count",
        ));
    };
    let callable = callable_signature(binding, parameter, table)?;
    let closure = table.signature(callable).ok_or_else(|| {
        error(
            binding,
            "runtime-exception callable has no checked signature",
        )
    })?;
    if signature.result != cc::ValueShape::Boolean {
        return Err(error(
            binding,
            "runtime-exception signature must return Boolean",
        ));
    }
    let operand = layout
        .value_type(&closure.parameters[0])
        .map_err(|_| error(binding, "runtime-exception operand has no concrete layout"))?;
    let guarded_result = layout
        .value_type(&closure.result)
        .map_err(|_| error(binding, "runtime-exception result has no concrete layout"))?;
    Ok(ExceptionProvider {
        module: binding.module.clone(),
        function: binding.function.clone(),
        source_module: binding.source_module,
        operation,
        callable,
        operand,
        guarded_result,
        span: binding.span,
    })
}

/// The canonical CC call signature: one guarded callable operand (checked as a
/// closure taking one erased `Unit`) and a `Boolean` result.
pub(crate) fn canonical_signature(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
) -> Result<cc::Signature, Vec<BackendError>> {
    let [parameter] = signature.parameters.as_slice() else {
        return Err(error(
            binding,
            "runtime-exception signature has the wrong parameter count",
        ));
    };
    callable_signature(binding, parameter, table)?;
    if signature.result != cc::ValueShape::Boolean {
        return Err(error(
            binding,
            "runtime-exception signature must return Boolean",
        ));
    }
    Ok(cc::Signature {
        parameters: signature.parameters.clone(),
        result: cc::ValueShape::Boolean,
    })
}

/// The guarded callable's closure signature: a non-nullable closure reference
/// whose checked signature takes exactly the erased `Unit`.
fn callable_signature(
    binding: &RuntimeBinding,
    parameter: &cc::ValueShape,
    table: &cc::RepresentationTable,
) -> Result<cc::SignatureId, Vec<BackendError>> {
    let cc::ValueShape::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Closure(signature),
    }) = parameter
    else {
        return Err(error(
            binding,
            "runtime-exception operand is not a callable",
        ));
    };
    let closure = table.signature(*signature).ok_or_else(|| {
        error(
            binding,
            "runtime-exception callable has no checked signature",
        )
    })?;
    if closure.parameters.as_slice() != [cc::ValueShape::Integer] {
        return Err(error(binding, "runtime-exception callable must take Unit"));
    }
    // The callable's result `a` is payload-erased: any shape is accepted.
    Ok(*signature)
}

fn error(binding: &RuntimeBinding, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::new("P8 runtime ABI", binding.span, message)
            .with_module(binding.source_module),
    ]
}
