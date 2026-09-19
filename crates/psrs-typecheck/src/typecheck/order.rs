use super::*;

/// Orders declarations into strongly connected components, dependencies first,
/// so each component can be generalized before the components that use it.
pub(super) fn declaration_order(module: &hir::Module) -> Vec<Vec<usize>> {
    let mut index_of = HashMap::new();
    for (index, declaration) in module.declarations.iter().enumerate() {
        index_of.insert(declaration.symbol, index);
    }
    let edges = module
        .declarations
        .iter()
        .map(|declaration| {
            let mut used = Vec::new();
            collect_globals(&declaration.value, &mut used);
            let mut edges = used
                .into_iter()
                .filter_map(|symbol| index_of.get(&symbol).copied())
                .collect::<Vec<_>>();
            edges.sort_unstable();
            edges.dedup();
            edges
        })
        .collect::<Vec<_>>();
    Tarjan::new(&edges).run()
}

fn collect_globals(expression: &hir::Expr, out: &mut Vec<SymbolId>) {
    match &expression.kind {
        hir::ExprKind::Local(_)
        | hir::ExprKind::Integer(_)
        | hir::ExprKind::String(_)
        | hir::ExprKind::Char(_) => {}
        hir::ExprKind::Global(symbol) => out.push(*symbol),
        hir::ExprKind::Operator {
            operator,
            left,
            right,
            ..
        } => {
            out.push(*operator);
            collect_globals(left, out);
            collect_globals(right, out);
        }
        hir::ExprKind::Application(function, argument) => {
            collect_globals(function, out);
            collect_globals(argument, out);
        }
        hir::ExprKind::Lambda { body, .. } => collect_globals(body, out),
        hir::ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_globals(&binding.value, out);
            }
            collect_globals(body, out);
        }
        hir::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_globals(condition, out);
            collect_globals(then_branch, out);
            collect_globals(else_branch, out);
        }
        hir::ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_globals(scrutinee, out);
            for branch in branches {
                collect_globals(&branch.value, out);
            }
        }
    }
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
