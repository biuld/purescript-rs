//! Module-graph helpers shared by the multi-module program pipeline: import
//! dependencies, dependency order, and the instance declarations visible to a
//! module through its import closure.

/// The imported module indices of every module, in declaration order.
pub(super) fn module_dependencies(modules: &[psrs_hir::Module]) -> Vec<Vec<usize>> {
    modules
        .iter()
        .map(|module| {
            module
                .imports
                .iter()
                .map(|import| import.module.0 as usize)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Orders modules so every module follows the modules it imports.
pub(super) fn typecheck_order(dependencies: &[Vec<usize>]) -> Vec<usize> {
    let mut order = Vec::with_capacity(dependencies.len());
    let mut visited = vec![false; dependencies.len()];
    for index in 0..dependencies.len() {
        visit(index, dependencies, &mut visited, &mut order);
    }
    order
}

/// Gathers the instance declarations visible to the module at `index`: those
/// declared in every module it imports, directly or transitively. Local
/// instances are supplied separately by the module itself.
pub(super) fn imported_instance_declarations(
    dependencies: &[Vec<usize>],
    index: usize,
    instance_sets: &[Vec<psrs_hir::InstanceDeclaration>],
) -> Vec<psrs_hir::InstanceDeclaration> {
    let mut seen = vec![false; dependencies.len()];
    let mut stack = dependencies.get(index).cloned().unwrap_or_default();
    let mut instances = Vec::new();
    while let Some(dependency) = stack.pop() {
        if seen.get(dependency).copied().unwrap_or(true) {
            continue;
        }
        seen[dependency] = true;
        if let Some(set) = instance_sets.get(dependency) {
            instances.extend(set.iter().cloned());
        }
        stack.extend(dependencies.get(dependency).into_iter().flatten().copied());
    }
    instances
}

fn visit(index: usize, dependencies: &[Vec<usize>], visited: &mut [bool], order: &mut Vec<usize>) {
    if visited.get(index).copied().unwrap_or(true) {
        return;
    }
    visited[index] = true;
    for &dependency in dependencies.get(index).into_iter().flatten() {
        visit(dependency, dependencies, visited, order);
    }
    order.push(index);
}
