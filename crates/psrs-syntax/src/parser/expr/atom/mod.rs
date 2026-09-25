mod records;

use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstName, Expr, ExprKind, RecordField, RecordUpdateField};
use psrs_span::TextRange;

use super::super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(super) fn parse_application(&mut self) -> Result<Expr, ParseError> {
        let mut function = self.parse_atom()?;
        loop {
            if self.at_raw(&RawTokenKind::Dot) && self.starts_label_at(1) {
                let dot_span = self.bump().span;
                let field = self.parse_label("record field")?;
                let span = TextRange::new(function.span.start, field.span.end);
                function = Expr {
                    kind: ExprKind::FieldAccess {
                        expression: Box::new(function),
                        dot_span,
                        field,
                    },
                    span,
                };
                continue;
            }
            if let LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) = &self.current().kind
                && operator == "@"
                && self.starts_type_atom_at(1)
            {
                let at_span = self.bump().span;
                let type_expr = self.parse_type_atom()?;
                let span = TextRange::new(function.span.start, type_expr.span.end);
                function = Expr {
                    kind: ExprKind::TypeApplication {
                        expression: Box::new(function),
                        at_span,
                        type_expr,
                    },
                    span,
                };
                continue;
            }
            if self.current().kind == LayoutTokenKind::Raw(RawTokenKind::LBrace) {
                let checkpoint = self.cursor;
                match self.parse_record_update(function.clone()) {
                    Ok(updated) => {
                        function = updated;
                        continue;
                    }
                    Err(_) => {
                        self.cursor = checkpoint;
                    }
                }
            }
            if !self.starts_atom() {
                break;
            }
            let argument = self.parse_atom()?;
            let span = TextRange::new(function.span.start, argument.span.end);
            function = Expr {
                kind: ExprKind::Application(Box::new(function), Box::new(argument)),
                span,
            };
        }
        Ok(function)
    }

    fn starts_type_atom_at(&self, offset: usize) -> bool {
        matches!(
            &self.peek(offset).kind,
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(_)
                    | RawTokenKind::UpperIdent(_)
                    | RawTokenKind::Integer(_)
                    | RawTokenKind::String(_)
                    | RawTokenKind::LParen
                    | RawTokenKind::LBrace
            )
        )
    }

    pub(crate) fn starts_label_at(&self, offset: usize) -> bool {
        match &self.peek(offset).kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_) | RawTokenKind::String(_)) => true,
            LayoutTokenKind::Raw(kind) => label_keyword(kind).is_some(),
            _ => false,
        }
    }

    pub(crate) fn parse_label(&mut self, what: &str) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        let text = match &token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name) | RawTokenKind::String(name)) => {
                name.clone()
            }
            LayoutTokenKind::Raw(kind) => match label_keyword(kind) {
                Some(text) => text.to_owned(),
                None => {
                    return Err(self.error(format!("expected {what}, found {}", self.found())));
                }
            },
            _ => return Err(self.error(format!("expected {what}, found {}", self.found()))),
        };
        self.bump();
        Ok(CstName::new(text, token.span))
    }

    pub(super) fn starts_atom(&self) -> bool {
        matches!(
            self.current().kind,
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(_)
                    | RawTokenKind::UpperIdent(_)
                    | RawTokenKind::Integer(_)
                    | RawTokenKind::Number(_)
                    | RawTokenKind::String(_)
                    | RawTokenKind::Char(_)
                    | RawTokenKind::As
                    | RawTokenKind::Hiding
                    | RawTokenKind::Role
                    | RawTokenKind::Hole(_)
                    | RawTokenKind::LParen
                    | RawTokenKind::LBracket
                    | RawTokenKind::LBrace
                    | RawTokenKind::Backslash
                    | RawTokenKind::If
                    | RawTokenKind::Let
                    | RawTokenKind::Case
                    | RawTokenKind::Do
                    | RawTokenKind::Ado
            )
        )
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let token = self.current().clone();
        match token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_)) => {
                let name = self.parse_qualified_value_name()?;
                if self.at_raw(&RawTokenKind::Dot) {
                    let dot = self.current();
                    if dot.span.start == name.span.end {
                        match self.peek(1).kind {
                            LayoutTokenKind::Raw(RawTokenKind::Do) => {
                                self.bump();
                                return self.parse_do(false);
                            }
                            LayoutTokenKind::Raw(RawTokenKind::Ado) => {
                                self.bump();
                                return self.parse_do(true);
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Expr {
                    kind: ExprKind::Name(name.clone()),
                    span: name.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::As | RawTokenKind::Hiding | RawTokenKind::Role) => {
                let name = self.parse_qualified_value_name()?;
                Ok(Expr {
                    kind: ExprKind::Name(name.clone()),
                    span: name.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => {
                let name = self.parse_qualified_value_name()?;
                if self.at_raw(&RawTokenKind::Dot) {
                    let dot = self.current();
                    if dot.span.start == name.span.end {
                        match self.peek(1).kind {
                            LayoutTokenKind::Raw(RawTokenKind::Do) => {
                                self.bump();
                                return self.parse_do(false);
                            }
                            LayoutTokenKind::Raw(RawTokenKind::Ado) => {
                                self.bump();
                                return self.parse_do(true);
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Expr {
                    kind: ExprKind::Name(name.clone()),
                    span: name.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Integer(value)) => {
                if self.is_decimal_point() {
                    return self.parse_number(value, token.span);
                }
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Integer(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Number(value)) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Number(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::String(value)) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::String(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Char(value)) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Char(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Hole(name)) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Hole(name),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) if operator == "-" => {
                let minus_span = self.bump().span;
                let expression = self.parse_atom()?;
                let span = TextRange::new(minus_span.start, expression.span.end);
                Ok(Expr {
                    kind: ExprKind::Negate {
                        minus_span,
                        expression: Box::new(expression),
                    },
                    span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::LParen) => self.parse_parenthesized_expression(),
            LayoutTokenKind::Raw(RawTokenKind::LBracket) => self.parse_array(),
            LayoutTokenKind::Raw(RawTokenKind::LBrace) => self.parse_record_literal(),
            LayoutTokenKind::Raw(RawTokenKind::Backslash) => self.parse_lambda(),
            LayoutTokenKind::Raw(RawTokenKind::If) => self.parse_if(),
            LayoutTokenKind::Raw(RawTokenKind::Let) => self.parse_let(),
            LayoutTokenKind::Raw(RawTokenKind::Case) => self.parse_case(),
            LayoutTokenKind::Raw(RawTokenKind::Do) => self.parse_do(false),
            LayoutTokenKind::Raw(RawTokenKind::Ado) => self.parse_do(true),
            _ => Err(self.error(format!("expected expression, found {}", self.found()))),
        }
    }

    fn is_decimal_point(&self) -> bool {
        if self.peek(1).kind != LayoutTokenKind::Raw(RawTokenKind::Dot) {
            return false;
        }
        let dot = self.peek(1);
        let fraction = self.peek(2);
        let LayoutTokenKind::Raw(RawTokenKind::Integer(_)) = fraction.kind else {
            return false;
        };
        let integer_end = self.current().span.end;
        dot.span.start == integer_end && fraction.span.start == dot.span.end
    }

    fn parse_number(
        &mut self,
        integer: String,
        integer_span: TextRange,
    ) -> Result<Expr, ParseError> {
        self.bump();
        self.bump();
        let fraction_token = self.current().clone();
        let LayoutTokenKind::Raw(RawTokenKind::Integer(fraction)) = fraction_token.kind else {
            return Err(self.error("expected digits after `.`".into()));
        };
        self.bump();
        let span = TextRange::new(integer_span.start, fraction_token.span.end);
        Ok(Expr {
            kind: ExprKind::Number(format!("{integer}.{fraction}")),
            span,
        })
    }

    fn parse_qualified_value_name(&mut self) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        let (text, _) = match &token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(text))
            | LayoutTokenKind::Raw(RawTokenKind::UpperIdent(text)) => (text.clone(), token.span),
            LayoutTokenKind::Raw(RawTokenKind::As) => ("as".to_owned(), token.span),
            LayoutTokenKind::Raw(RawTokenKind::Hiding) => ("hiding".to_owned(), token.span),
            LayoutTokenKind::Raw(RawTokenKind::Role) => ("role".to_owned(), token.span),
            _ => return Err(self.error(format!("expected name, found {}", self.found()))),
        };
        self.bump();
        let mut name = CstName::new(text, token.span);
        while self.at_raw(&RawTokenKind::Dot) {
            let dot_span = self.current().span;
            if dot_span.start != name.span.end {
                break;
            }
            let part = self.peek(1).clone();
            let (text, span) = match part.kind {
                LayoutTokenKind::Raw(RawTokenKind::LowerIdent(text))
                | LayoutTokenKind::Raw(RawTokenKind::UpperIdent(text)) => (text, part.span),
                _ => break,
            };
            if span.start != dot_span.end {
                break;
            }
            self.bump();
            self.bump();
            name = CstName::new(
                format!("{}.{}", name.text, text),
                TextRange::new(name.span.start, span.end),
            );
        }
        if self.at_raw(&RawTokenKind::Dot) {
            let dot_span = self.current().span;
            if dot_span.start == name.span.end
                && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::LParen)
            {
                self.bump();
                self.bump();
                let operator_token = self.current().clone();
                let operator = match operator_token.kind {
                    LayoutTokenKind::Raw(RawTokenKind::Operator(text))
                    | LayoutTokenKind::Raw(RawTokenKind::LowerIdent(text))
                    | LayoutTokenKind::Raw(RawTokenKind::UpperIdent(text)) => {
                        self.bump();
                        text
                    }
                    _ => {
                        return Err(
                            self.error(format!("expected an operator, found {}", self.found()))
                        );
                    }
                };
                let close_span = self.consume_raw(RawTokenKind::RParen)?.span;
                name = CstName::new(
                    format!("{}.({})", name.text, operator),
                    TextRange::new(name.span.start, close_span.end),
                );
            }
        }
        Ok(name)
    }

    fn parse_parenthesized_expression(&mut self) -> Result<Expr, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        if self.at_raw(&RawTokenKind::RParen) {
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::Name(CstName::new("unit", span)),
                span,
            });
        }
        if let LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) = &self.current().kind
            && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::RParen)
        {
            let operator = operator.clone();
            let operator_span = self.bump().span;
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Expr {
                kind: ExprKind::Name(CstName::new(operator, operator_span)),
                span,
            });
        }
        let first = self.parse_expression(0)?;
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

fn label_keyword(kind: &RawTokenKind) -> Option<&'static str> {
    Some(match kind {
        RawTokenKind::Ado => "ado",
        RawTokenKind::As => "as",
        RawTokenKind::Case => "case",
        RawTokenKind::Class => "class",
        RawTokenKind::Data => "data",
        RawTokenKind::Derive => "derive",
        RawTokenKind::Do => "do",
        RawTokenKind::Else => "else",
        RawTokenKind::Forall => "forall",
        RawTokenKind::Foreign => "foreign",
        RawTokenKind::Hiding => "hiding",
        RawTokenKind::If => "if",
        RawTokenKind::Import => "import",
        RawTokenKind::In => "in",
        RawTokenKind::Infix => "infix",
        RawTokenKind::Infixl => "infixl",
        RawTokenKind::Infixr => "infixr",
        RawTokenKind::Instance => "instance",
        RawTokenKind::Let => "let",
        RawTokenKind::Module => "module",
        RawTokenKind::Newtype => "newtype",
        RawTokenKind::Of => "of",
        RawTokenKind::Role => "role",
        RawTokenKind::Then => "then",
        RawTokenKind::Type => "type",
        RawTokenKind::Where => "where",
        _ => return None,
    })
}
