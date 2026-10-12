use psrs_hir::{self as hir, TypeId, TypeKind};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Synonym {
    pub(super) parameters: Vec<String>,
    body: hir::Type,
    cyclic: bool,
}

pub(crate) type SynonymMap = HashMap<TypeId, Synonym>;

pub(super) fn collect(modules: &[hir::Module]) -> SynonymMap {
    let mut definitions: SynonymMap = modules
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
                    cyclic: false,
                },
            ))
        })
        .collect();
    let edges = definitions
        .iter()
        .map(|(&id, item)| {
            let mut references = Vec::new();
            crate::kind::collect_type_ids(&item.body, &mut references);
            (
                id,
                references
                    .into_iter()
                    .map(|id| (id, item.body.span))
                    .collect(),
            )
        })
        .collect();
    for (id, _) in crate::kind::detect_cycle(definitions.keys().copied(), &edges) {
        if let Some(item) = definitions.get_mut(&id) {
            item.cyclic = true;
        }
    }
    definitions
}

pub(crate) fn expand(ty: &hir::Type, synonyms: &SynonymMap) -> Option<hir::Type> {
    expand_inner(ty, synonyms, &mut 65_536, &mut 0)
}

fn expand_inner(
    ty: &hir::Type,
    synonyms: &SynonymMap,
    fuel: &mut usize,
    next_fresh: &mut u32,
) -> Option<hir::Type> {
    let kind = match &ty.kind {
        TypeKind::Named(id)
            if synonyms
                .get(id)
                .is_some_and(|item| item.parameters.is_empty()) =>
        {
            return expand_synonym(*id, Vec::new(), synonyms, fuel, next_fresh);
        }
        TypeKind::Application(function, argument) => {
            let function = expand_inner(function, synonyms, fuel, next_fresh)?;
            let argument = expand_inner(argument, synonyms, fuel, next_fresh)?;
            let applied = hir::Type {
                kind: TypeKind::Application(Box::new(function), Box::new(argument)),
                span: ty.span,
            };
            let (head, arguments) = flatten_spine(&applied);
            if let TypeKind::Named(id) = head.kind
                && let Some(synonym) = synonyms.get(&id)
                && synonym.parameters.len() == arguments.len()
            {
                return expand_synonym(id, arguments, synonyms, fuel, next_fresh);
            }
            return Some(applied);
        }
        TypeKind::Function { parameter, result } => TypeKind::Function {
            parameter: Box::new(expand_inner(parameter, synonyms, fuel, next_fresh)?),
            result: Box::new(expand_inner(result, synonyms, fuel, next_fresh)?),
        },
        TypeKind::Forall { variables, body } => TypeKind::Forall {
            variables: variables
                .iter()
                .map(|variable| {
                    Some(hir::TypeParameter {
                        name: variable.name.clone(),
                        name_span: variable.name_span,
                        kind: match &variable.kind {
                            Some(kind) => Some(expand_inner(kind, synonyms, fuel, next_fresh)?),
                            None => None,
                        },
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            body: Box::new(expand_inner(body, synonyms, fuel, next_fresh)?),
        },
        TypeKind::Constrained { constraint, body } => TypeKind::Constrained {
            constraint: Box::new(expand_inner(constraint, synonyms, fuel, next_fresh)?),
            body: Box::new(expand_inner(body, synonyms, fuel, next_fresh)?),
        },
        TypeKind::Row { fields, tail } => TypeKind::Row {
            fields: expand_fields(fields, synonyms, fuel, next_fresh)?,
            tail: match tail {
                Some(tail) => Some(Box::new(expand_inner(tail, synonyms, fuel, next_fresh)?)),
                None => None,
            },
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: expand_fields(fields, synonyms, fuel, next_fresh)?,
            tail: match tail {
                Some(tail) => Some(Box::new(expand_inner(tail, synonyms, fuel, next_fresh)?)),
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
    fuel: &mut usize,
    next_fresh: &mut u32,
) -> Option<Vec<hir::TypeField>> {
    fields
        .iter()
        .map(|field| {
            Some(hir::TypeField {
                label: field.label.clone(),
                label_span: field.label_span,
                ty: expand_inner(&field.ty, synonyms, fuel, next_fresh)?,
                span: field.span,
            })
        })
        .collect()
}

fn expand_synonym(
    id: TypeId,
    arguments: Vec<hir::Type>,
    synonyms: &SynonymMap,
    fuel: &mut usize,
    next_fresh: &mut u32,
) -> Option<hir::Type> {
    let synonym = synonyms.get(&id)?;
    if synonym.parameters.len() != arguments.len() || synonym.cyclic || *fuel == 0 {
        return None;
    }
    *fuel -= 1;
    let mapping = synonym
        .parameters
        .iter()
        .cloned()
        .zip(arguments)
        .collect::<HashMap<_, _>>();
    let instantiated = hir::substitute_type_variables(&synonym.body, &mapping, next_fresh);
    let expanded = expand_inner(&instantiated, synonyms, fuel, next_fresh);
    expanded
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
