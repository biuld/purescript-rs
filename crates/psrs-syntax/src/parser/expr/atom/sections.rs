use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstName, Expr, ExprKind, OperatorSectionSide};
use psrs_span::TextRange;

use super::super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(super) fn parse_parenthesized_expression(&mut self) -> Result<Expr, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        if self.at_raw(&RawTokenKind::RParen) {
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::Name(CstName::new("unit", span)),
                span,
            });
        }
        if self.is_expression_operator()
            && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::RParen)
        {
            let operator = self
                .bump_expression_operator()
                .expect("operator was checked");
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::Name(operator),
                span,
            });
        }
        if matches!(
            self.current().kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(ref name)) if name == "_"
        ) && matches!(
            self.peek(1).kind,
            LayoutTokenKind::Raw(
                RawTokenKind::Operator(_) | RawTokenKind::Colon | RawTokenKind::DotDot
            )
        ) {
            self.bump();
            let operator = self
                .bump_expression_operator()
                .expect("operator was checked after an anonymous argument");
            let operand = self.parse_expression(0)?;
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::OperatorSection {
                    operator,
                    operand: Box::new(operand),
                    side: OperatorSectionSide::Right,
                },
                span,
            });
        }
        if self.is_expression_operator()
            && !matches!(
                &self.current().kind,
                LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) if operator == "-"
            )
        {
            let operator = self
                .bump_expression_operator()
                .expect("operator was checked");
            let function = Expr {
                kind: ExprKind::Name(operator.clone()),
                span: operator.span,
            };
            let argument = self.parse_expression(0)?;
            let application_span = TextRange::new(operator.span.start, argument.span.end);
            let application = Expr {
                kind: ExprKind::Application(Box::new(function), Box::new(argument)),
                span: application_span,
            };
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            return Ok(Expr {
                kind: ExprKind::Parens {
                    open_paren_span,
                    expression: Box::new(application),
                    close_paren_span,
                },
                span: TextRange::new(open_paren_span.start, close_paren_span.end),
            });
        }

        // A leading `-` is unary negation here, as in the source grammar. It
        // must not be consumed as a section operator: `(-5)` and `(-x)` are
        // parenthesized negative expressions. Sections use an explicit `_`.
        let first = self.parse_expression_impl(0, true, true)?;
        if let ExprKind::Operator {
            operator,
            left,
            right,
        } = &first.kind
            && matches!(&right.kind, ExprKind::Name(name) if name.text == "_")
        {
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::OperatorSection {
                    operator: operator.clone(),
                    operand: left.clone(),
                    side: OperatorSectionSide::Left,
                },
                span,
            });
        }
        if self.at_raw(&RawTokenKind::Comma) {
            let mut items = vec![first];
            while self.at_raw(&RawTokenKind::Comma) {
                self.bump();
                items.push(self.parse_expression(0)?);
            }
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::Tuple {
                    open_paren_span,
                    items,
                    close_paren_span,
                },
                span,
            });
        }
        let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
        Ok(Expr {
            kind: ExprKind::Parens {
                open_paren_span,
                expression: Box::new(first),
                close_paren_span,
            },
            span: TextRange::new(open_paren_span.start, close_paren_span.end),
        })
    }
}
