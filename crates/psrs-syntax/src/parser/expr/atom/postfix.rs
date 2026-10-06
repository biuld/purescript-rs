//! Record projection and update bind to one atom before value application.
use super::*;
use psrs_cst::RecordAccessorField;

impl Parser<'_> {
    pub(super) fn parse_postfix_atom(&mut self) -> Result<Expr, ParseError> {
        let mut function = self.parse_atom()?;
        loop {
            if self.at_raw(&RawTokenKind::Dot) && self.starts_label_at(1) {
                let dot_span = self.bump().span;
                let field = self.parse_label("record field")?;
                let field_end = field.span.end;
                function = match function.kind {
                    ExprKind::Name(name) if name.text == "_" => {
                        let marker_span = name.span;
                        Expr {
                            kind: ExprKind::RecordAccessor {
                                marker_span,
                                fields: vec![RecordAccessorField { dot_span, field }],
                            },
                            span: TextRange::new(marker_span.start, field_end),
                        }
                    }
                    ExprKind::RecordAccessor {
                        marker_span,
                        mut fields,
                    } => {
                        fields.push(RecordAccessorField { dot_span, field });
                        Expr {
                            kind: ExprKind::RecordAccessor {
                                marker_span,
                                fields,
                            },
                            span: TextRange::new(marker_span.start, field_end),
                        }
                    }
                    kind => {
                        let span = TextRange::new(function.span.start, field.span.end);
                        Expr {
                            kind: ExprKind::FieldAccess {
                                expression: Box::new(Expr {
                                    kind,
                                    span: function.span,
                                }),
                                dot_span,
                                field,
                            },
                            span,
                        }
                    }
                };
                continue;
            }
            // Empty braces are a record argument, never a record update.
            // Nonempty updates must contain at least one assignment.
            if self.current().kind == LayoutTokenKind::Raw(RawTokenKind::LBrace)
                && self.peek(1).kind != LayoutTokenKind::Raw(RawTokenKind::RBrace)
            {
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
            break;
        }
        Ok(function)
    }
}
