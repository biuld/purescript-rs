use super::{
    Locals, SchemeType, array_element, compatible, error, primitive_type_id, record_field,
    restore_local, verify_pattern, verify_type,
};
use crate::{Expr, ExprKind, Module, Type, TypeConstructor, TypeId, VerifyError};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;

mod arrays;
mod entry;
mod helpers;
mod intrinsic;
mod shapes;
mod string_bytes;
pub(super) use entry::verify_expr;
use helpers::{closure_call, strip_leading_foralls};

struct Context<'a> {
    module: &'a Module,
    /// Immutable source module used for declaration instantiation. `None`
    /// checks every relation against `module`.
    source: Option<&'a Module>,
    owner: ModuleId,
    globals: &'a HashMap<SymbolId, Option<SchemeType>>,
    locals: &'a mut Locals,
    errors: &'a mut Vec<VerifyError>,
}

/// Source relations apply only when every type id still addresses the
/// immutable source table. A newer id belongs to a representation closure
/// and is checked on the physical module.
fn viewed<'a>(physical: &'a Module, source: Option<&'a Module>, ids: &[TypeId]) -> &'a Module {
    match source {
        Some(source) if ids.iter().all(|id| (id.0 as usize) < source.types.len()) => source,
        _ => physical,
    }
}

impl Context<'_> {
    fn expr(&mut self, expression: &Expr, expected: Option<TypeId>) {
        psrs_span::with_sufficient_stack(|| self.expr_inner(expression, expected))
    }

    fn expr_inner(&mut self, expression: &Expr, expected: Option<TypeId>) {
        verify_type(
            expression.ty,
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
        if let Some(expected) = expected {
            let module = viewed(self.module, self.source, &[expression.ty, expected]);
            compatible(
                expression.ty,
                expected,
                module,
                self.owner,
                expression.span,
                self.errors,
            );
        }
        match &expression.kind {
            ExprKind::Local(id) => {
                let Some(local_type) = self.locals.get(id) else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "local reference is not in scope",
                    ));
                    return;
                };
                let module = viewed(self.module, self.source, &[local_type.ty, expression.ty]);
                if !super::types::scheme_instance(
                    local_type.ty,
                    &local_type.quantified,
                    expression.ty,
                    module,
                ) {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "Core expression type is inconsistent with its context",
                    ));
                }
            }
            ExprKind::Global(id) => match self.globals.get(id) {
                Some(Some(global_type)) => {
                    let module = viewed(self.module, self.source, &[global_type.ty, expression.ty]);
                    if !super::types::scheme_instance(
                        global_type.ty,
                        &global_type.quantified,
                        expression.ty,
                        module,
                    ) {
                        self.errors.push(error(
                            self.owner,
                            expression.span,
                            "Core expression type is inconsistent with its context",
                        ));
                    }
                }
                Some(None) => {}
                None => self.errors.push(error(
                    self.owner,
                    expression.span,
                    "global reference is not declared",
                )),
            },
            ExprKind::Integer(_) => self.shape(expression, TypeConstructor::Int),
            ExprKind::Number(_) => self.shape(expression, TypeConstructor::Number),
            ExprKind::Boolean(_) => self.shape(expression, TypeConstructor::Boolean),
            ExprKind::String(_) => self.shape(expression, TypeConstructor::String),
            ExprKind::Char(_) => self.shape(expression, TypeConstructor::Char),
            ExprKind::Unit => self.shape(expression, TypeConstructor::Unit),
            // The state token is the one value of the compiler-owned opaque
            // token type. Only effect lowering produces it.
            ExprKind::StateToken => {
                if !matches!(
                    self.module.types.get(expression.ty.0 as usize),
                    Some(Type::Constructor(TypeConstructor::User(id)))
                        if *id == psrs_hir::TypeId::STATE_TOKEN
                ) {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "state token expression does not have the compiler token type",
                    ));
                }
            }
            // A trap produces no value, so its type is only the one its context
            // wants; the surrounding context check already established that.
            ExprKind::Trap => {}
            ExprKind::IntrinsicCall {
                intrinsic,
                arguments,
            } => self.verify_intrinsic(expression, *intrinsic, arguments),
            ExprKind::Array { elements } => {
                let Some(element_type) = array_element(expression.ty, self.module) else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "array expression does not have an Array type",
                    ));
                    return;
                };
                for element in elements {
                    self.expr(element, Some(element_type));
                }
            }
            ExprKind::Record { fields } => {
                let Some(expected_fields) = self.module.record_fields(expression.ty) else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "record expression does not have a record type",
                    ));
                    return;
                };
                if expected_fields.len() != fields.len() {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "record field count does not match its type",
                    ));
                }
                for (label, value) in fields {
                    let field_type = expected_fields
                        .iter()
                        .find(|(name, _)| name == label)
                        .map(|(_, field_type)| *field_type);
                    self.expr(value, field_type);
                    if field_type.is_none() {
                        self.errors.push(error(
                            self.owner,
                            value.span,
                            "record field is not declared in its type",
                        ));
                    }
                }
            }
            ExprKind::RecordUpdate { record, .. } => self.record_update(expression, record),
            ExprKind::FieldAccess { record, field } => {
                self.expr(record, None);
                if let Some(field_type) = record_field(record.ty, field, self.module) {
                    let module = viewed(self.module, self.source, &[field_type, expression.ty]);
                    compatible(
                        field_type,
                        expression.ty,
                        module,
                        self.owner,
                        expression.span,
                        self.errors,
                    );
                } else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "record field is not declared",
                    ));
                }
            }
            ExprKind::RepresentationCast {
                value,
                source_type,
                target_type,
            } => {
                self.expr(value, Some(*source_type));
                if value.ty != *source_type || expression.ty != *target_type {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "representation cast types do not match its value and result",
                    ));
                }
            }
            ExprKind::Constructor { symbol, arguments } => {
                let Some(constructor) = self
                    .module
                    .constructors
                    .iter()
                    .find(|candidate| candidate.symbol == *symbol)
                    .cloned()
                else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "constructor reference is not declared",
                    ));
                    return;
                };
                if constructor.field_count != arguments.len() {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "constructor application has the wrong field count",
                    ));
                }
                let result_type = strip_leading_foralls(self.module, expression.ty);
                let Some((result_constructor, type_arguments)) =
                    self.module.applied_constructor(result_type)
                else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "constructor result is not an applied user type",
                    ));
                    return;
                };
                if result_constructor != TypeConstructor::User(constructor.type_id) {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "constructor result type does not match its parent type",
                    ));
                }
                let field_instances = arguments
                    .iter()
                    .map(|argument| argument.ty)
                    .collect::<Vec<_>>();
                if !super::types::constructor_fields_match(
                    self.module,
                    &constructor.parameters,
                    &type_arguments,
                    &constructor.field_types,
                    &field_instances,
                ) {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "Core expression type is inconsistent with its context",
                    ));
                }
                for argument in arguments {
                    self.expr(argument, Some(argument.ty));
                }
            }
            ExprKind::Application(function, argument) => {
                self.expr(function, None);
                self.expr(argument, None);
                let function_body = strip_leading_foralls(self.module, function.ty);
                if let Some((parameter, result)) = closure_call(self.module, function_body) {
                    let module = viewed(self.module, self.source, &[argument.ty, parameter]);
                    compatible(
                        argument.ty,
                        parameter,
                        module,
                        self.owner,
                        argument.span,
                        self.errors,
                    );
                    let module = viewed(self.module, self.source, &[result, expression.ty]);
                    compatible(
                        result,
                        expression.ty,
                        module,
                        self.owner,
                        expression.span,
                        self.errors,
                    );
                } else if crate::arrow_parts(&self.module.types, function_body).is_none() {
                    self.errors.push(error(
                        self.owner,
                        function.span,
                        "application target is not a function",
                    ));
                } else {
                    let module = viewed(
                        self.module,
                        self.source,
                        &[function.ty, argument.ty, expression.ty],
                    );
                    if !super::types::application_matches(
                        function.ty,
                        argument.ty,
                        expression.ty,
                        module,
                    ) {
                        self.errors.push(error(
                            self.owner,
                            function.span,
                            "Core expression type is inconsistent with its context",
                        ));
                    }
                }
            }
            ExprKind::Lambda { binder, body } => {
                let function_type = strip_leading_foralls(self.module, expression.ty);
                if let Some((parameter, result)) = closure_call(self.module, function_type) {
                    let module = viewed(self.module, self.source, &[binder.ty, parameter]);
                    compatible(
                        binder.ty,
                        parameter,
                        module,
                        self.owner,
                        binder.span,
                        self.errors,
                    );
                    let previous = self.locals.insert(
                        binder.id,
                        SchemeType {
                            ty: binder.ty,
                            quantified: Vec::new(),
                        },
                    );
                    self.expr(body, Some(result));
                    restore_local(self.locals, binder.id, previous);
                    return;
                }
                let Some((parameter, result)) =
                    crate::arrow_parts(&self.module.types, function_type)
                else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "lambda does not have a function type",
                    ));
                    return;
                };
                let module = viewed(self.module, self.source, &[binder.ty, parameter]);
                compatible(
                    binder.ty,
                    parameter,
                    module,
                    self.owner,
                    binder.span,
                    self.errors,
                );
                let previous = self.locals.insert(
                    binder.id,
                    SchemeType {
                        ty: binder.ty,
                        quantified: Vec::new(),
                    },
                );
                self.expr(body, Some(result));
                restore_local(self.locals, binder.id, previous);
            }
            ExprKind::Let { bindings, body } => {
                let body_type = strip_leading_foralls(self.module, expression.ty);
                let mut previous = Vec::new();
                for binding in bindings {
                    verify_type(
                        binding.binder.ty,
                        self.module,
                        self.owner,
                        binding.binder.span,
                        self.errors,
                    );
                    previous.push((
                        binding.binder.id,
                        self.locals.insert(
                            binding.binder.id,
                            SchemeType {
                                ty: binding.binder.ty,
                                quantified: binding.quantified.clone(),
                            },
                        ),
                    ));
                }
                for binding in bindings {
                    self.expr(&binding.value, Some(binding.binder.ty));
                }
                self.expr(body, Some(body_type));
                for (id, old) in previous.into_iter().rev() {
                    restore_local(self.locals, id, old);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let branch_type = strip_leading_foralls(self.module, expression.ty);
                self.expr(
                    condition,
                    Some(primitive_type_id(self.module, TypeConstructor::Boolean)),
                );
                self.expr(then_branch, Some(branch_type));
                self.expr(else_branch, Some(branch_type));
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                let branch_type = strip_leading_foralls(self.module, expression.ty);
                self.expr(scrutinee, None);
                for branch in branches {
                    let mut branch_locals = self.locals.clone();
                    verify_pattern(
                        &branch.pattern,
                        scrutinee.ty,
                        self.module,
                        self.owner,
                        &mut branch_locals,
                        self.errors,
                    );
                    let mut branch_context = Context {
                        module: self.module,
                        source: self.source,
                        owner: self.owner,
                        globals: self.globals,
                        locals: &mut branch_locals,
                        errors: self.errors,
                    };
                    branch_context.expr(&branch.value, Some(branch_type));
                }
            }
        }
    }
}
