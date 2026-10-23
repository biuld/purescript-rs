//! Explicit support scope for the official runner's rebuild-module mode.

use super::*;

#[derive(Clone, Copy, Default)]
pub(super) enum SupportEnvironment {
    #[default]
    Imports,
    Complete {
        prefix: usize,
    },
}

impl SupportEnvironment {
    pub(super) fn dependencies(
        self,
        dependencies: &mut [Vec<usize>],
    ) -> Result<(), Vec<ProgramDiagnostic>> {
        let Self::Complete { prefix } = self else {
            return Ok(());
        };
        if prefix > dependencies.len() {
            return Err(vec![ProgramDiagnostic {
                source: DiagnosticOrigin::Program,
                diagnostic: diagnostic(
                    "P5 typecheck",
                    psrs_span::TextRange::default(),
                    "complete support prefix exceeds the resolved source inventory",
                ),
            }]);
        }
        // Support producers use their ordinary imports. Only consumers of the
        // complete environment depend on every support producer's success.
        for imports in &mut dependencies[prefix..] {
            imports.extend(0..prefix);
            imports.sort_unstable();
            imports.dedup();
        }
        Ok(())
    }

    pub(super) fn values(
        self,
        module: psrs_hir::ModuleId,
        checked: &HashMap<psrs_hir::SymbolId, psrs_typecheck::CheckedValueSignature>,
        imported: &mut HashMap<psrs_hir::SymbolId, psrs_typecheck::CheckedValueSignature>,
    ) {
        if let Self::Complete { prefix } = self
            && module.0 as usize >= prefix
        {
            imported.extend(
                checked
                    .iter()
                    .filter(|(symbol, _)| (symbol.module.0 as usize) < prefix)
                    .map(|(symbol, signature)| (*symbol, signature.clone())),
            );
        }
    }
}

#[cfg(test)]
mod tests;
