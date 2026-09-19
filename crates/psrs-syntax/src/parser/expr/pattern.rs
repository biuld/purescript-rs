use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstName, Pattern, PatternKind, RecordPatternField};
use psrs_span::TextRange;

use super::super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(crate) fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let mut pattern = self.parse_pattern_prefix()?;
        loop {
            let operator = match &self.current().kind {
                LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) if operator != "@" => {
                    operator.clone()
                }
                LayoutTokenKind::Raw(RawTokenKind::Colon) => ":".to_owned(),
                _ => break,
            };
            let operator_span = self.bump().span;
            let right = self.parse_pattern_prefix()?;
            let span = TextRange::new(pattern.span.start, right.span.end);
            pattern = Pattern {
                kind: PatternKind::Constructor {
                    name: CstName::new(operator, operator_span),
                    arguments: vec![pattern, right],
                },
                span,
            };
        }
        if let PatternKind::Var(name) = &pattern.kind
            && self.at_operator_text("@")
        {
            let at_span = self.bump().span;
            let inner = self.parse_pattern()?;
            let span = TextRange::new(name.span.start, inner.span.end);
            return Ok(Pattern {
                kind: PatternKind::Named {
                    name: name.clone(),
                    at_span,
                    pattern: Box::new(inner),
                },
                span,
            });
        }
        if self.at_raw(&RawTokenKind::DoubleColon) {
            let double_colon_span = self.bump().span;
            let type_expr = self.parse_type()?;
            let span = TextRange::new(pattern.span.start, type_expr.span.end);
            return Ok(Pattern {
                kind: PatternKind::Typed {
                    pattern: Box::new(pattern),
                    double_colon_span,
                    type_expr,
                },
                span,
            });
        }
        Ok(pattern)
    }

    fn parse_pattern_prefix(&mut self) -> Result<Pattern, ParseError> {
        let pattern = self.parse_pattern_atom()?;
        let start = pattern.span.start;
        let mut pattern = pattern;
        if let PatternKind::Constructor {
            name,
            mut arguments,
        } = pattern.kind
        {
            while self.starts_pattern_atom() {
                arguments.push(self.parse_pattern_atom()?);
            }
            let end = arguments
                .last()
                .map(|argument| argument.span.end)
                .unwrap_or(start);
            pattern = Pattern {
                kind: PatternKind::Constructor { name, arguments },
                span: TextRange::new(start, end),
            };
        }
        Ok(pattern)
    }

    fn at_operator_text(&self, text: &str) -> bool {
        matches!(&self.current().kind, LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) if operator == text)
    }

    pub(crate) fn starts_pattern_atom(&self) -> bool {
        matches!(
            &self.current().kind,
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(_)
                    | RawTokenKind::UpperIdent(_)
                    | RawTokenKind::Integer(_)
                    | RawTokenKind::Number(_)
                    | RawTokenKind::String(_)
                    | RawTokenKind::Char(_)
                    | RawTokenKind::LParen
                    | RawTokenKind::LBracket
                    | RawTokenKind::LBrace
                    | RawTokenKind::As
                    | RawTokenKind::Hiding
                    | RawTokenKind::Role
            )
        )
    }

    pub(crate) fn parse_pattern_atom(&mut self) -> Result<Pattern, ParseError> {
        let token = self.current().clone();
        match token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name)) => {
                self.bump();
                if name == "_" {
                    return Ok(Pattern {
                        kind: PatternKind::Wildcard(token.span),
                        span: token.span,
                    });
                }
                if name == "true" || name == "false" {
                    return Ok(Pattern {
                        kind: PatternKind::Boolean(name == "true"),
                        span: token.span,
                    });
                }
                let pattern = Pattern {
                    kind: PatternKind::Var(CstName::new(name, token.span)),
                    span: token.span,
                };
                if self.at_operator_text("@") {
                    let at_span = self.bump().span;
                    let inner = self.parse_pattern_atom()?;
                    let span = TextRange::new(pattern.span.start, inner.span.end);
                    let PatternKind::Var(name) = pattern.kind else {
                        unreachable!("just constructed a variable pattern")
                    };
                    return Ok(Pattern {
                        kind: PatternKind::Named {
                            name,
                            at_span,
                            pattern: Box::new(inner),
                        },
                        span,
                    });
                }
                Ok(pattern)
            }
            LayoutTokenKind::Raw(RawTokenKind::Number(value)) => {
                self.bump();
                Ok(Pattern {
                    kind: PatternKind::Number(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Operator(operator))
                if operator == "-"
                    && matches!(
                        self.peek(1).kind,
                        LayoutTokenKind::Raw(RawTokenKind::Integer(_) | RawTokenKind::Number(_))
                    ) =>
            {
                let minus_span = self.bump().span;
                let value_token = self.current().clone();
                let value = match value_token.kind {
                    LayoutTokenKind::Raw(RawTokenKind::Integer(value))
                    | LayoutTokenKind::Raw(RawTokenKind::Number(value)) => value,
                    _ => unreachable!("checked for a numeric token"),
                };
                self.bump();
                let span = TextRange::new(minus_span.start, value_token.span.end);
                Ok(Pattern {
                    kind: PatternKind::Number(format!("-{value}")),
                    span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => {
                let name = self.parse_qualified_pattern_name()?;
                let span = name.span;
                Ok(Pattern {
                    kind: PatternKind::Constructor {
                        name,
                        arguments: Vec::new(),
                    },
                    span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Integer(value)) => {
                self.bump();
                Ok(Pattern {
                    kind: PatternKind::Integer(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::String(value)) => {
                self.bump();
                Ok(Pattern {
                    kind: PatternKind::String(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Char(value)) => {
                self.bump();
                Ok(Pattern {
                    kind: PatternKind::Char(value),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(
                kind @ (RawTokenKind::As | RawTokenKind::Hiding | RawTokenKind::Role),
            ) => {
                self.bump();
                let text = match kind {
                    RawTokenKind::As => "as",
                    RawTokenKind::Hiding => "hiding",
                    _ => "role",
                };
                Ok(Pattern {
                    kind: PatternKind::Var(CstName::new(text, token.span)),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::LParen) => self.parse_parenthesized_pattern(),
            LayoutTokenKind::Raw(RawTokenKind::LBracket) => self.parse_array_pattern(),
            LayoutTokenKind::Raw(RawTokenKind::LBrace) => self.parse_record_pattern(),
            _ => Err(self.error(format!("expected pattern, found {}", self.found()))),
        }
    }

    fn parse_qualified_pattern_name(&mut self) -> Result<CstName, ParseError> {
        let mut name = self.consume_upper_name("constructor name")?;
        while self.at_raw(&RawTokenKind::Dot) {
            let dot_span = self.current().span;
            if dot_span.start != name.span.end {
                break;
            }
            let part = self.peek(1).clone();
            let (text, span) = match part.kind {
                LayoutTokenKind::Raw(RawTokenKind::UpperIdent(text))
                | LayoutTokenKind::Raw(RawTokenKind::LowerIdent(text)) => (text, part.span),
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
        Ok(name)
    }

    fn parse_parenthesized_pattern(&mut self) -> Result<Pattern, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        if self.at_raw(&RawTokenKind::RParen) {
            let close_paren_span = self.bump().span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Pattern {
                kind: PatternKind::Constructor {
                    name: CstName::new("unit", span),
                    arguments: Vec::new(),
                },
                span,
            });
        }
        let first = self.parse_pattern()?;
        if self.at_raw(&RawTokenKind::Comma) {
            let mut elements = vec![first];
            while self.at_raw(&RawTokenKind::Comma) {
                self.bump();
                elements.push(self.parse_pattern()?);
            }
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            let span = TextRange::new(open_paren_span.start, close_paren_span.end);
            return Ok(Pattern {
                kind: PatternKind::Tuple {
                    open_paren_span,
                    elements,
                    close_paren_span,
                },
                span,
            });
        }
        let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
        let span = TextRange::new(open_paren_span.start, close_paren_span.end);
        Ok(Pattern {
            kind: PatternKind::Parens {
                open_paren_span,
                pattern: Box::new(first),
                close_paren_span,
            },
            span,
        })
    }

    fn parse_array_pattern(&mut self) -> Result<Pattern, ParseError> {
        let open_bracket_span = self.consume_raw(RawTokenKind::LBracket)?.span;
        let mut elements = Vec::new();
        if !self.at_raw(&RawTokenKind::RBracket) {
            loop {
                elements.push(self.parse_pattern()?);
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close_bracket_span = self.consume_raw(RawTokenKind::RBracket)?.span;
        let span = TextRange::new(open_bracket_span.start, close_bracket_span.end);
        Ok(Pattern {
            kind: PatternKind::Array {
                open_bracket_span,
                elements,
                close_bracket_span,
            },
            span,
        })
    }

    fn parse_record_pattern(&mut self) -> Result<Pattern, ParseError> {
        let open_brace_span = self.consume_raw(RawTokenKind::LBrace)?.span;
        let mut fields = Vec::new();
        let mut tail = None;
        if !self.at_raw(&RawTokenKind::RBrace) {
            loop {
                let label = self.parse_label("record label")?;
                let value = if self.at_raw(&RawTokenKind::Colon) {
                    let colon_span = self.bump().span;
                    let pattern = self.parse_pattern()?;
                    Some((colon_span, pattern))
                } else {
                    None
                };
                let end = value
                    .as_ref()
                    .map(|(_, pattern)| pattern.span.end)
                    .unwrap_or(label.span.end);
                fields.push(RecordPatternField {
                    span: TextRange::new(label.span.start, end),
                    label,
                    value,
                });
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
            if self.at_raw(&RawTokenKind::DotDot) {
                self.bump();
                if let LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name)) = &self.current().kind {
                    let name = name.clone();
                    let token = self.bump();
                    tail = Some(CstName::new(name, token.span));
                }
            }
        }
        let close_brace_span = self.consume_raw(RawTokenKind::RBrace)?.span;
        let span = TextRange::new(open_brace_span.start, close_brace_span.end);
        Ok(Pattern {
            kind: PatternKind::Record {
                open_brace_span,
                fields,
                tail,
                close_brace_span,
            },
            span,
        })
    }
}
