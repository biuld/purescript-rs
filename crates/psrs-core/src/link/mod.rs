use crate::{
    Binding, ConstructorInfo, Declaration, Expr, ExprKind, Module, PatternKind, Type, TypeId,
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
    // A reachable constructor keeps every case of its type so the variant
    // layout stays complete. Unused library types drop out with their cases.
    let mut used_types = module
        .constructors
        .iter()
        .filter(|constructor| reachable.contains(&constructor.symbol))
        .map(|constructor| constructor.type_id)
        .collect::<HashSet<_>>();
    // A foreign import's declared signature can name a library type the program
    // never constructs or matches, such as a newtype resource wrapper. Its
    // constructors are still needed to resolve and lay out the binding, so keep
    // every type the external signatures mention.
    for external in &module.externals {
        if let Some(signature) = &external.signature {
            collect_type_ids(signature, &mut used_types);
        }
    }
    module
        .constructors
        .retain(|constructor| used_types.contains(&constructor.type_id));
}

fn collect_type_ids(ty: &psrs_hir::Type, out: &mut HashSet<psrs_hir::TypeId>) {
    match &ty.kind {
        psrs_hir::TypeKind::Named(id) | psrs_hir::TypeKind::Opaque(id) => {
            out.insert(*id);
        }
        psrs_hir::TypeKind::Application(function, argument) => {
            collect_type_ids(function, out);
            collect_type_ids(argument, out);
        }
        psrs_hir::TypeKind::Function { parameter, result } => {
            collect_type_ids(parameter, out);
            collect_type_ids(result, out);
        }
        psrs_hir::TypeKind::Forall { body, .. } | psrs_hir::TypeKind::Constrained { body, .. } => {
            collect_type_ids(body, out)
        }
        psrs_hir::TypeKind::Row { fields, tail } | psrs_hir::TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_type_ids(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_type_ids(tail, out);
            }
        }
        _ => {}
    }
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
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        ExprKind::Array { elements } => {
            for element in elements {
                collect_references(element, out);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_references(value, out);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_references(record, out);
            for (_, value) in fields {
                collect_references(value, out);
            }
        }
        ExprKind::FieldAccess { record, .. } => collect_references(record, out),
        ExprKind::RepresentationCast { value, .. } => collect_references(value, out),
        ExprKind::ArrayLength(value) => collect_references(value, out),
        ExprKind::StringToBytes(value) | ExprKind::BytesToString(value) => {
            collect_references(value, out)
        }
        ExprKind::ArrayIndex { array, index } => {
            collect_references(array, out);
            collect_references(index, out);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_references(array, out);
            collect_references(index, out);
            collect_references(value, out);
        }
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_references(left, out);
            collect_references(right, out);
        }
        ExprKind::UnaryPrimitive { value, .. } => collect_references(value, out),
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
    match &pattern.kind {
        PatternKind::Constructor { symbol, arguments } => {
            out.push(*symbol);
            for argument in arguments {
                collect_pattern(argument, out);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                collect_pattern(field, out);
            }
        }
        _ => {}
    }
}
