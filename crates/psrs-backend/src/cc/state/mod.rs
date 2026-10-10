//! Dependencies derived from actual typed CC instructions, before physical
//! projection. Nominal source-region checking is discharged by checked Core;
//! these single-root function graphs retain local dependency provenance.
use super::{Assignment, Function, Module, Signature, ValueId, ValueShape};
use crate::BackendError;
use psrs_core::state::dependency::{
    Block, Dependency, DependencyId, Graph, RegionId, Terminator, Transition,
};
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

mod branches;
mod execution;
pub use execution::{ClosedExecution, execution_projection};
mod eval;
mod projection;
pub use projection::StateCallProjection;
pub(crate) use projection::register_payload_slots;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct Operation<'a> {
    pub assignment: &'a Assignment,
    pub input: DependencyId,
    pub output: DependencyId,
}

#[derive(Debug)]
pub struct FunctionFlow<'a> {
    source: &'a Module,
    pub function: &'a Function,
    pub graph: Graph,
    pub operations: Vec<Operation<'a>>,
    pub executions: Vec<ClosedExecution<'a>>,
    passthrough_safe: bool,
}

/// Ordinary CC typing and scope verification precede dependency publication.
pub fn check(module: &Module) -> Result<Vec<FunctionFlow<'_>>, Vec<BackendError>> {
    super::verify::verify_module(module)?;
    derive_all(module)
}

impl FunctionFlow<'_> {
    pub fn verify(&self, module: &Module) -> Result<(), Vec<BackendError>> {
        if !std::ptr::eq(self.source, module) {
            return Err(error(
                self.function,
                "CC state flow belongs to a different module",
            ));
        }
        let flows = derive_all(module)?;
        let expected = flows
            .iter()
            .find(|flow| std::ptr::eq(flow.function, self.function))
            .ok_or_else(|| error(self.function, "CC state flow has no owning function"))?;
        if self.executions != expected.executions
            || self.graph != expected.graph
            || self.operations.len() != expected.operations.len()
            || self
                .operations
                .iter()
                .zip(&expected.operations)
                .any(|(actual, expected)| {
                    !std::ptr::eq(actual.assignment, expected.assignment)
                        || actual.input != expected.input
                        || actual.output != expected.output
                })
        {
            return Err(error(
                self.function,
                "CC state flow changes instruction provenance or control flow",
            ));
        }
        Ok(())
    }
}

pub(crate) fn derive_all(module: &Module) -> Result<Vec<FunctionFlow<'_>>, Vec<BackendError>> {
    let signatures = module
        .functions
        .iter()
        .map(|function| {
            let shapes = function
                .values
                .iter()
                .map(|value| (value.id, value.ty))
                .collect::<HashMap<_, _>>();
            let parameters = function
                .parameters
                .iter()
                .map(|id| {
                    shapes.get(id).copied().ok_or_else(|| {
                        error(
                            function,
                            "CC state verification parameter has no value declaration",
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((
                function.symbol,
                Signature {
                    parameters,
                    result: function.result_type,
                },
            ))
        })
        .collect::<Result<Vec<_>, Vec<BackendError>>>()?
        .into_iter()
        .chain(module.externals.iter().filter_map(|external| {
            external
                .signature
                .clone()
                .map(|signature| (external.symbol, signature))
        }))
        .collect::<HashMap<_, _>>();
    // Least fixed point: unknown and recursive calls stay observable. Only
    // checked bodies returning the incoming dependency authorize aliases. This
    // is a State summary, not a general purity or allocation-effect summary.
    let mut passthrough = HashSet::new();
    loop {
        let mut flows = Vec::new();
        let mut errors = Vec::new();
        for function in &module.functions {
            match eval::function(module, function, &signatures, &passthrough) {
                Ok(Some(flow)) => flows.push(flow),
                Ok(None) => {}
                Err(error) => errors.extend(error),
            }
        }
        let mut changed = false;
        for flow in &flows {
            if flow.passthrough_safe && flow.operations.is_empty() {
                changed |= passthrough.insert(flow.function.symbol);
            }
        }
        if !changed {
            return if errors.is_empty() {
                Ok(flows)
            } else {
                Err(errors)
            };
        }
    }
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC state verification", function.span, message)
            .with_module(function.symbol.module),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Opaque,
    State(DependencyId),
    Product(Vec<Value>),
    Closure(SymbolId),
}

impl Value {
    fn remap(&mut self, from: DependencyId, to: DependencyId) {
        match self {
            Self::State(id) if *id == from => *id = to,
            Self::Product(fields) => {
                for field in fields {
                    field.remap(from, to);
                }
            }
            _ => {}
        }
    }
    fn states(&self, result: &mut Vec<DependencyId>) {
        match self {
            Self::State(id) => result.push(*id),
            Self::Product(fields) => {
                for field in fields {
                    field.states(result);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct Path {
    block: usize,
    current: DependencyId,
    revision: u32,
    values: HashMap<ValueId, Value>,
    terminated: bool,
}

struct Builder<'a, 'b> {
    module: &'a Module,
    function: &'a Function,
    signatures: &'b HashMap<SymbolId, Signature>,
    passthrough: &'b HashSet<SymbolId>,
    shapes: HashMap<ValueId, ValueShape>,
    state_root: bool,
    executions: Vec<ClosedExecution<'a>>,
    next: u32,
    blocks: Vec<Block>,
    operations: Vec<Operation<'a>>,
    passthrough_safe: bool,
}

impl Builder<'_, '_> {
    fn fresh(&mut self) -> DependencyId {
        let id = DependencyId(self.next);
        self.next += 1;
        id
    }
    fn value(&self, path: &Path, id: ValueId) -> Value {
        path.values.get(&id).cloned().unwrap_or(Value::Opaque)
    }
}
