use super::super::layout::scalar_type;
use super::super::{Assignment, AssignmentKind, Function, ValueDecl, ValueId, ValueType};
use super::{FunctionLowerer, LoweringContext};
use crate::BackendError;
use crate::types::{HeapType, RefType};
use psrs_core::{Declaration, Expr, ExprKind, PatternKind};
use psrs_hir::{LocalId, SymbolId};
use std::collections::{HashMap, HashSet};

pub(super) trait LambdaLowering {
    fn lower_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;

    fn child_lowerer(&self) -> FunctionLowerer<'_>;
}

impl LambdaLowering for FunctionLowerer<'_> {
    fn lower_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueType,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ExprKind::Lambda { binder, body } = &expression.kind else {
            unreachable!("lambda lowering received another expression");
        };
        let Some(type_index) = self.function_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "lambda has no runtime function type",
            )]);
        };
        let (Some(closure_type), Some(capture_array_type)) =
            (self.closure_type, self.capture_array_type)
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "lambda has no closure layout",
            )]);
        };
        let captures = lambda_captures(body, binder.id);
        let mut capture_values = Vec::with_capacity(captures.len());
        for capture in &captures {
            let Some(value) = self.locals.get(capture).copied() else {
                return Err(capture_error(expression));
            };
            capture_values.push(value);
        }
        let mut nested = self.child_lowerer();
        let closure_parameter = nested.fresh(closure_value_type());
        let parameter_type = scalar_type(
            self.module,
            binder.ty,
            binder.span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let parameter = nested.fresh(parameter_type);
        nested.locals.insert(binder.id, parameter);
        let mut nested_assignments = Vec::new();
        for (index, capture) in captures.into_iter().enumerate() {
            let Some(capture_type) = self
                .locals
                .get(&capture)
                .and_then(|value| self.values.iter().find(|decl| decl.id == *value))
                .map(|decl| decl.ty)
            else {
                return Err(capture_error(expression));
            };
            let destination = nested.fresh(capture_type);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: closure_parameter,
                    closure_type,
                    capture_array_type,
                    index: index as u32,
                },
                span: expression.span,
            });
            nested.locals.insert(capture, destination);
        }
        let result = nested
            .lower_value(body, &mut nested_assignments)
            .map_err(|_| capture_error(expression))?;
        let nested_result_type = scalar_type(
            self.module,
            body.ty,
            body.span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let symbol = SymbolId::new(self.module.id, u32::MAX - expression.span.start);
        let nested_function = Function {
            symbol,
            name: format!("lambda_{}", expression.span.start),
            parameters: vec![closure_parameter, parameter],
            values: nested.values,
            assignments: nested_assignments,
            result,
            result_type: nested_result_type,
            span: expression.span,
        };
        super::super::verify::verify_function(&nested_function, self.signatures)?;
        self.generated.extend(nested.generated);
        self.generated.push(nested_function);
        let destination = self.fresh(result_type);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::FunctionRef {
                function: symbol,
                type_index,
                closure_type,
                capture_array_type,
                captures: capture_values,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    fn child_lowerer(&self) -> FunctionLowerer<'_> {
        FunctionLowerer {
            next_value: 0,
            values: Vec::new(),
            locals: HashMap::new(),
            signatures: self.signatures,
            module: self.module,
            enum_types: self.enum_types,
            aggregate_types: self.aggregate_types,
            newtype_ids: self.newtype_ids,
            boxed_i32_type: self.boxed_i32_type,
            array_types: self.array_types,
            record_types: self.record_types,
            constructor_tags: self.constructor_tags,
            constructors_by_type: self.constructors_by_type,
            constructor_types: self.constructor_types,
            function_types: self.function_types,
            capture_array_type: self.capture_array_type,
            closure_type: self.closure_type,
            function_wrappers: self.function_wrappers,
            generated: Vec::new(),
        }
    }
}

pub(super) fn make_wrapper(
    source: &Function,
    declaration: &Declaration,
    context: &LoweringContext<'_>,
) -> Function {
    let symbol = context.function_wrappers[&declaration.symbol];
    let closure = ValueId(0);
    let mut values = vec![ValueDecl {
        id: closure,
        ty: closure_value_type(),
    }];
    let mut parameters = vec![closure];
    let mut arguments = Vec::with_capacity(source.parameters.len());
    for (index, parameter) in source.parameters.iter().enumerate() {
        let id = ValueId(index as u32 + 1);
        let ty = source
            .values
            .iter()
            .find(|value| value.id == *parameter)
            .map_or(ValueType::I32, |value| value.ty);
        values.push(ValueDecl { id, ty });
        parameters.push(id);
        arguments.push(id);
    }
    let result = ValueId(parameters.len() as u32);
    values.push(ValueDecl {
        id: result,
        ty: source.result_type,
    });
    Function {
        symbol,
        name: format!("{}_closure_wrapper", source.name),
        parameters,
        values,
        assignments: vec![Assignment {
            destination: result,
            kind: AssignmentKind::DirectCall {
                function: source.symbol,
                arguments,
            },
            span: declaration.span,
        }],
        result,
        result_type: source.result_type,
        span: declaration.span,
    }
}

fn lambda_captures(body: &Expr, binder: LocalId) -> Vec<LocalId> {
    let mut bound = HashSet::from([binder]);
    let mut captures = Vec::new();
    collect_captures(body, &mut bound, &mut captures);
    captures
}

fn collect_captures(expression: &Expr, bound: &mut HashSet<LocalId>, captures: &mut Vec<LocalId>) {
    match &expression.kind {
        ExprKind::Local(local) => {
            if !bound.contains(local) && !captures.contains(local) {
                captures.push(*local);
            }
        }
        ExprKind::Lambda { binder, body } => {
            let inserted = bound.insert(binder.id);
            collect_captures(body, bound, captures);
            if inserted {
                bound.remove(&binder.id);
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut nested = bound.clone();
            for binding in bindings {
                nested.insert(binding.binder.id);
            }
            for binding in bindings {
                collect_captures(&binding.value, &mut nested, captures);
            }
            collect_captures(body, &mut nested, captures);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_captures(scrutinee, bound, captures);
            for branch in branches {
                let mut nested = bound.clone();
                collect_pattern_locals(&branch.pattern.kind, &mut nested);
                collect_captures(&branch.value, &mut nested, captures);
            }
        }
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                collect_captures(argument, bound, captures);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_captures(value, bound, captures);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_captures(record, bound, captures);
            for (_, value) in fields {
                collect_captures(value, bound, captures);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            collect_captures(record, bound, captures)
        }
        ExprKind::ArrayIndex { array, index } => {
            collect_captures(array, bound, captures);
            collect_captures(index, bound, captures);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_captures(array, bound, captures);
            collect_captures(index, bound, captures);
            collect_captures(value, bound, captures);
        }
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_captures(left, bound, captures);
            collect_captures(right, bound, captures);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_captures(condition, bound, captures);
            collect_captures(then_branch, bound, captures);
            collect_captures(else_branch, bound, captures);
        }
        ExprKind::Global(_) | ExprKind::Integer(_) | ExprKind::Boolean(_) | ExprKind::String(_) => {
        }
    }
}

fn collect_pattern_locals(pattern: &PatternKind, bound: &mut HashSet<LocalId>) {
    match pattern {
        PatternKind::Var { id, .. } => {
            bound.insert(*id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_locals(&argument.kind, bound);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                collect_pattern_locals(&field.kind, bound);
            }
        }
        PatternKind::Wildcard => {}
    }
}

fn closure_value_type() -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Struct,
    })
}

fn capture_error(expression: &Expr) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 closure conversion",
        expression.span,
        "closure capture is not representable in the current runtime slice",
    )]
}
