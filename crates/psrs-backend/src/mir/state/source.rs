//! One immutable dependency authority per logical CC module. Physical function
//! verification consumes its checked summaries without deriving the entire
//! frozen module again for each body or optimizer checkpoint.
use super::*;
use psrs_hir::SymbolId;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::mir) struct SourceFlow {
    pub graph: Graph,
    pub executions: Vec<Graph>,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::mir) struct CheckedSource {
    pub module: Arc<cc::Module>,
    positions: HashMap<SymbolId, usize>,
    flows: HashMap<SymbolId, SourceFlow>,
}

impl CheckedSource {
    pub(in crate::mir) fn checked(module: Arc<cc::Module>) -> Result<Arc<Self>, Vec<BackendError>> {
        let derived = cc::state::derive_all(&module)?;
        if !derived.is_empty() {
            // Publish State summaries only with ordinary CC typing/scope
            // validation. An empty source retains the existing pure-module
            // inventory contract and publishes no additional typing claims.
            cc::state::check(&module)?;
        }
        let flows = derived
            .iter()
            .map(|flow| {
                (
                    flow.function.symbol,
                    SourceFlow {
                        graph: flow.graph.clone(),
                        executions: flow
                            .executions
                            .iter()
                            .map(|run| run.graph.clone())
                            .collect(),
                    },
                )
            })
            .collect();
        let positions = module
            .functions
            .iter()
            .enumerate()
            .map(|(id, body)| (body.symbol, id))
            .collect();
        Ok(Arc::new(Self {
            module,
            positions,
            flows,
        }))
    }

    pub(in crate::mir) fn required(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.flows.keys().copied()
    }

    pub(in crate::mir) fn flow(&self, symbol: SymbolId) -> Option<&SourceFlow> {
        self.flows.get(&symbol)
    }

    pub(in crate::mir) fn function(&self, symbol: SymbolId) -> Option<&cc::Function> {
        self.positions
            .get(&symbol)
            .map(|id| &self.module.functions[*id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_source_keeps_its_calls_when_a_source_copy_changes() {
        let (mut source, physical) = crate::mir::state::tests::fixture();
        let checked = CheckedSource::checked(source.clone()).unwrap();
        let original = DependencyFlow::checked_with_runtime(
            checked.clone(),
            &physical,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let changed = Arc::make_mut(&mut source);
        let replacement = psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 2);
        let mut external = changed.externals[0].clone();
        external.symbol = replacement;
        changed.externals.push(external);
        let cc::AssignmentKind::DirectCall { function, .. } =
            &mut changed.functions[0].assignments[0].kind
        else {
            panic!("fixture has an observable call");
        };
        *function = replacement;
        assert!(!Arc::ptr_eq(&checked.module, &source));
        original.verify(&physical).unwrap();
        assert!(DependencyFlow::checked(source, &physical).is_err());
    }
}
