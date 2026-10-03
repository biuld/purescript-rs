use psrs_hir::{
    self as hir, CaseBranch, CaseBranchCoverage, Expr, ExprKind, Guard, GuardedExpr, LocalBinder,
    LocalBinding, LocalId, Pattern, PatternKind,
};
use std::collections::{HashMap, HashSet};

pub(super) struct FreshLocals {
    next: u32,
}

impl FreshLocals {
    pub(super) fn after_module(module: &hir::Module) -> Self {
        let mut ids = HashSet::new();
        for declaration in &module.declarations {
            collect_expr_ids(&declaration.value, &mut ids);
        }
        for instance in &module.instances {
            for member in &instance.members {
                collect_expr_ids(&member.value, &mut ids);
            }
        }
        let next = ids
            .into_iter()
            .map(|id| id.0)
            .max()
            .map(|id| id.checked_add(1).expect("local ID space exhausted"))
            .unwrap_or(0);
        Self { next }
    }

    pub(super) fn alloc(&mut self) -> LocalId {
        let id = LocalId(self.next);
        self.next = self.next.checked_add(1).expect("local ID space exhausted");
        id
    }
}

pub(super) fn clone_branch(branch: &CaseBranch, fresh: &mut FreshLocals) -> CaseBranch {
    let mut binder_ids = HashSet::new();
    collect_pattern_ids(&branch.pattern, &mut binder_ids);
    collect_expr_ids(&branch.value, &mut binder_ids);
    let mut binder_ids = binder_ids.into_iter().collect::<Vec<_>>();
    binder_ids.sort_by_key(|id| id.0);
    let mapping = binder_ids
        .into_iter()
        .map(|id| (id, fresh.alloc()))
        .collect::<HashMap<_, _>>();
    let mut clone = branch.clone();
    clone.pattern = rename_pattern(clone.pattern, &mapping);
    clone.value = rename_expr(clone.value, &mapping);
    clone.coverage = CaseBranchCoverage::Generated;
    clone
}

pub(super) fn clone_expression(expression: &Expr, fresh: &mut FreshLocals) -> Expr {
    let mut binder_ids = HashSet::new();
    collect_expr_ids(expression, &mut binder_ids);
    let mut binder_ids = binder_ids.into_iter().collect::<Vec<_>>();
    binder_ids.sort_by_key(|id| id.0);
    let mapping = binder_ids
        .into_iter()
        .map(|id| (id, fresh.alloc()))
        .collect::<HashMap<_, _>>();
    rename_expr(expression.clone(), &mapping)
}

fn collect_expr_ids(expression: &Expr, ids: &mut HashSet<LocalId>) {
    match &expression.kind {
        ExprKind::Lambda { binder, body } => {
            ids.insert(binder.id);
            collect_expr_ids(body, ids);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                ids.insert(binding.binder.id);
                collect_expr_ids(&binding.value, ids);
            }
            collect_expr_ids(body, ids);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_ids(condition, ids);
            collect_expr_ids(then_branch, ids);
            collect_expr_ids(else_branch, ids);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_expr_ids(scrutinee, ids);
            for branch in branches {
                collect_pattern_ids(&branch.pattern, ids);
                collect_expr_ids(&branch.value, ids);
            }
        }
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                collect_guarded_ids(clause, ids);
            }
        }
        ExprKind::Array(elements) => {
            for element in elements {
                collect_expr_ids(element, ids);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                collect_expr_ids(value, ids);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            collect_expr_ids(expression, ids);
            for (_, value) in fields {
                collect_expr_ids(value, ids);
            }
        }
        ExprKind::FieldAccess { expression, .. }
        | ExprKind::Typed { expression, .. }
        | ExprKind::TypeApplication { expression, .. } => {
            collect_expr_ids(expression, ids);
        }
        ExprKind::Application(function, argument) => {
            collect_expr_ids(function, ids);
            collect_expr_ids(argument, ids);
        }
        ExprKind::Operator { left, right, .. } => {
            collect_expr_ids(left, ids);
            collect_expr_ids(right, ids);
        }
        ExprKind::Negate {
            function,
            expression,
            ..
        } => {
            collect_expr_ids(function, ids);
            collect_expr_ids(expression, ids);
        }
        ExprKind::OperatorChain { operands, .. } => {
            for operand in operands {
                collect_expr_ids(operand, ids);
            }
        }
        ExprKind::OperatorSection {
            operand, binder, ..
        } => {
            ids.insert(binder.id);
            collect_expr_ids(operand, ids);
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn collect_guarded_ids(clause: &GuardedExpr, ids: &mut HashSet<LocalId>) {
    for binding in &clause.where_bindings {
        ids.insert(binding.binder.id);
        collect_expr_ids(&binding.value, ids);
    }
    for guard in &clause.guards {
        match guard {
            Guard::Boolean(expression) => collect_expr_ids(expression, ids),
            Guard::Pattern { pattern, value } => {
                collect_expr_ids(value, ids);
                collect_pattern_ids(pattern, ids);
            }
            Guard::Let { bindings, .. } => {
                for binding in bindings {
                    ids.insert(binding.binder.id);
                    collect_expr_ids(&binding.value, ids);
                }
            }
        }
    }
    collect_expr_ids(&clause.value, ids);
}

fn collect_pattern_ids(pattern: &Pattern, ids: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Var(binder) => {
            ids.insert(binder.id);
        }
        PatternKind::Named { binder, pattern } => {
            ids.insert(binder.id);
            collect_pattern_ids(pattern, ids);
        }
        PatternKind::Array(elements) => {
            for element in elements {
                collect_pattern_ids(element, ids);
            }
        }
        PatternKind::Typed { pattern, .. } => collect_pattern_ids(pattern, ids),
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_ids(argument, ids);
            }
        }
        PatternKind::Record { fields, .. } => {
            for (_, field) in fields {
                collect_pattern_ids(field, ids);
            }
        }
        PatternKind::OperatorChain { operands, .. } => {
            for operand in operands {
                collect_pattern_ids(operand, ids);
            }
        }
        PatternKind::Wildcard
        | PatternKind::Boolean(_)
        | PatternKind::Integer(_)
        | PatternKind::Number(_)
        | PatternKind::String(_)
        | PatternKind::Char(_) => {}
    }
}

fn rename_expr(expression: Expr, mapping: &HashMap<LocalId, LocalId>) -> Expr {
    let span = expression.span;
    let kind = match expression.kind {
        ExprKind::Local(id) => ExprKind::Local(mapping.get(&id).copied().unwrap_or(id)),
        ExprKind::Array(elements) => ExprKind::Array(
            elements
                .into_iter()
                .map(|element| rename_expr(element, mapping))
                .collect(),
        ),
        ExprKind::Record(fields) => ExprKind::Record(
            fields
                .into_iter()
                .map(|(label, value)| (label, rename_expr(value, mapping)))
                .collect(),
        ),
        ExprKind::RecordUpdate { expression, fields } => ExprKind::RecordUpdate {
            expression: Box::new(rename_expr(*expression, mapping)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, rename_expr(value, mapping)))
                .collect(),
        },
        ExprKind::FieldAccess { expression, field } => ExprKind::FieldAccess {
            expression: Box::new(rename_expr(*expression, mapping)),
            field,
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(rename_expr(*function, mapping)),
            Box::new(rename_expr(*argument, mapping)),
        ),
        ExprKind::Typed { expression, ty } => ExprKind::Typed {
            expression: Box::new(rename_expr(*expression, mapping)),
            ty,
        },
        ExprKind::TypeApplication { expression, ty } => ExprKind::TypeApplication {
            expression: Box::new(rename_expr(*expression, mapping)),
            ty,
        },
        ExprKind::Operator {
            operator,
            operator_span,
            left,
            right,
        } => ExprKind::Operator {
            operator,
            operator_span,
            left: Box::new(rename_expr(*left, mapping)),
            right: Box::new(rename_expr(*right, mapping)),
        },
        ExprKind::Negate {
            function,
            minus_span,
            expression,
        } => ExprKind::Negate {
            function: Box::new(rename_expr(*function, mapping)),
            minus_span,
            expression: Box::new(rename_expr(*expression, mapping)),
        },
        ExprKind::OperatorChain {
            operands,
            operators,
        } => ExprKind::OperatorChain {
            operands: operands
                .into_iter()
                .map(|operand| rename_expr(operand, mapping))
                .collect(),
            operators,
        },
        ExprKind::OperatorSection {
            operator,
            operand,
            binder,
            side,
        } => ExprKind::OperatorSection {
            operator,
            operand: Box::new(rename_expr(*operand, mapping)),
            binder: rename_binder(binder, mapping),
            side,
        },
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder: rename_binder(binder, mapping),
            body: Box::new(rename_expr(*body, mapping)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| rename_binding(binding, mapping))
                .collect(),
            body: Box::new(rename_expr(*body, mapping)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(rename_expr(*condition, mapping)),
            then_branch: Box::new(rename_expr(*then_branch, mapping)),
            else_branch: Box::new(rename_expr(*else_branch, mapping)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(rename_expr(*scrutinee, mapping)),
            branches: branches
                .into_iter()
                .map(|branch| CaseBranch {
                    pattern: rename_pattern(branch.pattern, mapping),
                    value: rename_expr(branch.value, mapping),
                    ..branch
                })
                .collect(),
        },
        ExprKind::Guarded(clauses) => ExprKind::Guarded(
            clauses
                .into_iter()
                .map(|clause| rename_guarded(clause, mapping))
                .collect(),
        ),
        leaf @ (ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)) => leaf,
    };
    Expr { kind, span }
}

fn rename_guarded(clause: GuardedExpr, mapping: &HashMap<LocalId, LocalId>) -> GuardedExpr {
    GuardedExpr {
        guards: clause
            .guards
            .into_iter()
            .map(|guard| match guard {
                Guard::Boolean(expression) => Guard::Boolean(rename_expr(expression, mapping)),
                Guard::Pattern { pattern, value } => Guard::Pattern {
                    pattern: rename_pattern(pattern, mapping),
                    value: rename_expr(value, mapping),
                },
                Guard::Let { bindings, span } => Guard::Let {
                    bindings: bindings
                        .into_iter()
                        .map(|binding| rename_binding(binding, mapping))
                        .collect(),
                    span,
                },
            })
            .collect(),
        value: rename_expr(clause.value, mapping),
        where_bindings: clause
            .where_bindings
            .into_iter()
            .map(|binding| rename_binding(binding, mapping))
            .collect(),
        span: clause.span,
    }
}

fn rename_binding(binding: LocalBinding, mapping: &HashMap<LocalId, LocalId>) -> LocalBinding {
    LocalBinding {
        binder: rename_binder(binding.binder, mapping),
        value: rename_expr(binding.value, mapping),
        span: binding.span,
    }
}

fn rename_binder(binder: LocalBinder, mapping: &HashMap<LocalId, LocalId>) -> LocalBinder {
    LocalBinder {
        id: mapping.get(&binder.id).copied().unwrap_or(binder.id),
        ..binder
    }
}

fn rename_pattern(pattern: Pattern, mapping: &HashMap<LocalId, LocalId>) -> Pattern {
    let kind = match pattern.kind {
        PatternKind::Wildcard => PatternKind::Wildcard,
        PatternKind::Boolean(value) => PatternKind::Boolean(value),
        PatternKind::Integer(value) => PatternKind::Integer(value),
        PatternKind::Number(value) => PatternKind::Number(value),
        PatternKind::String(value) => PatternKind::String(value),
        PatternKind::Char(value) => PatternKind::Char(value),
        PatternKind::Array(elements) => PatternKind::Array(
            elements
                .into_iter()
                .map(|element| rename_pattern(element, mapping))
                .collect(),
        ),
        PatternKind::Var(binder) => PatternKind::Var(rename_binder(binder, mapping)),
        PatternKind::Named { binder, pattern } => PatternKind::Named {
            binder: rename_binder(binder, mapping),
            pattern: Box::new(rename_pattern(*pattern, mapping)),
        },
        PatternKind::Typed { pattern, ty } => PatternKind::Typed {
            pattern: Box::new(rename_pattern(*pattern, mapping)),
            ty,
        },
        PatternKind::Constructor {
            symbol,
            name_span,
            arguments,
        } => PatternKind::Constructor {
            symbol,
            name_span,
            arguments: arguments
                .into_iter()
                .map(|argument| rename_pattern(argument, mapping))
                .collect(),
        },
        PatternKind::Record { fields, mode } => PatternKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, field)| (label, rename_pattern(field, mapping)))
                .collect(),
            mode,
        },
        PatternKind::OperatorChain {
            operands,
            operators,
        } => PatternKind::OperatorChain {
            operands: operands
                .into_iter()
                .map(|operand| rename_pattern(operand, mapping))
                .collect(),
            operators,
        },
    };
    Pattern { kind, ..pattern }
}
