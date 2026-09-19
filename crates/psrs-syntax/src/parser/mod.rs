mod declaration;
mod expr;
mod import;
mod type_expr;

#[cfg(test)]
mod tests;

use crate::{LayoutToken, LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstName, Declaration, DeclarationBlock, Module};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub span: TextRange,
    pub message: String,
}

struct Parser<'a> {
    tokens: &'a [LayoutToken],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn current(&self) -> &'a LayoutToken {
        self.tokens
            .get(self.cursor)
            .unwrap_or_else(|| self.tokens.last().expect("lexer emits EOF"))
    }

    fn peek(&self, offset: usize) -> &'a LayoutToken {
        self.tokens
            .get(self.cursor + offset)
            .unwrap_or_else(|| self.tokens.last().expect("lexer emits EOF"))
    }

    fn bump(&mut self) -> &'a LayoutToken {
        let token = self.current();
        if !matches!(token.kind, LayoutTokenKind::Raw(RawTokenKind::Eof)) {
            self.cursor += 1;
        }
        token
    }

    fn at_raw(&self, expected: &RawTokenKind) -> bool {
        self.current().kind == LayoutTokenKind::Raw(expected.clone())
    }

    fn at_eof(&self) -> bool {
        self.at_raw(&RawTokenKind::Eof)
    }

    fn consume_raw(&mut self, expected: RawTokenKind) -> Result<&'a LayoutToken, ParseError> {
        if self.current().kind == LayoutTokenKind::Raw(expected.clone()) {
            Ok(self.bump())
        } else {
            Err(self.error(format!(
                "expected {}, found {}",
                raw_label(&expected),
                self.found()
            )))
        }
    }

    fn consume_layout(&mut self, expected: LayoutTokenKind) -> Result<&'a LayoutToken, ParseError> {
        if self.current().kind == expected {
            Ok(self.bump())
        } else {
            Err(self.error(format!(
                "expected {}, found {}",
                layout_label(&expected),
                self.found()
            )))
        }
    }

    fn found(&self) -> String {
        match &self.current().kind {
            LayoutTokenKind::Raw(kind) => kind.label().to_owned(),
            kind => layout_label(kind).to_owned(),
        }
    }

    fn error(&self, message: String) -> ParseError {
        ParseError {
            span: self.current().span,
            message,
        }
    }

    fn error_at(&self, span: TextRange, message: String) -> ParseError {
        ParseError { span, message }
    }

    fn consume_upper_name(&mut self, what: &str) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        let LayoutTokenKind::Raw(RawTokenKind::UpperIdent(name)) = token.kind else {
            return Err(self.error(format!("expected {what}, found {}", self.found())));
        };
        self.bump();
        Ok(CstName::new(name, token.span))
    }

    fn consume_lower_name(&mut self, what: &str) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        let LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name)) = token.kind else {
            return Err(self.error(format!("expected {what}, found {}", self.found())));
        };
        self.bump();
        Ok(CstName::new(name, token.span))
    }

    fn consume_name(&mut self, what: &str) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        let LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name) | RawTokenKind::UpperIdent(name)) =
            token.kind
        else {
            return Err(self.error(format!("expected {what}, found {}", self.found())));
        };
        self.bump();
        Ok(CstName::new(name, token.span))
    }

    fn at_separator(&self) -> bool {
        matches!(
            self.current().kind,
            LayoutTokenKind::LayoutSep | LayoutTokenKind::LayoutEnd
        ) || self.at_eof()
    }

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let module_keyword_span = self.consume_raw(RawTokenKind::Module)?.span;
        let name = self.parse_module_name()?;
        let exports = if self.at_raw(&RawTokenKind::LParen) {
            Some(self.parse_export_list()?)
        } else {
            None
        };
        let where_keyword_span = self.consume_raw(RawTokenKind::Where)?.span;

        let mut imports = Vec::new();
        let mut declarations = Vec::new();
        let mut body_end = where_keyword_span.end;
        if self.current().kind == LayoutTokenKind::LayoutStart {
            self.bump();
            loop {
                while self.current().kind == LayoutTokenKind::LayoutSep {
                    self.bump();
                }
                if self.current().kind == LayoutTokenKind::LayoutEnd {
                    body_end = self.bump().span.end;
                    break;
                }
                if self.at_eof() {
                    break;
                }
                if self.at_raw(&RawTokenKind::Import) {
                    imports.push(self.parse_import()?);
                } else {
                    declarations.push(self.parse_module_item(true, false)?);
                }
                match self.current().kind {
                    LayoutTokenKind::LayoutSep => {
                        self.bump();
                    }
                    LayoutTokenKind::LayoutEnd => {}
                    _ if self.at_eof() => {}
                    _ if self.at_raw(&RawTokenKind::Else) => {}
                    _ => return Err(self.error("expected a declaration separator".into())),
                }
            }
        } else if !self.at_eof() {
            return Err(self.error(format!("expected layout start, found {}", self.found())));
        }
        let last_span = declarations
            .last()
            .map(Declaration::span)
            .or_else(|| imports.last().map(|import| import.span));
        let module_end = last_span
            .map(|span| span.end)
            .unwrap_or(where_keyword_span.end)
            .max(body_end)
            .max(name.span.end);
        self.consume_raw(RawTokenKind::Eof)?;
        Ok(Module {
            module_keyword_span,
            name,
            exports,
            where_keyword_span,
            imports,
            declarations,
            span: TextRange::new(module_keyword_span.start, module_end),
        })
    }

    fn parse_module_name(&mut self) -> Result<CstName, ParseError> {
        let first = self.current().clone();
        let Some(LayoutTokenKind::Raw(RawTokenKind::UpperIdent(first_name))) =
            Some(first.kind.clone())
        else {
            return Err(self.error(format!("expected module name, found {}", self.found())));
        };
        if first_name.contains('_') || first_name.contains('\'') {
            return Err(self.error_at(first.span, "invalid module name".into()));
        }
        self.bump();
        let mut name = first_name;
        let mut end = first.span.end;
        while self.current().kind == LayoutTokenKind::Raw(RawTokenKind::Dot) {
            self.bump();
            let token = self.current().clone();
            if let LayoutTokenKind::Raw(RawTokenKind::UpperIdent(part)) = token.kind {
                if part.contains('_') || part.contains('\'') {
                    return Err(self.error_at(token.span, "invalid module name".into()));
                }
                self.bump();
                name.push('.');
                name.push_str(&part);
                end = token.span.end;
            } else {
                return Err(self.error("expected module name after `.`".into()));
            }
        }
        Ok(CstName::new(name, TextRange::new(first.span.start, end)))
    }

    fn parse_declaration_block(&mut self) -> Result<DeclarationBlock, ParseError> {
        let where_keyword_span = self.consume_raw(RawTokenKind::Where)?.span;
        if self.current().kind != LayoutTokenKind::LayoutStart {
            return Err(self.error("expected an indented `where` block".into()));
        }
        let layout_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
        let declarations =
            self.parse_declarations_until(&[LayoutTokenKind::LayoutEnd], true, true)?;
        let layout_end_span = self.consume_layout(LayoutTokenKind::LayoutEnd)?.span;
        let span = TextRange::new(where_keyword_span.start, layout_end_span.end);
        Ok(DeclarationBlock {
            where_keyword_span,
            layout_start_span,
            declarations,
            layout_end_span,
            span,
        })
    }

    fn parse_declarations_until(
        &mut self,
        terminators: &[LayoutTokenKind],
        allow_signatures: bool,
        allow_pattern: bool,
    ) -> Result<Vec<Declaration>, ParseError> {
        let mut declarations = Vec::new();
        loop {
            while self.current().kind == LayoutTokenKind::LayoutSep {
                self.bump();
            }
            if terminators.contains(&self.current().kind) || self.at_eof() {
                break;
            }
            declarations.push(self.parse_module_item(allow_signatures, allow_pattern)?);
            match self.current().kind {
                LayoutTokenKind::LayoutSep => {
                    self.bump();
                }
                _ if terminators.contains(&self.current().kind) => {}
                _ if self.at_eof() => {}
                _ if self.at_raw(&RawTokenKind::Else) => {}
                _ => return Err(self.error("expected a declaration separator".into())),
            }
        }
        Ok(declarations)
    }

    fn parse_module_item(
        &mut self,
        allow_signatures: bool,
        allow_pattern: bool,
    ) -> Result<Declaration, ParseError> {
        match &self.current().kind {
            LayoutTokenKind::Raw(RawTokenKind::Data) => self.parse_data_declaration(),
            LayoutTokenKind::Raw(RawTokenKind::Newtype) => self.parse_newtype_declaration(),
            LayoutTokenKind::Raw(RawTokenKind::Type) => self.parse_type_declaration(),
            LayoutTokenKind::Raw(RawTokenKind::Class) => self.parse_class_declaration(),
            LayoutTokenKind::Raw(RawTokenKind::Instance) => self.parse_instance_declaration(None),
            LayoutTokenKind::Raw(RawTokenKind::Else) => {
                let else_span = self.bump().span;
                while self.current().kind == LayoutTokenKind::LayoutSep {
                    self.bump();
                }
                self.parse_instance_declaration(Some(else_span))
            }
            LayoutTokenKind::Raw(RawTokenKind::Derive) => self.parse_derive_declaration(),
            LayoutTokenKind::Raw(RawTokenKind::Foreign) => self.parse_foreign_declaration(),
            LayoutTokenKind::Raw(
                RawTokenKind::Infix | RawTokenKind::Infixl | RawTokenKind::Infixr,
            ) => self.parse_fixity_declaration(),
            _ => self.parse_value_like_declaration(allow_signatures, allow_pattern),
        }
    }
}

pub fn parse_module(tokens: &[LayoutToken]) -> Result<Module, ParseError> {
    Parser { tokens, cursor: 0 }.parse_module()
}

fn raw_label(kind: &RawTokenKind) -> &'static str {
    kind.label()
}

fn layout_label(kind: &LayoutTokenKind) -> &'static str {
    match kind {
        LayoutTokenKind::Raw(_) => "raw token",
        LayoutTokenKind::LayoutStart => "layout start",
        LayoutTokenKind::LayoutSep => "layout separator",
        LayoutTokenKind::LayoutEnd => "layout end",
    }
}
