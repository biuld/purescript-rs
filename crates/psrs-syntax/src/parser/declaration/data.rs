use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{
    DataConstructor, DataDeclaration, Declaration, DerivingClause, KindSignature,
    NewtypeDeclaration,
};
use psrs_span::TextRange;

use super::super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(crate) fn parse_data_declaration(&mut self) -> Result<Declaration, ParseError> {
        let keyword_span = self.consume_raw(RawTokenKind::Data)?.span;
        let name = self.consume_upper_name("data type name")?;
        if self.at_raw(&RawTokenKind::DoubleColon) {
            let double_colon_span = self.bump().span;
            let kind = self.parse_type()?;
            let span = TextRange::new(keyword_span.start, kind.span.end);
            return Ok(Declaration::KindSignature(KindSignature {
                keyword_span,
                name,
                double_colon_span,
                kind,
                span,
            }));
        }
        let parameters = self.parse_type_var_binders()?;
        let mut equals_span = None;
        let mut constructors = Vec::new();
        if self.at_raw(&RawTokenKind::Equals) {
            equals_span = Some(self.bump().span);
            constructors = self.parse_data_constructors()?;
        }
        let derives = self.parse_deriving_clauses()?;
        let end = derives
            .last()
            .map(|clause| clause.span.end)
            .or_else(|| constructors.last().map(|ctor| ctor.span.end))
            .or_else(|| parameters.last().map(|parameter| parameter.span.end))
            .unwrap_or(name.span.end);
        Ok(Declaration::Data(DataDeclaration {
            keyword_span,
            name: name.clone(),
            parameters,
            kind: None,
            equals_span,
            constructors,
            derives,
            span: TextRange::new(keyword_span.start, end.max(name.span.end)),
        }))
    }

    fn parse_data_constructors(&mut self) -> Result<Vec<DataConstructor>, ParseError> {
        let mut constructors = Vec::new();
        loop {
            let name = self.consume_upper_name("data constructor name")?;
            let mut fields = Vec::new();
            while self.starts_type_atom() {
                let field = self.parse_type_atom()?;
                if super::super::type_expr::type_contains_wildcard(&field) {
                    return Err(self.error_at(field.span, "wildcards are not allowed here".into()));
                }
                fields.push(field);
            }
            let end = fields
                .last()
                .map(|field| field.span.end)
                .unwrap_or(name.span.end);
            constructors.push(DataConstructor {
                span: TextRange::new(name.span.start, end),
                name,
                fields,
            });
            if self.at_raw(&RawTokenKind::Pipe) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(constructors)
    }

    fn parse_deriving_clauses(&mut self) -> Result<Vec<DerivingClause>, ParseError> {
        let mut clauses = Vec::new();
        while self.at_raw(&RawTokenKind::Derive)
            && (self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::LParen)
                || self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::Newtype))
        {
            let derive_keyword_span = self.bump().span;
            if self.at_raw(&RawTokenKind::Newtype) {
                self.bump();
            }
            let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
            let mut constraints = Vec::new();
            if !self.at_raw(&RawTokenKind::RParen) {
                loop {
                    constraints.push(self.parse_type()?);
                    if self.at_raw(&RawTokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
            clauses.push(DerivingClause {
                span: TextRange::new(derive_keyword_span.start, close_paren_span.end),
                derive_keyword_span,
                open_paren_span,
                constraints,
                close_paren_span,
            });
        }
        Ok(clauses)
    }

    pub(crate) fn parse_newtype_declaration(&mut self) -> Result<Declaration, ParseError> {
        let keyword_span = self.consume_raw(RawTokenKind::Newtype)?.span;
        let name = self.consume_upper_name("newtype name")?;
        if self.at_raw(&RawTokenKind::DoubleColon) {
            let double_colon_span = self.bump().span;
            let kind = self.parse_type()?;
            let span = TextRange::new(keyword_span.start, kind.span.end);
            return Ok(Declaration::KindSignature(KindSignature {
                keyword_span,
                name,
                double_colon_span,
                kind,
                span,
            }));
        }
        let parameters = self.parse_type_var_binders()?;
        let mut equals_span = None;
        let mut constructor = None;
        if self.at_raw(&RawTokenKind::Equals) {
            equals_span = Some(self.bump().span);
            let ctor_name = self.consume_upper_name("newtype constructor name")?;
            let field = self.parse_type_atom()?;
            if self.starts_type_atom() {
                return Err(self.error("a newtype constructor takes exactly one field".into()));
            }
            let span = TextRange::new(ctor_name.span.start, field.span.end);
            constructor = Some(DataConstructor {
                name: ctor_name,
                fields: vec![field],
                span,
            });
        }
        let derives = self.parse_deriving_clauses()?;
        let end = derives
            .last()
            .map(|clause| clause.span.end)
            .or_else(|| constructor.as_ref().map(|ctor| ctor.span.end))
            .or_else(|| parameters.last().map(|parameter| parameter.span.end))
            .unwrap_or(name.span.end);
        Ok(Declaration::Newtype(NewtypeDeclaration {
            keyword_span,
            name: name.clone(),
            parameters,
            kind: None,
            equals_span,
            constructor,
            derives,
            span: TextRange::new(keyword_span.start, end.max(name.span.end)),
        }))
    }
}
