//! Lower finite closed uses of row-polymorphic local lambdas through checked
//! instantiation. Residual open uses retain their source scheme and remain an
//! explicit unsupported layout at the representation boundary.
use crate::locals::{FreshLocals, clone_with_fresh_locals};
use crate::{
    Binding, Expr, ExprKind, LocalId, Module, Pattern, PatternKind, Type, TypeId, VerifyError,
};
use std::collections::{HashMap, HashSet};

/// Materializes closed row instantiations of nonrecursive local lambdas.
/// This preparation has no optimization budget: every eligible closed use is
/// lowered. It changes only lambda creation, preserving evaluation and capture
/// of surrounding computations. Core checks the input and transformed output.
pub fn instantiate_local_rows(
    mut module: Module,
    source: Option<&Module>,
) -> Result<Module, Vec<VerifyError>> {
    module.verify_with_source(source.unwrap_or(&module))?;
    for index in 0..module.declarations.len() {
        let mut expression = module.declarations[index].value.clone();
        let mut fresh = FreshLocals::for_declaration(&expression);
        rewrite(
            &mut expression,
            &mut module,
            &mut fresh,
            &mut HashMap::new(),
        );
        module.declarations[index].value = expression;
    }
    module.verify_with_source(source.unwrap_or(&module))?;
    Ok(module)
}

fn rewrite(
    expr: &mut Expr,
    module: &mut Module,
    fresh: &mut FreshLocals,
    scope: &mut HashMap<LocalId, Binding>,
) {
    if let ExprKind::Local(id) = expr.kind {
        if let Some(binding) = scope.get(&id).cloned()
            && let Some(value) = instantiate(module, &binding, expr.ty, fresh)
        {
            *expr = value;
            rewrite(expr, module, fresh, scope);
        }
        return;
    }
    match &mut expr.kind {
        ExprKind::Lambda { binder, body } => {
            let previous = scope.remove(&binder.id);
            rewrite(body, module, fresh, scope);
            if let Some(binding) = previous {
                scope.insert(binder.id, binding);
            }
        }
        ExprKind::Let { bindings, body } => {
            let dependencies = bindings
                .iter()
                .map(|binding| {
                    (
                        binding.binder.id,
                        bindings
                            .iter()
                            .filter(|other| uses(&binding.value, other.binder.id))
                            .map(|other| other.binder.id)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<HashMap<_, _>>();
            let mut previous = Vec::new();
            for binding in bindings.iter() {
                previous.push((binding.binder.id, scope.remove(&binding.binder.id)));
                // Restrict this preparation to nonrecursive lambdas. Keeping a
                // recursive source binding is an explicit layout obligation.
                let recursive =
                    dependencies
                        .get(&binding.binder.id)
                        .is_some_and(|dependencies_from_binding| {
                            dependencies_from_binding.iter().any(|id| {
                                reaches(*id, binding.binder.id, &dependencies, &mut HashSet::new())
                            })
                        });
                if !recursive
                    && !quantifiers(module, binding).is_empty()
                    && matches!(binding.value.kind, ExprKind::Lambda { .. })
                {
                    scope.insert(binding.binder.id, binding.clone());
                }
            }
            for binding in bindings.iter_mut() {
                rewrite(&mut binding.value, module, fresh, scope);
            }
            rewrite(body, module, fresh, scope);
            let removable = bindings
                .iter()
                .filter(|binding| {
                    scope.contains_key(&binding.binder.id)
                        && !uses(body, binding.binder.id)
                        && !bindings
                            .iter()
                            .any(|other| uses(&other.value, binding.binder.id))
                })
                .map(|binding| binding.binder.id)
                .collect::<HashSet<_>>();
            bindings.retain(|binding| !removable.contains(&binding.binder.id));
            for (id, binding) in previous {
                scope.remove(&id);
                if let Some(binding) = binding {
                    scope.insert(id, binding);
                }
            }
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            rewrite(scrutinee, module, fresh, scope);
            for branch in branches {
                let mut ids = Vec::new();
                pattern_ids(&branch.pattern, &mut ids);
                let previous = ids
                    .into_iter()
                    .map(|id| (id, scope.remove(&id)))
                    .collect::<Vec<_>>();
                rewrite(&mut branch.value, module, fresh, scope);
                for (id, binding) in previous {
                    if let Some(binding) = binding {
                        scope.insert(id, binding);
                    }
                }
            }
        }
        _ => {
            for child in children(expr) {
                rewrite(child, module, fresh, scope);
            }
        }
    }
}

fn instantiate(
    module: &mut Module,
    binding: &Binding,
    use_type: TypeId,
    fresh: &mut FreshLocals,
) -> Option<Expr> {
    let rows = {
        let quantified = quantifiers(module, binding);
        let proof = module.checked_instantiation(binding.binder.ty, &quantified, use_type)?;
        quantified
            .iter()
            .map(|variable| {
                let row = proof.row(*variable)?;
                if row.tail.is_some() {
                    return None;
                }
                Some((*variable, row.fields))
            })
            .collect::<Option<Vec<_>>>()?
    };
    let mut replacements = HashMap::new();
    let empty = module
        .types
        .iter()
        .position(|ty| matches!(ty, Type::RowEmpty))?;
    for (variable, fields) in rows {
        let mut tail = TypeId(empty as u32);
        for (label, ty) in fields.into_iter().rev() {
            let node = Type::RowExtend { label, ty, tail };
            tail = if let Some(index) = module.types.iter().position(|ty| *ty == node) {
                TypeId(index as u32)
            } else {
                let id = TypeId(u32::try_from(module.types.len()).ok()?);
                module.types.push(node);
                id
            };
        }
        replacements.insert(variable, tail);
    }
    let value = super::substitution::instantiate_expression(module, &binding.value, &replacements)?;
    clone_with_fresh_locals(&value, fresh)
}

fn quantifiers(module: &Module, binding: &Binding) -> Vec<psrs_hir::TypeVariableId> {
    let mut variables = binding.quantified.clone();
    let mut ty = binding.binder.ty;
    while let Some(Type::ForAll {
        variables: bound,
        body,
    }) = module.types.get(ty.0 as usize)
    {
        variables.extend(bound);
        ty = *body;
    }
    variables
}

fn children(expr: &mut Expr) -> Vec<&mut Expr> {
    match &mut expr.kind {
        ExprKind::Constructor { arguments, .. }
        | ExprKind::IntrinsicCall { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter_mut().collect(),
        ExprKind::Record { fields } => fields.iter_mut().map(|(_, value)| value).collect(),
        ExprKind::RecordUpdate { record, fields } => std::iter::once(record.as_mut())
            .chain(fields.iter_mut().map(|(_, value)| value))
            .collect(),
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => vec![record],
        ExprKind::Application(function, argument) => vec![function, argument],
        ExprKind::Lambda { body, .. } => vec![body],
        ExprKind::Let { bindings, body } => bindings
            .iter_mut()
            .map(|binding| &mut binding.value)
            .chain(std::iter::once(body.as_mut()))
            .collect(),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => vec![condition, then_branch, else_branch],
        ExprKind::Case {
            scrutinee,
            branches,
        } => std::iter::once(scrutinee.as_mut())
            .chain(branches.iter_mut().map(|branch| &mut branch.value))
            .collect(),
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Unit
        | ExprKind::StateToken
        | ExprKind::Trap => Vec::new(),
    }
}

fn uses(expr: &Expr, id: LocalId) -> bool {
    // The traversal is scope aware, including malformed/reused local identities
    // accepted as lexical shadowing by Core's verifier.
    match &expr.kind {
        ExprKind::Local(found) => *found == id,
        ExprKind::Lambda { binder, .. } if binder.id == id => false,
        ExprKind::Let { bindings, .. }
            if bindings.iter().any(|binding| binding.binder.id == id) =>
        {
            false
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            uses(scrutinee, id)
                || branches.iter().any(|branch| {
                    let mut ids = Vec::new();
                    pattern_ids(&branch.pattern, &mut ids);
                    !ids.contains(&id) && uses(&branch.value, id)
                })
        }
        _ => children_ref(expr).into_iter().any(|child| uses(child, id)),
    }
}

fn reaches(
    current: LocalId,
    target: LocalId,
    dependencies: &HashMap<LocalId, Vec<LocalId>>,
    seen: &mut HashSet<LocalId>,
) -> bool {
    current == target
        || (seen.insert(current)
            && dependencies.get(&current).is_some_and(|next| {
                next.iter()
                    .any(|id| reaches(*id, target, dependencies, seen))
            }))
}

fn children_ref(expr: &Expr) -> Vec<&Expr> {
    match &expr.kind {
        ExprKind::Constructor { arguments, .. }
        | ExprKind::IntrinsicCall { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter().collect(),
        ExprKind::Record { fields } => fields.iter().map(|(_, value)| value).collect(),
        ExprKind::RecordUpdate { record, fields } => std::iter::once(record.as_ref())
            .chain(fields.iter().map(|(_, value)| value))
            .collect(),
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => vec![record],
        ExprKind::Application(function, argument) => vec![function, argument],
        ExprKind::Lambda { body, .. } => vec![body],
        ExprKind::Let { bindings, body } => bindings
            .iter()
            .map(|binding| &binding.value)
            .chain(std::iter::once(body.as_ref()))
            .collect(),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => vec![condition, then_branch, else_branch],
        ExprKind::Case {
            scrutinee,
            branches,
        } => std::iter::once(scrutinee.as_ref())
            .chain(branches.iter().map(|branch| &branch.value))
            .collect(),
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Unit
        | ExprKind::StateToken
        | ExprKind::Trap => Vec::new(),
    }
}

fn pattern_ids(pattern: &Pattern, ids: &mut Vec<LocalId>) {
    match &pattern.kind {
        PatternKind::Var { id, .. } => ids.push(*id),
        PatternKind::Named { id, pattern } => {
            ids.push(*id);
            pattern_ids(pattern, ids);
        }
        PatternKind::Array { elements }
        | PatternKind::Constructor {
            arguments: elements,
            ..
        } => {
            for pattern in elements {
                pattern_ids(pattern, ids);
            }
        }
        PatternKind::Record { fields } => {
            for (_, pattern) in fields {
                pattern_ids(pattern, ids);
            }
        }
        PatternKind::Wildcard | PatternKind::Literal { .. } => {}
    }
}
