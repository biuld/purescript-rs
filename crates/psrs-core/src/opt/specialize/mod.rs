mod calls;
mod types;

use super::{Budget, util::count_nodes};
use crate::{Declaration, Module};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::{HashMap, HashSet};
use types::{TypeKey, instantiate_declaration, match_instantiation};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    declaration: SymbolId,
    arguments: Vec<TypeKey>,
}

pub(super) struct State {
    max_declarations: usize,
    max_nodes: usize,
    declarations: usize,
    nodes: usize,
    next_serial: Option<usize>,
    cache: HashMap<Key, SymbolId>,
    generated: HashSet<SymbolId>,
    used_symbols: HashSet<SymbolId>,
    next_symbol: HashMap<ModuleId, Option<u32>>,
}

impl State {
    pub(super) fn new(module: &Module, budget: Budget) -> Self {
        let used_symbols = module
            .declarations
            .iter()
            .map(|declaration| declaration.symbol)
            .chain(module.externals.iter().map(|external| external.symbol))
            .chain(
                module
                    .constructors
                    .iter()
                    .map(|constructor| constructor.symbol),
            )
            .collect();
        Self {
            max_declarations: budget.max_specializations,
            max_nodes: budget.max_specialized_nodes,
            declarations: 0,
            nodes: 0,
            next_serial: Some(0),
            cache: HashMap::new(),
            generated: HashSet::new(),
            used_symbols,
            next_symbol: HashMap::new(),
        }
    }

    pub(super) fn generated_symbols(&self) -> &HashSet<SymbolId> {
        &self.generated
    }

    fn target(
        &mut self,
        module: &mut Module,
        declaration: &Declaration,
        call_type: crate::TypeId,
        pending: &mut Vec<Declaration>,
    ) -> Option<SymbolId> {
        let (replacements, arguments) = match_instantiation(module, declaration, call_type)?;
        let key = Key {
            declaration: declaration.symbol,
            arguments,
        };
        if let Some(symbol) = self.cache.get(&key) {
            return Some(*symbol);
        }
        let node_count = count_nodes(&declaration.value);
        if self.declarations >= self.max_declarations
            || node_count > self.max_nodes.saturating_sub(self.nodes)
        {
            return None;
        }
        let serial = self.next_serial?;
        let next_serial = serial.checked_add(1)?;
        let symbol = self.fresh_symbol(declaration.symbol.module)?;
        let specialized =
            instantiate_declaration(module, declaration, &replacements, symbol, serial)?;
        self.next_serial = Some(next_serial);
        self.declarations += 1;
        self.nodes += node_count;
        self.cache.insert(key, symbol);
        self.generated.insert(symbol);
        pending.push(specialized);
        Some(symbol)
    }

    fn fresh_symbol(&mut self, module: ModuleId) -> Option<SymbolId> {
        let next = self.next_symbol.entry(module).or_insert(Some(0));
        while let Some(index) = *next {
            *next = index.checked_add(1);
            let symbol = SymbolId::new(module, index);
            if self.used_symbols.insert(symbol) {
                return Some(symbol);
            }
        }
        None
    }
}

pub(super) fn run(mut module: Module, state: &mut State) -> Module {
    if state.max_declarations == 0 || state.nodes >= state.max_nodes {
        return module;
    }
    let mut declarations_by_module = HashMap::<ModuleId, HashMap<SymbolId, Declaration>>::new();
    for declaration in module.declarations.iter().cloned() {
        declarations_by_module
            .entry(declaration.symbol.module)
            .or_default()
            .insert(declaration.symbol, declaration);
    }
    let mut pending = Vec::new();
    for index in 0..module.declarations.len() {
        let value = module.declarations[index].value.clone();
        let caller_module = module.declarations[index].symbol.module;
        let Some(declarations) = declarations_by_module.get(&caller_module) else {
            continue;
        };
        module.declarations[index].value =
            calls::rewrite(value, &mut module, declarations, state, &mut pending);
    }
    module.declarations.extend(pending);
    module
}

pub(super) fn retain_live(mut module: Module, generated: &HashSet<SymbolId>) -> Module {
    if generated.is_empty() {
        return module;
    }
    let declarations = module
        .declarations
        .iter()
        .map(|declaration| (declaration.symbol, declaration))
        .collect::<HashMap<_, _>>();
    let mut work = Vec::new();
    for declaration in &module.declarations {
        if !generated.contains(&declaration.symbol) {
            collect_references(&declaration.value, &mut work);
        }
    }
    let mut reachable = HashSet::new();
    while let Some(symbol) = work.pop() {
        if !generated.contains(&symbol) || !reachable.insert(symbol) {
            continue;
        }
        if let Some(declaration) = declarations.get(&symbol) {
            collect_references(&declaration.value, &mut work);
        }
    }
    module.declarations.retain(|declaration| {
        !generated.contains(&declaration.symbol) || reachable.contains(&declaration.symbol)
    });
    module
}

fn collect_references(expression: &crate::Expr, out: &mut Vec<SymbolId>) {
    match &expression.kind {
        crate::ExprKind::Global(symbol) => out.push(*symbol),
        crate::ExprKind::Constructor { arguments, .. }
        | crate::ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                collect_references(argument, out);
            }
        }
        crate::ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_references(value, out);
            }
        }
        crate::ExprKind::RecordUpdate { record, fields } => {
            collect_references(record, out);
            for (_, value) in fields {
                collect_references(value, out);
            }
        }
        crate::ExprKind::FieldAccess { record, .. } | crate::ExprKind::ArrayLength(record) => {
            collect_references(record, out)
        }
        crate::ExprKind::UnaryPrimitive { value, .. } => collect_references(value, out),
        crate::ExprKind::ArrayIndex { array, index } => {
            collect_references(array, out);
            collect_references(index, out);
        }
        crate::ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_references(array, out);
            collect_references(index, out);
            collect_references(value, out);
        }
        crate::ExprKind::Primitive { left, right, .. }
        | crate::ExprKind::Application(left, right) => {
            collect_references(left, out);
            collect_references(right, out);
        }
        crate::ExprKind::Lambda { body, .. } => collect_references(body, out),
        crate::ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_references(&binding.value, out);
            }
            collect_references(body, out);
        }
        crate::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_references(condition, out);
            collect_references(then_branch, out);
            collect_references(else_branch, out);
        }
        crate::ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_references(scrutinee, out);
            for branch in branches {
                collect_references(&branch.value, out);
            }
        }
        crate::ExprKind::Local(_)
        | crate::ExprKind::Integer(_)
        | crate::ExprKind::Number(_)
        | crate::ExprKind::Boolean(_)
        | crate::ExprKind::String(_)
        | crate::ExprKind::Char(_) => {}
    }
}
