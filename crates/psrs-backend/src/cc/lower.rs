use super::layout::{Signature, depends_on_type_variable, scalar_type};
use super::{Assignment, AssignmentKind, Function, ValueDecl, ValueId, ValueType};
use crate::BackendError;
use psrs_core::{Expr, ExprKind, Module as CoreModule};
use psrs_hir::{LocalId, SymbolId, TypeId as HirTypeId};
use std::collections::{HashMap, HashSet};

mod array;
mod erased;

pub(super) struct LoweringContext<'a> {
    pub(super) module: &'a CoreModule,
    pub(super) signatures: &'a HashMap<SymbolId, Signature>,
    pub(super) enum_types: &'a HashSet<HirTypeId>,
    pub(super) aggregate_types: &'a HashSet<HirTypeId>,
    pub(super) newtype_ids: &'a HashSet<HirTypeId>,
    pub(super) boxed_i32_type: Option<u32>,
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) constructor_tags: &'a HashMap<SymbolId, u32>,
    pub(super) constructors_by_type: &'a HashMap<HirTypeId, Vec<(SymbolId, u32)>>,
    pub(super) constructor_types: &'a HashMap<SymbolId, u32>,
}

pub(super) fn lower_function(
    declaration: &psrs_core::Declaration,
    context: &LoweringContext<'_>,
) -> Result<Function, Vec<BackendError>> {
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
        array_types: context.array_types,
        constructor_tags: context.constructor_tags,
        constructors_by_type: context.constructors_by_type,
        constructor_types: context.constructor_types,
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
    Ok(function)
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
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, u32>,
    pub(super) constructor_tags: &'a HashMap<SymbolId, u32>,
    pub(super) constructors_by_type: &'a HashMap<HirTypeId, Vec<(SymbolId, u32)>>,
    pub(super) constructor_types: &'a HashMap<SymbolId, u32>,
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
                let Some(signature) = self.signatures.get(function) else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "global is not a local top-level function",
                    )]);
                };
                if signature.arity != 0 {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "a function value escapes direct-call position",
                    )]);
                }
                if ty != signature.result {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "global value type differs from its function result type",
                    )]);
                }
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::DirectCall {
                        function: *function,
                        arguments: Vec::new(),
                    },
                    span: expression.span,
                });
                Ok(destination)
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
            ExprKind::Boolean(value) => {
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::Constant(i32::from(*value)),
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Array { elements } => {
                self.lower_array(expression, elements, ty, assignments)
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
            ExprKind::Application(_, _) => {
                let (head, arguments) = collect_application(expression);
                let ExprKind::Global(function) = head.kind else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        head.span,
                        "only direct calls to top-level functions are supported",
                    )]);
                };
                let signature = self.signatures.get(&function).ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        head.span,
                        "call target is not a local top-level function",
                    )]
                })?;
                if signature.arity != arguments.len() {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        format!(
                            "direct call expects {} arguments but received {}",
                            signature.arity,
                            arguments.len()
                        ),
                    )]);
                }
                if ty != signature.result {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "direct call result type differs from the declared function type",
                    )]);
                }
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.lower_value(argument, assignments)?);
                }
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::DirectCall {
                        function,
                        arguments: values,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
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
            ExprKind::Lambda { .. } => Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "capturing or nested lambdas require closure conversion and are not in the first slice",
            )]),
        }
    }
}

fn collect_application(expression: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let ExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}
