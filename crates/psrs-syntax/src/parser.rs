use crate::{LayoutToken, LayoutTokenKind, RawTokenKind};
use psrs_cst::{CstBinder, CstName, Declaration, Expr, ExprKind, Module};
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

    fn bump(&mut self) -> &'a LayoutToken {
        let token = self.current();
        if !matches!(token.kind, LayoutTokenKind::Raw(RawTokenKind::Eof)) {
            self.cursor += 1;
        }
        token
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

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let module_keyword_span = self.consume_raw(RawTokenKind::Module)?.span;
        let name = self.parse_module_name()?;
        let where_keyword_span = self.consume_raw(RawTokenKind::Where)?.span;
        self.consume_layout(LayoutTokenKind::LayoutStart)?;
        let declarations = self.parse_declarations_until(&[LayoutTokenKind::LayoutEnd])?;
        let end = self.consume_layout(LayoutTokenKind::LayoutEnd)?.span.end;
        self.consume_raw(RawTokenKind::Eof)?;
        let module_end = end.max(name.span.end);
        Ok(Module {
            module_keyword_span,
            name,
            where_keyword_span,
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
        self.bump();
        let mut name = first_name;
        let mut end = first.span.end;
        while self.current().kind == LayoutTokenKind::Raw(RawTokenKind::Dot) {
            self.bump();
            let token = self.current().clone();
            if let LayoutTokenKind::Raw(RawTokenKind::UpperIdent(part)) = token.kind {
                self.bump();
                name.push('.');
                name.push_str(&part);
                end = token.span.end;
            } else {
                return Err(self.error("expected module name after `.`".into()));
            }
        }
        Ok(CstName {
            text: name,
            span: TextRange::new(first.span.start, end),
        })
    }

    fn parse_declarations_until(
        &mut self,
        terminators: &[LayoutTokenKind],
    ) -> Result<Vec<Declaration>, ParseError> {
        let mut declarations = Vec::new();
        while !terminators.contains(&self.current().kind)
            && !matches!(self.current().kind, LayoutTokenKind::Raw(RawTokenKind::Eof))
        {
            if self.current().kind == LayoutTokenKind::LayoutSep {
                self.bump();
                continue;
            }
            declarations.push(self.parse_value_declaration()?);
            if self.current().kind == LayoutTokenKind::LayoutSep {
                self.bump();
            } else if !terminators.contains(&self.current().kind)
                && !matches!(self.current().kind, LayoutTokenKind::Raw(RawTokenKind::Eof))
            {
                return Err(self.error("expected a declaration separator".into()));
            }
        }
        Ok(declarations)
    }

    fn parse_value_declaration(&mut self) -> Result<Declaration, ParseError> {
        let name_token = self.current().clone();
        let name = match &name_token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name)) => name.clone(),
            _ => return Err(self.error(format!("expected value name, found {}", self.found()))),
        };
        self.bump();
        let name = CstName {
            text: name,
            span: name_token.span,
        };
        let mut parameters = Vec::new();
        while let LayoutTokenKind::Raw(RawTokenKind::LowerIdent(parameter)) = &self.current().kind {
            let parameter = parameter.clone();
            let token = self.bump();
            parameters.push(CstBinder {
                name: CstName {
                    text: parameter,
                    span: token.span,
                },
            });
        }
        let equals_span = self.consume_raw(RawTokenKind::Equals)?.span;
        let value = self.parse_expression(0)?;
        Ok(Declaration {
            name,
            parameters,
            equals_span,
            span: TextRange::new(name_token.span.start, value.span.end),
            value,
        })
    }

    fn parse_expression(&mut self, min_precedence: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_application()?;
        while let LayoutTokenKind::Raw(RawTokenKind::Operator(operator)) = &self.current().kind {
            let operator = operator.clone();
            let precedence = precedence(&operator);
            if precedence < min_precedence {
                break;
            }
            let operator_token = self.bump();
            let right = self.parse_expression(precedence + 1)?;
            let span = TextRange::new(left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Operator {
                    operator: CstName {
                        text: operator,
                        span: operator_token.span,
                    },
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    fn parse_application(&mut self) -> Result<Expr, ParseError> {
        let mut function = self.parse_atom()?;
        while self.starts_atom() {
            let argument = self.parse_atom()?;
            let span = TextRange::new(function.span.start, argument.span.end);
            function = Expr {
                kind: ExprKind::Application(Box::new(function), Box::new(argument)),
                span,
            };
        }
        Ok(function)
    }

    fn starts_atom(&self) -> bool {
        matches!(
            self.current().kind,
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(_)
                    | RawTokenKind::UpperIdent(_)
                    | RawTokenKind::Integer(_)
                    | RawTokenKind::String(_)
                    | RawTokenKind::Char(_)
                    | RawTokenKind::LParen
                    | RawTokenKind::Backslash
                    | RawTokenKind::If
                    | RawTokenKind::Let
            )
        )
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let token = self.current().clone();
        match token.kind {
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(name) | RawTokenKind::UpperIdent(name),
            ) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Name(CstName {
                        text: name,
                        span: token.span,
                    }),
                    span: token.span,
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Integer(value)) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Integer(value),
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
            LayoutTokenKind::Raw(RawTokenKind::LParen) => {
                let open_paren_span = self.bump().span;
                let expression = self.parse_expression(0)?;
                let close_paren_span = self.consume_raw(RawTokenKind::RParen)?.span;
                Ok(Expr {
                    kind: ExprKind::Parens {
                        open_paren_span,
                        expression: Box::new(expression),
                        close_paren_span,
                    },
                    span: TextRange::new(open_paren_span.start, close_paren_span.end),
                })
            }
            LayoutTokenKind::Raw(RawTokenKind::Backslash) => self.parse_lambda(),
            LayoutTokenKind::Raw(RawTokenKind::If) => self.parse_if(),
            LayoutTokenKind::Raw(RawTokenKind::Let) => self.parse_let(),
            _ => Err(self.error(format!("expected expression, found {}", self.found()))),
        }
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let backslash_span = self.consume_raw(RawTokenKind::Backslash)?.span;
        let mut parameters = Vec::new();
        while let LayoutTokenKind::Raw(RawTokenKind::LowerIdent(parameter)) = &self.current().kind {
            let parameter = parameter.clone();
            let token = self.bump();
            parameters.push(CstBinder {
                name: CstName {
                    text: parameter,
                    span: token.span,
                },
            });
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

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
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

    fn parse_let(&mut self) -> Result<Expr, ParseError> {
        let let_keyword_span = self.consume_raw(RawTokenKind::Let)?.span;
        let layout_start_span = self.consume_layout(LayoutTokenKind::LayoutStart)?.span;
        let declarations = self.parse_declarations_until(&[LayoutTokenKind::LayoutEnd])?;
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
}

pub fn parse_module(tokens: &[LayoutToken]) -> Result<Module, ParseError> {
    Parser { tokens, cursor: 0 }.parse_module()
}

fn precedence(operator: &str) -> u8 {
    match operator {
        "||" => 1,
        "&&" => 2,
        "==" | "/=" | "<" | ">" | "<=" | ">=" => 3,
        "+" | "-" | "<>" => 4,
        "*" | "/" | "%" => 5,
        _ => 4,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{add_layout, lex};
    use psrs_span::SourceFile;

    fn parse(source: &str) -> Result<Module, ParseError> {
        let source_file = SourceFile::new("test.purs", source);
        let (tokens, errors) = lex(source);
        assert!(errors.is_empty(), "{errors:?}");
        parse_module(&add_layout(&source_file, &tokens))
    }

    #[test]
    fn parses_a_module_with_functions_and_local_let() {
        let module = parse("module Main where\n\nadd x y = x + y\n\nmain =\n  let\n    answer = add 40 2\n  in answer\n").unwrap();
        assert_eq!(module.name.text, "Main");
        assert_eq!(module.declarations.len(), 2);
        assert_eq!(
            module.declarations[0]
                .parameters
                .iter()
                .map(|binder| binder.name.text.as_str())
                .collect::<Vec<_>>(),
            ["x", "y"]
        );
        assert!(matches!(
            module.declarations[1].value.kind,
            ExprKind::Let { .. }
        ));
    }

    #[test]
    fn reports_a_useful_error_for_malformed_declarations() {
        let error = parse("module Main where\nmain 42\n").unwrap_err();
        assert!(
            error.message.contains("expected `=`") || error.message.contains("expected Equals")
        );
    }

    #[test]
    fn parses_lambdas_conditionals_and_operator_precedence() {
        let module =
            parse("module Main where\nmain = \\x -> if x < 1 then x + 1 * 2 else x\n").unwrap();
        let ExprKind::Lambda { body, .. } = &module.declarations[0].value.kind else {
            panic!("expected lambda expression");
        };
        let ExprKind::If { then_branch, .. } = &body.kind else {
            panic!("expected conditional expression");
        };
        let ExprKind::Operator {
            operator, right, ..
        } = &then_branch.kind
        else {
            panic!("expected addition");
        };
        assert_eq!(operator.text, "+");
        assert!(
            matches!(right.kind, ExprKind::Operator { ref operator, .. } if operator.text == "*")
        );
    }

    #[test]
    fn parses_single_line_let_blocks() {
        let module = parse("module Main where\nmain = let x = 1 in x\n").unwrap();
        assert!(matches!(
            module.declarations[0].value.kind,
            ExprKind::Let { .. }
        ));
    }

    #[test]
    fn cst_retains_binder_and_concrete_token_ranges() {
        let source = "module Main where\nmain x = (x)\n";
        let module = parse(source).unwrap();
        let declaration = &module.declarations[0];
        assert_eq!(
            &source[declaration.name.span.start as usize..declaration.name.span.end as usize],
            "main"
        );
        assert_eq!(
            &source[declaration.parameters[0].name.span.start as usize
                ..declaration.parameters[0].name.span.end as usize],
            "x"
        );
        assert_eq!(
            &source[declaration.equals_span.start as usize..declaration.equals_span.end as usize],
            "="
        );
        let ExprKind::Parens {
            open_paren_span,
            close_paren_span,
            ..
        } = declaration.value.kind
        else {
            panic!("expected concrete parentheses in CST");
        };
        assert_eq!(
            &source[open_paren_span.start as usize..open_paren_span.end as usize],
            "("
        );
        assert_eq!(
            &source[close_paren_span.start as usize..close_paren_span.end as usize],
            ")"
        );
    }
}
