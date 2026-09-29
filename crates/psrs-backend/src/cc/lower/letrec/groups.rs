use super::super::lambda::collect_captures;
use psrs_core::Binding;
use psrs_hir::LocalId;
use std::collections::{HashMap, HashSet};

/// Partitions local `let` bindings into strongly connected components in
/// dependency order, dependencies first. This mirrors how official PureScript
/// orders a local binding group before code generation: bindings that a value
/// references are lowered before it, and a cycle is lowered as one mutually
/// recursive closure group.
pub(super) fn binding_groups(bindings: &[Binding]) -> Vec<Vec<usize>> {
    let index_of = bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| (binding.binder.id, index))
        .collect::<HashMap<LocalId, usize>>();
    let edges = bindings
        .iter()
        .map(|binding| {
            let mut locals = Vec::new();
            collect_captures(&binding.value, &mut HashSet::new(), &mut locals);
            let mut dependencies = locals
                .into_iter()
                .filter_map(|local| index_of.get(&local).copied())
                .collect::<Vec<_>>();
            dependencies.sort_unstable();
            dependencies.dedup();
            dependencies
        })
        .collect::<Vec<_>>();
    Tarjan::new(&edges).run()
}

/// Whether a single binding references its own value, making it recursive.
pub(super) fn references_itself(binding: &Binding) -> bool {
    let mut locals = Vec::new();
    collect_captures(&binding.value, &mut HashSet::new(), &mut locals);
    locals.contains(&binding.binder.id)
}

struct Tarjan<'a> {
    edges: &'a [Vec<usize>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    components: Vec<Vec<usize>>,
}

impl<'a> Tarjan<'a> {
    fn new(edges: &'a [Vec<usize>]) -> Self {
        let count = edges.len();
        Self {
            edges,
            index: vec![None; count],
            low: vec![0; count],
            on_stack: vec![false; count],
            stack: Vec::new(),
            next: 0,
            components: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Vec<usize>> {
        for vertex in 0..self.edges.len() {
            if self.index[vertex].is_none() {
                self.visit(vertex);
            }
        }
        self.components
    }

    fn visit(&mut self, vertex: usize) {
        self.index[vertex] = Some(self.next);
        self.low[vertex] = self.next;
        self.next += 1;
        self.stack.push(vertex);
        self.on_stack[vertex] = true;
        let neighbors = self.edges[vertex].clone();
        for neighbor in neighbors {
            if self.index[neighbor].is_none() {
                self.visit(neighbor);
                self.low[vertex] = self.low[vertex].min(self.low[neighbor]);
            } else if self.on_stack[neighbor] {
                self.low[vertex] = self.low[vertex].min(self.index[neighbor].unwrap());
            }
        }
        if self.low[vertex] == self.index[vertex].unwrap() {
            let mut component = Vec::new();
            loop {
                let member = self.stack.pop().unwrap();
                self.on_stack[member] = false;
                component.push(member);
                if member == vertex {
                    break;
                }
            }
            self.components.push(component);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_core::{Binder, Expr, ExprKind, TypeId};
    use psrs_span::TextRange;

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(start, end)
    }

    fn local(id: u32, start: u32) -> Expr {
        Expr {
            kind: ExprKind::Local(LocalId(id)),
            ty: TypeId(0),
            span: range(start, start + 1),
        }
    }

    fn integer(start: u32) -> Expr {
        Expr {
            kind: ExprKind::Integer(1),
            ty: TypeId(0),
            span: range(start, start + 1),
        }
    }

    fn lambda(binder: u32, body: Expr, start: u32, end: u32) -> Expr {
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(binder),
                    name: "argument".into(),
                    ty: TypeId(0),
                    span: range(start, start + 1),
                },
                body: Box::new(body),
            },
            ty: TypeId(0),
            span: range(start, end),
        }
    }

    fn binding(id: u32, value: Expr, start: u32, end: u32) -> Binding {
        Binding {
            binder: Binder {
                id: LocalId(id),
                name: format!("binding{id}"),
                ty: TypeId(0),
                span: range(start, start + 1),
            },
            quantified: Vec::new(),
            value,
            span: range(start, end),
        }
    }

    #[test]
    fn orders_bindings_by_dependency_not_declaration() {
        // `first` references `second`, declared afterwards, so `second` must be
        // lowered before `first`.
        let first = binding(1, lambda(10, local(2, 5), 1, 6), 1, 7);
        let second = binding(2, lambda(11, integer(5), 3, 8), 3, 9);
        let bindings = vec![first, second];
        assert_eq!(binding_groups(&bindings), vec![vec![1], vec![0]]);
        assert!(!references_itself(&bindings[0]));
        assert!(!references_itself(&bindings[1]));
    }

    #[test]
    fn detects_a_self_recursive_binding() {
        let recursive = binding(3, lambda(12, local(3, 5), 1, 6), 1, 7);
        assert!(references_itself(&recursive));
    }
}
