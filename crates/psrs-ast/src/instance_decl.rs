use crate::{
    Declaration, DerivationStrategy, InstanceDeclaration, LowerError, Name, Type, lower_type,
};
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
    let members = match declaration.where_block {
        Some(block) => lower_instance_members(block.declarations)?,
        None => Vec::new(),
    };
    let mut member_names = std::collections::HashSet::new();
    for member in &members {
        if !member_names.insert(member.name.text.clone()) {
            return Err(LowerError::coded(
                member.name.span,
                "DuplicateValueDeclaration",
                "an instance cannot define the same class member more than once",
            ));
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

/// Lowers instance methods as declaration groups. A signature belongs to the
/// immediately following equation group with the same name; equations remain
/// grouped only while they are consecutive in source order.
fn lower_instance_members(
    declarations: Vec<cst::Declaration>,
) -> Result<Vec<Declaration>, LowerError> {
    let mut members = Vec::new();
    let mut index = 0;
    while index < declarations.len() {
        let (first, group_start) = match &declarations[index] {
            cst::Declaration::TypeSignature(signature) => {
                let Some(cst::Declaration::Value(value)) = declarations.get(index + 1) else {
                    return Err(orphan_instance_signature(signature));
                };
                if signature.name.text != value.name.text {
                    return Err(orphan_instance_signature(signature));
                }
                let mut value = value.clone();
                value.annotation = Some(signature.type_expr.clone());
                (value, index + 1)
            }
            cst::Declaration::Value(value) => (value.clone(), index),
            other => {
                return Err(LowerError::new(
                    other.span(),
                    "only value declarations are supported in an instance body",
                ));
            }
        };

        let mut end = group_start + 1;
        while let Some(cst::Declaration::Value(value)) = declarations.get(end)
            && value.name.text == first.name.text
        {
            end += 1;
        }
        let mut group = Vec::with_capacity(end - group_start);
        group.push(first);
        for declaration in &declarations[group_start + 1..end] {
            let cst::Declaration::Value(value) = declaration else {
                unreachable!("equation group contains only values")
            };
            group.push(value.clone());
        }
        members.push(crate::equations::lower_value_declarations(
            group,
            "DuplicateValueDeclaration",
        )?);
        index = end;
    }
    Ok(members)
}

fn orphan_instance_signature(signature: &cst::TypeSignature) -> LowerError {
    LowerError::coded(
        signature.span,
        "OrphanTypeDeclaration",
        "an instance member type declaration must be followed by a matching value declaration",
    )
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
