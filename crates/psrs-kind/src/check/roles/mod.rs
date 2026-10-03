use super::*;
use psrs_hir::{BuiltinType, ModuleId};
use std::collections::HashSet;

mod synonyms;

/// Infers representation roles for every data, newtype, and foreign-data type in a
/// resolved program. Explicit annotations constrain the published roles and
/// are checked against the fixed point.
///
/// Each diagnostic is attributed to the module that declares the type, which is
/// the module whose `type role` annotation and constructor fields it comes
/// from.
///
/// Compiler-provided declarations enter through the same registry the kind pass
/// builds its schemes from, so a `Prim` member is role-bearing exactly as a
/// source declaration is and carries its official role signature into the
/// checked environment.
pub(super) fn infer_roles_fixed_point(
    modules: &[hir::Module],
) -> (HashMap<TypeId, Vec<Role>>, Vec<KindDiagnostic>) {
    let primitives = psrs_hir::primitive_type_declarations();
    let declarations = modules
        .iter()
        .flat_map(|module| {
            module
                .types
                .iter()
                .filter(move |declaration| role_bearing(declaration.kind))
                .map(move |declaration| (module.id, declaration))
        })
        .chain(
            primitives
                .iter()
                .filter(|(_, declaration)| role_bearing(declaration.kind))
                .map(|(_, declaration)| (ModuleId::INTRINSICS, declaration)),
        )
        .collect::<Vec<_>>();
    let synonyms = synonyms::collect(modules);
    let mut roles = declarations
        .iter()
        .map(|(_, declaration)| {
            let arity = role_arity(declaration);
            let initial = match &declaration.declared_roles {
                Some(annotation) => annotation
                    .roles
                    .iter()
                    .map(|(role, _)| *role)
                    .collect::<Vec<_>>(),
                // A foreign type has no constructor representation to inspect.
                // Treat its parameters as nominal unless its declaration
                // explicitly supplies a trusted role signature.
                None if declaration.kind == TypeDeclarationKind::Foreign => {
                    vec![Role::Nominal; arity]
                }
                None => vec![Role::Phantom; arity],
            };
            (declaration.id, initial)
        })
        .collect::<HashMap<_, _>>();

    loop {
        let mut changed = false;
        for (_, declaration) in &declarations {
            if declaration.kind == TypeDeclarationKind::Foreign {
                continue;
            }
            let inferred = infer_declaration_roles(declaration, &roles, &synonyms);
            let target = roles.entry(declaration.id).or_default();
            if target.len() < inferred.len() {
                target.resize(inferred.len(), Role::Phantom);
            }
            for (index, inferred_role) in inferred.into_iter().enumerate() {
                let declared = declaration
                    .declared_roles
                    .as_ref()
                    .and_then(|annotation| annotation.roles.get(index))
                    .map(|(role, _)| *role)
                    .unwrap_or(Role::Phantom);
                let next = target[index].min(inferred_role).min(declared);
                if next != target[index] {
                    target[index] = next;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    let mut diagnostics = Vec::new();
    for (module_id, declaration) in declarations {
        let Some(annotation) = &declaration.declared_roles else {
            continue;
        };
        let expected = role_arity(declaration);
        let actual = annotation.roles.len();
        if actual != expected {
            diagnostics.push(KindDiagnostic::new(
                module_id,
                "RoleDeclarationArityMismatch",
                annotation.span,
                format!(
                    "the role declaration for `{}` has {actual} roles; expected {expected}",
                    declaration.name
                ),
            ));
            continue;
        }
        if !matches!(
            declaration.kind,
            TypeDeclarationKind::Data | TypeDeclarationKind::Newtype | TypeDeclarationKind::Foreign
        ) {
            diagnostics.push(KindDiagnostic::new(
                module_id,
                "UnsupportedRoleDeclaration",
                annotation.span,
                "role declarations are supported only for data, newtype, and foreign data types",
            ));
            continue;
        }
        if declaration.kind == TypeDeclarationKind::Foreign {
            continue;
        }
        let inferred = roles.get(&declaration.id).cloned().unwrap_or_default();
        for (index, (declared, span)) in annotation.roles.iter().enumerate() {
            if inferred
                .get(index)
                .is_some_and(|inferred| inferred < declared)
            {
                diagnostics.push(KindDiagnostic::new(
                    module_id,
                    "RoleMismatch",
                    *span,
                    format!(
                        "the declared role for parameter {} of `{}` is more permissive than its inferred role",
                        index + 1,
                        declaration.name
                    ),
                ));
            }
        }
    }
    (roles, diagnostics)
}

fn role_bearing(kind: TypeDeclarationKind) -> bool {
    matches!(
        kind,
        TypeDeclarationKind::Data | TypeDeclarationKind::Newtype | TypeDeclarationKind::Foreign
    )
}

fn role_arity(declaration: &hir::TypeDeclaration) -> usize {
    if declaration.kind != TypeDeclarationKind::Foreign {
        return declaration.parameters.len();
    }
    let mut kind = declaration.declared_kind.as_ref();
    while let Some(ty) = kind {
        match &ty.kind {
            TypeKind::Forall { body, .. } => kind = Some(body),
            TypeKind::Function { result, .. } => {
                let mut arity = 1;
                let mut result = result.as_ref();
                while let TypeKind::Function { result: next, .. } = &result.kind {
                    arity += 1;
                    result = next;
                }
                return arity;
            }
            _ => return 0,
        }
    }
    0
}

fn infer_declaration_roles(
    declaration: &hir::TypeDeclaration,
    roles: &HashMap<TypeId, Vec<Role>>,
    synonyms: &synonyms::SynonymMap,
) -> Vec<Role> {
    let mut occurrences = HashMap::new();
    for constructor in &declaration.constructors {
        for field in &constructor.fields {
            match synonyms::expand(field, synonyms) {
                Some(expanded) => walk(&expanded, &HashSet::new(), roles, &mut occurrences),
                None => mark_free_nominal(field, &HashSet::new(), &mut occurrences),
            }
        }
    }
    declaration
        .parameters
        .iter()
        .map(|parameter| {
            occurrences
                .get(&parameter.name)
                .copied()
                .unwrap_or(Role::Phantom)
        })
        .collect()
}

fn walk(
    ty: &hir::Type,
    bound: &HashSet<String>,
    roles: &HashMap<TypeId, Vec<Role>>,
    occurrences: &mut HashMap<String, Role>,
) {
    match &ty.kind {
        TypeKind::Variable(name) if !bound.contains(name) => {
            record(occurrences, name, Role::Representational);
        }
        TypeKind::Variable(_)
        | TypeKind::Wildcard
        | TypeKind::Constructor(_)
        | TypeKind::Named(_)
        | TypeKind::Opaque(_)
        | TypeKind::Integer(_)
        | TypeKind::String(_) => {}
        TypeKind::Application(..) => {
            let (head, arguments) = flatten_spine(ty);
            if let Some(constructor_roles) = role_of_head(head, roles) {
                for (index, argument) in arguments.iter().enumerate() {
                    match constructor_roles.get(index).copied() {
                        Some(Role::Nominal) => mark_free_nominal(argument, bound, occurrences),
                        Some(Role::Representational) => walk(argument, bound, roles, occurrences),
                        Some(Role::Phantom) => {}
                        None => mark_free_nominal(argument, bound, occurrences),
                    }
                }
            } else {
                walk(head, bound, roles, occurrences);
                for argument in arguments {
                    mark_free_nominal(argument, bound, occurrences);
                }
            }
        }
        TypeKind::OperatorChain { operands, .. } => {
            for operand in operands {
                mark_free_nominal(operand, bound, occurrences);
            }
        }
        TypeKind::Function { parameter, result } => {
            walk(parameter, bound, roles, occurrences);
            walk(result, bound, roles, occurrences);
        }
        TypeKind::Forall { variables, body } => {
            let mut nested = bound.clone();
            nested.extend(variables.iter().map(|variable| variable.name.clone()));
            walk(body, &nested, roles, occurrences);
        }
        TypeKind::Constrained { constraint, body } => {
            mark_free_nominal(constraint, bound, occurrences);
            walk(body, bound, roles, occurrences);
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            for field in fields {
                walk(&field.ty, bound, roles, occurrences);
            }
            if let Some(tail) = tail {
                walk(tail, bound, roles, occurrences);
            }
        }
    }
}

fn role_of_head(head: &hir::Type, roles: &HashMap<TypeId, Vec<Role>>) -> Option<Vec<Role>> {
    match head.kind {
        TypeKind::Named(id) | TypeKind::Opaque(id) => roles.get(&id).cloned(),
        TypeKind::Constructor(BuiltinType::Array) => Some(vec![Role::Representational]),
        TypeKind::Constructor(BuiltinType::Record) => Some(vec![Role::Representational]),
        TypeKind::Constructor(BuiltinType::Row) => Some(vec![Role::Phantom]),
        TypeKind::Constructor(BuiltinType::Function) => {
            Some(vec![Role::Representational, Role::Representational])
        }
        TypeKind::Constructor(_) => Some(Vec::new()),
        _ => None,
    }
}

fn mark_free_nominal(
    ty: &hir::Type,
    bound: &HashSet<String>,
    occurrences: &mut HashMap<String, Role>,
) {
    match &ty.kind {
        TypeKind::Variable(name) if !bound.contains(name) => {
            record(occurrences, name, Role::Nominal);
        }
        TypeKind::Application(function, argument) => {
            mark_free_nominal(function, bound, occurrences);
            mark_free_nominal(argument, bound, occurrences);
        }
        TypeKind::Function { parameter, result } => {
            mark_free_nominal(parameter, bound, occurrences);
            mark_free_nominal(result, bound, occurrences);
        }
        TypeKind::Forall { variables, body } => {
            let mut nested = bound.clone();
            nested.extend(variables.iter().map(|variable| variable.name.clone()));
            mark_free_nominal(body, &nested, occurrences);
        }
        TypeKind::Constrained { constraint, body } => {
            mark_free_nominal(constraint, bound, occurrences);
            mark_free_nominal(body, bound, occurrences);
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            for field in fields {
                mark_free_nominal(&field.ty, bound, occurrences);
            }
            if let Some(tail) = tail {
                mark_free_nominal(tail, bound, occurrences);
            }
        }
        _ => {}
    }
}

fn record(occurrences: &mut HashMap<String, Role>, name: &str, role: Role) {
    occurrences
        .entry(name.to_owned())
        .and_modify(|current| *current = (*current).min(role))
        .or_insert(role);
}

fn flatten_spine(ty: &hir::Type) -> (&hir::Type, Vec<&hir::Type>) {
    let mut head = ty;
    let mut arguments = Vec::new();
    while let TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}
