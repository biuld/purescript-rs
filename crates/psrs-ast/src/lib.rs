use psrs_cst::{self as cst, ExprKind as CstExprKind};
use psrs_span::TextRange;
use std::collections::HashSet;

mod export;
mod expr;
mod import;
mod ty;
mod type_decl;

pub use export::{ExportList, ExportRef, TypeMembers};
pub use expr::{Binder, CaseBranch, Declaration, Expr, ExprKind, Pattern, PatternKind};
pub use import::{Import, ImportList, ImportRef};
pub(crate) use ty::lower_type;
pub use ty::{Type, TypeField, TypeKind};
pub use type_decl::{
    ClassDeclaration, ClassMember, DataConstructor, DataDeclaration, ForeignDataDeclaration,
    NewtypeDeclaration, TypeDeclaration, TypeParameter, TypeSynonymDeclaration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: Name,
    pub exports: Option<ExportList>,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
    pub foreign_imports: Vec<ForeignImport>,
    pub type_declarations: Vec<TypeDeclaration>,
    pub span: TextRange,
}

/// A `foreign import` with a WIT binding: a value provided by a WIT interface
/// rather than defined in source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignImport {
    pub name: Name,
    pub annotation: Type,
    /// The WIT binding, `<interface>#<function>`.
    pub binding: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub text: String,
    pub span: TextRange,
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
    let mut foreign_imports = Vec::new();
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
            cst::Declaration::Foreign(declaration) => {
                if declaration.data_keyword_span.is_some() {
                    match type_decl::lower_foreign_data(declaration) {
                        Ok(declaration) => type_declarations.push(declaration),
                        Err(error) => errors.push(error),
                    }
                } else {
                    match lower_foreign_import(declaration) {
                        Ok(foreign) => foreign_imports.push(foreign),
                        Err(error) => errors.push(error),
                    }
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
            foreign_imports,
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

fn lower_foreign_import(declaration: cst::ForeignDeclaration) -> Result<ForeignImport, LowerError> {
    let Some(binding) = declaration.binding else {
        return Err(LowerError::new(
            declaration.span,
            "a foreign import requires a `\"<interface>#<function>\"` WIT binding",
        ));
    };
    Ok(ForeignImport {
        name: Name {
            text: declaration.name.text,
            span: declaration.name.span,
        },
        annotation: lower_type(declaration.type_expr)?,
        binding: binding.text,
        span: declaration.span,
    })
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
        value = expr::lower_pattern_lambda(parameter, value)?;
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
        if let Some(error) = expr::check_pattern_names(parameter, &mut seen) {
            return Some(error);
        }
    }
    None
}

fn lower_expr(expression: cst::Expr) -> Result<Expr, LowerError> {
    let span = expression.span;
    let kind = match expression.kind {
        CstExprKind::Name(name) => ExprKind::Name(lower_name(name)),
        CstExprKind::Integer(value) => ExprKind::Integer(value),
        CstExprKind::Number(value) => ExprKind::Number(value),
        CstExprKind::String(value) => ExprKind::String(value),
        CstExprKind::Char(value) => ExprKind::Char(value),
        CstExprKind::Array { elements, .. } => ExprKind::Array(
            elements
                .into_iter()
                .map(lower_expr)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        CstExprKind::Record { fields, tail, .. } => expr::lower_record(fields, tail, span)?,
        CstExprKind::RecordUpdate {
            expression, fields, ..
        } => expr::lower_record_update(*expression, fields)?,
        CstExprKind::FieldAccess {
            expression, field, ..
        } => ExprKind::FieldAccess {
            expression: Box::new(lower_expr(*expression)?),
            field: field.text,
        },
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
                body = expr::lower_pattern_lambda(parameter, body)?;
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
        CstExprKind::Case {
            scrutinees,
            alternatives,
            ..
        } => {
            if scrutinees.len() != 1 {
                return Err(LowerError::new(
                    span,
                    "case with multiple scrutinees is not supported yet",
                ));
            }
            let scrutinee = Box::new(lower_expr(scrutinees.into_iter().next().unwrap())?);
            let mut branches = Vec::with_capacity(alternatives.len());
            for alternative in alternatives {
                if alternative.patterns.len() != 1 {
                    return Err(LowerError::new(
                        alternative.span,
                        "case alternatives with multiple patterns are not supported yet",
                    ));
                }
                let cst::CaseRhs::Plain {
                    value, where_block, ..
                } = alternative.rhs
                else {
                    return Err(LowerError::new(
                        alternative.span,
                        "guarded case alternatives are not supported yet",
                    ));
                };
                if let Some(block) = where_block {
                    return Err(LowerError::new(
                        block.span,
                        "where blocks are not supported yet",
                    ));
                }
                let pattern =
                    expr::lower_pattern(alternative.patterns.into_iter().next().unwrap())?;
                let value = lower_expr(value)?;
                branches.push(CaseBranch {
                    pattern,
                    value,
                    span: alternative.span,
                });
            }
            ExprKind::Case {
                scrutinee,
                branches,
            }
        }
        CstExprKind::Parens { expression, .. } => {
            let mut expression = lower_expr(*expression)?;
            expression.span = span;
            return Ok(expression);
        }
        CstExprKind::Tuple { items, .. } => ExprKind::Record(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| Ok((tuple_label(index), lower_expr(item)?)))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        CstExprKind::Hole(_)
        | CstExprKind::Negate { .. }
        | CstExprKind::Do { .. }
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

/// Tuple component labels. A tuple is the closed record `{ _1, _2, ... }`.
pub(crate) fn tuple_label(index: usize) -> String {
    format!("_{}", index + 1)
}

pub(crate) fn lower_name(name: cst::CstName) -> Name {
    Name {
        text: name.text,
        span: name.span,
    }
}

#[cfg(test)]
mod tests;
