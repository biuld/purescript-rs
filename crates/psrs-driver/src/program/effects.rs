use super::{ProgramDiagnostic, diagnostic};
use psrs_hir::{Expr, ExprKind, Module, SymbolId};
use psrs_span::TextRange;

/// Restricts references to the trusted `Prelude.runEffect` value to the
/// selected command entry. This runs on resolved HIR, before type inference.
pub(super) fn check_run_effect_scope(
    modules: &[Module],
    trusted_prefix: usize,
) -> Result<(), Vec<ProgramDiagnostic>> {
    let Some(runner) = modules
        .iter()
        .take(trusted_prefix)
        .find(|module| module.name == "Prelude")
        .and_then(|module| {
            module
                .declarations
                .iter()
                .find(|declaration| declaration.name == "runEffect")
                .map(|declaration| declaration.symbol)
        })
    else {
        return Ok(());
    };
    let entry = selected_entry_symbol(modules);
    let mut errors = Vec::new();
    for (source, module) in modules.iter().enumerate() {
        for declaration in &module.declarations {
            let mut references = Vec::new();
            collect_runner_references(&declaration.value, runner, &mut references);
            if Some(declaration.symbol) == entry {
                continue;
            }
            for span in references {
                errors.push(ProgramDiagnostic {
                    source,
                    diagnostic: diagnostic(
                        "P7 entry selection",
                        span,
                        "the trusted `runEffect` binding may only be referenced from the selected command entry `main`",
                    ),
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn selected_entry_symbol(modules: &[Module]) -> Option<SymbolId> {
    let candidates = modules
        .iter()
        .flat_map(|module| {
            module
                .declarations
                .iter()
                .filter(|declaration| declaration.name == "main")
                .map(move |declaration| (module.name.as_str(), declaration.symbol))
        })
        .collect::<Vec<_>>();
    let main_module = candidates
        .iter()
        .filter(|(module_name, _)| *module_name == "Main")
        .collect::<Vec<_>>();
    match main_module.as_slice() {
        [(_, symbol)] => Some(*symbol),
        [] if candidates.len() == 1 => Some(candidates[0].1),
        _ => None,
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
        ExprKind::FieldAccess { expression, .. } => {
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
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}
