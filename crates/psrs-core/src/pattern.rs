use crate::{Module, TypeId, VerifyError};
use psrs_hir::{LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Var {
        id: LocalId,
        ty: TypeId,
    },
    /// A constructor pattern, with one nested pattern per field.
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pattern>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
    },
}

pub(crate) fn verify_pattern(
    pattern: &Pattern,
    module: &Module,
    owner: ModuleId,
    locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    crate::verify_type(pattern.ty, module, owner, pattern.span, errors);
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var { id, ty } => {
            crate::verify_type(*ty, module, owner, pattern.span, errors);
            locals.insert(*id);
        }
        PatternKind::Constructor { symbol, arguments } => {
            if !module
                .constructors
                .iter()
                .any(|constructor| constructor.symbol == *symbol)
            {
                errors.push(VerifyError {
                    module: owner,
                    span: pattern.span,
                    message: "pattern constructor is not declared",
                });
            }
            for argument in arguments {
                verify_pattern(argument, module, owner, locals, errors);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                verify_pattern(field, module, owner, locals, errors);
            }
        }
    }
}

pub(crate) fn remove_pattern_locals(pattern: &Pattern, locals: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Var { id, .. } => {
            locals.remove(id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                remove_pattern_locals(argument, locals);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                remove_pattern_locals(field, locals);
            }
        }
        PatternKind::Wildcard => {}
    }
}
