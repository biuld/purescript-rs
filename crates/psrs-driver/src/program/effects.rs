use super::{DiagnosticOrigin, ProgramDiagnostic, diagnostic};
use psrs_hir::{Module, SymbolId};
use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EntrySelection {
    pub symbol: SymbolId,
    pub source: usize,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntryResolution {
    Selected(EntrySelection),
    Missing,
    Ambiguous {
        entries: Vec<EntrySelection>,
        main_modules: bool,
    },
}

pub(super) fn select_entry(modules: &[Module]) -> EntryResolution {
    let candidates = modules
        .iter()
        .enumerate()
        .flat_map(|(source, module)| {
            module
                .declarations
                .iter()
                .filter(|declaration| declaration.name == "main")
                .map(move |declaration| EntrySelection {
                    symbol: declaration.symbol,
                    source,
                    span: declaration.name_span,
                })
        })
        .collect::<Vec<_>>();
    let main_modules = candidates
        .iter()
        .filter(|entry| modules[entry.source].name == "Main")
        .copied()
        .collect::<Vec<_>>();
    match main_modules.as_slice() {
        [entry] => EntryResolution::Selected(*entry),
        [] if candidates.len() == 1 => EntryResolution::Selected(candidates[0]),
        [] if candidates.is_empty() => EntryResolution::Missing,
        _ if !main_modules.is_empty() => EntryResolution::Ambiguous {
            entries: main_modules,
            main_modules: true,
        },
        _ => EntryResolution::Ambiguous {
            entries: candidates,
            main_modules: false,
        },
    }
}

pub(super) fn require_unambiguous_entry(
    resolution: EntryResolution,
) -> Result<Option<EntrySelection>, Vec<ProgramDiagnostic>> {
    match resolution {
        EntryResolution::Selected(entry) => Ok(Some(entry)),
        EntryResolution::Missing => Ok(None),
        EntryResolution::Ambiguous {
            entries,
            main_modules,
        } => {
            let message = if main_modules {
                "multiple `main` declarations exist in modules named `Main`"
            } else {
                "program has multiple `main` declarations; define one `main` in `Main`"
            };
            Err(entries
                .into_iter()
                .map(|entry| ProgramDiagnostic {
                    source: DiagnosticOrigin::Source(entry.source),
                    diagnostic: diagnostic("P7 entry selection", entry.span, message),
                })
                .collect())
        }
    }
}
