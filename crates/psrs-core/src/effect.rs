//! Representation lowering for the library effect type.
//!
//! `Effect τ` stays an abstract application through type checking and Typed
//! Core. This pass is the only one that matches that type's identity. It
//! replaces each application with a closure whose parameter list is the
//! runtime token, and it supplies bodies for `pure`, `bind`, and `runEffect`.

use crate::{
    Binder, Declaration, Expr, ExprKind, Module, Type, TypeConstructor, TypeId, closure_parts,
};
use psrs_hir::{ExternalKind, LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};
use psrs_span::TextRange;

const EFFECT_INTERFACE: &str = "psrs:effect";

/// Symbols whose abstract `psrs:effect` imports became ordinary declarations.
#[derive(Clone, Debug, Default)]
pub struct EffectLowering {
    pub synthesized: Vec<SymbolId>,
}

/// Lowers library effects in a linked Core module.
///
/// Programs that do not contain the opaque `Prelude.Effect` type are unchanged.
pub fn lower_effects(module: &mut Module) -> EffectLowering {
    let Some(effect) = library_effect(module) else {
        return EffectLowering::default();
    };
    let token = intern(module, Type::Constructor(TypeConstructor::Int));
    rewrite_effect_applications(module, effect, token);
    synthesize_operations(module, token)
}

fn library_effect(module: &Module) -> Option<HirTypeId> {
    module.type_names.iter().find_map(|(id, name)| {
        (*name == "Prelude.Effect" && module.opaque_ids.contains(id)).then_some(*id)
    })
}

fn rewrite_effect_applications(module: &mut Module, effect: HirTypeId, token: TypeId) {
    let targets = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| {
            let Type::Application(function, argument) = ty else {
                return None;
            };
            is_effect_constructor(module, *function, effect).then_some((index, *argument))
        })
        .collect::<Vec<_>>();
    for (index, result) in targets {
        module.types[index] = Type::Closure {
            parameters: vec![token],
            result,
        };
    }
}

fn is_effect_constructor(module: &Module, id: TypeId, effect: HirTypeId) -> bool {
    matches!(
        module.types.get(id.0 as usize),
        Some(Type::Constructor(TypeConstructor::User(id))) if *id == effect
    )
}

fn synthesize_operations(module: &mut Module, token: TypeId) -> EffectLowering {
    let operations = module
        .externals
        .iter()
        .enumerate()
        .filter_map(|(index, external)| {
            let ExternalKind::Wit {
                interface,
                function,
            } = &external.kind
            else {
                return None;
            };
            (*interface == EFFECT_INTERFACE).then_some((
                index,
                external.symbol,
                external.name.clone(),
                function.clone(),
                external_span(external),
            ))
        })
        .collect::<Vec<_>>();
    if operations.is_empty() {
        return EffectLowering::default();
    }
    let mut locals = LocalSupply::new(module);
    let mut variables = VariableSupply::new(module);
    let mut synthesized = Vec::new();
    let mut remove = Vec::new();
    for (index, symbol, name, function, span) in operations {
        let declaration = match function.as_str() {
            "pure" => Some(pure_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
                &mut variables,
            )),
            "bind" => Some(bind_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
                &mut variables,
            )),
            "run" => Some(run_declaration(
                module,
                symbol,
                &name,
                span,
                token,
                &mut locals,
                &mut variables,
            )),
            _ => None,
        };
        let Some(declaration) = declaration else {
            continue;
        };
        module.declarations.push(declaration);
        synthesized.push(symbol);
        remove.push(index);
    }
    remove.sort_unstable();
    for index in remove.into_iter().rev() {
        module.externals.remove(index);
    }
    EffectLowering { synthesized }
}

fn pure_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
    variables: &mut VariableSupply,
) -> Declaration {
    let parameter = variables.fresh();
    let parameter_ty = intern(module, Type::Variable(parameter));
    let result = closure_type(module, token, parameter_ty);
    let ty = arrow(module, parameter_ty, result);
    let value_local = locals.fresh();
    let token_local = locals.fresh();
    let body = expr(ExprKind::Local(value_local), parameter_ty, span);
    let closure = lambda(token_local, "token", token, body, result, span);
    let value = lambda(value_local, "value", parameter_ty, closure, ty, span);
    declaration(symbol, name, vec![parameter], ty, value, span)
}

fn bind_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
    variables: &mut VariableSupply,
) -> Declaration {
    let first_var = variables.fresh();
    let next_var = variables.fresh();
    let first_ty = intern(module, Type::Variable(first_var));
    let next_ty = intern(module, Type::Variable(next_var));
    let first_effect = closure_type(module, token, first_ty);
    let next_effect = closure_type(module, token, next_ty);
    let continuation = arrow(module, first_ty, next_effect);
    let suspended = arrow(module, continuation, next_effect);
    let ty = arrow(module, first_effect, suspended);
    let first_local = locals.fresh();
    let next_local = locals.fresh();
    let token_local = locals.fresh();
    let ran_first = application(
        expr(ExprKind::Local(first_local), first_effect, span),
        expr(ExprKind::Local(token_local), token, span),
        first_ty,
        span,
    );
    let ran_next = application(
        expr(ExprKind::Local(next_local), continuation, span),
        ran_first,
        next_effect,
        span,
    );
    let body = application(
        ran_next,
        expr(ExprKind::Local(token_local), token, span),
        next_ty,
        span,
    );
    let closure = lambda(token_local, "token", token, body, next_effect, span);
    let with_next = lambda(next_local, "next", continuation, closure, suspended, span);
    let value = lambda(first_local, "first", first_effect, with_next, ty, span);
    declaration(symbol, name, vec![first_var, next_var], ty, value, span)
}

fn run_declaration(
    module: &mut Module,
    symbol: SymbolId,
    name: &str,
    span: TextRange,
    token: TypeId,
    locals: &mut LocalSupply,
    variables: &mut VariableSupply,
) -> Declaration {
    let result_var = variables.fresh();
    let result_ty = intern(module, Type::Variable(result_var));
    let action_ty = closure_type(module, token, result_ty);
    let ty = arrow(module, action_ty, result_ty);
    let action = locals.fresh();
    let token_value = expr(ExprKind::Integer(0), token, span);
    let body = application(
        expr(ExprKind::Local(action), action_ty, span),
        token_value,
        result_ty,
        span,
    );
    let value = lambda(action, "action", action_ty, body, ty, span);
    declaration(symbol, name, vec![result_var], ty, value, span)
}

fn declaration(
    symbol: SymbolId,
    name: &str,
    quantified: Vec<TypeVariableId>,
    ty: TypeId,
    value: Expr,
    span: TextRange,
) -> Declaration {
    Declaration {
        symbol,
        name: name.to_string(),
        name_span: span,
        quantified,
        ty,
        value,
        span,
    }
}

fn closure_type(module: &mut Module, token: TypeId, result: TypeId) -> TypeId {
    intern(
        module,
        Type::Closure {
            parameters: vec![token],
            result,
        },
    )
}

fn arrow(module: &mut Module, parameter: TypeId, result: TypeId) -> TypeId {
    let function = intern(module, Type::Constructor(TypeConstructor::Function));
    let partial = intern(module, Type::Application(function, parameter));
    intern(module, Type::Application(partial, result))
}

fn lambda(
    id: LocalId,
    name: &str,
    ty: TypeId,
    body: Expr,
    function_ty: TypeId,
    span: TextRange,
) -> Expr {
    expr(
        ExprKind::Lambda {
            binder: Binder {
                id,
                name: name.to_string(),
                ty,
                span,
            },
            body: Box::new(body),
        },
        function_ty,
        span,
    )
}

fn application(function: Expr, argument: Expr, ty: TypeId, span: TextRange) -> Expr {
    expr(
        ExprKind::Application(Box::new(function), Box::new(argument)),
        ty,
        span,
    )
}

fn expr(kind: ExprKind, ty: TypeId, span: TextRange) -> Expr {
    Expr { kind, ty, span }
}

fn external_span(external: &psrs_hir::ExternalSymbol) -> TextRange {
    external
        .signature
        .as_ref()
        .map(|signature| signature.span)
        .unwrap_or_default()
}

fn intern(module: &mut Module, ty: Type) -> TypeId {
    if let Some(index) = module.types.iter().position(|existing| existing == &ty) {
        return TypeId(index as u32);
    }
    module.types.push(ty);
    TypeId((module.types.len() - 1) as u32)
}

struct LocalSupply {
    next: u32,
}

impl LocalSupply {
    fn new(module: &Module) -> Self {
        let mut next = 0;
        for declaration in &module.declarations {
            next = next.max(max_local(&declaration.value) + 1);
        }
        Self { next }
    }

    fn fresh(&mut self) -> LocalId {
        let id = LocalId(self.next);
        self.next += 1;
        id
    }
}

fn max_local(expression: &Expr) -> u32 {
    match &expression.kind {
        ExprKind::Local(id) => id.0,
        ExprKind::Lambda { binder, body } => binder.id.0.max(max_local(body)),
        ExprKind::Application(function, argument) => max_local(function).max(max_local(argument)),
        ExprKind::Let { bindings, body } => bindings
            .iter()
            .map(|binding| binding.binder.id.0.max(max_local(&binding.value)))
            .chain(std::iter::once(max_local(body)))
            .max()
            .unwrap_or(0),
        ExprKind::Case {
            scrutinee,
            branches,
        } => branches
            .iter()
            .map(|branch| max_local(&branch.value))
            .chain(std::iter::once(max_local(scrutinee)))
            .max()
            .unwrap_or(0),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => max_local(condition)
            .max(max_local(then_branch))
            .max(max_local(else_branch)),
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter().map(max_local).max().unwrap_or(0),
        ExprKind::Record { fields } => fields
            .iter()
            .map(|(_, value)| max_local(value))
            .max()
            .unwrap_or(0),
        ExprKind::RecordUpdate { record, fields } => fields
            .iter()
            .map(|(_, value)| max_local(value))
            .chain(std::iter::once(max_local(record)))
            .max()
            .unwrap_or(0),
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. }
        | ExprKind::ArrayLength(record)
        | ExprKind::UnaryPrimitive { value: record, .. } => max_local(record),
        ExprKind::ArrayIndex { array, index } => max_local(array).max(max_local(index)),
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => max_local(array).max(max_local(index)).max(max_local(value)),
        ExprKind::Primitive { left, right, .. } => max_local(left).max(max_local(right)),
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => 0,
    }
}

struct VariableSupply {
    next: u32,
}

impl VariableSupply {
    fn new(module: &Module) -> Self {
        let mut next = 0;
        for ty in &module.types {
            match ty {
                Type::Variable(variable) => next = next.max(variable.0 + 1),
                Type::ForAll { variables, .. } => {
                    for variable in variables {
                        next = next.max(variable.0 + 1);
                    }
                }
                _ => {}
            }
        }
        for declaration in &module.declarations {
            for variable in &declaration.quantified {
                next = next.max(variable.0 + 1);
            }
        }
        Self { next }
    }

    fn fresh(&mut self) -> TypeVariableId {
        let id = TypeVariableId(self.next);
        self.next += 1;
        id
    }
}

/// A suspended import: source parameters, the closure type, and its payload.
pub fn suspended_import(module: &Module, ty: TypeId) -> Option<(Vec<TypeId>, TypeId, TypeId)> {
    let mut current = ty;
    let mut seen = 0;
    while seen <= module.types.len()
        && let Some((_, body)) = crate::forall_parts(&module.types, current)
    {
        current = body;
        seen += 1;
    }
    let mut parameters = Vec::new();
    loop {
        if let Some((closure_parameters, result)) = closure_parts(&module.types, current) {
            if closure_parameters.len() != 1 {
                return None;
            }
            return Some((parameters, current, result));
        }
        let (parameter, result) = crate::arrow_parts(&module.types, current)?;
        parameters.push(parameter);
        current = result;
    }
}

/// Builds `parameters -> result` in the module type table.
pub fn function_type(module: &mut Module, parameters: &[TypeId], result: TypeId) -> TypeId {
    let mut ty = result;
    for parameter in parameters.iter().rev() {
        ty = arrow(module, *parameter, ty);
    }
    ty
}

/// The next foreign-symbol index above the symbols already in the module.
pub fn fresh_foreign_symbol(module: &Module) -> SymbolId {
    let mut next = psrs_hir::FOREIGN_SYMBOL_BASE;
    for external in &module.externals {
        if external.symbol.module == ModuleId::INTRINSICS {
            next = next.max(external.symbol.index.saturating_add(1));
        }
    }
    for declaration in &module.declarations {
        if declaration.symbol.module == ModuleId::INTRINSICS {
            next = next.max(declaration.symbol.index.saturating_add(1));
        }
    }
    SymbolId::new(ModuleId::INTRINSICS, next)
}

/// A fresh local id above every binder already in the module.
pub fn fresh_local(module: &Module) -> LocalId {
    LocalSupply::new(module).fresh()
}
