use crate::{LayoutTokenKind, RawTokenKind};
use psrs_cst::{
    ClassDeclaration, CstName, Declaration, DeriveDeclaration, Fixity, FixityDeclaration,
    ForeignDeclaration, FunctionalDependency, InstanceDeclaration, KindSignature, RoleDeclaration,
    TypeSynonymDeclaration,
};
use psrs_span::TextRange;

use super::super::{ParseError, Parser};

impl<'a> Parser<'a> {
    pub(crate) fn parse_type_declaration(&mut self) -> Result<Declaration, ParseError> {
        let keyword_span = self.consume_raw(RawTokenKind::Type)?.span;
        if self.at_raw(&RawTokenKind::Role) {
            let role_keyword_span = self.bump().span;
            let name = self.consume_upper_name("role type name")?;
            let mut roles = Vec::new();
            while !self.at_separator() {
                roles.push(self.consume_lower_name("role")?);
            }
            let end = roles
                .last()
                .map(|role| role.span.end)
                .unwrap_or(name.span.end);
            return Ok(Declaration::Role(RoleDeclaration {
                type_keyword_span: keyword_span,
                role_keyword_span,
                name: name.clone(),
                roles,
                span: TextRange::new(keyword_span.start, end),
            }));
        }
        let name = self.consume_upper_name("type name")?;
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
        let equals_span = self.consume_raw(RawTokenKind::Equals)?.span;
        let body = self.parse_type()?;
        if super::super::type_expr::type_contains_wildcard(&body) {
            return Err(self.error_at(body.span, "wildcards are not allowed here".into()));
        }
        let span = TextRange::new(keyword_span.start, body.span.end);
        Ok(Declaration::TypeSynonym(TypeSynonymDeclaration {
            keyword_span,
            name,
            parameters,
            equals_span,
            body,
            span,
        }))
    }

    pub(crate) fn parse_class_declaration(&mut self) -> Result<Declaration, ParseError> {
        let keyword_span = self.consume_raw(RawTokenKind::Class)?.span;
        if self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::DoubleColon)
            && matches!(
                self.current().kind,
                LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_))
            )
        {
            let name = self.consume_upper_name("class name")?;
            let double_colon_span = self.consume_raw(RawTokenKind::DoubleColon)?.span;
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
        let head = self.parse_type()?;
        let (superclasses, superclass_arrow_span, class_head) = match head.kind {
            psrs_cst::TypeExprKind::Operator {
                operator,
                left,
                right,
            } if operator.text == "<=" || operator.text == "⇐" => {
                (Some(left), Some(operator.span), *right)
            }
            _ => (None, None, head),
        };
        if let Some(superclasses) = &superclasses {
            let invalid = super::super::type_expr::type_contains_wildcard(superclasses)
                || super::super::type_expr::type_contains_forall(superclasses);
            if invalid {
                return Err(self.error_at(
                    superclasses.span,
                    "superclasses cannot contain wildcards or foralls".into(),
                ));
            }
        }
        let name = match &class_head.kind {
            psrs_cst::TypeExprKind::Name(name) => name.clone(),
            psrs_cst::TypeExprKind::Application(function, _) => match &function.kind {
                psrs_cst::TypeExprKind::Name(name) => name.clone(),
                _ => {
                    return Err(
                        self.error_at(class_head.span, "expected class name in class head".into())
                    );
                }
            },
            _ => {
                return Err(
                    self.error_at(class_head.span, "expected class name in class head".into())
                );
            }
        };
        let fundeps = if self.at_raw(&RawTokenKind::Pipe) {
            self.parse_functional_dependencies()?
        } else {
            Vec::new()
        };
        let where_block = if self.at_raw(&RawTokenKind::Where) {
            Some(self.parse_declaration_block()?)
        } else {
            None
        };
        let end = where_block
            .as_ref()
            .map(|block| block.span.end)
            .or_else(|| fundeps.last().map(|dep| dep.span.end))
            .unwrap_or(class_head.span.end);
        Ok(Declaration::Class(ClassDeclaration {
            keyword_span,
            superclasses,
            superclass_arrow_span,
            head: class_head,
            name,
            fundeps,
            where_block,
            span: TextRange::new(keyword_span.start, end),
        }))
    }

    fn parse_functional_dependencies(&mut self) -> Result<Vec<FunctionalDependency>, ParseError> {
        let mut dependencies = Vec::new();
        if !self.at_raw(&RawTokenKind::Pipe) {
            return Ok(dependencies);
        }
        let bar_span = self.bump().span;
        loop {
            let mut from = Vec::new();
            while !self.at_raw(&RawTokenKind::Arrow)
                && !self.at_raw(&RawTokenKind::Comma)
                && !self.at_raw(&RawTokenKind::Where)
                && !self.at_separator()
            {
                from.push(self.consume_lower_name("type variable")?);
            }
            let arrow_span = self.consume_raw(RawTokenKind::Arrow)?.span;
            let mut to = Vec::new();
            while !self.at_raw(&RawTokenKind::Pipe)
                && !self.at_raw(&RawTokenKind::Comma)
                && !self.at_raw(&RawTokenKind::Where)
                && !self.at_separator()
            {
                to.push(self.consume_lower_name("type variable")?);
            }
            let end = to
                .last()
                .map(|name| name.span.end)
                .or_else(|| from.last().map(|name| name.span.end))
                .unwrap_or(arrow_span.end);
            dependencies.push(FunctionalDependency {
                span: TextRange::new(bar_span.start, end),
                bar_span,
                from,
                arrow_span,
                to,
            });
            if self.at_raw(&RawTokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(dependencies)
    }

    pub(crate) fn parse_instance_declaration(
        &mut self,
        else_keyword_span: Option<TextRange>,
    ) -> Result<Declaration, ParseError> {
        let keyword_span = self.consume_raw(RawTokenKind::Instance)?.span;
        if matches!(
            self.current().kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_))
        ) && self.peek(1).kind != LayoutTokenKind::Raw(RawTokenKind::DoubleColon)
        {
            return Err(self.error("expected `::` after the instance name".into()));
        }
        let name = if matches!(
            self.current().kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_))
        ) && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::DoubleColon)
        {
            let name = self.consume_lower_name("instance name")?;
            self.consume_raw(RawTokenKind::DoubleColon)?;
            Some(name)
        } else {
            None
        };
        let head = self.parse_type()?;
        let (constraints, constraint_arrow_span, instance_head) = split_constraint(head);
        if let Some(constraints) = &constraints {
            let invalid = super::super::type_expr::type_contains_wildcard(constraints)
                || super::super::type_expr::type_contains_forall(constraints);
            if invalid {
                return Err(self.error_at(
                    constraints.span,
                    "constraints cannot contain wildcards or foralls".into(),
                ));
            }
        }
        let where_block = if self.at_raw(&RawTokenKind::Where) {
            Some(self.parse_declaration_block()?)
        } else {
            None
        };
        let end = where_block
            .as_ref()
            .map(|block| block.span.end)
            .unwrap_or(instance_head.span.end);
        let start = else_keyword_span
            .map(|span| span.start)
            .unwrap_or(keyword_span.start);
        Ok(Declaration::Instance(InstanceDeclaration {
            else_keyword_span,
            keyword_span,
            name,
            constraints,
            constraint_arrow_span,
            head: instance_head,
            where_block,
            span: TextRange::new(start, end),
        }))
    }

    pub(crate) fn parse_derive_declaration(&mut self) -> Result<Declaration, ParseError> {
        let derive_keyword_span = self.consume_raw(RawTokenKind::Derive)?.span;
        let newtype_keyword_span = if self.at_raw(&RawTokenKind::Newtype) {
            Some(self.bump().span)
        } else {
            None
        };
        let instance_keyword_span = self.consume_raw(RawTokenKind::Instance)?.span;
        let name = if matches!(
            self.current().kind,
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_))
        ) && self.peek(1).kind == LayoutTokenKind::Raw(RawTokenKind::DoubleColon)
        {
            let name = self.consume_lower_name("instance name")?;
            self.consume_raw(RawTokenKind::DoubleColon)?;
            Some(name)
        } else {
            None
        };
        let head = self.parse_type()?;
        let (constraints, constraint_arrow_span, derive_head) = split_constraint(head);
        let span = TextRange::new(derive_keyword_span.start, derive_head.span.end);
        Ok(Declaration::Derive(DeriveDeclaration {
            derive_keyword_span,
            newtype_keyword_span,
            instance_keyword_span,
            name,
            constraints,
            constraint_arrow_span,
            head: derive_head,
            span,
        }))
    }

    pub(crate) fn parse_foreign_declaration(&mut self) -> Result<Declaration, ParseError> {
        let foreign_keyword_span = self.consume_raw(RawTokenKind::Foreign)?.span;
        let import_keyword_span = self.consume_raw(RawTokenKind::Import)?.span;
        let data_keyword_span = if self.at_raw(&RawTokenKind::Data) {
            Some(self.bump().span)
        } else {
            None
        };
        let name = if data_keyword_span.is_some() {
            self.consume_upper_name("foreign data name")?
        } else {
            self.consume_name("foreign value name")?
        };
        let double_colon_span = self.consume_raw(RawTokenKind::DoubleColon)?.span;
        let type_expr = self.parse_type()?;
        let span = TextRange::new(foreign_keyword_span.start, type_expr.span.end);
        Ok(Declaration::Foreign(ForeignDeclaration {
            foreign_keyword_span,
            import_keyword_span,
            data_keyword_span,
            name,
            double_colon_span,
            type_expr,
            span,
        }))
    }

    pub(crate) fn parse_fixity_declaration(&mut self) -> Result<Declaration, ParseError> {
        let token = self.current().clone();
        let associativity = match token.kind {
            LayoutTokenKind::Raw(RawTokenKind::Infixl) => Fixity::Left,
            LayoutTokenKind::Raw(RawTokenKind::Infixr) => Fixity::Right,
            LayoutTokenKind::Raw(RawTokenKind::Infix) => Fixity::None,
            _ => return Err(self.error("expected a fixity declaration".into())),
        };
        self.bump();
        let precedence_token = self.current().clone();
        let LayoutTokenKind::Raw(RawTokenKind::Integer(precedence)) = precedence_token.kind else {
            return Err(self.error("expected a fixity precedence".into()));
        };
        self.bump();
        let namespace_span = if self.at_raw(&RawTokenKind::Type) {
            Some(self.bump().span)
        } else {
            None
        };
        let operator = self.parse_fixity_target()?;
        let alias = if self.at_raw(&RawTokenKind::As) {
            self.bump();
            Some(self.parse_operator_name("operator alias")?)
        } else {
            None
        };
        let end = alias
            .as_ref()
            .map(|name| name.span.end)
            .unwrap_or(operator.span.end);
        Ok(Declaration::Fixity(FixityDeclaration {
            associativity,
            associativity_span: token.span,
            precedence,
            precedence_span: precedence_token.span,
            namespace_span,
            operator,
            alias,
            span: TextRange::new(token.span.start, end),
        }))
    }

    fn parse_fixity_target(&mut self) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        let name = match &token.kind {
            LayoutTokenKind::Raw(RawTokenKind::LowerIdent(name))
            | LayoutTokenKind::Raw(RawTokenKind::UpperIdent(name)) => name.clone(),
            LayoutTokenKind::Raw(RawTokenKind::Operator(name)) => {
                if name == "@" {
                    return Err(self.error("`@` cannot be used as an operator name".into()));
                }
                self.bump();
                return Ok(CstName::new(name.clone(), token.span));
            }
            LayoutTokenKind::Raw(RawTokenKind::Colon) => {
                self.bump();
                return Ok(CstName::new(":", token.span));
            }
            LayoutTokenKind::Raw(RawTokenKind::DotDot) => {
                self.bump();
                return Ok(CstName::new("..", token.span));
            }
            LayoutTokenKind::Raw(RawTokenKind::Backslash) => {
                self.bump();
                return Ok(CstName::new("\\", token.span));
            }
            _ => return Err(self.error("expected an operator name".into())),
        };
        self.bump();
        let mut result = CstName::new(name, token.span);
        while self.at_raw(&RawTokenKind::Dot) {
            let dot_span = self.current().span;
            if dot_span.start != result.span.end {
                break;
            }
            let part = self.peek(1).clone();
            let (text, span) = match &part.kind {
                LayoutTokenKind::Raw(RawTokenKind::LowerIdent(text))
                | LayoutTokenKind::Raw(RawTokenKind::UpperIdent(text)) => (text.clone(), part.span),
                _ => break,
            };
            if span.start != dot_span.end {
                break;
            }
            self.bump();
            self.bump();
            result = CstName::new(
                format!("{}.{}", result.text, text),
                TextRange::new(result.span.start, span.end),
            );
        }
        Ok(result)
    }

    fn parse_operator_name(&mut self, what: &str) -> Result<CstName, ParseError> {
        let token = self.current().clone();
        match token.kind {
            LayoutTokenKind::Raw(
                RawTokenKind::LowerIdent(name)
                | RawTokenKind::UpperIdent(name)
                | RawTokenKind::Operator(name),
            ) => {
                if name == "@" {
                    return Err(self.error(format!("`@` cannot be used as {what}")));
                }
                self.bump();
                Ok(CstName::new(name, token.span))
            }
            LayoutTokenKind::Raw(RawTokenKind::Colon) => {
                self.bump();
                Ok(CstName::new(":", token.span))
            }
            LayoutTokenKind::Raw(RawTokenKind::DotDot) => {
                self.bump();
                Ok(CstName::new("..", token.span))
            }
            LayoutTokenKind::Raw(RawTokenKind::Backslash) => {
                self.bump();
                Ok(CstName::new("\\", token.span))
            }
            _ => Err(self.error(format!("expected {what}, found {}", self.found()))),
        }
    }
}

fn split_constraint(
    head: psrs_cst::TypeExpr,
) -> (
    Option<Box<psrs_cst::TypeExpr>>,
    Option<TextRange>,
    psrs_cst::TypeExpr,
) {
    match head.kind {
        psrs_cst::TypeExprKind::Constrained {
            constraint,
            arrow_span,
            body,
        } => (Some(constraint), Some(arrow_span), *body),
        _ => (None, None, head),
    }
}
