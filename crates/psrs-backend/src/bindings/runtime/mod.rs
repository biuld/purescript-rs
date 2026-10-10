//! Checked source contracts for ordinary runtime provider bindings.
//! Export lookup and physical operations belong to psrs-runtime, not HIR.

use crate::BackendError;
use psrs_core::{Module, Type, TypeConstructor, state::StateSignature};
use psrs_hir::ExternalKind;
use psrs_runtime::{StorageOperation, StorageValue};

pub(super) mod call;
pub(super) mod metadata;
pub(super) mod target;

/// Validate all declarations, including unused ones, before lowering/erasure.
/// The immutable checked Core arena owns every signature identity used here.
pub(crate) fn validate(module: &Module) -> Result<(), Vec<BackendError>> {
    for external in &module.externals {
        let ExternalKind::Runtime {
            module: provider,
            function,
        } = &external.kind
        else {
            continue;
        };
        let checked = module
            .external_types
            .iter()
            .find(|ty| ty.symbol == external.symbol);
        let source_module = checked.map_or(module.id, |ty| ty.source_module);
        let span = external
            .signature
            .as_ref()
            .map_or(module.span, |ty| ty.span);
        let error = |message| {
            vec![BackendError::new("P8 runtime linking", span, message).with_module(source_module)]
        };
        if provider != psrs_runtime::STORAGE_MODULE {
            return Err(error(format!("unknown runtime provider `{provider}`")));
        }
        let operation = StorageOperation::from_export(function).ok_or_else(|| {
            error(format!(
                "runtime provider `{provider}` has no export `{function}`"
            ))
        })?;
        let checked = checked
            .ok_or_else(|| error("runtime binding has no checked source signature".into()))?;
        let signature = psrs_core::state::signature(module, checked.ty)
            .map_err(|message| error(format!("runtime binding `{function}`: {message}")))?;
        validate_contract(module, operation, &signature)
            .map_err(|message| error(format!("runtime binding `{function}`: {message}")))?;
    }
    Ok(())
}

fn validate_contract(
    module: &Module,
    operation: StorageOperation,
    signature: &StateSignature,
) -> Result<(), &'static str> {
    let projection = operation.projection()?;
    let source = projection.source();
    if signature.parameters.len() != source.parameters.len() {
        return Err("runtime source and physical argument counts disagree");
    }
    let mut element = None;
    for (ty, expected) in signature
        .parameters
        .iter()
        .copied()
        .zip(projection.operands().map(|(source, _)| source))
        .chain([(signature.payload, source.payload)])
    {
        let checked = match expected {
            StorageValue::Int => {
                if module.types.get(ty.0 as usize) != Some(&Type::Constructor(TypeConstructor::Int))
                {
                    return Err("runtime index or length must have checked type Int");
                }
                None
            }
            StorageValue::Unit => {
                if module.types.get(ty.0 as usize)
                    != Some(&Type::Constructor(TypeConstructor::Unit))
                {
                    return Err("runtime void operation must have checked payload Unit");
                }
                None
            }
            StorageValue::Array => {
                let (head, arguments) = module
                    .applied_constructor(ty)
                    .ok_or("runtime storage operand must be Array")?;
                if head != TypeConstructor::Array || arguments.len() != 1 {
                    return Err("runtime storage operand must be a saturated Array");
                }
                Some(arguments[0])
            }
            StorageValue::Element => Some(ty),
            StorageValue::Any => None,
        };
        if let Some(checked) = checked {
            if let Some(element) = element {
                if !module.types_equivalent(element, checked) {
                    return Err("runtime storage element and checked payload types disagree");
                }
            } else {
                element = Some(checked);
            }
        }
    }
    Ok(())
}
