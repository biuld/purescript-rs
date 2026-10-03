use super::{
    Locals, SchemeType, array_element, compatible, error, primitive_type_id, primitive_types,
    record_field, restore_local, unary_primitive_types, verify_pattern, verify_type,
};
use crate::{Expr, ExprKind, Module, TypeConstructor, TypeId, VerifyError};
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
    owner: ModuleId,
    globals: &'a HashMap<SymbolId, Option<SchemeType>>,
    locals: &'a mut Locals,
    errors: &'a mut Vec<VerifyError>,
}

impl Context<'_> {
    fn expr(&mut self, expression: &Expr, expected: Option<TypeId>) {
        verify_type(
            expression.ty,
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
        if let Some(expected) = expected {
            compatible(
                expression.ty,
                expected,
                self.module,
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
                if !super::types::scheme_instance(
                    local_type.ty,
                    &local_type.quantified,
                    expression.ty,
                    self.module,
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
                    if !super::types::scheme_instance(
                        global_type.ty,
                        &global_type.quantified,
                        expression.ty,
                        self.module,
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
                    compatible(
                        field_type,
                        expression.ty,
                        self.module,
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
            ExprKind::StringToBytes(value) => self.verify_string_to_bytes(expression, value),
            ExprKind::BytesToString(value) => self.verify_bytes_to_string(expression, value),
            ExprKind::ArrayLength(array) => self.verify_array_length(expression, array),
            ExprKind::ArrayAppend { left, right } => {
                self.verify_array_append(expression, left, right)
            }
            ExprKind::ArrayIndex { array, index } => {
                self.verify_array_index(expression, array, index)
            }
            ExprKind::ArrayUpdate {
                array,
                index,
                value,
            } => self.verify_array_update(expression, array, index, value),
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
            ExprKind::Primitive { op, left, right } => {
                let (operand, result) = primitive_types(*op, self.module);
                self.expr(left, Some(operand));
                self.expr(right, Some(operand));
                compatible(
                    result,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                );
            }
            ExprKind::UnaryPrimitive { op, value } => {
                let (operand, result) = unary_primitive_types(*op, self.module);
                self.expr(value, Some(operand));
                compatible(
                    result,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                );
            }
            ExprKind::Application(function, argument) => {
                self.expr(function, None);
                self.expr(argument, None);
                let function_body = strip_leading_foralls(self.module, function.ty);
                if let Some((parameter, result)) = closure_call(self.module, function_body) {
                    compatible(
                        argument.ty,
                        parameter,
                        self.module,
                        self.owner,
                        argument.span,
                        self.errors,
                    );
                    compatible(
                        result,
                        expression.ty,
                        self.module,
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
                } else if !super::types::application_matches(
                    function.ty,
                    argument.ty,
                    expression.ty,
                    self.module,
                ) {
                    self.errors.push(error(
                        self.owner,
                        function.span,
                        "Core expression type is inconsistent with its context",
                    ));
                }
            }
            ExprKind::Lambda { binder, body } => {
                let function_type = strip_leading_foralls(self.module, expression.ty);
                if let Some((parameter, result)) = closure_call(self.module, function_type) {
                    compatible(
                        binder.ty,
                        parameter,
                        self.module,
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
                compatible(
                    binder.ty,
                    parameter,
                    self.module,
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
