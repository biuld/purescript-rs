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
    Name(Name),
    Application(Box<Type>, Box<Type>),
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
