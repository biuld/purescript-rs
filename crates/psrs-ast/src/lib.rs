use psrs_cst::{self as cst, ExprKind as CstExprKind, TypeExprKind as CstTypeExprKind};
use psrs_span::TextRange;
use std::collections::HashSet;

mod export;
mod import;
mod ty;
mod type_decl;

pub use export::{ExportList, ExportRef, TypeMembers};
pub use import::{Import, ImportList, ImportRef};
pub use ty::{Type, TypeField, TypeKind};
pub use type_decl::{
    ClassDeclaration, ClassMember, DataConstructor, DataDeclaration, NewtypeDeclaration,
    TypeDeclaration, TypeParameter, TypeSynonymDeclaration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: Name,
    pub exports: Option<ExportList>,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
    pub type_declarations: Vec<TypeDeclaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub text: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binder {
    pub name: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub name: Name,
    pub value: Expr,
    pub span: TextRange,
    pub annotation: Option<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Name(Name),
    Integer(String),
    String(String),
    Char(char),
    Application(Box<Expr>, Box<Expr>),
    Operator {
        operator: Name,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Lambda {
        binder: Binder,
        body: Box<Expr>,
    },
    Let {
        declarations: Vec<Declaration>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
}

/// A construct that parsed but has no AST lowering yet, or a name-level error
/// the surface layer can detect. Returning an error instead of inventing a node
/// keeps unsupported syntax from reaching later passes, and lets the parser
/// grow ahead of resolution and type checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerError {
    pub span: TextRange,
    pub message: &'static str,
    /// The official PureScript `errorCode` for this diagnostic, when one
    /// applies. Unsupported-syntax errors have no code.
    pub code: Option<&'static str>,
}

impl LowerError {
    pub fn new(span: TextRange, message: &'static str) -> Self {
        Self {
            span,
            message,
            code: None,
        }
    }

    pub fn coded(span: TextRange, code: &'static str, message: &'static str) -> Self {
        Self {
            span,
            message,
            code: Some(code),
        }
    }
}

/// Converts source-oriented CST nodes into a normalized, unresolved surface AST.
pub fn lower_module(module: cst::Module) -> Result<Module, Vec<LowerError>> {
    let mut errors = Vec::new();
    let mut declarations = Vec::new();
    let mut type_declarations = Vec::new();
    let mut index = 0;
    while index < module.declarations.len() {
        let declaration = module.declarations[index].clone();
        match declaration {
            cst::Declaration::KindSignature(signature) => {
                if matches_kind_declaration(&signature, module.declarations.get(index + 1)) {
                    let target = module.declarations[index + 1].clone();
                    match type_decl::lower_type_declaration(Some(signature), target) {
                        Ok(declaration) => type_declarations.push(declaration),
                        Err(error) => errors.push(error),
                    }
                    index += 2;
                } else {
                    errors.push(LowerError::coded(
                        signature.span,
                        "OrphanKindDeclaration",
                        "a kind declaration must be followed by a matching declaration",
                    ));
                    index += 1;
                }
            }
            cst::Declaration::Data(_)
            | cst::Declaration::Newtype(_)
            | cst::Declaration::TypeSynonym(_)
            | cst::Declaration::Class(_) => {
                match type_decl::lower_type_declaration(None, declaration) {
                    Ok(declaration) => type_declarations.push(declaration),
                    Err(error) => errors.push(error),
                }
                index += 1;
            }
            other => {
                match lower_declaration(other) {
                    Ok(declaration) => declarations.push(declaration),
                    Err(error) => errors.push(error),
                }
                index += 1;
            }
        }
    }
    if errors.is_empty() {
        Ok(Module {
            name: lower_name(module.name),
            exports: module.exports.map(export::lower_export_list),
            imports: module
                .imports
                .into_iter()
                .map(import::lower_import)
                .collect(),
            declarations,
            type_declarations,
            span: module.span,
        })
    } else {
        Err(errors)
    }
}

/// A kind declaration is matched by the declaration that immediately follows it
/// with the same name and the same declaration keyword, as `purs` requires.
fn matches_kind_declaration(
    signature: &cst::KindSignature,
    next: Option<&cst::Declaration>,
) -> bool {
    let Some(next) = next else {
        return false;
    };
    let name = signature.name.text.as_str();
    match (signature.kind_for, next) {
        (cst::KindFor::Data, cst::Declaration::Data(declaration)) => declaration.name.text == name,
        (cst::KindFor::Newtype, cst::Declaration::Newtype(declaration)) => {
            declaration.name.text == name
        }
        (cst::KindFor::TypeSynonym, cst::Declaration::TypeSynonym(declaration)) => {
            declaration.name.text == name
        }
        (cst::KindFor::Class, cst::Declaration::Class(declaration)) => {
            declaration.name.text == name
        }
        _ => false,
    }
}

fn lower_declaration(declaration: cst::Declaration) -> Result<Declaration, LowerError> {
    match declaration {
        cst::Declaration::Value(declaration) => lower_value_declaration(declaration),
        cst::Declaration::TypeSignature(signature) => Err(LowerError::coded(
            signature.span,
            "OrphanTypeDeclaration",
            "a type declaration must be followed by a matching value declaration",
        )),
        other => Err(LowerError::new(
            other.span(),
            "this declaration is not supported yet",
        )),
    }
}

fn lower_value_declaration(declaration: cst::ValueDeclaration) -> Result<Declaration, LowerError> {
    if let Some(error) = check_argument_names(&declaration.parameters) {
        return Err(error);
    }
    let cst::ValueRhs::Plain { value, .. } = declaration.rhs else {
        return Err(LowerError::new(
            declaration.span,
            "guarded equations are not supported yet",
        ));
    };
    if let Some(block) = declaration.where_block {
        return Err(LowerError::new(
            block.span,
            "where blocks are not supported yet",
        ));
    }
    let mut value = lower_expr(value)?;
    for parameter in declaration.parameters.into_iter().rev() {
        value = lower_pattern_lambda(parameter, value)?;
    }
    Ok(Declaration {
        name: lower_name(declaration.name),
        value,
        span: declaration.span,
        annotation: declaration.annotation.map(lower_type).transpose()?,
    })
}

/// Reports the second occurrence of a repeated value argument name.
fn check_argument_names(parameters: &[cst::Pattern]) -> Option<LowerError> {
    let mut seen = HashSet::new();
    for parameter in parameters {
        if let cst::PatternKind::Var(name) = &parameter.kind
            && !seen.insert(name.text.as_str())
        {
            return Some(LowerError::coded(
                parameter.span,
                "OverlappingArgNames",
                "two arguments share the same name",
            ));
        }
    }
    None
}

fn lower_pattern_lambda(pattern: cst::Pattern, body: Expr) -> Result<Expr, LowerError> {
    match pattern.kind {
        cst::PatternKind::Var(name) => Ok(lower_lambda(
            Binder {
                name: name.text,
                span: name.span,
            },
            body,
        )),
        cst::PatternKind::Parens { pattern, .. } => lower_pattern_lambda(*pattern, body),
        _ => Err(LowerError::new(
            pattern.span,
            "only variable binders are supported yet",
        )),
    }
}

pub(crate) fn lower_type(expression: cst::TypeExpr) -> Result<Type, LowerError> {
    let span = expression.span;
    let kind = match expression.kind {
        CstTypeExprKind::Application(function, arguments) => {
            let mut lowered = lower_type(*function)?;
            for argument in arguments {
                let argument = lower_type(argument)?;
                lowered = Type {
                    kind: TypeKind::Application(Box::new(lowered), Box::new(argument)),
                    span,
                };
            }
            return Ok(lowered);
        }
        CstTypeExprKind::Name(name) => TypeKind::Name(lower_name(name)),
        CstTypeExprKind::Function { left, right, .. } => TypeKind::Function {
            parameter: Box::new(lower_type(*left)?),
            result: Box::new(lower_type(*right)?),
        },
        CstTypeExprKind::Forall {
            variables, body, ..
        } => TypeKind::Forall {
            variables: variables
                .into_iter()
                .map(lower_type_parameter)
                .collect::<Result<_, _>>()?,
            body: Box::new(lower_type(*body)?),
        },
        CstTypeExprKind::Constrained {
            constraint, body, ..
        } => TypeKind::Constrained {
            constraint: Box::new(lower_type(*constraint)?),
            body: Box::new(lower_type(*body)?),
        },
        CstTypeExprKind::Row { fields, tail, .. } => TypeKind::Row {
            fields: fields
                .into_iter()
                .map(lower_type_field)
                .collect::<Result<_, _>>()?,
            tail: tail
                .map(|tail| lower_type(*tail))
                .transpose()?
                .map(Box::new),
        },
        CstTypeExprKind::Record { fields, tail, .. } => TypeKind::Record {
            fields: fields
                .into_iter()
                .map(lower_type_field)
                .collect::<Result<_, _>>()?,
            tail: tail
                .map(|tail| lower_type(*tail))
                .transpose()?
                .map(Box::new),
        },
        CstTypeExprKind::Integer(value) => TypeKind::Integer(value),
        CstTypeExprKind::String(value) => TypeKind::String(value),
        CstTypeExprKind::Parens { expression, .. } => {
            let mut expression = lower_type(*expression)?;
            expression.span = span;
            return Ok(expression);
        }
        CstTypeExprKind::KindAnnotation { expression, .. } => {
            let mut expression = lower_type(*expression)?;
            expression.span = span;
            return Ok(expression);
        }
        CstTypeExprKind::Operator {
            operator,
            left,
            right,
        } if operator.text == "~>" => TypeKind::Function {
            parameter: Box::new(lower_type(*left)?),
            result: Box::new(lower_type(*right)?),
        },
        CstTypeExprKind::Wildcard(_)
        | CstTypeExprKind::Hole(_)
        | CstTypeExprKind::Operator { .. }
        | CstTypeExprKind::PrefixOperator { .. }
        | CstTypeExprKind::Tuple { .. } => {
            return Err(LowerError::new(
                span,
                "this type syntax is not supported yet",
            ));
        }
    };
    Ok(Type { kind, span })
}

fn lower_type_parameter(parameter: cst::TypeVarBinder) -> Result<TypeParameter, LowerError> {
    Ok(TypeParameter {
        name: lower_name(parameter.name),
        kind: parameter.kind.map(lower_type).transpose()?,
        span: parameter.span,
    })
}

fn lower_type_field(field: cst::TypeField) -> Result<TypeField, LowerError> {
    Ok(TypeField {
        label: lower_name(field.label),
        ty: lower_type(field.type_expr)?,
        span: field.span,
    })
}

fn lower_expr(expression: cst::Expr) -> Result<Expr, LowerError> {
    let span = expression.span;
    let kind = match expression.kind {
        CstExprKind::Name(name) => ExprKind::Name(lower_name(name)),
        CstExprKind::Integer(value) => ExprKind::Integer(value),
        CstExprKind::String(value) => ExprKind::String(value),
        CstExprKind::Char(value) => ExprKind::Char(value),
        CstExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(lower_expr(*function)?),
            Box::new(lower_expr(*argument)?),
        ),
        CstExprKind::Operator {
            operator,
            left,
            right,
        } => ExprKind::Operator {
            operator: lower_name(operator),
            left: Box::new(lower_expr(*left)?),
            right: Box::new(lower_expr(*right)?),
        },
        CstExprKind::Lambda {
            parameters, body, ..
        } => {
            let mut body = lower_expr(*body)?;
            for parameter in parameters.into_iter().rev() {
                body = lower_pattern_lambda(parameter, body)?;
            }
            return Ok(Expr {
                kind: body.kind,
                span,
            });
        }
        CstExprKind::Let {
            declarations, body, ..
        } => {
            let mut lowered = Vec::with_capacity(declarations.len());
            for declaration in declarations {
                lowered.push(lower_declaration(declaration)?);
            }
            ExprKind::Let {
                declarations: lowered,
                body: Box::new(lower_expr(*body)?),
            }
        }
        CstExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => ExprKind::If {
            condition: Box::new(lower_expr(*condition)?),
            then_branch: Box::new(lower_expr(*then_branch)?),
            else_branch: Box::new(lower_expr(*else_branch)?),
        },
        CstExprKind::Parens { expression, .. } => {
            let mut expression = lower_expr(*expression)?;
            expression.span = span;
            return Ok(expression);
        }
        CstExprKind::Hole(_)
        | CstExprKind::Number(_)
        | CstExprKind::Array { .. }
        | CstExprKind::Record { .. }
        | CstExprKind::RecordUpdate { .. }
        | CstExprKind::FieldAccess { .. }
        | CstExprKind::Negate { .. }
        | CstExprKind::Case { .. }
        | CstExprKind::Do { .. }
        | CstExprKind::Tuple { .. }
        | CstExprKind::Typed { .. }
        | CstExprKind::TypeApplication { .. } => {
            return Err(LowerError::new(
                span,
                "this expression syntax is not supported yet",
            ));
        }
    };
    Ok(Expr { kind, span })
}

fn lower_lambda(binder: Binder, body: Expr) -> Expr {
    let span = TextRange::new(binder.span.start, body.span.end);
    Expr {
        kind: ExprKind::Lambda {
            binder,
            body: Box::new(body),
        },
        span,
    }
}

pub(crate) fn lower_name(name: cst::CstName) -> Name {
    Name {
        text: name.text,
        span: name.span,
    }
}

#[cfg(test)]
mod tests;
