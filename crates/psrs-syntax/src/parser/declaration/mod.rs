mod class;
mod data;

use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{
    Declaration, Guard, GuardedRhs, PatternDeclaration, TypeSignature, ValueDeclaration, ValueRhs,
};
use psrs_span::TextRange;

use super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(crate) fn parse_value_like_declaration(
        &mut self,
        allow_signatures: bool,
        allow_pattern: bool,
    ) -> Result<Declaration, ParseError> {
        let is_named = matches!(
            &self.current().kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name)) if name != "_"
        ) && !matches!(
            &self.peek(1).kind,
            LayoutTokenKind::Raw(RawTokenKind::Operator(_) | RawTokenKind::Colon)
        );
        if is_named {
            if allow_signatures
                && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::DoubleColon)
            {
                return self.parse_signature();
            }
            return self.parse_value_declaration().map(Declaration::Value);
        }
        if !allow_pattern {
            return Err(self.error("expected a value declaration".into()));
        }
        let pattern = self.parse_pattern()?;
        let equals_span = self.consume_raw(RawTokenKind::Equals)?.span;
        let value = self.parse_expression(0)?;
        let span = TextRange::new(pattern.span.start, value.span.end);
        Ok(Declaration::Pattern(PatternDeclaration {
            pattern,
            equals_span,
            value,
            span,
        }))
    }

    fn parse_signature(&mut self) -> Result<Declaration, ParseError> {
        let name = self.consume_lower_name("value name")?;
        let double_colon_span = self.consume_raw(RawTokenKind::DoubleColon)?.span;
        let type_expr = self.parse_type()?;
        let span = TextRange::new(name.span.start, type_expr.span.end);

        let mut offset = 0;
        while self.peek(offset).kind == LayoutTokenKind::LayoutSep {
            offset += 1;
        }
        let matches_declaration = matches!(
            &self.peek(offset).kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(next)) if next == &name.text
        );
        if matches_declaration {
            for _ in 0..offset {
                self.bump();
            }
            let mut declaration = self.parse_value_declaration()?;
            declaration.annotation = Some(type_expr);
            declaration.span = TextRange::new(name.span.start, declaration.span.end);
            return Ok(Declaration::Value(declaration));
        }
        Ok(Declaration::TypeSignature(TypeSignature {
            name,
            double_colon_span,
            type_expr,
            span,
        }))
    }

    fn parse_value_declaration(&mut self) -> Result<ValueDeclaration, ParseError> {
        let name = self.consume_lower_name("value name")?;
        let mut parameters = Vec::new();
        while self.starts_pattern_atom() {
            parameters.push(self.parse_pattern_atom()?);
        }
        let rhs = if self.at_raw(&RawTokenKind::Pipe) {
            ValueRhs::Guarded(self.parse_guarded_rhs()?)
        } else {
            let equals_span = self.consume_raw(RawTokenKind::Equals)?.span;
            let value = self.parse_expression(0)?;
            ValueRhs::Plain { equals_span, value }
        };
        let where_block = if self.at_raw(&RawTokenKind::Where) {
            Some(self.parse_declaration_block()?)
        } else {
            None
        };
        let end = where_block
            .as_ref()
            .map(|block| block.span.end)
            .unwrap_or_else(|| match &rhs {
                ValueRhs::Plain { value, .. } => value.span.end,
                ValueRhs::Guarded(clauses) => clauses.last().map(|c| c.span.end).unwrap_or(0),
            });
        Ok(ValueDeclaration {
            name: name.clone(),
            parameters,
            rhs,
            where_block,
            span: TextRange::new(name.span.start, end),
            annotation: None,
        })
    }

    fn parse_guarded_rhs(&mut self) -> Result<Vec<GuardedRhs>, ParseError> {
        let mut clauses = Vec::new();
        while self.at_raw(&RawTokenKind::Pipe) {
            let bar_span = self.bump().span;
            let guards = self.parse_guard_list()?;
            let equals_span = self.consume_raw(RawTokenKind::Equals)?.span;
            let value = self.parse_expression(0)?;
            let span = TextRange::new(bar_span.start, value.span.end);
            clauses.push(GuardedRhs {
                bar_span,
                guards,
                equals_span,
                value,
                span,
            });
        }
        Ok(clauses)
    }

    pub(crate) fn parse_guard_list(&mut self) -> Result<Vec<Guard>, ParseError> {
        let mut guards = Vec::new();
        loop {
            guards.push(self.parse_guard()?);
            if self.at_raw(&RawTokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(guards)
    }

    fn parse_guard(&mut self) -> Result<Guard, ParseError> {
        if self.at_raw(&RawTokenKind::Let) {
            let let_keyword_span = self.bump().span;
            let layout_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
            let declarations =
                self.parse_declarations_until(&[LayoutTokenKind::LayoutEnd], true, true)?;
            let layout_end_span = self.consume_layout(LayoutTokenKind::LayoutEnd)?.span;
            return Ok(Guard::Let {
                let_keyword_span,
                declarations,
                layout_start_span,
                layout_end_span,
            });
        }
        let checkpoint = self.cursor;
        if let Ok(pattern) = self.parse_pattern()
            && self.at_raw(&RawTokenKind::LeftArrow)
        {
            let left_arrow_span = self.bump().span;
            let value = self.parse_expression(0)?;
            return Ok(Guard::Pattern {
                pattern,
                left_arrow_span,
                value,
            });
        }
        self.cursor = checkpoint;
        Ok(Guard::Boolean(self.parse_expression(0)?))
    }
}
