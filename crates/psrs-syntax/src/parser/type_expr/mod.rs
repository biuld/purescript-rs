use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstName, TypeExpr, TypeExprKind, TypeField, TypeVarBinder};
use psrs_span::TextRange;

use super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(super) fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        if self.current().kind == LayoutTokenKind::Raw(RawTokenKind::Forall) {
            self.parse_forall_type()
        } else {
            self.parse_constrained_type()
        }
    }

    fn parse_forall_type(&mut self) -> Result<TypeExpr, ParseError> {
        let forall_span = self.consume_raw(RawTokenKind::Forall)?.span;
        let variables = self.parse_type_var_binders()?;
        if variables.is_empty() {
            return Err(self.error("expected at least one type variable after `forall`".into()));
        }
        let dot_span = self.consume_raw(RawTokenKind::Dot)?.span;
        let body = self.parse_type()?;
        let span = TextRange::new(forall_span.start, body.span.end);
        Ok(TypeExpr {
            kind: TypeExprKind::Forall {
                forall_span,
                variables,
                dot_span,
                body: Box::new(body),
            },
            span,
        })
    }

    pub(super) fn parse_type_var_binders(&mut self) -> Result<Vec<TypeVarBinder>, ParseError> {
        let mut binders = Vec::new();
        loop {
            if self.at_raw(&RawTokenKind::LParen) {
                let checkpoint = self.cursor;
                self.bump();
                if self.at_raw(&RawTokenKind::Operator("@".into())) {
                    self.bump();
                }
                let is_binder = matches!(
                    &self.current().kind,
                    LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_))
                ) && self.peek(1).kind
                    == LayoutTokenKind::Raw(RawTokenKind::DoubleColon);
                if !is_binder {
                    self.cursor = checkpoint;
                    break;
                }
                let name = self.consume_lower_name("type variable")?;
                self.consume_raw(RawTokenKind::DoubleColon)?;
                let kind = self.parse_type()?;
                let close_span = self.consume_raw(RawTokenKind::RParen)?.span;
                let span = TextRange::new(name.span.start, close_span.end);
                binders.push(TypeVarBinder {
                    name,
                    kind: Some(kind),
                    span,
                });
            } else if self.at_raw(&RawTokenKind::Operator("@".into()))
                && matches!(
                    self.peek(1).kind,
                    LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_))
                )
            {
                self.bump();
                let name = self.consume_lower_name("type variable")?;
                let span = name.span;
                binders.push(TypeVarBinder {
                    name,
                    kind: None,
                    span,
                });
            } else if let LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name)) =
                &self.current().kind
            {
                let name = name.clone();
                let token = self.bump();
                let span = token.span;
                binders.push(TypeVarBinder {
                    name: CstName::new(name, span),
                    kind: None,
                    span,
                });
            } else {
                break;
            }
        }
        Ok(binders)
    }

    fn parse_constrained_type(&mut self) -> Result<TypeExpr, ParseError> {
        let left = self.parse_function_type()?;
        if !self.at_raw(&RawTokenKind::FatArrow) {
            return Ok(left);
        }
        let arrow_span = self.bump().span;
        let body = self.parse_type()?;
        let span = TextRange::new(left.span.start, body.span.end);
        Ok(TypeExpr {
            kind: TypeExprKind::Constrained {
                constraint: Box::new(left),
                arrow_span,
                body: Box::new(body),
            },
            span,
        })
    }

    fn parse_function_type(&mut self) -> Result<TypeExpr, ParseError> {
        let left = self.parse_type_operator(0)?;
        if self.current().kind != LayoutTokenKind::Raw(RawTokenKind::Arrow) {
            return Ok(left);
        }
        let arrow_span = self.bump().span;
        let right = self.parse_type()?;
        let span = TextRange::new(left.span.start, right.span.end);
        Ok(TypeExpr {
            kind: TypeExprKind::Function {
                left: Box::new(left),
                arrow_span,
                right: Box::new(right),
            },
            span,
        })
    }

    fn parse_type_operator(&mut self, min_precedence: u8) -> Result<TypeExpr, ParseError> {
        let mut left = self.parse_kind_annotated_type()?;
        while let LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) = &self.current().kind {
            let precedence = type_operator_precedence(operator);
            if precedence < min_precedence {
                break;
            }
            let operator = operator.clone();
            let operator_token = self.bump();
            let right = self.parse_type_operator(precedence + 1)?;
            let span = TextRange::new(left.span.start, right.span.end);
            left = TypeExpr {
                kind: TypeExprKind::Operator {
                    operator: CstName::new(operator, operator_token.span),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_kind_annotated_type(&mut self) -> Result<TypeExpr, ParseError> {
        let expression = self.parse_type_application()?;
        if !self.at_raw(&RawTokenKind::DoubleColon) {
            return Ok(expression);
        }
        let double_colon_span = self.bump().span;
        let kind = self.parse_type()?;
        let span = TextRange::new(expression.span.start, kind.span.end);
        Ok(TypeExpr {
            kind: TypeExprKind::KindAnnotation {
                expression: Box::new(expression),
                double_colon_span,
                kind: Box::new(kind),
            },
            span,
        })
    }

    fn parse_type_application(&mut self) -> Result<TypeExpr, ParseError> {
        if let LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) = &self.current().kind {
            let operator = operator.clone();
            let operator_token = self.bump();
            let operand = self.parse_type_application()?;
            let span = TextRange::new(operator_token.span.start, operand.span.end);
            return Ok(TypeExpr {
                kind: TypeExprKind::PrefixOperator {
                    operator: CstName::new(operator, operator_token.span),
                    operand: Box::new(operand),
                },
                span,
            });
        }
        let head = self.parse_type_atom()?;
        let mut arguments = Vec::new();
        while self.starts_type_atom() {
            arguments.push(self.parse_type_atom()?);
        }
        if arguments.is_empty() {
            return Ok(head);
        }
        let span = TextRange::new(
            head.span.start,
            arguments
                .last()
                .map(|argument| argument.span.end)
                .unwrap_or(head.span.end),
        );
        Ok(TypeExpr {
            kind: TypeExprKind::Application(Box::new(head), arguments),
            span,
        })
    }

    pub(super) fn starts_type_atom(&self) -> bool {
        matches!(
            &self.current().kind,
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(_)
                    | RawTokenKind::UpperIdent(_)
                    | RawTokenKind::Integer(_)
                    | RawTokenKind::String(_)
                    | RawTokenKind::Hole(_)
                    | RawTokenKind::LParen
                    | RawTokenKind::LBrace
            )
        )
    }

    pub(super) fn parse_type_atom(&mut self) -> Result<TypeExpr, ParseError> {
        let token = self.current().clone();
        match token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_)) => {
                let name = self.parse_lower_type_name()?;
                if name.text == "_" {
                    return Ok(TypeExpr {
                        kind: TypeExprKind::Wildcard(name.span),
                        span: name.span,
                    });
                }
                Ok(TypeExpr {
                    kind: TypeExprKind::Name(name.clone()),
                    span: name.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => {
                let name = self.parse_qualified_type_name()?;
                Ok(TypeExpr {
                    kind: TypeExprKind::Name(name.clone()),
                    span: name.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Integer(value)) => {
                self.bump();
                Ok(TypeExpr {
                    kind: TypeExprKind::Integer(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::String(value)) => {
                self.bump();
                Ok(TypeExpr {
                    kind: TypeExprKind::String(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Hole(name)) => {
                self.bump();
                Ok(TypeExpr {
                    kind: TypeExprKind::Hole(name),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::LParen) => self.parse_parenthesized_type(),
            LayoutTokenKind::Raw(RawTokenKind::LBrace) => self.parse_brace_record_type(),
            _ => Err(self.error(format!("expected type, found {}", self.found()))),
        }
    }

    fn parse_lower_type_name(&mut self) -> Result<CstName, ParseError> {
        self.consume_lower_name("type name")
    }

    fn parse_qualified_type_name(&mut self) -> Result<CstName, ParseError> {
        let mut name = self.consume_upper_name("type name")?;
        while self.at_raw(&RawTokenKind::Dot)
            && matches!(
                &self.peek(1).kind,
                LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_))
                    | LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_))
            )
        {
            let dot_span = self.bump().span;
            if dot_span.start != name.span.end {
                break;
            }
            let part = self.current().clone();
            let (text, span) = match part.kind {
                LayoutTokenKind::Raw(RawTokenKind::UpperIdent(text))
                | LayoutTokenKind::Raw(RawTokenKind::LowerIdent(text)) => (text, part.span),
                _ => break,
            };
            if span.start != dot_span.end {
                break;
            }
            self.bump();
            name = CstName::new(
                format!("{}.{}", name.text, text),
                TextRange::new(name.span.start, span.end),
            );
        }
        Ok(name)
    }

    fn parse_parenthesized_type(&mut self) -> Result<TypeExpr, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        if self.at_raw(&RawTokenKind::RParen) {
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(TypeExpr {
                kind: TypeExprKind::Name(CstName::new("Unit", span)),
                span,
            });
        }
        if let Some(text) = operator_name_text(&self.current().kind)
            && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::RParen)
        {
            self.bump();
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(TypeExpr {
                kind: TypeExprKind::Name(CstName::new(text, span)),
                span,
            });
        }
        if self.is_row_start() {
            let row = self.parse_row_contents(open_paren_span, RawTokenKind::RParen)?;
            return Ok(row);
        }
        let first = self.parse_type()?;
        if self.at_raw(&RawTokenKind::Comma) {
            let mut items = vec![first];
            while self.at_raw(&RawTokenKind::Comma) {
                self.bump();
                items.push(self.parse_type()?);
            }
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(TypeExpr {
                kind: TypeExprKind::Tuple {
                    open_paren_span,
                    items,
                    close_paren_span,
                },
                span,
            });
        }
        let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
        let span = TextRange::new(open_paren_span.start, close_paren_span.end);
        Ok(TypeExpr {
            kind: TypeExprKind::Parens {
                open_paren_span,
                expression: Box::new(first),
                close_paren_span,
            },
            span,
        })
    }

    fn is_row_start(&self) -> bool {
        self.at_raw(&RawTokenKind::Pipe)
            || (self.starts_label_at(0)
                && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::DoubleColon))
    }

    fn parse_row_contents(
        &mut self,
        open_span: TextRange,
        close: RawTokenKind,
    ) -> Result<TypeExpr, ParseError> {
        let is_brace = close == RawTokenKind::RBrace;
        let mut fields = Vec::new();
        if !self.at_raw(&RawTokenKind::Pipe) && !self.at_raw(&close) {
            loop {
                let label = self.parse_label("row label")?;
                let double_colon_span = self.consume_raw(RawTokenKind::DoubleColon)?.span;
                let type_expr = self.parse_type()?;
                let span = TextRange::new(label.span.start, type_expr.span.end);
                fields.push(TypeField {
                    label,
                    double_colon_span,
                    type_expr,
                    span,
                });
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let tail = if self.at_raw(&RawTokenKind::Pipe) {
            self.bump();
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };
        let close_span = self.consume_raw(close)?.span;
        let span = TextRange::new(open_span.start, close_span.end);
        if is_brace {
            Ok(TypeExpr {
                kind: TypeExprKind::Record {
                    open_brace_span: open_span,
                    fields,
                    tail,
                    close_brace_span: close_span,
                },
                span,
            })
        } else {
            Ok(TypeExpr {
                kind: TypeExprKind::Row {
                    open_paren_span: open_span,
                    fields,
                    tail,
                    close_paren_span: close_span,
                },
                span,
            })
        }
    }

    fn parse_brace_record_type(&mut self) -> Result<TypeExpr, ParseError> {
        let open_brace_span = self.consume_raw(RawTokenKind::LBrace)?.span;
        if self.at_raw(&RawTokenKind::RBrace) {
            let close_brace_span = self.bump().span;
            let span = TextRange::new(open_brace_span.start, close_brace_span.end);
            return Ok(TypeExpr {
                kind: TypeExprKind::Record {
                    open_brace_span,
                    fields: Vec::new(),
                    tail: None,
                    close_brace_span,
                },
                span,
            });
        }
        self.parse_row_contents(open_brace_span, RawTokenKind::RBrace)
    }
}

fn type_operator_precedence(operator: &str) -> u8 {
    match operator {
        "<=" => 1,
        _ => 2,
    }
}

fn operator_name_text(kind: &LayoutTokenKind) -> Option<String> {
    let LayoutTokenKind::Raw(inner) = kind else {
        return None;
    };
    let text = match inner {
        RawTokenKind::Operator(text) => text.clone(),
        RawTokenKind::Colon => ":".to_owned(),
        RawTokenKind::DotDot => "..".to_owned(),
        RawTokenKind::Pipe => "|".to_owned(),
        RawTokenKind::Backslash => "\\".to_owned(),
        RawTokenKind::Arrow => "->".to_owned(),
        RawTokenKind::FatArrow => "=>".to_owned(),
        RawTokenKind::LeftArrow => "<-".to_owned(),
        _ => return None,
    };
    Some(text)
}

mod inspect;

pub(crate) use inspect::{type_contains_forall, type_contains_wildcard};
