//! The primitive diagnostic interfaces: `Fail`, `Warn`, and `Partial`.
//!
//! `Fail` and `Partial` never create dictionary evidence. Their report-only
//! rules are reached only when an obligation cannot be generalized; otherwise
//! the ordinary residual-constraint path preserves them for a later use site.
//! `Warn` prefers an in-scope dictionary, and when none exists it records a
//! warning and returns an empty primitive dictionary that THIR verifies and
//! Core erases.

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;

/// The report interfaces this module owns.
pub(in crate::typecheck) const RULES: [PrimitiveRule; 3] = [FAIL, WARN, PARTIAL];

const FAIL: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_TYPE_ERROR_FAIL,
    evidence: EvidenceClass::ReportOnly,
    arity: 1,
    solve: solve_fail,
};

const WARN: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_TYPE_ERROR_WARN,
    evidence: EvidenceClass::ReportingDictionary,
    arity: 1,
    solve: solve_warn,
};

const PARTIAL: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_PARTIAL,
    evidence: EvidenceClass::ReportOnly,
    arity: 0,
    solve: solve_partial,
};

/// Whether this member's unresolved diagnostic is the result rather than a
/// missing user instance. The member identity is the authority; spellings and
/// import paths do not affect the classification.
pub(in crate::typecheck) fn is_report_only(class_id: hir::TypeId) -> bool {
    super::primitive_rule(class_id).is_some_and(|rule| rule.evidence == EvidenceClass::ReportOnly)
}

fn solve_fail(_checker: &mut Checker, _args: &PrimitiveArgs) -> PrimitiveOutcome {
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Report,
        deferred: Vec::new(),
    }
}

fn solve_partial(_checker: &mut Checker, _args: &PrimitiveArgs) -> PrimitiveOutcome {
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Report,
        deferred: Vec::new(),
    }
}

fn solve_warn(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let message = arguments
        .first()
        .and_then(|message| render_doc(checker, message))
        .unwrap_or_else(|| {
            arguments
                .first()
                .map(|message| checker.display_type(message))
                .unwrap_or_else(|| "<missing warning message>".into())
        });
    checker.state.warnings.push(TypeCheckWarning {
        span: args.report_span(),
        message: format!(
            "A custom warning occurred while solving type class constraints:\n{}",
            indent(&message, 2)
        ),
    });
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary { arguments },
        deferred: Vec::new(),
    }
}

/// Builds the official `NoInstanceFound` diagnostic for an unresolved report
/// obligation. A well-formed `Fail Doc` uses its custom message. `Partial`
/// currently has no exhaustiveness payload in HIR, so it keeps the ordinary
/// no-instance wording until the owning coverage pass can provide that data.
pub(in crate::typecheck) fn unresolved_report_error(
    checker: &Checker,
    class_id: hir::TypeId,
    arguments: &[InferType],
    span: psrs_span::TextRange,
) -> TypeCheckError {
    let message = if class_id == hir::TypeId::PRIM_TYPE_ERROR_FAIL {
        arguments
            .first()
            .and_then(|message| render_doc(checker, message))
            .map(|message| format!("Custom error:\n{}", indent(&message, 2)))
    } else {
        None
    };
    TypeCheckError::new(
        TypeCheckErrorKind::NoInstance,
        span,
        message.unwrap_or_else(|| {
            let rendered = checker.display_constraint(class_id, arguments);
            format!("no instance for constraint {rendered}")
        }),
    )
}

/// Returns the rendered lines for a well-formed `Doc` type. An unknown or
/// malformed constructor is not guessed: callers can use the ordinary type
/// renderer as the fallback, matching PureScript's `toTypelevelString = Nothing`.
fn render_doc(checker: &Checker, ty: &InferType) -> Option<String> {
    let ty = checker.resolve_type(ty.clone());
    let (head, arguments) = spine(&ty);
    let InferType::Constructor(TypeConstructor::User(id)) = head else {
        return None;
    };
    match (*id, arguments.as_slice()) {
        (hir::TypeId::PRIM_TYPE_ERROR_TEXT, [InferType::TypeLevelString(text)]) => {
            Some(text.clone())
        }
        (hir::TypeId::PRIM_TYPE_ERROR_QUOTE, [quoted]) => {
            Some(display_quoted_type(checker, quoted))
        }
        (hir::TypeId::PRIM_TYPE_ERROR_QUOTE_LABEL, [InferType::TypeLevelString(label)]) => {
            Some(pretty_label(label))
        }
        (hir::TypeId::PRIM_TYPE_ERROR_BESIDE, [left, right]) => {
            let left = render_doc(checker, left)?;
            let right = render_doc(checker, right)?;
            Some(beside(&left, &right))
        }
        (hir::TypeId::PRIM_TYPE_ERROR_ABOVE, [top, bottom]) => {
            let mut lines = render_doc(checker, top)?
                .split('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>();
            lines.extend(render_doc(checker, bottom)?.split('\n').map(str::to_owned));
            Some(lines.join("\n"))
        }
        _ => None,
    }
}

fn spine(ty: &InferType) -> (&InferType, Vec<&InferType>) {
    fn collect<'a>(ty: &'a InferType, arguments: &mut Vec<&'a InferType>) -> &'a InferType {
        match ty {
            InferType::Application(function, argument) => {
                arguments.push(argument);
                collect(function, arguments)
            }
            head => head,
        }
    }
    let mut arguments = Vec::new();
    let head = collect(ty, &mut arguments);
    arguments.reverse();
    (head, arguments)
}

fn beside(left: &str, right: &str) -> String {
    let left = left.split('\n').collect::<Vec<_>>();
    let right = right.split('\n').collect::<Vec<_>>();
    let height = left.len().max(right.len());
    let left_width = left
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default();
    (0..height)
        .map(|index| {
            let left_line = left.get(index).copied().unwrap_or_default();
            let right_line = right.get(index).copied().unwrap_or_default();
            let padding = left_width.saturating_sub(left_line.chars().count());
            format!("{left_line}{}{right_line}", " ".repeat(padding))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn display_quoted_type(checker: &Checker, ty: &InferType) -> String {
    let displayed = checker.display_type(ty);
    let Some(inner) = displayed
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
    else {
        return displayed;
    };
    let mut depth = 1_i32;
    let mut closes_before_end = false;
    for (index, character) in displayed.char_indices().skip(1) {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && index + character.len_utf8() < displayed.len() {
                    closes_before_end = true;
                    break;
                }
            }
            _ => {}
        }
    }
    if closes_before_end {
        displayed
    } else {
        inner.to_owned()
    }
}

fn pretty_label(label: &str) -> String {
    let mut characters = label.chars();
    let bare = characters
        .next()
        .is_some_and(|first| first == '_' || first.is_lowercase())
        && characters
            .all(|character| character == '_' || character == '\'' || character.is_alphanumeric());
    if bare {
        return label.to_owned();
    }
    let escaped = label
        .chars()
        .flat_map(|character| match character {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            character => vec![character],
        })
        .collect::<String>();
    format!("\"{escaped}\"")
}

fn indent(message: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    message
        .split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
