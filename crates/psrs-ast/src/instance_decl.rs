use crate::{DerivationStrategy, InstanceDeclaration, LowerError, Name, Type, lower_type};
use psrs_cst as cst;

/// Lowers an `instance` declaration. Its where-block members become ordinary
/// value declarations, one per method implementation.
pub(crate) fn lower_instance(
    declaration: cst::InstanceDeclaration,
    chain_id: u32,
    chain_position: u32,
    derivation: Option<DerivationStrategy>,
) -> Result<InstanceDeclaration, LowerError> {
    let context = match declaration.constraints {
        Some(constraints) => lower_constraints(*constraints)?,
        None => Vec::new(),
    };
    let head = lower_type(declaration.head)?;
    let mut members = Vec::new();
    if let Some(block) = declaration.where_block {
        for member in block.declarations {
            match member {
                cst::Declaration::Value(value) => {
                    members.push(crate::lower_value_declaration(value)?);
                }
                cst::Declaration::TypeSignature(_) => {}
                other => {
                    return Err(LowerError::new(
                        other.span(),
                        "this instance member is not supported yet",
                    ));
                }
            }
        }
    }
    let name = match declaration.name {
        Some(name) => crate::lower_name(name),
        None => Name {
            text: String::new(),
            span: declaration.span,
        },
    };
    Ok(InstanceDeclaration {
        name,
        chain_id,
        chain_position,
        context,
        head,
        members,
        derivation,
        span: declaration.span,
    })
}

pub(crate) fn lower_derive(
    declaration: cst::DeriveDeclaration,
    chain_id: u32,
) -> Result<InstanceDeclaration, LowerError> {
    let context = match declaration.constraints {
        Some(constraints) => lower_constraints(*constraints)?,
        None => Vec::new(),
    };
    Ok(InstanceDeclaration {
        name: match declaration.name {
            Some(name) => crate::lower_name(name),
            None => Name {
                text: String::new(),
                span: declaration.span,
            },
        },
        chain_id,
        chain_position: 0,
        context,
        head: lower_type(declaration.head)?,
        members: Vec::new(),
        derivation: Some(if declaration.newtype_keyword_span.is_some() {
            DerivationStrategy::Newtype
        } else {
            DerivationStrategy::KnownClass
        }),
        span: declaration.span,
    })
}

fn lower_constraints(expression: cst::TypeExpr) -> Result<Vec<Type>, LowerError> {
    match expression.kind {
        cst::TypeExprKind::Tuple { items, .. } => items.into_iter().map(lower_type).collect(),
        cst::TypeExprKind::Parens { expression, .. } => lower_constraints(*expression),
        kind => Ok(vec![lower_type(cst::TypeExpr {
            kind,
            span: expression.span,
        })?]),
    }
}
