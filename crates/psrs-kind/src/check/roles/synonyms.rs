use psrs_hir::{self as hir, TypeId, TypeKind};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) struct Synonym {
    parameters: Vec<String>,
    body: hir::Type,
}

pub(super) type SynonymMap = HashMap<TypeId, Synonym>;

pub(super) fn collect(modules: &[hir::Module]) -> SynonymMap {
    modules
        .iter()
        .flat_map(|module| &module.types)
        .filter(|declaration| declaration.kind == hir::TypeDeclarationKind::TypeSynonym)
        .filter_map(|declaration| {
            Some((
                declaration.id,
                Synonym {
                    parameters: declaration
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                    body: declaration.body.clone()?,
                },
            ))
        })
        .collect()
}

pub(super) fn expand(ty: &hir::Type, synonyms: &SynonymMap) -> Option<hir::Type> {
    expand_inner(ty, synonyms, &mut HashSet::new(), &mut 0)
}

fn expand_inner(
    ty: &hir::Type,
    synonyms: &SynonymMap,
    active: &mut HashSet<TypeId>,
    next_fresh: &mut u32,
) -> Option<hir::Type> {
    let kind = match &ty.kind {
        TypeKind::Named(id)
            if synonyms
                .get(id)
                .is_some_and(|item| item.parameters.is_empty()) =>
        {
            return expand_synonym(*id, Vec::new(), synonyms, active, next_fresh);
        }
        TypeKind::Application(function, argument) => {
            let function = expand_inner(function, synonyms, active, next_fresh)?;
            let argument = expand_inner(argument, synonyms, active, next_fresh)?;
            let applied = hir::Type {
                kind: TypeKind::Application(Box::new(function), Box::new(argument)),
                span: ty.span,
            };
            let (head, arguments) = flatten_spine(&applied);
            if let TypeKind::Named(id) = head.kind
                && let Some(synonym) = synonyms.get(&id)
                && synonym.parameters.len() == arguments.len()
            {
                return expand_synonym(id, arguments, synonyms, active, next_fresh);
            }
            return Some(applied);
        }
        TypeKind::Function { parameter, result } => TypeKind::Function {
            parameter: Box::new(expand_inner(parameter, synonyms, active, next_fresh)?),
            result: Box::new(expand_inner(result, synonyms, active, next_fresh)?),
        },
        TypeKind::Forall { variables, body } => TypeKind::Forall {
            variables: variables
                .iter()
                .map(|variable| {
                    Some(hir::TypeParameter {
                        name: variable.name.clone(),
                        name_span: variable.name_span,
                        kind: match &variable.kind {
                            Some(kind) => Some(expand_inner(kind, synonyms, active, next_fresh)?),
                            None => None,
                        },
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            body: Box::new(expand_inner(body, synonyms, active, next_fresh)?),
        },
        TypeKind::Constrained { constraint, body } => TypeKind::Constrained {
            constraint: Box::new(expand_inner(constraint, synonyms, active, next_fresh)?),
            body: Box::new(expand_inner(body, synonyms, active, next_fresh)?),
        },
        TypeKind::Row { fields, tail } => TypeKind::Row {
            fields: expand_fields(fields, synonyms, active, next_fresh)?,
            tail: match tail {
                Some(tail) => Some(Box::new(expand_inner(tail, synonyms, active, next_fresh)?)),
                None => None,
            },
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: expand_fields(fields, synonyms, active, next_fresh)?,
            tail: match tail {
                Some(tail) => Some(Box::new(expand_inner(tail, synonyms, active, next_fresh)?)),
                None => None,
            },
        },
        _ => return Some(ty.clone()),
    };
    Some(hir::Type {
        kind,
        span: ty.span,
    })
}

fn expand_fields(
    fields: &[hir::TypeField],
    synonyms: &SynonymMap,
    active: &mut HashSet<TypeId>,
    next_fresh: &mut u32,
) -> Option<Vec<hir::TypeField>> {
    fields
        .iter()
        .map(|field| {
            Some(hir::TypeField {
                label: field.label.clone(),
                label_span: field.label_span,
                ty: expand_inner(&field.ty, synonyms, active, next_fresh)?,
                span: field.span,
            })
        })
        .collect()
}

fn expand_synonym(
    id: TypeId,
    arguments: Vec<hir::Type>,
    synonyms: &SynonymMap,
    active: &mut HashSet<TypeId>,
    next_fresh: &mut u32,
) -> Option<hir::Type> {
    let synonym = synonyms.get(&id)?;
    if synonym.parameters.len() != arguments.len() || !active.insert(id) {
        return None;
    }
    let mapping = synonym
        .parameters
        .iter()
        .cloned()
        .zip(arguments)
        .collect::<HashMap<_, _>>();
    let instantiated = substitute(&synonym.body, &mapping, next_fresh);
    let expanded = expand_inner(&instantiated, synonyms, active, next_fresh);
    active.remove(&id);
    expanded
}

fn substitute(
    ty: &hir::Type,
    mapping: &HashMap<String, hir::Type>,
    next_fresh: &mut u32,
) -> hir::Type {
    if let TypeKind::Variable(name) = &ty.kind {
        return mapping.get(name).cloned().unwrap_or_else(|| ty.clone());
    }
    let kind = match &ty.kind {
        TypeKind::Application(function, argument) => TypeKind::Application(
            Box::new(substitute(function, mapping, next_fresh)),
            Box::new(substitute(argument, mapping, next_fresh)),
        ),
        TypeKind::Function { parameter, result } => TypeKind::Function {
            parameter: Box::new(substitute(parameter, mapping, next_fresh)),
            result: Box::new(substitute(result, mapping, next_fresh)),
        },
        TypeKind::Forall { variables, body } => {
            substitute_forall(variables, body, mapping, next_fresh)
        }
        TypeKind::Constrained { constraint, body } => TypeKind::Constrained {
            constraint: Box::new(substitute(constraint, mapping, next_fresh)),
            body: Box::new(substitute(body, mapping, next_fresh)),
        },
        TypeKind::Row { fields, tail } => TypeKind::Row {
            fields: substitute_fields(fields, mapping, next_fresh),
            tail: tail
                .as_ref()
                .map(|tail| Box::new(substitute(tail, mapping, next_fresh))),
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: substitute_fields(fields, mapping, next_fresh),
            tail: tail
                .as_ref()
                .map(|tail| Box::new(substitute(tail, mapping, next_fresh))),
        },
        _ => return ty.clone(),
    };
    hir::Type {
        kind,
        span: ty.span,
    }
}

fn substitute_forall(
    variables: &[hir::TypeParameter],
    body: &hir::Type,
    mapping: &HashMap<String, hir::Type>,
    next_fresh: &mut u32,
) -> TypeKind {
    let replacement_free = mapping
        .values()
        .flat_map(free_variables)
        .collect::<HashSet<_>>();
    let mut local_mapping = mapping.clone();
    let mut variables = variables.to_vec();
    let mut body = body.clone();
    for index in 0..variables.len() {
        let old_name = variables[index].name.clone();
        if replacement_free.contains(&old_name) {
            let fresh_name =
                fresh_name(&old_name, &body, &variables, &replacement_free, next_fresh);
            body = rename_bound(&body, &old_name, &fresh_name);
            for variable in variables.iter_mut().skip(index + 1) {
                if let Some(kind) = &variable.kind {
                    variable.kind = Some(rename_bound(kind, &old_name, &fresh_name));
                }
            }
            variables[index].name = fresh_name;
        }
        if let Some(kind) = &variables[index].kind {
            variables[index].kind = Some(substitute(kind, &local_mapping, next_fresh));
        }
        local_mapping.remove(&old_name);
    }
    TypeKind::Forall {
        variables,
        body: Box::new(substitute(&body, &local_mapping, next_fresh)),
    }
}

fn substitute_fields(
    fields: &[hir::TypeField],
    mapping: &HashMap<String, hir::Type>,
    next_fresh: &mut u32,
) -> Vec<hir::TypeField> {
    fields
        .iter()
        .map(|field| hir::TypeField {
            label: field.label.clone(),
            label_span: field.label_span,
            ty: substitute(&field.ty, mapping, next_fresh),
            span: field.span,
        })
        .collect()
}

fn rename_bound(ty: &hir::Type, old: &str, new: &str) -> hir::Type {
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
                    hir::TypeParameter {
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
                .as_ref()
                .map(|tail| Box::new(rename_bound(tail, old, new))),
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: rename_fields(fields, old, new),
            tail: tail
                .as_ref()
                .map(|tail| Box::new(rename_bound(tail, old, new))),
        },
        _ => return ty.clone(),
    };
    hir::Type {
        kind,
        span: ty.span,
    }
}

fn rename_fields(fields: &[hir::TypeField], old: &str, new: &str) -> Vec<hir::TypeField> {
    fields
        .iter()
        .map(|field| hir::TypeField {
            label: field.label.clone(),
            label_span: field.label_span,
            ty: rename_bound(&field.ty, old, new),
            span: field.span,
        })
        .collect()
}

fn free_variables(ty: &hir::Type) -> HashSet<String> {
    let mut free = HashSet::new();
    collect_free_variables(ty, &mut HashSet::new(), &mut free);
    free
}

fn collect_free_variables(ty: &hir::Type, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
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

fn flatten_spine(ty: &hir::Type) -> (&hir::Type, Vec<hir::Type>) {
    let mut head = ty;
    let mut arguments = Vec::new();
    while let TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref().clone());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}

fn fresh_name(
    base: &str,
    body: &hir::Type,
    variables: &[hir::TypeParameter],
    replacement_free: &HashSet<String>,
    next_fresh: &mut u32,
) -> String {
    loop {
        let candidate = format!("__psrs_role_{}_{}", base, *next_fresh);
        *next_fresh += 1;
        if !replacement_free.contains(&candidate)
            && !all_variable_names(body).contains(&candidate)
            && !variables.iter().any(|variable| variable.name == candidate)
        {
            return candidate;
        }
    }
}

fn all_variable_names(ty: &hir::Type) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_variable_names(ty, &mut names);
    names
}

fn collect_variable_names(ty: &hir::Type, names: &mut HashSet<String>) {
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
