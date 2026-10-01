use crate::{Type, TypeField, TypeKind, TypeParameter};
use std::collections::{HashMap, HashSet};

/// Substitutes free type variables, renaming `forall` binders when a
/// replacement would otherwise be captured.
///
/// `next_fresh` is a caller-owned name supply so a sequence of substitutions
/// can use distinct names. Existing names in the type and replacement types
/// are always excluded as well.
pub fn substitute_type_variables(
    ty: &Type,
    substitutions: &HashMap<String, Type>,
    next_fresh: &mut u32,
) -> Type {
    substitute(ty, substitutions, next_fresh)
}

fn substitute(ty: &Type, substitutions: &HashMap<String, Type>, next_fresh: &mut u32) -> Type {
    if let TypeKind::Variable(name) = &ty.kind {
        return substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone());
    }
    let kind = match &ty.kind {
        TypeKind::Application(function, argument) => TypeKind::Application(
            Box::new(substitute(function, substitutions, next_fresh)),
            Box::new(substitute(argument, substitutions, next_fresh)),
        ),
        TypeKind::Function { parameter, result } => TypeKind::Function {
            parameter: Box::new(substitute(parameter, substitutions, next_fresh)),
            result: Box::new(substitute(result, substitutions, next_fresh)),
        },
        TypeKind::Forall { variables, body } => {
            substitute_forall(variables, body, substitutions, next_fresh)
        }
        TypeKind::Constrained { constraint, body } => TypeKind::Constrained {
            constraint: Box::new(substitute(constraint, substitutions, next_fresh)),
            body: Box::new(substitute(body, substitutions, next_fresh)),
        },
        TypeKind::Row { fields, tail } => TypeKind::Row {
            fields: substitute_fields(fields, substitutions, next_fresh),
            tail: tail
                .as_deref()
                .map(|tail| Box::new(substitute(tail, substitutions, next_fresh))),
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: substitute_fields(fields, substitutions, next_fresh),
            tail: tail
                .as_deref()
                .map(|tail| Box::new(substitute(tail, substitutions, next_fresh))),
        },
        _ => return ty.clone(),
    };
    Type {
        kind,
        span: ty.span,
    }
}

fn substitute_forall(
    variables: &[TypeParameter],
    body: &Type,
    substitutions: &HashMap<String, Type>,
    next_fresh: &mut u32,
) -> TypeKind {
    let replacement_free = substitutions
        .values()
        .flat_map(free_variables)
        .collect::<HashSet<_>>();
    let substitution_names = substitutions.keys().cloned().collect::<HashSet<_>>();
    let mut scoped_substitutions = substitutions.clone();
    let mut variables = variables.to_vec();
    let mut body = body.clone();
    for index in 0..variables.len() {
        let old_name = variables[index].name.clone();
        if replacement_free.contains(&old_name) {
            let fresh_name = fresh_name(
                &old_name,
                &body,
                &variables,
                &replacement_free,
                &substitution_names,
                next_fresh,
            );
            body = rename_bound(&body, &old_name, &fresh_name);
            for variable in variables.iter_mut().skip(index + 1) {
                if let Some(kind) = &variable.kind {
                    variable.kind = Some(rename_bound(kind, &old_name, &fresh_name));
                }
            }
            variables[index].name = fresh_name;
        }
        if let Some(kind) = &variables[index].kind {
            variables[index].kind = Some(substitute(kind, &scoped_substitutions, next_fresh));
        }
        scoped_substitutions.remove(&old_name);
    }
    TypeKind::Forall {
        variables,
        body: Box::new(substitute(&body, &scoped_substitutions, next_fresh)),
    }
}

fn substitute_fields(
    fields: &[TypeField],
    substitutions: &HashMap<String, Type>,
    next_fresh: &mut u32,
) -> Vec<TypeField> {
    fields
        .iter()
        .map(|field| TypeField {
            ty: substitute(&field.ty, substitutions, next_fresh),
            ..field.clone()
        })
        .collect()
}

fn rename_bound(ty: &Type, old: &str, new: &str) -> Type {
    let kind = match &ty.kind {
        TypeKind::Variable(name) if name == old => TypeKind::Variable(new.to_owned()),
        TypeKind::Application(function, argument) => TypeKind::Application(
            Box::new(rename_bound(function, old, new)),
            Box::new(rename_bound(argument, old, new)),
        ),
        TypeKind::Function { parameter, result } => TypeKind::Function {
            parameter: Box::new(rename_bound(parameter, old, new)),
            result: Box::new(rename_bound(result, old, new)),
        },
        TypeKind::Forall { variables, body } => {
            let mut shadowed = false;
            let variables = variables
                .iter()
                .map(|variable| {
                    let kind = variable.kind.as_ref().map(|kind| {
                        if shadowed {
                            kind.clone()
                        } else {
                            rename_bound(kind, old, new)
                        }
                    });
                    shadowed |= variable.name == old;
                    TypeParameter {
                        name: variable.name.clone(),
                        name_span: variable.name_span,
                        kind,
                    }
                })
                .collect();
            let body = if shadowed {
                body.as_ref().clone()
            } else {
                rename_bound(body, old, new)
            };
            TypeKind::Forall {
                variables,
                body: Box::new(body),
            }
        }
        TypeKind::Constrained { constraint, body } => TypeKind::Constrained {
            constraint: Box::new(rename_bound(constraint, old, new)),
            body: Box::new(rename_bound(body, old, new)),
        },
        TypeKind::Row { fields, tail } => TypeKind::Row {
            fields: rename_fields(fields, old, new),
            tail: tail
                .as_deref()
                .map(|tail| Box::new(rename_bound(tail, old, new))),
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: rename_fields(fields, old, new),
            tail: tail
                .as_deref()
                .map(|tail| Box::new(rename_bound(tail, old, new))),
        },
        _ => return ty.clone(),
    };
    Type {
        kind,
        span: ty.span,
    }
}

fn rename_fields(fields: &[TypeField], old: &str, new: &str) -> Vec<TypeField> {
    fields
        .iter()
        .map(|field| TypeField {
            ty: rename_bound(&field.ty, old, new),
            ..field.clone()
        })
        .collect()
}

fn free_variables(ty: &Type) -> HashSet<String> {
    let mut free = HashSet::new();
    collect_free_variables(ty, &mut HashSet::new(), &mut free);
    free
}

fn collect_free_variables(ty: &Type, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
    match &ty.kind {
        TypeKind::Variable(name) if !bound.contains(name) => {
            free.insert(name.clone());
        }
        TypeKind::Application(function, argument) => {
            collect_free_variables(function, bound, free);
            collect_free_variables(argument, bound, free);
        }
        TypeKind::Function { parameter, result } => {
            collect_free_variables(parameter, bound, free);
            collect_free_variables(result, bound, free);
        }
        TypeKind::Forall { variables, body } => {
            let mut inserted = Vec::new();
            for variable in variables {
                if let Some(kind) = &variable.kind {
                    collect_free_variables(kind, bound, free);
                }
                if bound.insert(variable.name.clone()) {
                    inserted.push(variable.name.clone());
                }
            }
            collect_free_variables(body, bound, free);
            for name in inserted {
                bound.remove(&name);
            }
        }
        TypeKind::Constrained { constraint, body } => {
            collect_free_variables(constraint, bound, free);
            collect_free_variables(body, bound, free);
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_free_variables(&field.ty, bound, free);
            }
            if let Some(tail) = tail {
                collect_free_variables(tail, bound, free);
            }
        }
        _ => {}
    }
}

fn fresh_name(
    base: &str,
    body: &Type,
    variables: &[TypeParameter],
    replacement_free: &HashSet<String>,
    substitution_names: &HashSet<String>,
    next_fresh: &mut u32,
) -> String {
    let existing_names = all_variable_names(body);
    loop {
        let candidate = format!("__psrs_type_subst_{}_{}", base, *next_fresh);
        *next_fresh += 1;
        if !replacement_free.contains(&candidate)
            && !substitution_names.contains(&candidate)
            && !existing_names.contains(&candidate)
            && !variables.iter().any(|variable| variable.name == candidate)
        {
            return candidate;
        }
    }
}

fn all_variable_names(ty: &Type) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_variable_names(ty, &mut names);
    names
}

fn collect_variable_names(ty: &Type, names: &mut HashSet<String>) {
    match &ty.kind {
        TypeKind::Variable(name) => {
            names.insert(name.clone());
        }
        TypeKind::Application(function, argument) => {
            collect_variable_names(function, names);
            collect_variable_names(argument, names);
        }
        TypeKind::Function { parameter, result } => {
            collect_variable_names(parameter, names);
            collect_variable_names(result, names);
        }
        TypeKind::Forall { variables, body } => {
            for variable in variables {
                names.insert(variable.name.clone());
                if let Some(kind) = &variable.kind {
                    collect_variable_names(kind, names);
                }
            }
            collect_variable_names(body, names);
        }
        TypeKind::Constrained { constraint, body } => {
            collect_variable_names(constraint, names);
            collect_variable_names(body, names);
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_variable_names(&field.ty, names);
            }
            if let Some(tail) = tail {
                collect_variable_names(tail, names);
            }
        }
        _ => {}
    }
}
