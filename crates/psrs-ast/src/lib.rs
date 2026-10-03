use psrs_cst::{self as cst, ExprKind as CstExprKind};
use psrs_span::TextRange;
use std::collections::HashSet;

mod do_notation;
mod equations;
mod export;
mod expr;
mod fixity;
mod import;
mod instance_decl;
mod local;
mod lower;
mod role;
mod ty;
mod type_decl;

pub use export::{ExportList, ExportRef, TypeMembers};
pub use expr::{
    Binder, CaseBranch, Declaration, Expr, ExprKind, Guard, GuardedExpr, Pattern, PatternKind,
    RecordPatternMode,
};
pub use fixity::{Associativity, FixityDeclaration, FixityNamespace, Operator, SectionSide};
pub use import::{Import, ImportList, ImportRef};
pub use lower::lower_module;
pub use role::{RoleAnnotation, RoleDeclaration, TypeRole};
pub(crate) use ty::lower_type;
pub use ty::{Type, TypeField, TypeKind};
pub use type_decl::{
    ClassDeclaration, ClassMember, DataConstructor, DataDeclaration, DerivationStrategy,
    ForeignDataDeclaration, FunctionalDependency, InstanceDeclaration, NewtypeDeclaration,
    TypeDeclaration, TypeParameter, TypeSynonymDeclaration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: Name,
    pub exports: Option<ExportList>,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
    pub foreign_imports: Vec<ForeignImport>,
    pub type_declarations: Vec<TypeDeclaration>,
    /// Source role annotations retained until name resolution attaches them to
    /// their local type declaration.
    pub role_declarations: Vec<RoleDeclaration>,
    pub fixities: Vec<FixityDeclaration>,
    pub instances: Vec<InstanceDeclaration>,
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

pub(crate) fn lower_declarations(
    declarations: Vec<cst::Declaration>,
) -> Result<Vec<Declaration>, LowerError> {
    let mut lowered = Vec::new();
    let mut index = 0;
    while index < declarations.len() {
        if let cst::Declaration::Value(first) = &declarations[index] {
            let mut end = index + 1;
            while let Some(cst::Declaration::Value(next)) = declarations.get(end)
                && next.name.text == first.name.text
            {
                end += 1;
            }
            let group = declarations[index..end]
                .iter()
                .map(|declaration| match declaration {
                    cst::Declaration::Value(value) => Ok(value.clone()),
                    _ => unreachable!("the group contains only value declarations"),
                })
                .collect::<Result<Vec<_>, LowerError>>()?;
            lowered.push(equations::lower_value_declarations(
                group,
                "OverlappingNamesInLet",
            )?);
            index = end;
        } else {
            lowered.push(lower_declaration(declarations[index].clone())?);
            index += 1;
        }
    }
    Ok(lowered)
}

pub(crate) fn lower_value_declaration(
    declaration: cst::ValueDeclaration,
) -> Result<Declaration, LowerError> {
    equations::lower_value_declarations(vec![declaration], "DuplicateValueDeclaration")
}

pub(crate) fn wrap_where(
    block: Option<cst::DeclarationBlock>,
    value: Expr,
) -> Result<Expr, LowerError> {
    let Some(block) = block else {
        return Ok(value);
    };
    let span = TextRange::new(block.span.start, value.span.end);
    local::lower_local_declarations(block.declarations, value, span)
}

/// Reports the second occurrence of a repeated value argument name.
pub(crate) fn check_argument_names(parameters: &[cst::Pattern]) -> Option<LowerError> {
    let mut seen = HashSet::new();
    for parameter in parameters {
        if let Some(error) = expr::check_pattern_names(parameter, &mut seen) {
            return Some(error);
        }
    }
    None
}

pub(crate) fn lower_expr(expression: cst::Expr) -> Result<Expr, LowerError> {
    let span = expression.span;
    let cst_kind = match expression.kind {
        CstExprKind::Let {
            declarations, body, ..
        } => {
            return local::lower_local_declarations(declarations, lower_expr(*body)?, span);
        }
        kind => kind,
    };
    let kind = match cst_kind {
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
        } => return lower_operator_chain(operator, *left, *right, span),
        CstExprKind::OperatorSection {
            operator,
            operand,
            side,
        } => ExprKind::OperatorSection {
            operator: lower_name(operator),
            operand: Box::new(lower_expr(*operand)?),
            side: match side {
                cst::OperatorSectionSide::Left => SectionSide::Left,
                cst::OperatorSectionSide::Right => SectionSide::Right,
            },
        },
        CstExprKind::Negate {
            minus_span,
            expression,
        } => ExprKind::Negate {
            minus_span,
            expression: Box::new(lower_expr(*expression)?),
        },
        CstExprKind::Lambda {
            parameters, body, ..
        } => {
            if let Some(error) = check_argument_names(&parameters) {
                return Err(error);
            }
            let mut body = lower_expr(*body)?;
            for parameter in parameters.into_iter().rev() {
                body = expr::lower_pattern_lambda(parameter, body)?;
            }
            return Ok(Expr {
                kind: body.kind,
                span,
            });
        }
        CstExprKind::Let { .. } => {
            unreachable!("let expressions are lowered before this match")
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
            let (scrutinee, anonymous_inputs) = expr::lower_case_scrutinees(scrutinees, span)?;
            let mut branches = Vec::with_capacity(alternatives.len());
            for alternative in alternatives {
                if alternative.patterns.is_empty() {
                    return Err(LowerError::new(
                        alternative.span,
                        "case alternatives require at least one pattern",
                    ));
                }
                let (pattern, pattern_guards) =
                    expr::lower_case_patterns(alternative.patterns, alternative.span)?;
                let value = match alternative.rhs {
                    cst::CaseRhs::Plain {
                        value, where_block, ..
                    } => wrap_where(where_block, lower_expr(value)?)?,
                    cst::CaseRhs::Guarded(clauses) => Expr {
                        kind: ExprKind::Guarded(
                            clauses
                                .into_iter()
                                .map(|clause| {
                                    let where_declarations = match clause.where_block {
                                        Some(block) => lower_declarations(block.declarations)?,
                                        None => Vec::new(),
                                    };
                                    Ok(expr::GuardedExpr {
                                        guards: clause
                                            .guards
                                            .into_iter()
                                            .map(expr::lower_guard)
                                            .collect::<Result<Vec<_>, _>>()?
                                            .into_iter()
                                            .flatten()
                                            .collect(),
                                        value: lower_expr(clause.value)?,
                                        where_declarations,
                                        span: clause.span,
                                    })
                                })
                                .collect::<Result<Vec<_>, LowerError>>()?,
                        ),
                        span: alternative.span,
                    },
                };
                let value = expr::prepend_guards(value, pattern_guards, alternative.span);
                branches.push(CaseBranch {
                    pattern,
                    value,
                    span: alternative.span,
                });
            }
            let mut case = Expr {
                kind: ExprKind::Case {
                    scrutinee: Box::new(scrutinee),
                    branches,
                },
                span,
            };
            for binder in anonymous_inputs.into_iter().rev() {
                case = Expr {
                    kind: ExprKind::Lambda {
                        binder,
                        body: Box::new(case),
                    },
                    span,
                };
            }
            return Ok(case);
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
        CstExprKind::Do {
            statements,
            result: Some(result),
            ..
        } => {
            return do_notation::lower_ado(statements, *result, span);
        }
        CstExprKind::Do {
            do_keyword_span,
            statements,
            ..
        } => {
            return do_notation::lower_do(statements, do_keyword_span, span);
        }
        CstExprKind::Typed {
            expression,
            type_expr,
            ..
        } => {
            // The outer span already covers the expression and its type, so the
            // ascription keeps it and the parser's punctuation span is dropped.
            let expression = lower_expr(*expression)?;
            let ty = lower_type(type_expr)?;
            ExprKind::Typed {
                expression: Box::new(expression),
                ty,
            }
        }
        CstExprKind::TypeApplication {
            expression,
            type_expr,
            ..
        } => {
            // The outer span already covers the expression and its type, so the
            // application keeps it and the `@` span is dropped. The written type
            // is the argument the checker substitutes for the quantifier the
            // expression's own `forall` binds at this position.
            let expression = lower_expr(*expression)?;
            let ty = lower_type(type_expr)?;
            ExprKind::TypeApplication {
                expression: Box::new(expression),
                ty,
            }
        }
        CstExprKind::Hole(_) => {
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

fn lower_operator_chain(
    operator: cst::CstName,
    left: cst::Expr,
    right: cst::Expr,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let mut operands = Vec::new();
    let mut operators = Vec::new();
    collect_operator_chain(left, &mut operands, &mut operators)?;
    operators.push(Operator {
        name: lower_name(operator.clone()),
        span: operator.span,
    });
    collect_operator_chain(right, &mut operands, &mut operators)?;
    Ok(Expr {
        kind: ExprKind::OperatorChain {
            operands,
            operators,
        },
        span,
    })
}

fn collect_operator_chain(
    expression: cst::Expr,
    operands: &mut Vec<Expr>,
    operators: &mut Vec<Operator>,
) -> Result<(), LowerError> {
    let span = expression.span;
    match expression.kind {
        CstExprKind::Operator {
            operator,
            left,
            right,
        } => {
            collect_operator_chain(*left, operands, operators)?;
            operators.push(Operator {
                name: lower_name(operator.clone()),
                span: operator.span,
            });
            collect_operator_chain(*right, operands, operators)
        }
        kind => {
            operands.push(lower_expr(cst::Expr { kind, span })?);
            Ok(())
        }
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
#[cfg(test)]
mod unary_minus_tests;
