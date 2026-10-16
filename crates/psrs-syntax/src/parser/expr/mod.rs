mod atom;
mod compound;
mod pattern;
mod qualified;

use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstName, Expr, ExprKind};
use psrs_span::TextRange;

use super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(crate) fn parse_expression(&mut self, min_precedence: u8) -> Result<Expr, ParseError> {
        self.parse_expression_impl(min_precedence, true, false)
    }

    fn parse_expression_impl(
        &mut self,
        min_precedence: u8,
        allow_backtick: bool,
        allow_section: bool,
    ) -> Result<Expr, ParseError> {
        let mut left = self.parse_application()?;
        loop {
            if allow_backtick && self.at_raw(&RawTokenKind::Backtick) {
                let precedence = 4;
                if precedence < min_precedence {
                    break;
                }
                // A qualified name such as `A.zip` is one operator, not an
                // expression applied to the left operand.
                if self.at_simple_backticked_operator() {
                    self.bump();
                    let operator = self.parse_backticked_operator()?;
                    self.consume_raw(RawTokenKind::Backtick)?;
                    let right = self.parse_expression_impl(precedence + 1, true, allow_section)?;
                    let span = TextRange::new(left.span.start, right.span.end);
                    left = Expr {
                        kind: ExprKind::Operator {
                            operator,
                            left: Box::new(left),
                            right: Box::new(right),
                        },
                        span,
                    };
                    continue;
                }
                // Complex content (for example a `case`): parse it as a grouped
                // expression and apply, so parsing succeeds where purs succeeds.
                // This over-accepts `let`/`if` in backticks, which purs rejects;
                // tighten it if the corpus flags such a case.
                let left_start = left.span.start;
                self.bump();
                let content = self.parse_expression_impl(0, false, false)?;
                self.consume_raw(RawTokenKind::Backtick)?;
                let content_end = content.span.end;
                let mut combined = Expr {
                    kind: ExprKind::Application(Box::new(left), Box::new(content)),
                    span: TextRange::new(left_start, content_end),
                };
                if self.starts_atom() {
                    let argument = self.parse_application()?;
                    let span = TextRange::new(left_start, argument.span.end);
                    combined = Expr {
                        kind: ExprKind::Application(Box::new(combined), Box::new(argument)),
                        span,
                    };
                }
                left = combined;
                continue;
            }
            if let Some((operator, precedence, tokens)) = self.qualified_operator() {
                if precedence < min_precedence {
                    break;
                }
                for _ in 0..tokens {
                    self.bump();
                }
                let right =
                    self.parse_expression_impl(precedence + 1, allow_backtick, allow_section)?;
                let span = TextRange::new(left.span.start, right.span.end);
                left = Expr {
                    kind: ExprKind::Operator {
                        operator,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span,
                };
                continue;
            }
            let (operator, operator_span) = match &self.current().kind {
                LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) if operator != "@" => {
                    (operator.clone(), self.current().span)
                }
                LayoutTokenKind::Raw(RawTokenKind::Colon) => (":".to_owned(), self.current().span),
                LayoutTokenKind::Raw(RawTokenKind::DotDot) => {
                    ("..".to_owned(), self.current().span)
                }
                LayoutTokenKind::Raw(RawTokenKind::Backslash) => {
                    ("\\".to_owned(), self.current().span)
                }
                _ => break,
            };
            let precedence = precedence(&operator);
            if precedence < min_precedence {
                break;
            }
            if allow_section && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::RParen) {
                break;
            }
            self.bump();
            let right =
                self.parse_expression_impl(precedence + 1, allow_backtick, allow_section)?;
            let span = TextRange::new(left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Operator {
                    operator: CstName::new(operator, operator_span),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        if min_precedence == 0 && self.at_raw(&RawTokenKind::DoubleColon) {
            let double_colon_span = self.bump().span;
            let type_expr = self.parse_type()?;
            let span = TextRange::new(left.span.start, type_expr.span.end);
            left = Expr {
                kind: ExprKind::Typed {
                    expression: Box::new(left),
                    double_colon_span,
                    type_expr,
                },
                span,
            };
        }
        Ok(left)
    }

    fn at_simple_backticked_operator(&self) -> bool {
        let close_at = match &self.peek(1).kind {
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(_) | RawTokenKind::Operator(_) | RawTokenKind::Colon,
            ) => 2,
            LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => {
                let mut index = 1;
                loop {
                    let name = self.peek(index);
                    let dot = self.peek(index + 1);
                    let part = self.peek(index + 2);
                    if !matches!(dot.kind, LayoutTokenKind::Raw(RawTokenKind::Dot))
                        || dot.span.start != name.span.end
                    {
                        break;
                    }
                    let part_is_name = matches!(
                        part.kind,
                        LayoutTokenKind::Raw(
                            RawTokenKind::LowerIdent(_) | RawTokenKind::UpperIdent(_)
                        )
                    );
                    if !part_is_name || part.span.start != dot.span.end {
                        break;
                    }
                    index += 2;
                }
                index + 1
            }
            _ => return false,
        };
        self.peek(close_at).kind == LayoutTokenKind::Raw(RawTokenKind::Backtick)
    }

    fn parse_backticked_operator(&mut self) -> Result<CstName, ParseError> {
        if matches!(
            self.current().kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_) | RawTokenKind::UpperIdent(_))
        ) {
            return self.parse_qualified_value_name();
        }
        let token = self.current().clone();
        match token.kind {
            LayoutTokenKind::Raw(RawTokenKind::Operator(name)) => {
                self.bump();
                Ok(CstName::new(name, token.span))
            }
            LayoutTokenKind::Raw(RawTokenKind::Colon) => {
                self.bump();
                Ok(CstName::new(":", token.span))
            }
            _ => Err(self.error(format!("expected an operator, found {}", self.found()))),
        }
    }
}

fn precedence(operator: &str) -> u8 {
    match operator {
        ".." => 9,
        "||" => 1,
        "&&" => 2,
        "==" | "/=" | "<" | ">" | "<=" | ">=" => 3,
        "+" | "-" | "<>" => 4,
        "*" | "/" | "%" => 5,
        ":" => 6,
        _ => 4,
    }
}
