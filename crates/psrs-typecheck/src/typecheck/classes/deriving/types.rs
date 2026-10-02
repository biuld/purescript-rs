use super::super::super::*;

impl Checker {
    pub(super) fn normalize_deriving_type(&self, ty: &hir::Type) -> hir::Type {
        normalize_type(ty, &self.synonyms, &mut HashSet::new())
    }
}

/// Whether a type mentions a type wildcard anywhere inside it.
///
/// A wildcard is a fresh unification variable in a value signature, but an
/// instance head is a pattern the solver matches against, so a wildcard there
/// has nothing to solve against. `purs` rejects it in
/// `TypeChecker.checkTypeClassInstance` with `InvalidInstanceHead`; see
/// `failing/TypeWildcards3.purs`. A wildcard in an instance *context* is a
/// different matter and stays legal, as `passing/WildcardInInstance.purs` needs.
pub(crate) fn contains_wildcard(ty: &hir::Type) -> bool {
    match &ty.kind {
        hir::TypeKind::Wildcard => true,
        hir::TypeKind::Application(function, argument) => {
            contains_wildcard(function) || contains_wildcard(argument)
        }
        hir::TypeKind::OperatorChain { operands, .. } => operands.iter().any(contains_wildcard),
        hir::TypeKind::Function {
            parameter: input,
            result,
        } => contains_wildcard(input) || contains_wildcard(result),
        hir::TypeKind::Record { fields, tail } | hir::TypeKind::Row { fields, tail } => {
            fields.iter().any(|field| contains_wildcard(&field.ty))
                || tail.as_deref().is_some_and(contains_wildcard)
        }
        hir::TypeKind::Forall { body, .. } => contains_wildcard(body),
        hir::TypeKind::Constrained { constraint, body } => {
            contains_wildcard(constraint) || contains_wildcard(body)
        }
        hir::TypeKind::Constructor(_)
        | hir::TypeKind::Named(_)
        | hir::TypeKind::Opaque(_)
        | hir::TypeKind::Variable(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => false,
    }
}

pub(super) fn contains_parameter(ty: &hir::Type, parameter: &str) -> bool {
    match &ty.kind {
        hir::TypeKind::Variable(name) => name == parameter,
        hir::TypeKind::Application(function, argument) => {
            contains_parameter(function, parameter) || contains_parameter(argument, parameter)
        }
        hir::TypeKind::OperatorChain { operands, .. } => operands
            .iter()
            .any(|operand| contains_parameter(operand, parameter)),
        hir::TypeKind::Function {
            parameter: input,
            result,
        } => contains_parameter(input, parameter) || contains_parameter(result, parameter),
        hir::TypeKind::Record { fields, tail } | hir::TypeKind::Row { fields, tail } => {
            fields
                .iter()
                .any(|field| contains_parameter(&field.ty, parameter))
                || tail
                    .as_deref()
                    .is_some_and(|tail| contains_parameter(tail, parameter))
        }
        hir::TypeKind::Forall { variables, body } => {
            !variables.iter().any(|variable| variable.name == parameter)
                && contains_parameter(body, parameter)
        }
        hir::TypeKind::Constrained { constraint, body } => {
            contains_parameter(constraint, parameter) || contains_parameter(body, parameter)
        }
        hir::TypeKind::Wildcard
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Named(_)
        | hir::TypeKind::Opaque(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => false,
    }
}

pub(super) fn flatten_type_application(ty: &hir::Type) -> (&hir::Type, Vec<&hir::Type>) {
    let mut head = ty;
    let mut arguments = Vec::new();
    while let hir::TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}

fn normalize_type(
    ty: &hir::Type,
    synonyms: &HashMap<hir::TypeId, Synonym>,
    expanding: &mut HashSet<hir::TypeId>,
) -> hir::Type {
    match &ty.kind {
        hir::TypeKind::Application(_, _) => {
            let (head, arguments) = flatten_type_application(ty);
            let normalized_arguments = arguments
                .iter()
                .map(|argument| normalize_type(argument, synonyms, expanding))
                .collect::<Vec<_>>();
            if let hir::TypeKind::Named(id) = &head.kind
                && let Some(synonym) = synonyms.get(id)
                && normalized_arguments.len() >= synonym.parameters.len()
                && expanding.insert(*id)
            {
                let substitutions = synonym
                    .parameters
                    .iter()
                    .cloned()
                    .zip(
                        normalized_arguments
                            .iter()
                            .take(synonym.parameters.len())
                            .cloned(),
                    )
                    .collect::<HashMap<_, _>>();
                let mut fresh_name_supply = 0;
                let mut expanded = hir::substitute_type_variables(
                    &synonym.body,
                    &substitutions,
                    &mut fresh_name_supply,
                );
                expanded = normalize_type(&expanded, synonyms, expanding);
                for argument in normalized_arguments.iter().skip(synonym.parameters.len()) {
                    expanded = hir::Type {
                        kind: hir::TypeKind::Application(
                            Box::new(expanded),
                            Box::new(argument.clone()),
                        ),
                        span: ty.span,
                    };
                }
                expanding.remove(id);
                expanded
            } else {
                let mut normalized = normalize_type(head, synonyms, expanding);
                for argument in normalized_arguments {
                    normalized = hir::Type {
                        kind: hir::TypeKind::Application(Box::new(normalized), Box::new(argument)),
                        span: ty.span,
                    };
                }
                normalized
            }
        }
        hir::TypeKind::Named(id) if synonyms.contains_key(id) => {
            let synonym = &synonyms[id];
            if synonym.parameters.is_empty() && expanding.insert(*id) {
                let expanded = normalize_type(&synonym.body, synonyms, expanding);
                expanding.remove(id);
                expanded
            } else {
                ty.clone()
            }
        }
        hir::TypeKind::Function { parameter, result } => hir::Type {
            kind: hir::TypeKind::Function {
                parameter: Box::new(normalize_type(parameter, synonyms, expanding)),
                result: Box::new(normalize_type(result, synonyms, expanding)),
            },
            span: ty.span,
        },
        hir::TypeKind::Forall { variables, body } => hir::Type {
            kind: hir::TypeKind::Forall {
                variables: variables.clone(),
                body: Box::new(normalize_type(body, synonyms, expanding)),
            },
            span: ty.span,
        },
        hir::TypeKind::Constrained { constraint, body } => hir::Type {
            kind: hir::TypeKind::Constrained {
                constraint: Box::new(normalize_type(constraint, synonyms, expanding)),
                body: Box::new(normalize_type(body, synonyms, expanding)),
            },
            span: ty.span,
        },
        hir::TypeKind::Row { fields, tail } => hir::Type {
            kind: hir::TypeKind::Row {
                fields: fields
                    .iter()
                    .map(|field| hir::TypeField {
                        ty: normalize_type(&field.ty, synonyms, expanding),
                        ..field.clone()
                    })
                    .collect(),
                tail: tail
                    .as_deref()
                    .map(|tail| Box::new(normalize_type(tail, synonyms, expanding))),
            },
            span: ty.span,
        },
        hir::TypeKind::Record { fields, tail } => hir::Type {
            kind: hir::TypeKind::Record {
                fields: fields
                    .iter()
                    .map(|field| hir::TypeField {
                        ty: normalize_type(&field.ty, synonyms, expanding),
                        ..field.clone()
                    })
                    .collect(),
                tail: tail
                    .as_deref()
                    .map(|tail| Box::new(normalize_type(tail, synonyms, expanding))),
            },
            span: ty.span,
        },
        _ => ty.clone(),
    }
}
