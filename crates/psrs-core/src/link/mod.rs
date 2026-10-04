use crate::{
    Binding, ConstructorInfo, Declaration, Expr, ExprKind, ExternalType, Module, PatternKind, Type,
    TypeId,
};
use psrs_hir::{ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;
use std::collections::HashSet;

mod shift;
mod variables;

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
    let mut newtype_ids = Vec::new();
    let mut opaque_ids = Vec::new();
    let mut callable_types: Vec<(psrs_hir::TypeId, u32)> = Vec::new();
    let mut constructors = Vec::new();
    let mut seen_constructors = HashSet::new();
    let mut declarations = Vec::new();
    let mut externals = Vec::new();
    let mut seen_externals = std::collections::HashSet::new();
    let mut external_types = Vec::new();
    let mut seen_external_types = HashSet::new();
    let mut type_names = Vec::new();
    let mut seen_type_names = HashSet::new();
    let mut type_variable_offset = 0u32;
    for module in modules {
        let offset = types.len() as u32;
        let variable_span = variables::variable_span(&module);
        let module_variable_offset = type_variable_offset;
        type_variable_offset = type_variable_offset.saturating_add(variable_span);
        for ty in &module.types {
            types.push(shift_type(ty, offset, module_variable_offset));
        }
        newtype_ids.extend(module.newtype_ids);
        opaque_ids.extend(module.opaque_ids);
        for callable in module.callable_types {
            if !callable_types.contains(&callable) {
                callable_types.push(callable);
            }
        }
        for constructor in module.constructors {
            if !seen_constructors.insert(constructor.symbol) {
                continue;
            }
            constructors.push(ConstructorInfo {
                field_types: constructor
                    .field_types
                    .into_iter()
                    .map(|field| shift_id(field, offset))
                    .collect(),
                parameters: constructor
                    .parameters
                    .into_iter()
                    .map(|variable| shift_variable(variable, module_variable_offset))
                    .collect(),
                ..constructor
            });
        }
        declarations.extend(
            module
                .declarations
                .into_iter()
                .map(|declaration| shift_declaration(declaration, offset, module_variable_offset)),
        );
        for external in module.externals {
            if seen_externals.insert(external.symbol) {
                externals.push(external);
            }
        }
        for external in module.external_types {
            if seen_external_types.insert(external.symbol) {
                external_types.push(ExternalType {
                    symbol: external.symbol,
                    source_module: external.source_module,
                    ty: shift_id(external.ty, offset),
                });
            }
        }
        for (id, name) in module.type_names {
            if seen_type_names.insert(id) {
                type_names.push((id, name));
            }
        }
    }
    Module {
        id: ModuleId(0),
        name,
        externals,
        external_types,
        types,
        newtype_ids,
        opaque_ids,
        callable_types,
        constructors,
        declarations,
        type_names,
        // Chosen by the caller once the program entry is known.
        entry: None,
        span,
    }
}

fn shift_id(id: TypeId, offset: u32) -> TypeId {
    TypeId(id.0 + offset)
}

fn shift_variable(variable: TypeVariableId, offset: u32) -> TypeVariableId {
    TypeVariableId(variable.0 + offset)
}

fn shift_type(ty: &Type, offset: u32, variable_offset: u32) -> Type {
    match ty {
        Type::Application(parameter, argument) => {
            Type::Application(shift_id(*parameter, offset), shift_id(*argument, offset))
        }
        Type::Variable(variable) => Type::Variable(shift_variable(*variable, variable_offset)),
        Type::ForAll { variables, body } => Type::ForAll {
            variables: variables
                .iter()
                .map(|variable| shift_variable(*variable, variable_offset))
                .collect(),
            body: shift_id(*body, offset),
        },
        Type::RowExtend { label, ty, tail } => Type::RowExtend {
            label: label.clone(),
            ty: shift_id(*ty, offset),
            tail: shift_id(*tail, offset),
        },
        Type::Closure { parameters, result } => Type::Closure {
            parameters: parameters
                .iter()
                .map(|parameter| shift_id(*parameter, offset))
                .collect(),
            result: shift_id(*result, offset),
        },
        other => other.clone(),
    }
}

fn shift_binding(binding: Binding, offset: u32, variable_offset: u32) -> Binding {
    Binding {
        binder: crate::Binder {
            id: binding.binder.id,
            name: binding.binder.name,
            ty: shift_id(binding.binder.ty, offset),
            span: binding.binder.span,
        },
        quantified: binding
            .quantified
            .into_iter()
            .map(|variable| shift_variable(variable, variable_offset))
            .collect(),
        value: shift_expr(binding.value, offset, variable_offset),
        span: binding.span,
    }
}

fn shift_declaration(declaration: Declaration, offset: u32, variable_offset: u32) -> Declaration {
    Declaration {
        symbol: declaration.symbol,
        name: declaration.name,
        name_span: declaration.name_span,
        quantified: declaration
            .quantified
            .into_iter()
            .map(|variable| shift_variable(variable, variable_offset))
            .collect(),
        ty: shift_id(declaration.ty, offset),
        value: shift_expr(declaration.value, offset, variable_offset),
        span: declaration.span,
    }
}

fn shift_expr(expression: Expr, offset: u32, variable_offset: u32) -> Expr {
    Expr {
        kind: shift::shift_kind(expression.kind, offset, variable_offset),
        ty: shift_id(expression.ty, offset),
        span: expression.span,
    }
}

fn shift_pattern(pattern: crate::Pattern, offset: u32) -> crate::Pattern {
    let kind = match pattern.kind {
        PatternKind::Wildcard => PatternKind::Wildcard,
        PatternKind::Literal { value } => PatternKind::Literal { value },
        PatternKind::Array { elements } => PatternKind::Array {
            elements: elements
                .into_iter()
                .map(|element| shift_pattern(element, offset))
                .collect(),
        },
        PatternKind::Named { id, pattern } => PatternKind::Named {
            id,
            pattern: Box::new(shift_pattern(*pattern, offset)),
        },
        PatternKind::Var { id, ty } => PatternKind::Var {
            id,
            ty: shift_id(ty, offset),
        },
        PatternKind::Constructor { symbol, arguments } => PatternKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| shift_pattern(argument, offset))
                .collect(),
        },
        PatternKind::Record { fields } => PatternKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, pattern)| (label, shift_pattern(pattern, offset)))
                .collect(),
        },
    };
    crate::Pattern {
        kind,
        ty: shift_id(pattern.ty, offset),
        span: pattern.span,
    }
}

/// Removes declarations not reachable from `root` through global and
/// constructor references. Used after linking so a program only carries the
/// library declarations it reaches.
pub fn prune_unreachable(module: &mut Module, root: SymbolId) {
    let mut reachable = HashSet::new();
    let mut used_types = HashSet::new();
    let mut visited_types = HashSet::new();
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
            collect_core_type_ids(declaration.ty, module, &mut used_types, &mut visited_types);
            collect_references(
                &declaration.value,
                &mut work,
                module,
                &mut used_types,
                &mut visited_types,
            );
        }
    }
    module
        .declarations
        .retain(|declaration| reachable.contains(&declaration.symbol));
    // A reachable constructor keeps every case of its type so the variant
    // layout stays complete. Unused library types drop out with their cases.
    used_types.extend(
        module
            .constructors
            .iter()
            .filter(|constructor| reachable.contains(&constructor.symbol))
            .map(|constructor| constructor.type_id),
    );
    // Reachable declaration and foreign-import signatures can mention library
    // types that the program never constructs or matches directly. Their
    // constructors are still needed to lay out those signatures (for example,
    // the nullary `Proxy` constructor in a class method type).
    for external in &module.external_types {
        collect_core_type_ids(external.ty, module, &mut used_types, &mut visited_types);
    }
    loop {
        let before = used_types.len();
        let field_types = module
            .constructors
            .iter()
            .filter(|constructor| used_types.contains(&constructor.type_id))
            .flat_map(|constructor| constructor.field_types.iter().copied())
            .collect::<Vec<_>>();
        for field_type in field_types {
            collect_core_type_ids(field_type, module, &mut used_types, &mut visited_types);
        }
        if used_types.len() == before {
            break;
        }
    }
    module
        .constructors
        .retain(|constructor| used_types.contains(&constructor.type_id));
}

fn collect_core_type_ids(
    id: TypeId,
    module: &Module,
    out: &mut HashSet<psrs_hir::TypeId>,
    visited: &mut HashSet<TypeId>,
) {
    if !visited.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Constructor(crate::TypeConstructor::User(type_id))) => {
            out.insert(*type_id);
        }
        Some(Type::Application(function, argument)) => {
            collect_core_type_ids(*function, module, out, visited);
            collect_core_type_ids(*argument, module, out, visited);
        }
        Some(Type::ForAll { body, .. }) => {
            collect_core_type_ids(*body, module, out, visited);
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            collect_core_type_ids(*ty, module, out, visited);
            collect_core_type_ids(*tail, module, out, visited);
        }
        Some(Type::Closure { parameters, result }) => {
            for parameter in parameters {
                collect_core_type_ids(*parameter, module, out, visited);
            }
            collect_core_type_ids(*result, module, out, visited);
        }
        _ => {}
    }
}

fn collect_references(
    expression: &Expr,
    out: &mut Vec<SymbolId>,
    module: &Module,
    used_types: &mut HashSet<psrs_hir::TypeId>,
    visited_types: &mut HashSet<TypeId>,
) {
    collect_core_type_ids(expression.ty, module, used_types, visited_types);
    match &expression.kind {
        ExprKind::Global(symbol) => out.push(*symbol),
        ExprKind::Constructor { symbol, arguments } => {
            out.push(*symbol);
            for argument in arguments {
                collect_references(argument, out, module, used_types, visited_types);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Unit
        | ExprKind::Trap => {}
        ExprKind::Array { elements } => {
            for element in elements {
                collect_references(element, out, module, used_types, visited_types);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_references(value, out, module, used_types, visited_types);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_references(record, out, module, used_types, visited_types);
            for (_, value) in fields {
                collect_references(value, out, module, used_types, visited_types);
            }
        }
        ExprKind::FieldAccess { record, .. } => {
            collect_references(record, out, module, used_types, visited_types)
        }
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => {
            collect_core_type_ids(*source_type, module, used_types, visited_types);
            collect_core_type_ids(*target_type, module, used_types, visited_types);
            collect_references(value, out, module, used_types, visited_types);
        }
        ExprKind::IntrinsicCall { arguments, .. } => {
            for argument in arguments {
                collect_references(argument, out, module, used_types, visited_types);
            }
        }
        ExprKind::Application(left, right) => {
            collect_references(left, out, module, used_types, visited_types);
            collect_references(right, out, module, used_types, visited_types);
        }
        ExprKind::Lambda { binder, body } => {
            collect_core_type_ids(binder.ty, module, used_types, visited_types);
            collect_references(body, out, module, used_types, visited_types);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_core_type_ids(binding.binder.ty, module, used_types, visited_types);
                collect_references(&binding.value, out, module, used_types, visited_types);
            }
            collect_references(body, out, module, used_types, visited_types);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_references(condition, out, module, used_types, visited_types);
            collect_references(then_branch, out, module, used_types, visited_types);
            collect_references(else_branch, out, module, used_types, visited_types);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_references(scrutinee, out, module, used_types, visited_types);
            for branch in branches {
                collect_pattern(&branch.pattern, out, module, used_types, visited_types);
                collect_references(&branch.value, out, module, used_types, visited_types);
            }
        }
    }
}

fn collect_pattern(
    pattern: &crate::Pattern,
    out: &mut Vec<SymbolId>,
    module: &Module,
    used_types: &mut HashSet<psrs_hir::TypeId>,
    visited_types: &mut HashSet<TypeId>,
) {
    collect_core_type_ids(pattern.ty, module, used_types, visited_types);
    match &pattern.kind {
        PatternKind::Constructor { symbol, arguments } => {
            out.push(*symbol);
            for argument in arguments {
                collect_pattern(argument, out, module, used_types, visited_types);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                collect_pattern(field, out, module, used_types, visited_types);
            }
        }
        PatternKind::Array { elements } => {
            for element in elements {
                collect_pattern(element, out, module, used_types, visited_types);
            }
        }
        PatternKind::Named { pattern, .. } => {
            collect_pattern(pattern, out, module, used_types, visited_types)
        }
        _ => {}
    }
}
