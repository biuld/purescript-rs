use crate::{
    BuiltinType, FunctionalDependency, Role, RoleDeclaration, Type, TypeDeclaration,
    TypeDeclarationKind, TypeId, TypeKind, TypeParameter,
};
use psrs_span::TextRange;

mod core;
mod numbers;
mod rows;
mod state;
mod type_error;

#[cfg(test)]
mod tests;

/// HIR declarations for official primitive names with stable identities on the
/// shared type spine. Each declaration is owned by its virtual interface.
pub fn primitive_type_declarations() -> Vec<(&'static str, TypeDeclaration)> {
    [
        core::declarations(),
        rows::declarations(),
        numbers::declarations(),
        type_error::declarations(),
        state::declarations(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn class(
    id: TypeId,
    name: &str,
    parameters: &[&str],
    declared_kind: Type,
    fundeps: Vec<FunctionalDependency>,
) -> TypeDeclaration {
    TypeDeclaration {
        id,
        name: name.to_owned(),
        name_span: empty_span(),
        kind: TypeDeclarationKind::Class,
        compiler_class: None,
        parameters: parameters
            .iter()
            .map(|name| TypeParameter {
                name: (*name).to_owned(),
                name_span: empty_span(),
                kind: None,
            })
            .collect(),
        constructors: Vec::new(),
        members: Vec::new(),
        body: None,
        superclasses: Vec::new(),
        fundeps,
        declared_kind: Some(declared_kind),
        declared_roles: None,
        span: empty_span(),
    }
}

fn foreign_type(id: TypeId, name: &str, declared_kind: Type, roles: &[Role]) -> TypeDeclaration {
    TypeDeclaration {
        id,
        name: name.to_owned(),
        name_span: empty_span(),
        kind: TypeDeclarationKind::Foreign,
        compiler_class: None,
        parameters: Vec::new(),
        constructors: Vec::new(),
        members: Vec::new(),
        body: None,
        superclasses: Vec::new(),
        fundeps: Vec::new(),
        declared_kind: Some(declared_kind),
        declared_roles: Some(role_declaration(roles)),
        span: empty_span(),
    }
}

/// A foreign type's trusted role signature. The shared role check compares it
/// against the arity its declared kind implies, so an omitted or extra role is
/// a diagnostic rather than a silently different coercion. Every registry
/// foreign type therefore states its roles, including an empty vector.
fn role_declaration(roles: &[Role]) -> RoleDeclaration {
    RoleDeclaration {
        roles: roles
            .iter()
            .map(|role| (*role, empty_span()))
            .collect::<Vec<_>>(),
        span: empty_span(),
    }
}

fn fd(from: &[&str], to: &[&str]) -> FunctionalDependency {
    FunctionalDependency {
        from: from.iter().map(|name| (*name).to_owned()).collect(),
        to: to.iter().map(|name| (*name).to_owned()).collect(),
        span: empty_span(),
    }
}

fn forall_kind(variable_name: &str, parameters: Vec<Type>) -> Type {
    forall(
        variable_name,
        arrow_kind(parameters, builtin(BuiltinType::Constraint)),
    )
}

fn forall_kind_result(variable_name: &str, parameters: Vec<Type>, result: Type) -> Type {
    forall(variable_name, arrow_kind(parameters, result))
}

/// `forall k. body` over one quantified kind variable. `body` is written out
/// rather than given as arrow parameters because `RowList.Nil` is the one
/// official member whose kind takes the kind variable as an ordinary argument,
/// `forall k. RowList k`, instead of as an arrow parameter.
fn forall(variable_name: &str, body: Type) -> Type {
    Type {
        kind: TypeKind::Forall {
            variables: vec![TypeParameter {
                name: variable_name.to_owned(),
                name_span: empty_span(),
                kind: None,
            }],
            body: Box::new(body),
        },
        span: empty_span(),
    }
}

fn function_kind(parameters: Vec<Type>) -> Type {
    arrow_kind(parameters, builtin(BuiltinType::Constraint))
}

fn arrow_kind(parameters: Vec<Type>, result: Type) -> Type {
    parameters
        .into_iter()
        .rev()
        .fold(result, |result, parameter| Type {
            kind: TypeKind::Function {
                parameter: Box::new(parameter),
                result: Box::new(result),
            },
            span: empty_span(),
        })
}

fn row_kind(element: Type) -> Type {
    apply(builtin(BuiltinType::Row), element)
}

fn apply(function: Type, argument: Type) -> Type {
    Type {
        kind: TypeKind::Application(Box::new(function), Box::new(argument)),
        span: empty_span(),
    }
}

fn named(id: TypeId) -> Type {
    Type {
        kind: TypeKind::Named(id),
        span: empty_span(),
    }
}

fn variable(name: &str) -> Type {
    Type {
        kind: TypeKind::Variable(name.to_owned()),
        span: empty_span(),
    }
}

fn builtin(builtin: BuiltinType) -> Type {
    Type {
        kind: TypeKind::Constructor(builtin),
        span: empty_span(),
    }
}

fn empty_span() -> TextRange {
    TextRange::new(0, 0)
}
