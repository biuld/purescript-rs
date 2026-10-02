use crate::{LowerError, Name, lower_name};
use psrs_cst as cst;
use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixityNamespace {
    Value,
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Associativity {
    Left,
    Right,
    None,
}

/// An unresolved source fixity declaration. `target` names the value,
/// constructor, or type that the operator aliases; `operator` is the spelling
/// used in expressions, patterns, types, imports, and exports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixityDeclaration {
    pub namespace: FixityNamespace,
    pub associativity: Associativity,
    pub precedence: u32,
    pub target: Name,
    pub operator: Name,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operator {
    pub name: Name,
    pub span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionSide {
    /// `(left operator _)` supplies the left operand.
    Left,
    /// `(_ operator right)` supplies the right operand.
    Right,
}

pub(crate) fn lower_declaration(
    declaration: cst::FixityDeclaration,
) -> Result<FixityDeclaration, LowerError> {
    let precedence = declaration.precedence.parse::<u32>().map_err(|_| {
        LowerError::new(
            declaration.precedence_span,
            "fixity precedence must be a non-negative integer",
        )
    })?;
    let namespace = if declaration.namespace_span.is_some() {
        FixityNamespace::Type
    } else {
        FixityNamespace::Value
    };
    let associativity = match declaration.associativity {
        cst::Fixity::Left => Associativity::Left,
        cst::Fixity::Right => Associativity::Right,
        cst::Fixity::None => Associativity::None,
    };
    let target = lower_name(declaration.operator);
    let operator = declaration
        .alias
        .map(lower_name)
        .unwrap_or_else(|| target.clone());
    Ok(FixityDeclaration {
        namespace,
        associativity,
        precedence,
        target,
        operator,
        span: declaration.span,
    })
}
