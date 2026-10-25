//! Local access semantics survive capture and lifting in one scope entry.
use crate::cc::ValueId;
use psrs_hir::LocalId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub(in crate::cc) enum LocalBinding {
    Value(ValueId),
    Getter {
        value: ValueId,
        signature: crate::cc::SignatureId,
    },
}

impl LocalBinding {
    /// The value physically carried by a closure capture.
    pub(in crate::cc) fn storage(self) -> ValueId {
        match self {
            Self::Value(value) | Self::Getter { value, .. } => value,
        }
    }

    /// Restore an access mode over the new SSA identity of a captured slot.
    pub(in crate::cc) fn with_storage(self, value: ValueId) -> Self {
        match self {
            Self::Value(_) => Self::Value(value),
            Self::Getter { signature, .. } => Self::Getter { value, signature },
        }
    }
}

#[derive(Default)]
pub(in crate::cc) struct LocalScope {
    bindings: HashMap<LocalId, LocalBinding>,
}

impl LocalScope {
    pub(in crate::cc) fn insert(&mut self, local: LocalId, value: ValueId) -> Option<LocalBinding> {
        self.bind(local, LocalBinding::Value(value))
    }

    pub(in crate::cc) fn bind(
        &mut self,
        local: LocalId,
        binding: LocalBinding,
    ) -> Option<LocalBinding> {
        self.bindings.insert(local, binding)
    }

    pub(in crate::cc) fn remove(&mut self, local: &LocalId) -> Option<LocalBinding> {
        self.bindings.remove(local)
    }

    pub(in crate::cc) fn get(&self, local: &LocalId) -> Option<&LocalBinding> {
        self.bindings.get(local)
    }
}

impl super::FunctionLowerer<'_> {
    pub(super) fn read_local_binding(
        &mut self,
        binding: LocalBinding,
        span: psrs_span::TextRange,
        assignments: &mut Vec<crate::cc::Assignment>,
    ) -> Result<ValueId, Vec<crate::BackendError>> {
        match binding {
            LocalBinding::Value(value) => Ok(value),
            LocalBinding::Getter { value, signature } => {
                let result = self
                    .representations
                    .signature(signature)
                    .ok_or_else(|| {
                        vec![crate::BackendError::invalid_ir(
                            "P8 closure conversion",
                            span,
                            "recursive getter has no signature",
                        )]
                    })?
                    .result;
                let destination = self.fresh(result);
                assignments.push(crate::cc::Assignment {
                    destination,
                    span,
                    kind: crate::cc::AssignmentKind::IndirectCall {
                        function: value,
                        signature,
                        arguments: vec![],
                    },
                });
                Ok(destination)
            }
        }
    }
}
