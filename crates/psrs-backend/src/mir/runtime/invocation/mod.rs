//! Source-anchored raw calls with immutable operand and recovery witnesses.
use super::*;
use crate::mir::Instruction;
use crate::types::{ValueDecl, ValueId};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

mod emit;
mod verify;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::mir) struct RawInvocation {
    owner: SymbolId,
    source: cc::Assignment,
    import: Import,
    arguments: Vec<ValueId>,
    argument_types: Vec<ValueType>,
    destination: ValueId,
    result_type: ValueType,
    result: RawCallResult,
    span: TextRange,
    first_temporary: u32,
}

impl RawInvocation {
    /// Obtain the actual assignment from immutable checked CC, rather than
    /// accepting an arbitrary caller-supplied argument list or signature.
    pub(in crate::mir) fn from_source(
        binding: &RuntimeBinding,
        module: &cc::Module,
        owner: SymbolId,
        destination: ValueId,
        layout: &PlannedLayout,
        first_temporary: u32,
    ) -> Result<Self, Vec<BackendError>> {
        let flows = cc::state::check(module)?;
        let flow = flows
            .iter()
            .find(|flow| flow.function.symbol == owner)
            .ok_or_else(|| error(binding, "runtime invocation has no checked owning CC body"))?;
        let operation = flow
            .operations
            .iter()
            .find(|operation| operation.assignment.destination == destination)
            .ok_or_else(|| error(binding, "runtime invocation has no source assignment"))?;
        let cc::AssignmentKind::DirectCall {
            function,
            arguments,
        } = &operation.assignment.kind
        else {
            return Err(error(
                binding,
                "runtime invocation source is not a direct call",
            ));
        };
        if *function != binding.symbol {
            return Err(error(
                binding,
                "runtime invocation source and binding identities disagree",
            ));
        }
        if flow
            .function
            .values
            .iter()
            .any(|value| value.id.0 >= first_temporary)
        {
            return Err(error(
                binding,
                "runtime invocation temporaries overlap logical CC values",
            ));
        }
        let (import, result) = plan_import(binding, module, layout)?;
        verify_import(&import, &layout.types).map_err(|message| error(binding, &message))?;
        let signature = module
            .externals
            .iter()
            .find(|external| external.symbol == binding.symbol)
            .and_then(|external| external.signature.as_ref())
            .unwrap();
        let state = cc::state::StateCallProjection::checked(signature, &module.representations)
            .map_err(|message| error(binding, message))?
            .unwrap();
        let arguments = arguments[..state.state_parameter].to_vec();
        let argument_types = signature.parameters[..state.state_parameter]
            .iter()
            .map(|shape| {
                layout
                    .value_type(shape)
                    .map_err(|_| error(binding, "runtime operand has no source layout"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result_type = layout
            .value_type(&state.payload)
            .map_err(|_| error(binding, "runtime payload has no source layout"))?;
        let eq = cc::payload::erased_shape();
        for (index, raw) in import.parameters.iter().enumerate() {
            if matches!(
                raw,
                ValueType::Ref(RefType {
                    nullable: true,
                    heap: HeapType::Eq
                })
            ) && signature.parameters[index] != eq
            {
                return Err(vec![BackendError::new(
                    "P9 runtime projection",
                    binding.span,
                    "runtime element requires checked canonical payload conversion before raw invocation",
                )]);
            }
        }
        if import.result
            == Some(ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }))
            && state.payload != eq
        {
            return Err(vec![BackendError::new(
                "P9 runtime projection",
                binding.span,
                "runtime element result requires checked canonical payload recovery",
            )]);
        }
        Ok(Self {
            owner,
            source: operation.assignment.clone(),
            import,
            arguments,
            argument_types,
            destination,
            result_type,
            result,
            span: operation.assignment.span,
            first_temporary,
        })
    }

    pub(in crate::mir) fn never_returns(&self) -> bool {
        self.result == RawCallResult::Never
    }

    pub(in crate::mir) fn import(&self) -> &Import {
        &self.import
    }

    pub(in crate::mir) fn matches_source(&self, owner: SymbolId, source: &cc::Assignment) -> bool {
        self.owner == owner && self.source == *source
    }

    pub(in crate::mir) fn destination(&self) -> ValueId {
        self.destination
    }

    pub(in crate::mir) fn verify_source_values(
        &self,
        function: &crate::mir::Function,
    ) -> Result<(), Vec<BackendError>> {
        for (id, ty) in self
            .arguments
            .iter()
            .zip(&self.argument_types)
            .chain(std::iter::once((&self.destination, &self.result_type)))
        {
            if !function
                .values
                .iter()
                .any(|value| value.id == *id && value.ty == *ty)
            {
                return Err(self.error("runtime invocation changes checked source value types"));
            }
        }
        Ok(())
    }

    fn error(&self, message: &str) -> Vec<BackendError> {
        vec![
            BackendError::invalid_ir("P9 runtime invocation verification", self.span, message)
                .with_module(self.owner.module),
        ]
    }
}

#[cfg(test)]
mod tests;
