use crate::{
    Binding, CaseBranch, ConstructorInfo, Declaration, Expr, ExprKind, Module, PatternKind, Type,
    TypeId,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::HashSet;

/// Links lowered modules into one module: type IDs are renumbered into a single
/// type table, while declarations, constructors, and externals are concatenated.
/// Symbols already carry their module, so declaration and global references stay
/// unique across modules. Local IDs are per declaration and need no remapping.
pub fn link(modules: Vec<Module>) -> Module {
    let name = modules
        .first()
        .map(|module| module.name.clone())
        .unwrap_or_default();
    let span = modules
        .first()
        .map_or_else(|| TextRange::new(0, 0), |module| module.span);
    let mut types = Vec::new();
    let mut constructors = Vec::new();
    let mut declarations = Vec::new();
    let mut externals = Vec::new();
    let mut seen_externals = std::collections::HashSet::new();
    for module in modules {
        let offset = types.len() as u32;
        for ty in &module.types {
            types.push(shift_type(ty, offset));
        }
        constructors.extend(module.constructors.into_iter().map(|constructor| {
            ConstructorInfo {
                field_types: constructor
                    .field_types
                    .into_iter()
                    .map(|field| shift_id(field, offset))
                    .collect(),
                ..constructor
            }
        }));
        declarations.extend(
            module
                .declarations
                .into_iter()
                .map(|declaration| shift_declaration(declaration, offset)),
        );
        for external in module.externals {
            if seen_externals.insert(external.symbol) {
                externals.push(external);
            }
        }
    }
    Module {
        id: ModuleId(0),
        name,
        externals,
        types,
        constructors,
        declarations,
        // Chosen by the caller once the program entry is known.
        entry: None,
        span,
    }
}

fn shift_id(id: TypeId, offset: u32) -> TypeId {
    TypeId(id.0 + offset)
}

fn shift_type(ty: &Type, offset: u32) -> Type {
    match ty {
        Type::Application(parameter, argument) => {
            Type::Application(shift_id(*parameter, offset), shift_id(*argument, offset))
        }
        Type::Function { parameter, result } => Type::Function {
            parameter: shift_id(*parameter, offset),
            result: shift_id(*result, offset),
        },
        other => other.clone(),
    }
}

fn shift_binding(binding: Binding, offset: u32) -> Binding {
    Binding {
        binder: crate::Binder {
            id: binding.binder.id,
            name: binding.binder.name,
            ty: shift_id(binding.binder.ty, offset),
            span: binding.binder.span,
        },
        quantified: binding.quantified,
        value: shift_expr(binding.value, offset),
        span: binding.span,
    }
}

fn shift_declaration(declaration: Declaration, offset: u32) -> Declaration {
    Declaration {
        symbol: declaration.symbol,
        name: declaration.name,
        name_span: declaration.name_span,
        quantified: declaration.quantified,
        ty: shift_id(declaration.ty, offset),
        value: shift_expr(declaration.value, offset),
        span: declaration.span,
    }
}

fn shift_expr(expression: Expr, offset: u32) -> Expr {
    Expr {
        kind: shift_kind(expression.kind, offset),
        ty: shift_id(expression.ty, offset),
        span: expression.span,
    }
}

fn shift_kind(kind: ExprKind, offset: u32) -> ExprKind {
    match kind {
        ExprKind::Local(id) => ExprKind::Local(id),
        ExprKind::Global(symbol) => ExprKind::Global(symbol),
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| shift_expr(argument, offset))
                .collect(),
        },
        ExprKind::Integer(value) => ExprKind::Integer(value),
        ExprKind::Boolean(value) => ExprKind::Boolean(value),
        ExprKind::String(value) => ExprKind::String(value),
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op,
            left: Box::new(shift_expr(*left, offset)),
            right: Box::new(shift_expr(*right, offset)),
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(shift_expr(*function, offset)),
            Box::new(shift_expr(*argument, offset)),
        ),
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder: crate::Binder {
                id: binder.id,
                name: binder.name,
                ty: shift_id(binder.ty, offset),
                span: binder.span,
            },
            body: Box::new(shift_expr(*body, offset)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| shift_binding(binding, offset))
                .collect(),
            body: Box::new(shift_expr(*body, offset)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(shift_expr(*condition, offset)),
            then_branch: Box::new(shift_expr(*then_branch, offset)),
            else_branch: Box::new(shift_expr(*else_branch, offset)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(shift_expr(*scrutinee, offset)),
            branches: branches
                .into_iter()
                .map(|branch| CaseBranch {
                    pattern: branch.pattern,
                    value: shift_expr(branch.value, offset),
                    span: branch.span,
                })
                .collect(),
        },
    }
}

/// Removes declarations not reachable from `root` through global and
/// constructor references. Used after linking so a program only carries the
/// library declarations it reaches.
pub fn prune_unreachable(module: &mut Module, root: SymbolId) {
    let mut reachable = HashSet::new();
    let mut work = vec![root];
    while let Some(symbol) = work.pop() {
        if !reachable.insert(symbol) {
            continue;
        }
        if let Some(declaration) = module
            .declarations
            .iter()
            .find(|declaration| declaration.symbol == symbol)
        {
            collect_references(&declaration.value, &mut work);
        }
    }
    module
        .declarations
        .retain(|declaration| reachable.contains(&declaration.symbol));
}

fn collect_references(expression: &Expr, out: &mut Vec<SymbolId>) {
    match &expression.kind {
        ExprKind::Global(symbol) => out.push(*symbol),
        ExprKind::Constructor { symbol, arguments } => {
            out.push(*symbol);
            for argument in arguments {
                collect_references(argument, out);
            }
        }
        ExprKind::Local(_) | ExprKind::Integer(_) | ExprKind::Boolean(_) | ExprKind::String(_) => {}
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_references(left, out);
            collect_references(right, out);
        }
        ExprKind::Lambda { body, .. } => collect_references(body, out),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_references(&binding.value, out);
            }
            collect_references(body, out);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_references(condition, out);
            collect_references(then_branch, out);
            collect_references(else_branch, out);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_references(scrutinee, out);
            for branch in branches {
                collect_pattern(&branch.pattern, out);
                collect_references(&branch.value, out);
            }
        }
    }
}

fn collect_pattern(pattern: &crate::Pattern, out: &mut Vec<SymbolId>) {
    if let PatternKind::Constructor { symbol, arguments } = &pattern.kind {
        out.push(*symbol);
        for argument in arguments {
            collect_pattern(argument, out);
        }
    }
}
