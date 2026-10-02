use super::names::Resolver;
use psrs_ast as ast;
use psrs_hir::{self as hir, Associativity, ExprKind, ResolvedOperator};
use std::collections::HashMap;

pub(super) fn merge_fixities(
    local: Vec<hir::Fixity>,
    imports: &[hir::Import],
) -> (HashMap<String, hir::Fixity>, HashMap<String, hir::Fixity>) {
    let mut value_fixities = HashMap::new();
    let mut type_fixities = HashMap::new();
    for fixity in imports.iter().flat_map(|import| &import.fixities) {
        let destination = match fixity.namespace {
            hir::FixityNamespace::Value => &mut value_fixities,
            hir::FixityNamespace::Type => &mut type_fixities,
        };
        destination.insert(fixity.operator.clone(), fixity.clone());
    }
    for fixity in local {
        let destination = match fixity.namespace {
            hir::FixityNamespace::Value => &mut value_fixities,
            hir::FixityNamespace::Type => &mut type_fixities,
        };
        destination.insert(fixity.operator.clone(), fixity);
    }
    for import in imports {
        let qualifier = import.alias.as_ref().unwrap_or(&import.module_name);
        for fixity in &import.fixities {
            let destination = match fixity.namespace {
                hir::FixityNamespace::Value => &mut value_fixities,
                hir::FixityNamespace::Type => &mut type_fixities,
            };
            destination.insert(format!("{qualifier}.({})", fixity.operator), fixity.clone());
        }
    }
    (value_fixities, type_fixities)
}

impl Resolver {
    pub(super) fn resolve_operator_chain(
        &mut self,
        operands: Vec<ast::Expr>,
        operators: Vec<ast::Operator>,
    ) -> Option<ExprKind> {
        let operands = operands
            .into_iter()
            .map(|operand| self.resolve_expr(operand))
            .collect::<Option<Vec<_>>>()?;
        let operators = operators
            .into_iter()
            .map(|operator| self.resolve_operator(&operator.name.text, operator.span))
            .collect::<Option<Vec<_>>>()?;
        Some(ExprKind::OperatorChain {
            operands,
            operators,
        })
    }

    pub(super) fn resolve_operator_section(
        &mut self,
        operator: ast::Name,
        operand: ast::Expr,
        side: ast::SectionSide,
        span: psrs_span::TextRange,
    ) -> Option<ExprKind> {
        let operator = self.resolve_operator(&operator.text, operator.span)?;
        let operand = self.resolve_expr(operand)?;
        let binder = self.new_local(format!("__psrs_section_{}", span.start), span);
        Some(ExprKind::OperatorSection {
            operator,
            operand: Box::new(operand),
            binder,
            side: match side {
                ast::SectionSide::Left => hir::SectionSide::Left,
                ast::SectionSide::Right => hir::SectionSide::Right,
            },
        })
    }

    pub(super) fn resolve_operator(
        &mut self,
        name: &str,
        span: psrs_span::TextRange,
    ) -> Option<ResolvedOperator> {
        let symbol = self.lookup_global(name, span)?;
        let (associativity, precedence) = self
            .fixities
            .get(name)
            .map(|fixity| (fixity.associativity, fixity.precedence))
            .unwrap_or_else(|| default_fixity(name));
        Some(ResolvedOperator {
            symbol,
            operator_span: span,
            associativity,
            precedence,
        })
    }
}

fn default_fixity(name: &str) -> (Associativity, u32) {
    let precedence = match name {
        ".." => 9,
        "||" => 1,
        "&&" => 2,
        "==" | "/=" | "<" | ">" | "<=" | ">=" => 3,
        "+" | "-" | "<>" => 4,
        "*" | "/" | "%" => 5,
        ":" => 6,
        _ => 9,
    };
    (Associativity::Left, precedence)
}
