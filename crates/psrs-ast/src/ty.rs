use crate::{LowerError, Name, TypeParameter, lower_name};
use psrs_cst::{self as cst, TypeExprKind as CstTypeExprKind};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: TextRange,
}

/// A row or record field. The label is unresolved; resolution replaces it with
/// the same spelling so row checks can compare labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeField {
    pub label: Name,
    pub ty: Type,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    /// An anonymous type variable inferred from this source annotation.
    Wildcard,
    Name(Name),
    Application(Box<Type>, Box<Type>),
    OperatorChain {
        operands: Vec<Type>,
        operators: Vec<super::Operator>,
    },
    Function {
        parameter: Box<Type>,
        result: Box<Type>,
    },
    Forall {
        variables: Vec<TypeParameter>,
        body: Box<Type>,
    },
    Constrained {
        constraint: Box<Type>,
        body: Box<Type>,
    },
    Row {
        fields: Vec<TypeField>,
        tail: Option<Box<Type>>,
    },
    Record {
        fields: Vec<TypeField>,
        tail: Option<Box<Type>>,
    },
    Integer(String),
    String(String),
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
        } => {
            return lower_operator_chain(operator, *left, *right, span);
        }
        CstTypeExprKind::Tuple { items, .. } => TypeKind::Record {
            fields: items
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    let ty = lower_type(item)?;
                    let span = ty.span;
                    Ok(TypeField {
                        label: Name {
                            text: super::tuple_label(index),
                            span,
                        },
                        ty,
                        span,
                    })
                })
                .collect::<Result<_, _>>()?,
            tail: None,
        },
        CstTypeExprKind::Wildcard(_) => TypeKind::Wildcard,
        CstTypeExprKind::Hole(_) | CstTypeExprKind::PrefixOperator { .. } => {
            return Err(LowerError::new(
                span,
                "this type syntax is not supported yet",
            ));
        }
    };
    Ok(Type { kind, span })
}

fn lower_operator_chain(
    operator: cst::CstName,
    left: cst::TypeExpr,
    right: cst::TypeExpr,
    span: TextRange,
) -> Result<Type, LowerError> {
    fn append(
        expression: cst::TypeExpr,
        operands: &mut Vec<Type>,
        operators: &mut Vec<super::Operator>,
    ) -> Result<(), LowerError> {
        let span = expression.span;
        match expression.kind {
            CstTypeExprKind::Operator {
                operator,
                left,
                right,
            } => {
                append(*left, operands, operators)?;
                operators.push(super::Operator {
                    name: lower_name(operator.clone()),
                    span: operator.span,
                });
                append(*right, operands, operators)
            }
            kind => {
                operands.push(lower_type(cst::TypeExpr { kind, span })?);
                Ok(())
            }
        }
    }

    let mut operands = Vec::new();
    let mut operators = Vec::new();
    append(left, &mut operands, &mut operators)?;
    operators.push(super::Operator {
        name: lower_name(operator.clone()),
        span: operator.span,
    });
    append(right, &mut operands, &mut operators)?;
    Ok(Type {
        kind: TypeKind::OperatorChain {
            operands,
            operators,
        },
        span,
    })
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
