use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{CaseAlternative, CaseRhs, DoStatement, Expr, ExprKind, GuardedCaseRhs};
use psrs_span::TextRange;

use super::super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(super) fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let backslash_span = self.consume_raw(RawTokenKind::Backslash)?.span;
        if self.at_raw(&RawTokenKind::Case) {
            return self.parse_lambda_case(backslash_span);
        }
        let mut parameters = Vec::new();
        while self.starts_pattern_atom() {
            parameters.push(self.parse_pattern_atom()?);
        }
        if parameters.is_empty() {
            return Err(self.error("expected at least one lambda parameter".into()));
        }
        let arrow_span = self.consume_raw(RawTokenKind::Arrow)?.span;
        let body = self.parse_expression(0)?;
        let end = body.span.end;
        Ok(Expr {
            kind: ExprKind::Lambda {
                backslash_span,
                parameters,
                arrow_span,
                body: Box::new(body),
            },
            span: TextRange::new(backslash_span.start, end),
        })
    }

    fn parse_lambda_case(&mut self, backslash_span: TextRange) -> Result<Expr, ParseError> {
        let case_keyword_span = self.consume_raw(RawTokenKind::Case)?.span;
        let (alternatives, layout_start_span, layout_end_span) = self.parse_case_alternatives()?;
        let end = layout_end_span.end;
        Ok(Expr {
            kind: ExprKind::Case {
                case_keyword_span,
                scrutinees: Vec::new(),
                of_keyword_span: backslash_span,
                layout_start_span,
                alternatives,
                layout_end_span,
            },
            span: TextRange::new(backslash_span.start, end),
        })
    }

    pub(super) fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let if_keyword_span = self.consume_raw(RawTokenKind::If)?.span;
        let condition = self.parse_expression(0)?;
        let then_keyword_span = self.consume_raw(RawTokenKind::Then)?.span;
        let then_branch = self.parse_expression(0)?;
        let else_keyword_span = self.consume_raw(RawTokenKind::Else)?.span;
        let else_branch = self.parse_expression(0)?;
        let end = else_branch.span.end;
        Ok(Expr {
            kind: ExprKind::If {
                if_keyword_span,
                condition: Box::new(condition),
                then_keyword_span,
                then_branch: Box::new(then_branch),
                else_keyword_span,
                else_branch: Box::new(else_branch),
            },
            span: TextRange::new(if_keyword_span.start, end),
        })
    }

    pub(super) fn parse_let(&mut self) -> Result<Expr, ParseError> {
        let let_keyword_span = self.consume_raw(RawTokenKind::Let)?.span;
        let layout_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
        let declarations = self.parse_declarations_until(&[LayoutTokenKind::LayoutEnd], true)?;
        let layout_end_span = self.consume_layout(LayoutTokenKind::LayoutEnd)?.span;
        let in_keyword_span = self.consume_raw(RawTokenKind::In)?.span;
        let body = self.parse_expression(0)?;
        let end = body.span.end;
        Ok(Expr {
            kind: ExprKind::Let {
                let_keyword_span,
                declarations,
                layout_start_span,
                layout_end_span,
                in_keyword_span,
                body: Box::new(body),
            },
            span: TextRange::new(let_keyword_span.start, end),
        })
    }

    pub(super) fn parse_case(&mut self) -> Result<Expr, ParseError> {
        let case_keyword_span = self.consume_raw(RawTokenKind::Case)?.span;
        let mut scrutinees = vec![self.parse_expression(0)?];
        while self.at_raw(&RawTokenKind::Comma) {
            self.bump();
            scrutinees.push(self.parse_expression(0)?);
        }
        let of_keyword_span = self.consume_raw(RawTokenKind::Of)?.span;
        let (alternatives, layout_start_span, layout_end_span) = self.parse_case_alternatives()?;
        let end = layout_end_span.end;
        Ok(Expr {
            kind: ExprKind::Case {
                case_keyword_span,
                scrutinees,
                of_keyword_span,
                layout_start_span,
                alternatives,
                layout_end_span,
            },
            span: TextRange::new(case_keyword_span.start, end),
        })
    }

    fn parse_case_alternatives(
        &mut self,
    ) -> Result<(Vec<CaseAlternative>, TextRange, TextRange), ParseError> {
        let layout_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
        let mut alternatives: Vec<CaseAlternative> = Vec::new();
        loop {
            while self.current().kind == LayoutTokenKind::LayoutSep {
                self.bump();
            }
            if self.current().kind == LayoutTokenKind::LayoutEnd || self.at_eof() {
                break;
            }
            if self.at_raw(&RawTokenKind::Where) {
                let block = self.parse_declaration_block()?;
                let block_end = block.span.end;
                let Some(last) = alternatives.pop() else {
                    return Err(self.error("expected a case alternative".into()));
                };
                let start = last.span.start;
                let rhs = match last.rhs {
                    CaseRhs::Plain {
                        arrow_span,
                        value,
                        where_block: None,
                    } => CaseRhs::Plain {
                        arrow_span,
                        value,
                        where_block: Some(block),
                    },
                    CaseRhs::Plain { .. } => {
                        return Err(self.error("duplicate where block".into()));
                    }
                    CaseRhs::Guarded(mut clauses) => {
                        let Some(clause) = clauses.last_mut() else {
                            return Err(self.error("expected a guarded case".into()));
                        };
                        if clause.where_block.is_some() {
                            return Err(self.error("duplicate where block".into()));
                        }
                        clause.where_block = Some(block);
                        CaseRhs::Guarded(clauses)
                    }
                };
                alternatives.push(CaseAlternative {
                    patterns: last.patterns,
                    rhs,
                    span: TextRange::new(start, block_end),
                });
                continue;
            }
            alternatives.push(self.parse_case_alternative()?);
            match self.current().kind {
                LayoutTokenKind::LayoutSep => {
                    self.bump();
                }
                LayoutTokenKind::LayoutEnd => {}
                _ if self.at_eof() => {}
                _ => return Err(self.error("expected a case separator".into())),
            }
        }
        let layout_end_span = self.consume_layout(LayoutTokenKind::LayoutEnd)?.span;
        Ok((alternatives, layout_start_span, layout_end_span))
    }

    fn parse_case_alternative(&mut self) -> Result<CaseAlternative, ParseError> {
        let mut patterns = vec![self.parse_pattern()?];
        while self.at_raw(&RawTokenKind::Comma) {
            self.bump();
            patterns.push(self.parse_pattern()?);
        }
        let rhs = if self.at_raw(&RawTokenKind::Arrow) {
            let arrow_span = self.bump().span;
            let value = self.parse_expression(0)?;
            let where_block = if self.at_raw(&RawTokenKind::Where) {
                Some(self.parse_declaration_block()?)
            } else {
                None
            };
            CaseRhs::Plain {
                arrow_span,
                value,
                where_block,
            }
        } else if self.at_raw(&RawTokenKind::Pipe) {
            let mut guarded = Vec::new();
            while self.at_raw(&RawTokenKind::Pipe) {
                let bar_span = self.bump().span;
                let guards = self.parse_guard_list()?;
                let arrow_span = self.consume_raw(RawTokenKind::Arrow)?.span;
                let value = self.parse_expression(0)?;
                let where_block = if self.at_raw(&RawTokenKind::Where) {
                    Some(self.parse_declaration_block()?)
                } else {
                    None
                };
                let end = where_block
                    .as_ref()
                    .map(|block| block.span.end)
                    .unwrap_or(value.span.end);
                let span = TextRange::new(bar_span.start, end);
                guarded.push(GuardedCaseRhs {
                    bar_span,
                    guards,
                    arrow_span,
                    value,
                    where_block,
                    span,
                });
            }
            CaseRhs::Guarded(guarded)
        } else {
            return Err(self.error("expected `->` or a guard in case alternative".into()));
        };
        let start = patterns
            .first()
            .map(|pattern| pattern.span.start)
            .unwrap_or(0);
        let end = match &rhs {
            CaseRhs::Plain {
                value, where_block, ..
            } => where_block
                .as_ref()
                .map(|block| block.span.end)
                .unwrap_or(value.span.end),
            CaseRhs::Guarded(clauses) => clauses.last().map(|clause| clause.span.end).unwrap_or(0),
        };
        Ok(CaseAlternative {
            patterns,
            rhs,
            span: TextRange::new(start, end),
        })
    }

    pub(super) fn parse_do(&mut self, is_ado: bool) -> Result<Expr, ParseError> {
        let do_keyword_span = self.bump().span;
        let layout_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
        let mut statements = Vec::new();
        loop {
            while self.current().kind == LayoutTokenKind::LayoutSep {
                self.bump();
            }
            if self.current().kind == LayoutTokenKind::LayoutEnd
                || self.at_eof()
                || (is_ado && self.at_raw(&RawTokenKind::In))
            {
                break;
            }
            if self.at_raw(&RawTokenKind::Let) {
                let let_keyword_span = self.bump().span;
                let inner_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
                let declarations =
                    self.parse_declarations_until(&[LayoutTokenKind::LayoutEnd], true)?;
                let inner_end_span = self.consume_layout(LayoutTokenKind::LayoutEnd)?.span;
                statements.push(DoStatement::Let {
                    let_keyword_span,
                    declarations,
                    layout_start_span: inner_start_span,
                    layout_end_span: inner_end_span,
                });
            } else {
                let checkpoint = self.cursor;
                let mut bound = None;
                if let Ok(pattern) = self.parse_pattern()
                    && self.at_raw(&RawTokenKind::LeftArrow)
                {
                    let left_arrow_span = self.bump().span;
                    let value = self.parse_expression(0)?;
                    bound = Some(DoStatement::Bind {
                        pattern,
                        left_arrow_span,
                        value,
                    });
                }
                if let Some(statement) = bound {
                    statements.push(statement);
                } else {
                    self.cursor = checkpoint;
                    statements.push(DoStatement::Discard(self.parse_expression(0)?));
                }
            }
            match self.current().kind {
                LayoutTokenKind::LayoutSep => {
                    self.bump();
                }
                LayoutTokenKind::LayoutEnd => {}
                _ if self.at_eof() => {}
                _ if is_ado && self.at_raw(&RawTokenKind::In) => {}
                _ => return Err(self.error("expected a do separator".into())),
            }
        }
        let (in_keyword_span, result) = if is_ado && self.at_raw(&RawTokenKind::In) {
            let in_keyword_span = self.bump().span;
            let result = self.parse_expression(0)?;
            (Some(in_keyword_span), Some(Box::new(result)))
        } else {
            (None, None)
        };
        let layout_end_span = if self.current().kind == LayoutTokenKind::LayoutEnd {
            self.bump().span
        } else {
            layout_start_span
        };
        let end = result
            .as_ref()
            .map(|result| result.span.end)
            .unwrap_or(layout_end_span.end);
        Ok(Expr {
            kind: ExprKind::Do {
                do_keyword_span,
                layout_start_span,
                statements,
                layout_end_span,
                in_keyword_span,
                result,
            },
            span: TextRange::new(do_keyword_span.start, end),
        })
    }
}
