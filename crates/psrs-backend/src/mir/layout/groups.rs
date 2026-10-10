//! Dependency-minimal recursion groups and their complete type-ID remapping.
use super::*;

pub(super) fn partition(
    definitions: Vec<DefinedType>,
) -> Result<(Vec<RecGroup>, Vec<DefinedTypeId>), LayoutError> {
    let edges = definitions.iter().map(dependencies).collect::<Vec<_>>();
    if edges.iter().flatten().any(|id| *id >= definitions.len()) {
        return Err(LayoutError::UnknownRepresentation);
    }
    let components = components(&edges);
    let mut mapping = vec![DefinedTypeId(0); definitions.len()];
    let mut next = 0;
    for component in &components {
        for original in component {
            mapping[*original] = DefinedTypeId(next);
            next += 1;
        }
    }
    let groups = components
        .into_iter()
        .map(|component| {
            RecGroup(
                component
                    .into_iter()
                    .map(|original| {
                        let mut definition = definitions[original].clone();
                        remap(&mut definition, &mapping);
                        definition
                    })
                    .collect(),
            )
        })
        .collect();
    Ok((groups, mapping))
}

fn dependencies(definition: &DefinedType) -> Vec<usize> {
    let mut result = definition
        .supertype
        .into_iter()
        .map(|id| id.0 as usize)
        .collect::<Vec<_>>();
    let mut reference = |heap| {
        if let HeapType::Index(id) = heap {
            result.push(id.0 as usize);
        }
    };
    match &definition.composite {
        CompositeType::Func {
            parameters,
            results,
        } => {
            for value in parameters.iter().chain(results) {
                if let ValueType::Ref(ty) = value {
                    reference(ty.heap);
                }
            }
        }
        CompositeType::Struct(fields) => {
            for field in fields {
                if let StorageType::Ref(ty) = field.storage {
                    reference(ty.heap);
                }
            }
        }
        CompositeType::Array(field) => {
            if let StorageType::Ref(ty) = field.storage {
                reference(ty.heap);
            }
        }
    }
    result.sort_unstable();
    result.dedup();
    result
}

/// Iterative Tarjan traversal avoids Rust stack growth for large type graphs.
/// Edges point from a definition to its dependencies, so completed components
/// already have the order required by Wasm: dependencies precede dependents.
fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut indices = vec![None; edges.len()];
    let mut low = vec![0; edges.len()];
    let mut active = vec![false; edges.len()];
    let mut stack = Vec::new();
    let mut frames = Vec::new();
    let mut result = Vec::new();
    let mut next = 0;
    for root in 0..edges.len() {
        if indices[root].is_some() {
            continue;
        }
        indices[root] = Some(next);
        low[root] = next;
        next += 1;
        active[root] = true;
        stack.push(root);
        frames.push((root, 0));
        while let Some((node, edge)) = frames.last_mut() {
            let node = *node;
            if let Some(child) = edges[node].get(*edge).copied() {
                *edge += 1;
                if let Some(index) = indices[child] {
                    if active[child] {
                        low[node] = low[node].min(index);
                    }
                } else {
                    indices[child] = Some(next);
                    low[child] = next;
                    next += 1;
                    active[child] = true;
                    stack.push(child);
                    frames.push((child, 0));
                }
                continue;
            }
            frames.pop();
            if low[node] == indices[node].unwrap() {
                let mut component = Vec::new();
                loop {
                    let member = stack.pop().unwrap();
                    active[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.sort_unstable();
                result.push(component);
            }
            if let Some((parent, _)) = frames.last() {
                low[*parent] = low[*parent].min(low[node]);
            }
        }
    }
    result
}

fn remap(definition: &mut DefinedType, mapping: &[DefinedTypeId]) {
    let reference = |ty: &mut RefType| {
        if let HeapType::Index(id) = &mut ty.heap {
            *id = mapping[id.0 as usize];
        }
    };
    if let Some(id) = &mut definition.supertype {
        *id = mapping[id.0 as usize];
    }
    match &mut definition.composite {
        CompositeType::Func {
            parameters,
            results,
        } => {
            for value in parameters.iter_mut().chain(results) {
                if let ValueType::Ref(ty) = value {
                    reference(ty);
                }
            }
        }
        CompositeType::Struct(fields) => {
            for field in fields {
                if let StorageType::Ref(ty) = &mut field.storage {
                    reference(ty);
                }
            }
        }
        CompositeType::Array(field) => {
            if let StorageType::Ref(ty) = &mut field.storage {
                reference(ty);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(references: &[u32]) -> DefinedType {
        DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(
                references
                    .iter()
                    .map(|id| FieldType {
                        storage: StorageType::Ref(RefType {
                            nullable: true,
                            heap: HeapType::Index(DefinedTypeId(*id)),
                        }),
                        mutable: false,
                    })
                    .collect(),
            ),
        }
    }

    #[test]
    fn cycles_share_a_group_and_acyclic_dependencies_precede_users() {
        let (groups, mapping) = partition(vec![
            structure(&[1, 3]),
            structure(&[2]),
            structure(&[1]),
            structure(&[]),
            structure(&[]),
        ])
        .unwrap();
        assert_eq!(
            groups.iter().map(|g| g.0.len()).collect::<Vec<_>>(),
            [2, 1, 1, 1]
        );
        assert_eq!(
            mapping,
            [
                DefinedTypeId(3),
                DefinedTypeId(0),
                DefinedTypeId(1),
                DefinedTypeId(2),
                DefinedTypeId(4)
            ]
        );
        let CompositeType::Struct(fields) = &groups[2].0[0].composite else {
            panic!("struct expected");
        };
        assert_eq!(
            fields[0].storage,
            StorageType::Ref(RefType {
                nullable: true,
                heap: HeapType::Index(mapping[1])
            })
        );
        assert_eq!(
            fields[1].storage,
            StorageType::Ref(RefType {
                nullable: true,
                heap: HeapType::Index(mapping[3])
            })
        );
    }

    #[test]
    fn a_large_acyclic_type_chain_uses_no_recursive_rust_traversal() {
        let mut definitions = (1..10_000).map(|id| structure(&[id])).collect::<Vec<_>>();
        definitions.push(structure(&[]));
        let (groups, mapping) = partition(definitions).unwrap();
        assert_eq!(groups.len(), 10_000);
        assert_eq!(mapping[0], DefinedTypeId(9_999));
        assert_eq!(mapping[9_999], DefinedTypeId(0));
    }

    #[test]
    fn dangling_type_edges_are_rejected_before_remapping() {
        assert!(partition(vec![structure(&[1])]).is_err());
    }
}
