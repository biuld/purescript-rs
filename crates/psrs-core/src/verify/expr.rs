use super::{
    Locals, array_element, callable_result, compatible, error, primitive_type_id, primitive_types,
    record_field, restore_local, unary_primitive_types, user_type_constructor, verify_pattern,
    verify_type,
};
use crate::{Expr, ExprKind, Module, Type, TypeConstructor, TypeId, VerifyError};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;

pub(super) fn verify_expr(
    expression: &Expr,
    expected: Option<TypeId>,
    module: &Module,
    owner: ModuleId,
    globals: &HashMap<SymbolId, Option<TypeId>>,
    locals: &mut Locals,
    errors: &mut Vec<VerifyError>,
) {
    let mut context = Context {
        module,
        owner,
        globals,
        locals,
        errors,
    };
    context.expr(expression, expected);
}

struct Context<'a> {
    module: &'a Module,
    owner: ModuleId,
    globals: &'a HashMap<SymbolId, Option<TypeId>>,
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
                let Some(local_type) = self.locals.get(id).copied() else {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "local reference is not in scope",
                    ));
                    return;
                };
                compatible(
                    local_type,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                );
            }
            ExprKind::Global(id) => match self.globals.get(id) {
                Some(Some(global_type)) => compatible(
                    *global_type,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                ),
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
            ExprKind::RecordUpdate { record, fields } => {
                self.expr(record, None);
                for (label, value) in fields {
                    if record_field(record.ty, label, self.module).is_none() {
                        self.errors.push(error(
                            self.owner,
                            value.span,
                            "record update field is not declared",
                        ));
                    }
                    let field_type = record_field(expression.ty, label, self.module);
                    self.expr(value, field_type);
                    if field_type.is_none() {
                        self.errors.push(error(
                            self.owner,
                            value.span,
                            "record update field is not declared",
                        ));
                    }
                }
            }
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
            ExprKind::ArrayLength(array) => {
                self.expr(array, None);
                if array_element(array.ty, self.module).is_none() {
                    self.errors.push(error(
                        self.owner,
                        array.span,
                        "arrayLength expects an Array value",
                    ));
                }
                self.shape(expression, TypeConstructor::Int);
            }
            ExprKind::ArrayIndex { array, index } => {
                let Some(element_type) = array_element(array.ty, self.module) else {
                    self.errors.push(error(
                        self.owner,
                        array.span,
                        "arrayIndex expects an Array value",
                    ));
                    return;
                };
                self.expr(array, None);
                self.expr(
                    index,
                    Some(primitive_type_id(self.module, TypeConstructor::Int)),
                );
                compatible(
                    element_type,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                );
            }
            ExprKind::ArrayUpdate {
                array,
                index,
                value,
            } => {
                let Some(element_type) = array_element(array.ty, self.module) else {
                    self.errors.push(error(
                        self.owner,
                        array.span,
                        "arrayUpdate expects an Array value",
                    ));
                    return;
                };
                self.expr(array, None);
                self.expr(
                    index,
                    Some(primitive_type_id(self.module, TypeConstructor::Int)),
                );
                self.expr(value, Some(element_type));
                compatible(
                    array.ty,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                );
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
                if user_type_constructor(expression.ty, self.module) != Some(constructor.type_id) {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "constructor result type does not match its parent type",
                    ));
                }
                for (argument, field_type) in arguments.iter().zip(constructor.field_types) {
                    self.expr(argument, Some(field_type));
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
                if let Some((parameter, result)) =
                    crate::arrow_parts(&self.module.types, function.ty)
                {
                    compatible(
                        parameter,
                        argument.ty,
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
                } else if let Some(result) = callable_result(self.module, function.ty) {
                    compatible(
                        result,
                        expression.ty,
                        self.module,
                        self.owner,
                        expression.span,
                        self.errors,
                    );
                } else {
                    match self.module.types.get(function.ty.0 as usize) {
                        Some(Type::Variable(_)) => {}
                        _ => self.errors.push(error(
                            self.owner,
                            function.span,
                            "application target is not a function",
                        )),
                    }
                }
            }
            ExprKind::Lambda { binder, body } => {
                // A lambda whose type is a callable constructor application is
                // the runtime closure of that value: its binder is the hidden
                // context parameter and its body produces the value type. The
                // context parameter is a calling-convention detail, so only the
                // body result is checked.
                if let Some(result) = callable_result(self.module, expression.ty) {
                    let previous = self.locals.insert(binder.id, binder.ty);
                    self.expr(body, Some(result));
                    restore_local(self.locals, binder.id, previous);
                    return;
                }
                let Some((parameter, result)) =
                    crate::arrow_parts(&self.module.types, expression.ty)
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
                let previous = self.locals.insert(binder.id, binder.ty);
                self.expr(body, Some(result));
                restore_local(self.locals, binder.id, previous);
            }
            ExprKind::Let { bindings, body } => {
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
                        self.locals.insert(binding.binder.id, binding.binder.ty),
                    ));
                }
                for binding in bindings {
                    self.expr(&binding.value, Some(binding.binder.ty));
                }
                self.expr(body, Some(expression.ty));
                for (id, old) in previous.into_iter().rev() {
                    restore_local(self.locals, id, old);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(
                    condition,
                    Some(primitive_type_id(self.module, TypeConstructor::Boolean)),
                );
                self.expr(then_branch, Some(expression.ty));
                self.expr(else_branch, Some(expression.ty));
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
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
                    branch_context.expr(&branch.value, Some(expression.ty));
                }
            }
        }
    }

    fn shape(&mut self, expression: &Expr, shape: TypeConstructor) {
        compatible(
            expression.ty,
            primitive_type_id(self.module, shape),
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
    }
}
