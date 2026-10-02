use super::*;

/// Checks hidden local type dependencies of inferred public values after their
/// schemes have been generalized. Stable HIR TypeIds cover aliases,
/// applications, function parameters, and record fields uniformly.
pub(super) fn check(
    module: &hir::Module,
    inferred: &[Option<InferredDeclaration>],
    errors: &mut Vec<TypeCheckError>,
) {
    let exported_symbols = match &module.exports {
        Some(exports) => exports
            .values
            .iter()
            .map(|value| value.symbol)
            .collect::<HashSet<_>>(),
        None => module
            .declarations
            .iter()
            .map(|declaration| declaration.symbol)
            .collect(),
    };
    let exported_types = match &module.exports {
        Some(exports) => exports
            .types
            .iter()
            .filter_map(|exported| match exported.reference {
                hir::TypeReference::Named(id) => Some(id),
                hir::TypeReference::Builtin(_) => None,
            })
            .collect::<HashSet<_>>(),
        None => module
            .types
            .iter()
            .map(|declaration| declaration.id)
            .collect(),
    };
    let source_declarations = module
        .declarations
        .iter()
        .map(|declaration| (declaration.symbol, declaration))
        .collect::<HashMap<_, _>>();
    let own_types = module
        .types
        .iter()
        .map(|declaration| (declaration.id, declaration))
        .collect::<HashMap<_, _>>();

    for declaration in inferred.iter().flatten() {
        let Some(source) = source_declarations.get(&declaration.symbol) else {
            continue;
        };
        if source.signature.is_some() || !exported_symbols.contains(&declaration.symbol) {
            continue;
        }

        let mut referenced = HashSet::new();
        collect_scheme_types(&declaration.scheme, &mut referenced);
        let mut missing = referenced
            .into_iter()
            .filter(|id| id.module == module.id && !exported_types.contains(id))
            .filter_map(|id| own_types.get(&id))
            .collect::<Vec<_>>();
        missing.sort_by(|left, right| left.name.cmp(&right.name));
        for hidden in missing {
            errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TransitiveExport,
                declaration.name_span,
                format!(
                    "the export of `{}` requires `{}` to be exported",
                    declaration.name, hidden.name
                ),
            ));
        }
    }
}

fn collect_scheme_types(scheme: &Scheme, out: &mut HashSet<hir::TypeId>) {
    collect_type_ids(&scheme.ty, out);
    for constraint in &scheme.constraints {
        out.insert(constraint.class_id);
        for argument in &constraint.arguments {
            collect_type_ids(argument, out);
        }
    }
}

fn collect_type_ids(ty: &InferType, out: &mut HashSet<hir::TypeId>) {
    match ty {
        InferType::Constructor(TypeConstructor::User(id)) => {
            out.insert(*id);
        }
        InferType::Application(function, argument) => {
            collect_type_ids(function, out);
            collect_type_ids(argument, out);
        }
        InferType::ForAll { body, .. } => collect_type_ids(body, out),
        InferType::Constrained { constraints, body } => {
            for constraint in constraints {
                out.insert(constraint.class_id);
                for argument in &constraint.arguments {
                    collect_type_ids(argument, out);
                }
            }
            collect_type_ids(body, out);
        }
        InferType::RowExtend { ty, tail, .. } => {
            collect_type_ids(ty, out);
            collect_type_ids(tail, out);
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => {}
    }
}
