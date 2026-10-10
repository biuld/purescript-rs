//! Logical dependency evidence retained alongside concrete MIR instructions.
use super::{Function, Instruction};
use crate::{BackendError, cc};
use psrs_core::state::dependency::Graph;
use std::sync::Arc;
mod control;
mod inventory;
pub(crate) use inventory::Inventory;
mod projection;
mod runtime;
pub(super) use projection::project;
pub(in crate::mir) use runtime::RuntimeInvocation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyFlow {
    source: Arc<cc::Module>,
    graph: Graph,
    executions: Vec<Graph>,
    runtime: Vec<RuntimeInvocation>,
    host: Vec<super::wit::CallPlan>,
}

impl DependencyFlow {
    /// Publishes a projection only after both its source and actual MIR calls
    /// agree. Structured binary choices retain their actual CFG correspondence;
    /// a graph alone never authorizes physical erasure.
    pub fn checked(
        source: Arc<cc::Module>,
        function: &Function,
    ) -> Result<Self, Vec<BackendError>> {
        Self::checked_with_runtime(source, function, Vec::new(), Vec::new())
    }

    pub(in crate::mir) fn checked_with_runtime(
        source: Arc<cc::Module>,
        function: &Function,
        runtime: Vec<RuntimeInvocation>,
        host: Vec<super::wit::CallPlan>,
    ) -> Result<Self, Vec<BackendError>> {
        let flows = cc::state::check(&source)?;
        let flow = flows
            .iter()
            .find(|flow| flow.function.symbol == function.symbol)
            .ok_or_else(|| error(function, "MIR dependency projection has no checked CC body"))?;
        let graph = flow.graph.clone();
        let executions = flow
            .executions
            .iter()
            .map(|execution| execution.graph.clone())
            .collect();
        let projection = Self {
            source,
            graph,
            executions,
            runtime,
            host,
        };
        projection.verify(function)?;
        Ok(projection)
    }

    pub fn executions(&self) -> &[Graph] {
        &self.executions
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub(crate) fn verify(&self, function: &Function) -> Result<(), Vec<BackendError>> {
        let flows = cc::state::derive_all(&self.source)?;
        let expected = flows
            .iter()
            .find(|flow| flow.function.symbol == function.symbol)
            .ok_or_else(|| {
                error(
                    function,
                    "MIR dependency projection has no owning CC function",
                )
            })?;
        if self.executions
            != expected
                .executions
                .iter()
                .map(|execution| execution.graph.clone())
                .collect::<Vec<_>>()
            || self.graph != expected.graph
        {
            return Err(error(
                function,
                "MIR dependency projection changes checked control flow",
            ));
        }
        self.graph
            .verify()
            .map_err(|failure| error(function, failure.message))?;
        let source_function = expected.function;
        let states = source_function
            .values
            .iter()
            .filter(|value| value.ty == cc::ValueShape::State)
            .map(|value| value.id)
            .collect::<Vec<_>>();
        let parameters = source_function
            .parameters
            .iter()
            .filter(|id| !states.contains(id))
            .copied()
            .collect::<Vec<_>>();
        if function.parameters != parameters
            || function
                .values
                .iter()
                .any(|value| states.contains(&value.id))
        {
            return Err(error(
                function,
                "MIR dependency projection materializes a logical State slot",
            ));
        }
        control::verify(
            source_function,
            function,
            &self.source.representations,
            &self.runtime,
            &self.host,
        )?;
        Ok(())
    }

    pub(crate) fn verify_imports(&self, module: &super::Module) -> Result<(), Vec<BackendError>> {
        for invocation in &self.runtime {
            invocation.verify_import(module)?;
        }
        for invocation in &self.host {
            invocation.verify_import(module)?;
        }
        Ok(())
    }
}

fn error(function: &Function, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P9 MIR dependency verification", function.span, message)
            .with_module(function.symbol.module),
    ]
}

#[cfg(test)]
mod branch_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod operand_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod switch_tests;
