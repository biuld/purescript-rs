use super::layout::{Signature, depends_on_type_variable, scalar_type};
use super::{Assignment, AssignmentKind, Function, ValueDecl, ValueId, ValueType};
use crate::BackendError;
use psrs_core::{Expr, ExprKind, Module as CoreModule};
use psrs_hir::{LocalId, SymbolId, TypeId as HirTypeId};
use std::collections::{HashMap, HashSet};

mod array;
mod call;
mod erased;
mod global;
mod lambda;
mod record;
use call::ApplicationLowering;
use global::GlobalLowering;
use lambda::LambdaLowering;

pub(super) struct LoweringContext<'a> {
    pub(super) module: &'a CoreModule,
    pub(super) signatures: &'a HashMap<SymbolId, Signature>,
    pub(super) enum_types: &'a HashSet<HirTypeId>,
    pub(super) aggregate_types: &'a HashSet<HirTypeId>,
    pub(super) newtype_ids: &'a HashSet<HirTypeId>,
    pub(super) boxed_i32_type: Option<u32>,
    pub(super) boxed_f64_type: Option<u32>,
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) record_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) constructor_tags: &'a HashMap<SymbolId, u32>,
    pub(super) constructors_by_type: &'a HashMap<HirTypeId, Vec<(SymbolId, u32)>>,
    pub(super) constructor_types: &'a HashMap<SymbolId, u32>,
    pub(super) function_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) capture_array_type: Option<u32>,
    pub(super) closure_type: Option<u32>,
    pub(super) function_wrappers: &'a HashMap<SymbolId, SymbolId>,
}

pub(super) fn lower_function(
    declaration: &psrs_core::Declaration,
    context: &LoweringContext<'_>,
) -> Result<(Function, Vec<Function>), Vec<BackendError>> {
    let module = context.module;
    let mut state = FunctionLowerer {
        next_value: 0,
        values: Vec::new(),
        locals: HashMap::new(),
        signatures: context.signatures,
        module,
        enum_types: context.enum_types,
        aggregate_types: context.aggregate_types,
        newtype_ids: context.newtype_ids,
        boxed_i32_type: context.boxed_i32_type,
        boxed_f64_type: context.boxed_f64_type,
        array_types: context.array_types,
        record_types: context.record_types,
        constructor_tags: context.constructor_tags,
        constructors_by_type: context.constructors_by_type,
        constructor_types: context.constructor_types,
        function_types: context.function_types,
        capture_array_type: context.capture_array_type,
        closure_type: context.closure_type,
        function_wrappers: context.function_wrappers,
        erased_function_types: HashMap::new(),
        generated: Vec::new(),
    };
    let mut value = &declaration.value;
    let mut parameters = Vec::new();
    while let ExprKind::Lambda { binder, body } = &value.kind {
        let ty = scalar_type(
            module,
            binder.ty,
            binder.span,
            context.enum_types,
            context.aggregate_types,
            context.newtype_ids,
            context.array_types,
            context.record_types,
            context.function_types,
        )?;
        let id = state.fresh(ty);
        state.locals.insert(binder.id, id);
        parameters.push(id);
        value = body;
    }
    let mut assignments = Vec::new();
    let result = state.lower_value(value, &mut assignments)?;
    let result_type = scalar_type(
        module,
        value.ty,
        value.span,
        context.enum_types,
        context.aggregate_types,
        context.newtype_ids,
        context.array_types,
        context.record_types,
        context.function_types,
    )?;
    let function = Function {
        symbol: declaration.symbol,
        name: declaration.name.clone(),
        parameters,
        values: state.values,
        assignments,
        result,
        result_type,
        span: declaration.span,
    };
    super::verify::verify_function(&function, context.signatures)?;
    let mut generated = state.generated;
    generated.push(lambda::make_wrapper(&function, declaration, context));
    Ok((function, generated))
}

pub(super) struct FunctionLowerer<'a> {
    pub(super) next_value: u32,
    pub(super) values: Vec<ValueDecl>,
    pub(super) locals: HashMap<LocalId, ValueId>,
    pub(super) signatures: &'a HashMap<SymbolId, Signature>,
    pub(super) module: &'a CoreModule,
    pub(super) enum_types: &'a HashSet<HirTypeId>,
    pub(super) aggregate_types: &'a HashSet<HirTypeId>,
    pub(super) newtype_ids: &'a HashSet<HirTypeId>,
    pub(super) boxed_i32_type: Option<u32>,
    pub(super) boxed_f64_type: Option<u32>,
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) record_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) constructor_tags: &'a HashMap<SymbolId, u32>,
    pub(super) constructors_by_type: &'a HashMap<HirTypeId, Vec<(SymbolId, u32)>>,
    pub(super) constructor_types: &'a HashMap<SymbolId, u32>,
    pub(super) function_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) capture_array_type: Option<u32>,
    pub(super) closure_type: Option<u32>,
    pub(super) function_wrappers: &'a HashMap<SymbolId, SymbolId>,
    pub(super) erased_function_types: HashMap<ValueId, psrs_core::TypeId>,
    pub(super) generated: Vec<Function>,
}

impl FunctionLowerer<'_> {
    pub(super) fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
    }

    pub(super) fn lower_value(
        &mut self,
        expression: &Expr,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ty = scalar_type(
            self.module,
            expression.ty,
            expression.span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        match &expression.kind {
            ExprKind::Local(local) => self.locals.get(local).copied().ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "local value is unavailable; recursive or escaping local functions are unsupported",
                )]
            }),
            ExprKind::Global(function) => {
                self.lower_global(expression, *function, ty, assignments)
            }
            ExprKind::Integer(value) => {
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::Constant(*value),
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Number(value) => {
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::NumberConstant(value.clone()),
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Boolean(value) => {
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::Constant(i32::from(*value)),
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Char(value) => {
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::Constant(*value as i32),
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Array { elements } => {
                self.lower_array(expression, elements, ty, assignments)
            }
            ExprKind::Record { fields } => self.lower_record(expression, fields, ty, assignments),
            ExprKind::RecordUpdate { record, fields } => {
                self.lower_record_update(expression, record, fields, ty, assignments)
            }
            ExprKind::FieldAccess { record, field } => {
                self.lower_field_access(expression, record, field, ty, assignments)
            }
            ExprKind::ArrayIndex { array, index } => {
                let Some(type_index) = self.array_types.get(&array.ty).copied() else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "array expression has no concrete GC array layout",
                    )]);
                };
                let array = self.lower_value(array, assignments)?;
                let index = self.lower_value(index, assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::ArrayGet {
                        destination,
                        type_index,
                        value: array,
                        index,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::ArrayUpdate {
                array,
                index,
                value,
            } => self.lower_array_update(expression, array, index, value, assignments),
            ExprKind::ArrayLength(value) => {
                let value = self.lower_value(value, assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::ArrayLen { destination, value },
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Constructor { symbol, arguments } => {
                let constructor = self
                    .module
                    .constructors
                    .iter()
                    .find(|constructor| constructor.symbol == *symbol)
                    .ok_or_else(|| {
                        vec![BackendError::new(
                            "P8 closure conversion",
                            expression.span,
                            "constructor value has no declaration",
                        )]
                    })?;
                if constructor.field_count != arguments.len() {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "constructor application is not saturated",
                    )]);
                }
                if self.newtype_ids.contains(&constructor.type_id) {
                    if constructor.field_count != 1 {
                        return Err(vec![BackendError::new(
                            "P8 closure conversion",
                            expression.span,
                            "newtype constructor must have exactly one field",
                        )]);
                    }
                    return self.lower_value(&arguments[0], assignments);
                }
                let tag = self.constructor_tags.get(symbol).copied().ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "constructor value has no known tag",
                    )]
                })?;
                let destination = self.fresh(ty);
                if self.aggregate_types.contains(&constructor.type_id) {
                    let mut values = Vec::with_capacity(arguments.len() + 1);
                    let tag_value = self.fresh(ValueType::I32);
                    assignments.push(Assignment {
                        destination: tag_value,
                        kind: AssignmentKind::Constant(tag as i32),
                        span: expression.span,
                    });
                    values.push(tag_value);
                    for (index, argument) in arguments.iter().enumerate() {
                        let value = self.lower_value(argument, assignments)?;
                        if depends_on_type_variable(
                            self.module,
                            constructor.field_types[index],
                        ) {
                            values.push(self.box_erased_value(value, expression.span, assignments)?);
                        } else {
                            values.push(value);
                        }
                    }
                    let Some(type_index) = self.constructor_types.get(symbol).copied() else {
                        return Err(vec![BackendError::new(
                            "P8 closure conversion",
                            expression.span,
                            "aggregate constructor has no GC type layout",
                        )]);
                    };
                    assignments.push(Assignment {
                        destination,
                        kind: AssignmentKind::StructNew {
                            destination,
                            type_index,
                            arguments: values,
                        },
                        span: expression.span,
                    });
                } else {
                    assignments.push(Assignment {
                        destination,
                        kind: AssignmentKind::Constant(tag as i32),
                        span: expression.span,
                    });
                }
                Ok(destination)
            }
            ExprKind::String(text) => {
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::StringConstant(text.clone()),
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Primitive { op, left, right } => {
                let left = self.lower_value(left, assignments)?;
                let right = self.lower_value(right, assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::Primitive {
                        op: *op,
                        left,
                        right,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Application(_, _) => self.lower_application(expression, ty, assignments),
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    let value = self.lower_value(&binding.value, assignments)?;
                    self.locals.insert(binding.binder.id, value);
                }
                self.lower_value(body, assignments)
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.lower_value(condition, assignments)?;
                let mut then_assignments = Vec::new();
                let then_value = self.lower_value(then_branch, &mut then_assignments)?;
                let mut else_assignments = Vec::new();
                let else_value = self.lower_value(else_branch, &mut else_assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::If {
                        condition,
                        then_assignments,
                        then_value,
                        else_assignments,
                        else_value,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                let scrutinee_type = scrutinee.ty;
                let scrutinee = self.lower_value(scrutinee, assignments)?;
                self.lower_case(
                    scrutinee_type,
                    scrutinee,
                    branches,
                    ty,
                    expression.span,
                    assignments,
                )
            }
            ExprKind::Lambda { .. } => self.lower_lambda(expression, ty, assignments),
        }
    }
}
