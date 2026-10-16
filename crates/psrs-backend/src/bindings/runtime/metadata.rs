//! Lossless source runtime binding identity beside target-neutral CC.
use crate::BackendError;
use psrs_core::{Module as CoreModule, TypeId as CoreTypeId};
use psrs_hir::{ExternalKind, ModuleId, SymbolId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeBinding {
    pub symbol: SymbolId,
    pub source_module: ModuleId,
    pub module: String,
    pub function: String,
    /// Identity of the checked source scheme in the owning Core arena.
    pub type_id: Option<CoreTypeId>,
    pub span: psrs_span::TextRange,
}

pub(crate) fn from_core(module: &CoreModule) -> Vec<RuntimeBinding> {
    module
        .externals
        .iter()
        .filter_map(|external| {
            let ExternalKind::Runtime {
                module: provider,
                function,
            } = &external.kind
            else {
                return None;
            };
            let checked = module
                .external_types
                .iter()
                .find(|checked| checked.symbol == external.symbol);
            Some(RuntimeBinding {
                symbol: external.symbol,
                source_module: checked.map_or(module.id, |checked| checked.source_module),
                module: provider.clone(),
                function: function.clone(),
                type_id: checked.map(|checked| checked.ty),
                span: external
                    .signature
                    .as_ref()
                    .map_or(module.span, |signature| signature.span),
            })
        })
        .collect()
}

pub(crate) fn validate_core(
    bindings: &[RuntimeBinding],
    module: &CoreModule,
) -> Result<(), Vec<BackendError>> {
    let expected = from_core(module)
        .into_iter()
        .map(|binding| (binding.symbol, binding))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for binding in bindings {
        let message = if !seen.insert(binding.symbol) {
            Some("runtime binding is duplicated")
        } else {
            match expected.get(&binding.symbol) {
                None => Some("runtime binding is not a source runtime import"),
                Some(expected) if binding != expected => {
                    Some("runtime binding disagrees with its checked source identity")
                }
                Some(_) => None,
            }
        };
        if let Some(message) = message {
            errors.push(
                BackendError::invalid_ir("P8 external binding validation", binding.span, message)
                    .with_module(binding.source_module),
            );
        }
    }
    for (symbol, binding) in expected {
        if !seen.contains(&symbol) {
            errors.push(
                BackendError::invalid_ir(
                    "P8 external binding validation",
                    binding.span,
                    "source runtime import has no external binding",
                )
                .with_module(binding.source_module),
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> CoreModule {
        let mut module = crate::bindings::tests::module(Vec::new());
        module.externals.push(psrs_hir::ExternalSymbol {
            source_module: None,
            symbol: SymbolId::new(module.id, 7),
            name: "read".into(),
            kind: ExternalKind::Runtime {
                module: psrs_runtime::STORAGE_MODULE.into(),
                function: "array_read".into(),
            },
            signature: None,
        });
        module.types.push(psrs_core::Type::Constructor(
            psrs_core::TypeConstructor::Int,
        ));
        module.external_types.push(psrs_core::ExternalType {
            symbol: module.externals[0].symbol,
            source_module: ModuleId(42),
            ty: CoreTypeId(0),
        });
        module
    }

    #[test]
    fn runtime_metadata_preserves_identity_and_rejects_substitution() {
        let module = fixture();
        let bindings = from_core(&module);
        validate_core(&bindings, &module).unwrap();
        assert_eq!(bindings[0].source_module, ModuleId(42));
        assert_eq!(bindings[0].type_id, Some(CoreTypeId(0)));
        for change in 0..4 {
            let mut wrong = bindings.clone();
            match change {
                0 => wrong[0].function = "array_write".into(),
                1 => wrong[0].module = "another-provider".into(),
                2 => wrong[0].type_id = Some(CoreTypeId(1)),
                _ => wrong[0].source_module = ModuleId(999),
            }
            assert!(validate_core(&wrong, &module).is_err());
        }
        assert!(validate_core(&[], &module).is_err());
        assert!(validate_core(&[bindings[0].clone(), bindings[0].clone()], &module).is_err());
        let mut wrong = bindings;
        wrong[0].symbol = SymbolId::new(module.id, 8);
        assert!(validate_core(&wrong, &module).is_err());
    }
}
