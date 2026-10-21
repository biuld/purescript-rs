//! Checked source contract for the `psrs:runtime-exception` provider.
//!
//! The provider has no host ABI: a call to one of its exports is discharged by
//! the MIR lowering into inline try regions and throw terminators
//! (`mir/lower/exception.rs`), so `plan_import` plans no import for it. The
//! contract here is what the discharge relies on.
//!
//! Three exports share one provider:
//!
//! - `checkThrows :: forall a. (Unit -> a) -> Boolean` observes whether the
//!   guarded callable throws, including stack exhaustion.
//! - `throw :: forall a. String -> a` raises the source-error tag carrying its
//!   message.
//! - `catch :: forall a. (Unit -> a) -> (String -> a) -> a` recovers the
//!   source-error tag and hands its message to the handler. It does not observe
//!   any other tag: `psrs:stack-exhaustion` and unrelated tags propagate.
//!
//! See `docs/design/backend/fp/exception-handling-and-stack-exhaustion.md`.

use crate::mir::TagId;
use crate::mir::layout::PlannedLayout;
use crate::types::ValueType;
use crate::{BackendError, RuntimeBinding, cc};
use psrs_core::{Module, Type, TypeConstructor, TypeId};
use psrs_hir::ModuleId;

#[cfg(test)]
mod tests;

pub(crate) const RUNTIME_EXCEPTION_MODULE: &str = "psrs:runtime-exception";
pub(crate) const CHECK_THROWS_EXPORT: &str = "checkThrows";
pub(crate) const THROW_EXPORT: &str = "throw";
pub(crate) const CATCH_EXPORT: &str = "catch";
/// The source error channel's tag. It is deliberately distinct from the
/// budget's payload-less `psrs:stack-exhaustion`, so a source handler never
/// recovers stack exhaustion or turns a compiler budget event into a value.
pub(crate) const SOURCE_ERROR_TAG_NAME: &str = "psrs:source-error";

/// The provider's exports with their checked source contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExceptionOperation {
    /// `checkThrows :: forall a. (Unit -> a) -> Boolean` — true when the
    /// argument throws when applied, including stack exhaustion.
    CheckThrows,
    /// `throw :: forall a. String -> a` — raise the source-error tag.
    Throw,
    /// `catch :: forall a. (Unit -> a) -> (String -> a) -> a` — recover the
    /// source-error tag only.
    Catch,
}

impl ExceptionOperation {
    pub(crate) const ALL: [Self; 3] = [Self::CheckThrows, Self::Throw, Self::Catch];

    pub(crate) fn from_export(export: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.export() == export)
    }

    const fn export(self) -> &'static str {
        match self {
            Self::CheckThrows => CHECK_THROWS_EXPORT,
            Self::Throw => THROW_EXPORT,
            Self::Catch => CATCH_EXPORT,
        }
    }

    /// Whether the operation raises or recovers the source error tag, and so
    /// requires that tag to be planned.
    pub(crate) const fn uses_source_error_tag(self) -> bool {
        matches!(self, Self::Throw | Self::Catch)
    }

    /// Source type agreement checked before CC publication, including unused
    /// bindings.
    pub(super) fn validate_source(self, module: &Module, ty: TypeId) -> Result<(), String> {
        let (parameters, result) =
            psrs_core::call_parts(&module.types, ty).map_err(str::to_owned)?;
        match self {
            Self::CheckThrows => {
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
                if module.types.get(result.0 as usize)
                    != Some(&Type::Constructor(TypeConstructor::Boolean))
                {
                    return Err("runtime-exception signature must return Boolean".into());
                }
                Ok(())
            }
            Self::Throw => {
                if parameters.len() != 1 {
                    return Err("runtime-exception throw has the wrong parameter count".into());
                }
                expect_type(
                    module,
                    parameters[0],
                    TypeConstructor::String,
                    "runtime-exception throw must take String",
                )
            }
            Self::Catch => {
                if parameters.len() != 2 {
                    return Err("runtime-exception catch has the wrong parameter count".into());
                }
                let body = expect_callable_parameter(
                    module,
                    parameters[0],
                    TypeConstructor::Unit,
                    "runtime-exception catch body must take Unit",
                )?;
                let handler = expect_callable_parameter(
                    module,
                    parameters[1],
                    TypeConstructor::String,
                    "runtime-exception catch handler must take String",
                )?;
                if !module.types_equivalent(body, handler) || !module.types_equivalent(body, result)
                {
                    return Err(
                        "runtime-exception catch body and handler must agree on their result"
                            .into(),
                    );
                }
                Ok(())
            }
        }
    }
}

/// The checked provider facts a function lowerer needs to discharge a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExceptionProvider {
    pub module: String,
    pub function: String,
    pub source_module: ModuleId,
    pub operation: ExceptionOperation,
    /// `CheckThrows`: the guarded callable. `Catch`: the guarded body
    /// callable. `Throw`: none.
    pub callable: Option<cc::SignatureId>,
    /// The callable's erased `Unit` operand type (`CheckThrows`, `Catch`).
    pub operand: Option<ValueType>,
    /// The callable's payload-erased result type (`CheckThrows`, `Catch`).
    pub guarded_result: Option<ValueType>,
    /// `Catch`: the recovery callable and its `String` operand type.
    pub handler: Option<cc::SignatureId>,
    pub handler_operand: Option<ValueType>,
    /// The planned source-error tag and its single payload type
    /// (`Throw`, `Catch`).
    pub tag: Option<TagId>,
    pub payload: Option<ValueType>,
    pub span: psrs_span::TextRange,
}

impl ExceptionProvider {
    fn new(binding: &RuntimeBinding, operation: ExceptionOperation) -> Self {
        Self {
            module: binding.module.clone(),
            function: binding.function.clone(),
            source_module: binding.source_module,
            operation,
            callable: None,
            operand: None,
            guarded_result: None,
            handler: None,
            handler_operand: None,
            tag: None,
            payload: None,
            span: binding.span,
        }
    }
}

/// Rechecks the provider's CC call shape at the physical boundary and collects
/// what the discharge needs. Runs once per live binding; the discharge trusts
/// this result.
///
/// `source_error` carries the planned tag and payload when the module contains
/// a live `throw`/`catch` binding; it is absent for `checkThrows`-only modules
/// so they never gain a tag section.
pub(crate) fn checked_provider(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
    layout: &PlannedLayout,
    source_error: Option<(TagId, ValueType)>,
) -> Result<ExceptionProvider, Vec<BackendError>> {
    let operation = ExceptionOperation::from_export(&binding.function)
        .ok_or_else(|| error(binding, "unknown runtime-exception export"))?;
    let mut provider = ExceptionProvider::new(binding, operation);
    match operation {
        ExceptionOperation::CheckThrows => {
            let [parameter] = signature.parameters.as_slice() else {
                return Err(wrong_parameter_count(binding));
            };
            let callable = callable_signature(binding, parameter, table, cc::ValueShape::Integer)?;
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
            provider.callable = Some(callable);
            provider.operand =
                Some(layout.value_type(&closure.parameters[0]).map_err(|_| {
                    error(binding, "runtime-exception operand has no concrete layout")
                })?);
            provider.guarded_result =
                Some(layout.value_type(&closure.result).map_err(|_| {
                    error(binding, "runtime-exception result has no concrete layout")
                })?);
        }
        ExceptionOperation::Throw => {
            let [message] = signature.parameters.as_slice() else {
                return Err(wrong_parameter_count(binding));
            };
            if *message != cc::ValueShape::String {
                return Err(error(
                    binding,
                    "runtime-exception throw must take a string message",
                ));
            }
            let (tag, payload) = require_source_error(binding, source_error)?;
            let message_type = layout
                .value_type(message)
                .map_err(|_| error(binding, "runtime-exception message has no concrete layout"))?;
            if message_type != payload {
                return Err(error(
                    binding,
                    "runtime-exception message layout disagrees with the source-error payload",
                ));
            }
            provider.tag = Some(tag);
            provider.payload = Some(payload);
        }
        ExceptionOperation::Catch => {
            let [body, handler] = signature.parameters.as_slice() else {
                return Err(wrong_parameter_count(binding));
            };
            let body_signature = callable_signature(binding, body, table, cc::ValueShape::Integer)?;
            let handler_signature =
                callable_signature(binding, handler, table, cc::ValueShape::String)?;
            let body_closure = table
                .signature(body_signature)
                .ok_or_else(|| error(binding, "runtime-exception body has no checked signature"))?;
            let handler_closure = table.signature(handler_signature).ok_or_else(|| {
                error(
                    binding,
                    "runtime-exception handler has no checked signature",
                )
            })?;
            if body_closure.result != handler_closure.result
                || signature.result != body_closure.result
            {
                return Err(error(
                    binding,
                    "runtime-exception catch body and handler must agree on their result",
                ));
            }
            let (tag, payload) = require_source_error(binding, source_error)?;
            let operand = layout
                .value_type(&body_closure.parameters[0])
                .map_err(|_| error(binding, "runtime-exception operand has no concrete layout"))?;
            let handler_operand = layout
                .value_type(&handler_closure.parameters[0])
                .map_err(|_| error(binding, "runtime-exception handler operand has no layout"))?;
            if handler_operand != payload {
                return Err(error(
                    binding,
                    "runtime-exception handler layout disagrees with the source-error payload",
                ));
            }
            provider.callable = Some(body_signature);
            provider.operand = Some(operand);
            provider.guarded_result =
                Some(layout.value_type(&signature.result).map_err(|_| {
                    error(binding, "runtime-exception result has no concrete layout")
                })?);
            provider.handler = Some(handler_signature);
            provider.handler_operand = Some(handler_operand);
            provider.tag = Some(tag);
            provider.payload = Some(payload);
        }
    }
    Ok(provider)
}

/// The canonical CC call signature for each export. The source-to-CC
/// projection of these signatures is the identity, so the canonical signature
/// re-validates the physical shape and returns the external's own signature.
pub(crate) fn canonical_signature(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
) -> Result<cc::Signature, Vec<BackendError>> {
    let operation = ExceptionOperation::from_export(&binding.function)
        .ok_or_else(|| error(binding, "unknown runtime-exception export"))?;
    match operation {
        ExceptionOperation::CheckThrows => {
            let [parameter] = signature.parameters.as_slice() else {
                return Err(wrong_parameter_count(binding));
            };
            callable_signature(binding, parameter, table, cc::ValueShape::Integer)?;
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
        ExceptionOperation::Throw => {
            let [message] = signature.parameters.as_slice() else {
                return Err(wrong_parameter_count(binding));
            };
            if *message != cc::ValueShape::String {
                return Err(error(
                    binding,
                    "runtime-exception throw must take a string message",
                ));
            }
            Ok(signature.clone())
        }
        ExceptionOperation::Catch => {
            let [body, handler] = signature.parameters.as_slice() else {
                return Err(wrong_parameter_count(binding));
            };
            let body_signature = callable_signature(binding, body, table, cc::ValueShape::Integer)?;
            let handler_signature =
                callable_signature(binding, handler, table, cc::ValueShape::String)?;
            let body_closure = table
                .signature(body_signature)
                .ok_or_else(|| error(binding, "runtime-exception body has no checked signature"))?;
            let handler_closure = table.signature(handler_signature).ok_or_else(|| {
                error(
                    binding,
                    "runtime-exception handler has no checked signature",
                )
            })?;
            if body_closure.result != handler_closure.result
                || signature.result != body_closure.result
            {
                return Err(error(
                    binding,
                    "runtime-exception catch body and handler must agree on their result",
                ));
            }
            Ok(signature.clone())
        }
    }
}

/// A callable operand: a non-nullable closure reference whose checked
/// signature takes exactly one parameter of `operand`.
fn callable_signature(
    binding: &RuntimeBinding,
    parameter: &cc::ValueShape,
    table: &cc::RepresentationTable,
    operand: cc::ValueShape,
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
    if closure.parameters.as_slice() != [operand] {
        return Err(error(
            binding,
            "runtime-exception callable has the wrong operand type",
        ));
    }
    // The callable's result `a` is payload-erased: any shape is accepted.
    Ok(*signature)
}

fn expect_type(
    module: &Module,
    id: TypeId,
    expected: TypeConstructor,
    message: &str,
) -> Result<(), String> {
    if module.types.get(id.0 as usize) != Some(&Type::Constructor(expected)) {
        return Err(message.into());
    }
    Ok(())
}

fn expect_callable_parameter(
    module: &Module,
    id: TypeId,
    expected: TypeConstructor,
    message: &str,
) -> Result<TypeId, String> {
    let (parameters, result) = psrs_core::call_parts(&module.types, id).map_err(str::to_owned)?;
    if parameters.len() != 1 {
        return Err(message.into());
    }
    expect_type(module, parameters[0], expected, message)?;
    Ok(result)
}

fn require_source_error(
    binding: &RuntimeBinding,
    source_error: Option<(TagId, ValueType)>,
) -> Result<(TagId, ValueType), Vec<BackendError>> {
    source_error.ok_or_else(|| {
        error(
            binding,
            "runtime-exception source-error tag was not planned",
        )
    })
}

fn wrong_parameter_count(binding: &RuntimeBinding) -> Vec<BackendError> {
    error(
        binding,
        "runtime-exception signature has the wrong parameter count",
    )
}

fn error(binding: &RuntimeBinding, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::new("P8 runtime ABI", binding.span, message)
            .with_module(binding.source_module),
    ]
}
