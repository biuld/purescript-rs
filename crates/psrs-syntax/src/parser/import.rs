use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{ExportList, ExportRef, ImportDeclaration, ImportList, ImportRef, TypeMembers};
use psrs_span::TextRange;

use super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(super) fn parse_export_list(&mut self) -> Result<ExportList, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        let mut items = Vec::new();
        if !self.at_raw(&RawTokenKind::RParen) {
            loop {
                items.push(self.parse_export_ref()?);
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
        Ok(ExportList {
            span: TextRange::new(open_paren_span.start, close_paren_span.end),
            open_paren_span,
            items,
            close_paren_span,
        })
    }

    fn parse_export_ref(&mut self) -> Result<ExportRef, ParseError> {
        if self.at_raw(&RawTokenKind::Module) {
            self.bump();
            return Ok(ExportRef::Module(self.parse_module_name()?));
        }
        if self.at_raw(&RawTokenKind::Type) || self.at_raw(&RawTokenKind::Class) {
            self.bump();
            return self.parse_export_type_ref();
        }
        if self.at_raw(&RawTokenKind::LParen) {
            return Ok(ExportRef::Operator(self.parse_parenthesized_operator()?));
        }
        match &self.current().kind {
            LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => self.parse_export_type_ref(),
            _ => Ok(ExportRef::Value(self.consume_lower_name("export name")?)),
        }
    }

    fn parse_export_type_ref(&mut self) -> Result<ExportRef, ParseError> {
        if self.at_raw(&RawTokenKind::LParen) {
            return Ok(ExportRef::Operator(self.parse_parenthesized_operator()?));
        }
        let name = self.consume_upper_name("type export name")?;
        let members = if self.at_raw(&RawTokenKind::LParen) {
            Some(self.parse_type_members()?)
        } else {
            None
        };
        Ok(ExportRef::Type { name, members })
    }

    fn parse_type_members(&mut self) -> Result<TypeMembers, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        let mut all = false;
        let mut names = Vec::new();
        if self.at_raw(&RawTokenKind::DotDot) {
            all = true;
            self.bump();
        } else if !self.at_raw(&RawTokenKind::RParen) {
            loop {
                if self.at_raw(&RawTokenKind::LParen) {
                    names.push(self.parse_parenthesized_operator()?);
                } else {
                    names.push(self.consume_upper_name("data constructor name")?);
                }
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
        Ok(TypeMembers {
            span: TextRange::new(open_paren_span.start, close_paren_span.end),
            open_paren_span,
            all,
            names,
            close_paren_span,
        })
    }

    fn parse_parenthesized_operator(&mut self) -> Result<psrs_cst::CstName, ParseError> {
        let open = self.consume_raw(RawTokenKind::LParen)?.span;
        let token = self.current().clone();
        let name = match token.kind {
            LayoutTokenKind::Raw(
                RawTokenKind::Operator(text)
                | RawTokenKind::LowerIdent(text)
                | RawTokenKind::UpperIdent(text),
            ) => {
                self.bump();
                psrs_cst::CstName::new(text, token.span)
            }
            LayoutTokenKind::Raw(RawTokenKind::Colon) => {
                self.bump();
                psrs_cst::CstName::new(":", token.span)
            }
            LayoutTokenKind::Raw(RawTokenKind::DotDot) => {
                self.bump();
                psrs_cst::CstName::new("..", token.span)
            }
            LayoutTokenKind::Raw(RawTokenKind::Backslash) => {
                self.bump();
                psrs_cst::CstName::new("\\", token.span)
            }
            _ => {
                return Err(self.error(format!(
                    "expected an operator in parentheses after position {}",
                    open.start
                )));
            }
        };
        self.consume_raw(RawTokenKind::RParen)?;
        Ok(name)
    }

    pub(super) fn parse_import(&mut self) -> Result<ImportDeclaration, ParseError> {
        let import_keyword_span = self.consume_raw(RawTokenKind::Import)?.span;
        let module = self.parse_module_name()?;
        let mut alias = None;
        let mut list = None;
        loop {
            if self.at_raw(&RawTokenKind::As) {
                self.bump();
                alias = Some(self.parse_module_name()?);
            } else if self.at_raw(&RawTokenKind::Hiding) || self.at_raw(&RawTokenKind::LParen) {
                let hiding_keyword_span = if self.at_raw(&RawTokenKind::Hiding) {
                    Some(self.bump().span)
                } else {
                    None
                };
                list = Some(self.parse_import_list(hiding_keyword_span)?);
            } else {
                break;
            }
        }
        let end = list
            .as_ref()
            .map(|list| list.span.end)
            .or_else(|| alias.as_ref().map(|alias| alias.span.end))
            .unwrap_or(module.span.end);
        Ok(ImportDeclaration {
            import_keyword_span,
            module,
            alias,
            list,
            span: TextRange::new(import_keyword_span.start, end),
        })
    }

    fn parse_import_list(
        &mut self,
        hiding_keyword_span: Option<TextRange>,
    ) -> Result<ImportList, ParseError> {
        let open_paren_span = self.consume_raw(RawTokenKind::LParen)?.span;
        let mut items = Vec::new();
        if !self.at_raw(&RawTokenKind::RParen) {
            loop {
                items.push(self.parse_import_ref()?);
                if self.at_raw(&RawTokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
        Ok(ImportList {
            span: TextRange::new(open_paren_span.start, close_paren_span.end),
            hiding_keyword_span,
            open_paren_span,
            items,
            close_paren_span,
        })
    }

    fn parse_import_ref(&mut self) -> Result<ImportRef, ParseError> {
        if self.at_raw(&RawTokenKind::Class) {
            self.bump();
            return Ok(ImportRef::Class(
                self.consume_upper_name("class import name")?,
            ));
        }
        if self.at_raw(&RawTokenKind::Type) {
            self.bump();
            return self.parse_import_type_ref();
        }
        if self.at_raw(&RawTokenKind::LParen) {
            return Ok(ImportRef::Operator(self.parse_parenthesized_operator()?));
        }
        match &self.current().kind {
            LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => self.parse_import_type_ref(),
            _ => Ok(ImportRef::Value(self.consume_lower_name("import name")?)),
        }
    }

    fn parse_import_type_ref(&mut self) -> Result<ImportRef, ParseError> {
        if self.at_raw(&RawTokenKind::LParen) {
            return Ok(ImportRef::Operator(self.parse_parenthesized_operator()?));
        }
        let name = self.consume_upper_name("type import name")?;
        let members = if self.at_raw(&RawTokenKind::LParen) {
            Some(self.parse_type_members()?)
        } else {
            None
        };
        Ok(ImportRef::Type { name, members })
    }
}
