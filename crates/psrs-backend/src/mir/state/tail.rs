//! Tail conversion preserves the checked physical predecessor through the
//! common tail rewrite. Actual MIR must implement that exact conversion;
//! source correspondence is still checked against the immutable predecessor.
use super::*;
use crate::TargetCapabilities;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TailTransfer {
    source: Arc<Function>,
    target: TargetCapabilities,
}

impl DependencyFlow {
    pub(in crate::mir) fn project_tail(
        &self,
        function: &Function,
        target: TargetCapabilities,
    ) -> Result<(Function, Self), Vec<BackendError>> {
        self.verify(function)?;
        if self.tail.is_some() {
            return Err(error(
                function,
                "tail dependency transfer is already projected",
            ));
        }
        let mut source = function.clone();
        source.state = None;
        let mut transformed = source.clone();
        crate::mir::lower::tail::rewrite(&mut transformed, target)?;
        let mut flow = self.clone();
        if transformed != source {
            flow.tail = Some(TailTransfer {
                source: Arc::new(source),
                target,
            });
        }
        flow.verify(&transformed)?;
        Ok((transformed, flow))
    }
}

impl TailTransfer {
    pub(super) fn source(&self) -> &Function {
        &self.source
    }

    pub(super) fn verify(&self, function: &Function) -> Result<(), Vec<BackendError>> {
        let mut expected = (*self.source).clone();
        crate::mir::lower::tail::rewrite(&mut expected, self.target)?;
        let mut actual = function.clone();
        actual.state = None;
        // FunctionId is the compacted module position, not semantic identity.
        // Source inventories and calls remain anchored to the stable SymbolId.
        expected.id = actual.id;
        // P10 already permits proven scalar folding in State functions. Use
        // that same operation to compare instructions, without admitting any
        // call, operand, capture, block-parameter or control-flow change.
        crate::mir::opt::constants::fold_scalar_instructions(&mut expected);
        crate::mir::opt::constants::fold_scalar_instructions(&mut actual);
        if actual != expected {
            return Err(error(
                function,
                "MIR tail conversion changes checked dependency instructions or control flow",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_tail_transfer_rejects_a_changed_call_target() {
        let (source, function) = crate::mir::state::tests::fixture();
        let flow = DependencyFlow::checked(source, &function).unwrap();
        let (mut projected, flow) = flow
            .project_tail(&function, TargetCapabilities::default())
            .unwrap();
        let Some(crate::mir::Terminator::ReturnCall {
            function: target, ..
        }) = &mut projected.blocks[0].terminator
        else {
            panic!("the checked terminal call must become a tail call");
        };
        *target = projected.symbol;
        assert!(flow.verify(&projected).is_err());
    }

    #[test]
    fn checked_tail_transfer_preserves_the_preceding_observable_call() {
        let (source, function) = crate::mir::state::tests::fixture();
        let flow = DependencyFlow::checked(source, &function).unwrap();
        let (mut projected, flow) = flow
            .project_tail(&function, TargetCapabilities::default())
            .unwrap();
        flow.verify(&projected).unwrap();
        projected.blocks[0].instructions.clear();
        assert!(flow.verify(&projected).is_err());
    }
}
