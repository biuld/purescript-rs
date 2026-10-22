//! Strongly connected groups of one module's type declarations.
//!
//! Inside a group, inferred parameter kinds stay shared. That is what lets
//! `type B a = F A` solve `A`'s parameter, and what the occurs check reads on
//! `data Branch m = Branch (m Branch)`. A use in another group instantiates
//! those kinds, so a constructor field cannot solve a different declaration.
//! Official kind checking walks one binding group at a time
//! (`TypeChecker/Kinds.hs`).

use super::*;
use std::collections::HashSet;

/// Group number of each local declaration, and the declarations in an order
/// that checks a group after the groups it references.
pub(super) fn order(module: &hir::Module) -> (HashMap<TypeId, u32>, Vec<TypeId>) {
    let source: Vec<TypeId> = module
        .types
        .iter()
        .map(|declaration| declaration.id)
        .collect();
    let local: HashSet<TypeId> = source.iter().copied().collect();
    let mut position = HashMap::new();
    for (index, id) in source.iter().enumerate() {
        position.insert(*id, index);
    }
    let mut edges = HashMap::new();
    for declaration in &module.types {
        let mut referenced = declaration_references(declaration);
        referenced.retain(|id| local.contains(id));
        referenced.sort_by_key(|id| position.get(id).copied().unwrap_or(usize::MAX));
        referenced.dedup();
        edges.insert(declaration.id, referenced);
    }
    let mut components = tarjan(&source, &edges);
    for component in &mut components {
        component.sort_by_key(|id| position.get(id).copied().unwrap_or(usize::MAX));
    }
    let mut groups = HashMap::new();
    let mut ordered = Vec::new();
    for (index, component) in components.iter().enumerate() {
        let group = u32::try_from(index).unwrap_or(u32::MAX);
        for id in component {
            groups.insert(*id, group);
            ordered.push(*id);
        }
    }
    (groups, ordered)
}

impl Checker<'_> {
    /// Whether `id` is in the binding group of the declaration being checked.
    /// A declaration this module does not define is outside the group.
    pub(super) fn same_kind_group(&self, id: TypeId) -> bool {
        let Some(current) = self.current_type else {
            return false;
        };
        match (self.kind_group.get(&current), self.kind_group.get(&id)) {
            (Some(group), Some(referenced)) => group == referenced,
            _ => false,
        }
    }
}

fn declaration_references(declaration: &hir::TypeDeclaration) -> Vec<TypeId> {
    let mut ids = Vec::new();
    for parameter in &declaration.parameters {
        if let Some(kind) = &parameter.kind {
            collect_type_ids(kind, &mut ids);
        }
    }
    for constructor in &declaration.constructors {
        for field in &constructor.fields {
            collect_type_ids(field, &mut ids);
        }
    }
    if let Some(body) = &declaration.body {
        collect_type_ids(body, &mut ids);
    }
    for superclass in &declaration.superclasses {
        collect_type_ids(superclass, &mut ids);
    }
    for member in &declaration.members {
        if let Some(signature) = &member.signature {
            collect_type_ids(signature, &mut ids);
        }
    }
    if let Some(kind) = &declaration.declared_kind {
        collect_type_ids(kind, &mut ids);
    }
    ids
}

/// Tarjan's algorithm. Components come out with dependencies first.
fn tarjan(nodes: &[TypeId], edges: &HashMap<TypeId, Vec<TypeId>>) -> Vec<Vec<TypeId>> {
    let mut search = Tarjan {
        edges,
        index: HashMap::new(),
        low: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        next: 0,
        components: Vec::new(),
    };
    for node in nodes {
        if !search.index.contains_key(node) {
            search.visit(*node);
        }
    }
    search.components
}

struct Tarjan<'a> {
    edges: &'a HashMap<TypeId, Vec<TypeId>>,
    index: HashMap<TypeId, usize>,
    low: HashMap<TypeId, usize>,
    stack: Vec<TypeId>,
    on_stack: HashSet<TypeId>,
    next: usize,
    components: Vec<Vec<TypeId>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, node: TypeId) {
        let index = self.next;
        self.next += 1;
        self.index.insert(node, index);
        self.low.insert(node, index);
        self.stack.push(node);
        self.on_stack.insert(node);
        if let Some(targets) = self.edges.get(&node).cloned() {
            for target in targets {
                if !self.index.contains_key(&target) {
                    self.visit(target);
                    let low = self.low[&node].min(self.low[&target]);
                    self.low.insert(node, low);
                } else if self.on_stack.contains(&target) {
                    let low = self.low[&node].min(self.index[&target]);
                    self.low.insert(node, low);
                }
            }
        }
        if self.low[&node] == self.index[&node] {
            let mut component = Vec::new();
            while let Some(member) = self.stack.pop() {
                self.on_stack.remove(&member);
                let root = member == node;
                component.push(member);
                if root {
                    break;
                }
            }
            self.components.push(component);
        }
    }
}
