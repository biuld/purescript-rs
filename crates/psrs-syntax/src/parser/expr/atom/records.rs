use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_array(&mut self) -> Result<Expr, ParseError> {
        let open_bracket_span = self.consume_raw(RawTokenKind::LBracket)?.span;
        let mut elements = Vec::new();
        if !self.at_raw(&RawTokenKind::RBracket) {
            loop {
                elements.push(self.parse_expression(0)?);
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close_bracket_span = self.consume_raw(RawTokenKind::RBracket)?.span;
        let span = TextRange::new(open_bracket_span.start, close_bracket_span.end);
        Ok(Expr {
            kind: ExprKind::Array {
                open_bracket_span,
                elements,
                close_bracket_span,
            },
            span,
        })
    }

    pub(super) fn parse_record_literal(&mut self) -> Result<Expr, ParseError> {
        let open_brace_span = self.consume_raw(RawTokenKind::LBrace)?.span;
        let mut fields = Vec::new();
        let mut tail = None;
        if !self.at_raw(&RawTokenKind::RBrace) {
            loop {
                if matches!(
                    self.current().kind,
                    LayoutTokenKind::Raw(RawTokenKind::String(_))
                ) && self.peek(1).kind != LayoutTokenKind::Raw(RawTokenKind::Colon)
                {
                    return Err(self.error("string record labels require a value".into()));
                }
                let label = self.parse_label("record label")?;
                if self.at_raw(&RawTokenKind::Colon) {
                    let colon_span = self.bump().span;
                    let value = self.parse_expression(0)?;
                    let span = TextRange::new(label.span.start, value.span.end);
                    fields.push(RecordField {
                        label,
                        colon_span,
                        value,
                        span,
                    });
                } else {
                    let span = label.span;
                    fields.push(RecordField {
                        label: label.clone(),
                        colon_span: span,
                        value: Expr {
                            kind: ExprKind::Name(label),
                            span,
                        },
                        span,
                    });
                }
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
            if self.at_raw(&RawTokenKind::Pipe) {
                self.bump();
                tail = Some(Box::new(self.parse_expression(0)?));
            }
        }
        let close_brace_span = self.consume_raw(RawTokenKind::RBrace)?.span;
        let span = TextRange::new(open_brace_span.start, close_brace_span.end);
        Ok(Expr {
            kind: ExprKind::Record {
                open_brace_span,
                fields,
                tail,
                close_brace_span,
            },
            span,
        })
    }

    pub(super) fn parse_record_update(&mut self, expression: Expr) -> Result<Expr, ParseError> {
        let open_brace_span = self.consume_raw(RawTokenKind::LBrace)?.span;
        let mut fields = Vec::new();
        if !self.at_raw(&RawTokenKind::RBrace) {
            loop {
                let label = self.parse_label("record label")?;
                let (equals_span, value) = if self.at_raw(&RawTokenKind::LBrace) {
                    let base = Expr {
                        kind: ExprKind::Name(label.clone()),
                        span: label.span,
                    };
                    let nested = self.parse_record_update(base)?;
                    (label.span, nested)
                } else {
                    let equals_span = self.consume_raw(RawTokenKind::Equals)?.span;
                    (equals_span, self.parse_expression(0)?)
                };
                let span = TextRange::new(label.span.start, value.span.end);
                fields.push(RecordUpdateField {
                    label,
                    equals_span,
                    value,
                    span,
                });
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close_brace_span = self.consume_raw(RawTokenKind::RBrace)?.span;
        let span = TextRange::new(expression.span.start, close_brace_span.end);
        Ok(Expr {
            kind: ExprKind::RecordUpdate {
                expression: Box::new(expression),
                open_brace_span,
                fields,
                close_brace_span,
            },
            span,
        })
    }
}
