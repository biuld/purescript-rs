use super::{DiagnosticOrigin, ProgramDiagnostic, diagnostic};
use psrs_core::effect::EffectOperation;
use psrs_hir::{Expr, ExprKind, ExternalKind, Module, SymbolId, TypeDeclarationKind};
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

pub(super) fn trusted_effect(
    modules: &[Module],
    trusted_prefix: usize,
) -> Result<Option<psrs_core::effect::TrustedEffect>, Vec<ProgramDiagnostic>> {
    let preludes = modules
        .iter()
        .take(trusted_prefix)
        .enumerate()
        .filter(|(_, module)| module.name == "Prelude")
        .collect::<Vec<_>>();
    if preludes.is_empty() {
        return Ok(None);
    }
    if preludes.len() != 1 {
        return Err(vec![ProgramDiagnostic {
            source: DiagnosticOrigin::Program,
            diagnostic: diagnostic(
                "P7 Effect contract",
                preludes[1].1.span,
                "trusted program contains multiple Prelude modules",
            ),
        }]);
    }
    let (source, prelude) = preludes[0];
    let Some(effect) = prelude.types.iter().find(|declaration| {
        declaration.name == "Effect" && declaration.kind == TypeDeclarationKind::Foreign
    }) else {
        return Ok(None);
    };
    let mut operations = Vec::new();
    for (name, operation, wit_name) in [
        ("pure", EffectOperation::Pure, "pure"),
        ("bind", EffectOperation::Bind, "bind"),
        ("runEffect", EffectOperation::Run, "run"),
        ("trap", EffectOperation::Trap, "trap"),
    ] {
        let Some(external) = prelude
            .externals
            .iter()
            .find(|external| external.name == name)
        else {
            return Err(vec![ProgramDiagnostic {
                source: DiagnosticOrigin::Source(source),
                diagnostic: diagnostic(
                    "P7 Effect contract",
                    prelude.span,
                    "trusted Prelude is missing a required Effect operation binding",
                ),
            }]);
        };
        if !matches!(
            &external.kind,
            ExternalKind::Wit { interface, function }
                if interface == "psrs:effect" && function == wit_name
        ) {
            return Err(vec![ProgramDiagnostic {
                source: DiagnosticOrigin::Source(source),
                diagnostic: diagnostic(
                    "P7 Effect contract",
                    external
                        .signature
                        .as_ref()
                        .map_or(prelude.span, |ty| ty.span),
                    "trusted Prelude Effect operation has the wrong WIT identity",
                ),
            }]);
        }
        operations.push(psrs_core::effect::EffectOperationBinding {
            operation,
            symbol: external.symbol,
        });
    }
    Ok(Some(psrs_core::effect::TrustedEffect {
        effect_type: effect.id,
        operations,
    }))
}

/// Enforces the source compatibility rule: `runEffect` may be referenced only
/// inside the selected declaration. Passing it from that declaration to a
/// helper is allowed; this is a lexical rule, not capability confinement.
pub(super) fn check_run_effect_scope(
    modules: &[Module],
    entry: Option<EntrySelection>,
    trusted: Option<&psrs_core::effect::TrustedEffect>,
) -> Result<(), Vec<ProgramDiagnostic>> {
    let Some(runner) = trusted
        .and_then(|trusted| {
            trusted
                .operations
                .iter()
                .find(|operation| operation.operation == EffectOperation::Run)
        })
        .map(|operation| operation.symbol)
    else {
        return Ok(());
    };
    let mut errors = Vec::new();
    for (source, module) in modules.iter().enumerate() {
        for declaration in &module.declarations {
            let mut references = Vec::new();
            collect_runner_references(&declaration.value, runner, &mut references);
            if entry.is_some_and(|entry| declaration.symbol == entry.symbol) {
                continue;
            }
            for span in references {
                errors.push(ProgramDiagnostic {
                    source: DiagnosticOrigin::Source(source),
                    diagnostic: diagnostic(
                        "P7 entry selection",
                        span,
                        "the trusted `runEffect` binding may only be referenced from the selected command entry `main`",
                    ),
                });
            }
        }
        for instance in &module.instances {
            for member in &instance.members {
                let mut references = Vec::new();
                collect_runner_references(&member.value, runner, &mut references);
                for span in references {
                    errors.push(ProgramDiagnostic {
                        source: DiagnosticOrigin::Source(source),
                        diagnostic: diagnostic(
                            "P7 entry selection",
                            span,
                            "the trusted `runEffect` binding may only be referenced from the selected command entry `main`",
                        ),
                    });
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn collect_runner_references(expression: &Expr, runner: SymbolId, spans: &mut Vec<TextRange>) {
    match &expression.kind {
        ExprKind::Global(symbol) if *symbol == runner => spans.push(expression.span),
        ExprKind::Array(elements) => {
            for element in elements {
                collect_runner_references(element, runner, spans);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                collect_runner_references(value, runner, spans);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            collect_runner_references(expression, runner, spans);
            for (_, value) in fields {
                collect_runner_references(value, runner, spans);
            }
        }
        ExprKind::FieldAccess { expression, .. }
        | ExprKind::Typed { expression, .. }
        | ExprKind::TypeApplication { expression, .. } => {
            collect_runner_references(expression, runner, spans);
        }
        ExprKind::Application(function, argument) => {
            collect_runner_references(function, runner, spans);
            collect_runner_references(argument, runner, spans);
        }
        ExprKind::Operator {
            operator,
            operator_span,
            left,
            right,
        } => {
            if *operator == runner {
                spans.push(*operator_span);
            }
            collect_runner_references(left, runner, spans);
            collect_runner_references(right, runner, spans);
        }
        ExprKind::Negate {
            function,
            expression,
            ..
        } => {
            collect_runner_references(function, runner, spans);
            collect_runner_references(expression, runner, spans);
        }
        ExprKind::OperatorChain {
            operands,
            operators,
        } => {
            for operator in operators {
                if operator.symbol == runner {
                    spans.push(operator.operator_span);
                }
            }
            for operand in operands {
                collect_runner_references(operand, runner, spans);
            }
        }
        ExprKind::OperatorSection {
            operator, operand, ..
        } => {
            if operator.symbol == runner {
                spans.push(operator.operator_span);
            }
            collect_runner_references(operand, runner, spans);
        }
        ExprKind::Lambda { body, .. } => collect_runner_references(body, runner, spans),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_runner_references(&binding.value, runner, spans);
            }
            collect_runner_references(body, runner, spans);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_runner_references(condition, runner, spans);
            collect_runner_references(then_branch, runner, spans);
            collect_runner_references(else_branch, runner, spans);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_runner_references(scrutinee, runner, spans);
            for branch in branches {
                collect_runner_references(&branch.value, runner, spans);
            }
        }
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                for binding in &clause.where_bindings {
                    collect_runner_references(&binding.value, runner, spans);
                }
                for guard in &clause.guards {
                    match guard {
                        psrs_hir::Guard::Boolean(value) => {
                            collect_runner_references(value, runner, spans)
                        }
                        psrs_hir::Guard::Pattern { value, .. } => {
                            collect_runner_references(value, runner, spans)
                        }
                        psrs_hir::Guard::Let { bindings, .. } => {
                            for binding in bindings {
                                collect_runner_references(&binding.value, runner, spans);
                            }
                        }
                    }
                }
                collect_runner_references(&clause.value, runner, spans);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}
