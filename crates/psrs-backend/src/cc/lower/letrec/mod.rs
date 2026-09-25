use super::super::layout::scalar_type;
use super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, Signature, SignatureId, ValueId,
    ValueShape,
};
use super::FunctionLowerer;
use super::lambda::{LambdaLowering, collect_captures};
use crate::BackendError;
use psrs_core::{Binder, Binding, Expr, ExprKind};
use psrs_hir::{LocalId, SymbolId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

pub(super) trait LetLowering {
    fn lower_let(
        &mut self,
        bindings: &[Binding],
        body: &Expr,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;
}

impl LetLowering for FunctionLowerer<'_> {
    fn lower_let(
        &mut self,
        bindings: &[Binding],
        body: &Expr,
        _span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let recursive = recursive_indices(bindings);
        if recursive.is_empty() {
            for binding in bindings {
                let value = self.lower_value(&binding.value, assignments)?;
                self.locals.insert(binding.binder.id, value);
            }
            return self.lower_value(body, assignments);
        }

        let recursive_ids = recursive
            .iter()
            .map(|index| bindings[*index].binder.id)
            .collect::<HashSet<_>>();
        let functions = self.recursive_functions(bindings, &recursive)?;
        let captures = recursive_captures(bindings, &recursive, &recursive_ids);
        let function_by_local = functions
            .iter()
            .map(|function| (function.binding.binder.id, function))
            .collect::<HashMap<_, _>>();
        let mut capture_values = None;
        for (index, binding) in bindings.iter().enumerate() {
            if recursive.contains(&index) {
                if capture_values.is_none() {
                    let values = captures
                        .iter()
                        .map(|local| {
                            self.locals.get(local).copied().ok_or_else(|| {
                                vec![BackendError::new(
                                    "P8 closure conversion",
                                    binding.span,
                                    "recursive local function capture is not initialized at this binding",
                                )]
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let shapes = values
                        .iter()
                        .map(|value| self.local_value_shape(*value, binding.span))
                        .collect::<Result<Vec<_>, _>>()?;
                    for function in &functions {
                        self.lower_recursive_function(function, &functions, &captures, &shapes)?;
                    }
                    capture_values = Some(values);
                }
                let function = function_by_local[&binding.binder.id];
                let value = emit_function_ref(
                    self,
                    function.binding,
                    function.symbol,
                    function.signature_id,
                    capture_values
                        .as_deref()
                        .expect("recursive captures are initialized at the first group member"),
                    binding.span,
                    assignments,
                )?;
                self.locals.insert(binding.binder.id, value);
            } else {
                let value = self.lower_value(&binding.value, assignments)?;
                self.locals.insert(binding.binder.id, value);
            }
        }
        self.lower_value(body, assignments)
    }
}

struct RecursiveFunction<'a> {
    binding: &'a Binding,
    symbol: SymbolId,
    signature_id: SignatureId,
    signature: Signature,
    parameters: Vec<&'a Binder>,
    body: &'a Expr,
}

impl FunctionLowerer<'_> {
    fn recursive_functions<'a>(
        &mut self,
        bindings: &'a [Binding],
        recursive: &HashSet<usize>,
    ) -> Result<Vec<RecursiveFunction<'a>>, Vec<BackendError>> {
        let mut functions = Vec::with_capacity(recursive.len());
        for (index, binding) in bindings.iter().enumerate() {
            if !recursive.contains(&index) {
                continue;
            }
            let Some(signature_id) = self.function_types.get(&binding.binder.ty).copied() else {
                return Err(lowering_error(
                    binding.span,
                    "recursive local binding is not a function",
                ));
            };
            let Some(signature) = self.representations.signature(signature_id).cloned() else {
                return Err(lowering_error(
                    binding.span,
                    "recursive local function has an unknown CC signature",
                ));
            };
            let (parameters, body) = peel_parameters(&binding.value, signature.parameters.len())
                .ok_or_else(|| {
                    lowering_error(
                        binding.span,
                        "recursive local function must bind every parameter in its CC signature",
                    )
                })?;
            let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
            functions.push(RecursiveFunction {
                binding,
                symbol,
                signature_id,
                signature,
                parameters,
                body,
            });
        }
        Ok(functions)
    }

    fn lower_recursive_function(
        &mut self,
        function: &RecursiveFunction<'_>,
        group: &[RecursiveFunction<'_>],
        captures: &[LocalId],
        capture_shapes: &[ValueShape],
    ) -> Result<(), Vec<BackendError>> {
        let mut nested = self.child_lowerer();
        let closure_parameter = nested.fresh(aggregate_shape());
        let mut parameters = vec![closure_parameter];
        for (binder, expected) in function
            .parameters
            .iter()
            .zip(&function.signature.parameters)
        {
            let shape = scalar_shape(&nested, binder.ty, binder.span)?;
            if shape != *expected {
                return Err(lowering_error(
                    binder.span,
                    "recursive local function parameter has an incompatible CC shape",
                ));
            }
            let parameter = nested.fresh(shape);
            nested.locals.insert(binder.id, parameter);
            parameters.push(parameter);
        }

        let mut nested_assignments = Vec::new();
        for (index, (local, shape)) in captures.iter().zip(capture_shapes).enumerate() {
            let destination = nested.fresh(*shape);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: closure_parameter,
                    index: index as u32,
                },
                span: function.binding.value.span,
            });
            nested.locals.insert(*local, destination);
        }

        let nested_capture_values = captures
            .iter()
            .map(|local| {
                nested.locals.get(local).copied().ok_or_else(|| {
                    lowering_error(
                        function.binding.value.span,
                        "recursive local function capture was not restored",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for member in group {
            let value = emit_function_ref(
                &mut nested,
                member.binding,
                member.symbol,
                member.signature_id,
                &nested_capture_values,
                member.binding.span,
                &mut nested_assignments,
            )?;
            nested.locals.insert(member.binding.binder.id, value);
        }

        let result = nested.lower_value(function.body, &mut nested_assignments)?;
        let result_type = scalar_shape(&nested, function.body.ty, function.body.span)?;
        if result_type != function.signature.result {
            return Err(lowering_error(
                function.body.span,
                "recursive local function body has an incompatible CC result shape",
            ));
        }
        let generated_function = Function {
            symbol: function.symbol,
            name: format!(
                "{}_letrec_{}",
                function.binding.binder.name, function.binding.span.start
            ),
            parameters,
            values: nested.values,
            assignments: nested_assignments,
            result,
            result_type,
            span: function.binding.value.span,
        };
        super::super::verify::verify_function(
            &generated_function,
            self.signatures,
            self.representations,
        )?;
        let FunctionLowerer {
            generated: nested_generated,
            warnings: nested_warnings,
            ..
        } = nested;
        self.generated.extend(nested_generated);
        self.generated.push(generated_function);
        self.warnings.extend(nested_warnings);
        Ok(())
    }

    fn local_value_shape(
        &self,
        value: ValueId,
        span: TextRange,
    ) -> Result<ValueShape, Vec<BackendError>> {
        self.values
            .iter()
            .find(|declaration| declaration.id == value)
            .map(|declaration| declaration.ty)
            .ok_or_else(|| lowering_error(span, "local capture has no CC value declaration"))
    }
}

fn recursive_indices(bindings: &[Binding]) -> HashSet<usize> {
    let indices = bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| (binding.binder.id, index))
        .collect::<HashMap<_, _>>();
    let dependencies = bindings
        .iter()
        .map(|binding| {
            let mut locals = Vec::new();
            collect_captures(&binding.value, &mut HashSet::new(), &mut locals);
            locals
                .into_iter()
                .filter_map(|local| indices.get(&local).copied())
                .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
    (0..bindings.len())
        .filter(|start| reaches_itself(*start, &dependencies))
        .collect()
}

fn reaches_itself(start: usize, dependencies: &[HashSet<usize>]) -> bool {
    let mut pending = dependencies[start].iter().copied().collect::<Vec<_>>();
    let mut visited = HashSet::new();
    while let Some(next) = pending.pop() {
        if next == start {
            return true;
        }
        if visited.insert(next) {
            pending.extend(dependencies[next].iter().copied());
        }
    }
    false
}

fn recursive_captures(
    bindings: &[Binding],
    recursive: &HashSet<usize>,
    recursive_ids: &HashSet<LocalId>,
) -> Vec<LocalId> {
    let mut bound = recursive_ids.clone();
    let mut captures = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        if recursive.contains(&index) {
            collect_captures(&binding.value, &mut bound, &mut captures);
        }
    }
    captures
}

fn peel_parameters(expression: &Expr, count: usize) -> Option<(Vec<&Binder>, &Expr)> {
    let mut current = expression;
    let mut parameters = Vec::with_capacity(count);
    for _ in 0..count {
        let ExprKind::Lambda { binder, body } = &current.kind else {
            return None;
        };
        parameters.push(binder);
        current = body;
    }
    Some((parameters, current))
}

fn emit_function_ref(
    lowerer: &mut FunctionLowerer<'_>,
    binding: &Binding,
    symbol: SymbolId,
    signature: SignatureId,
    captures: &[ValueId],
    span: TextRange,
    assignments: &mut Vec<Assignment>,
) -> Result<ValueId, Vec<BackendError>> {
    let closure_type = closure_shape(signature);
    let closure = lowerer.fresh(closure_type);
    assignments.push(Assignment {
        destination: closure,
        kind: AssignmentKind::FunctionRef {
            function: symbol,
            signature,
            captures: captures.to_vec(),
        },
        span,
    });
    let binding_type = scalar_shape(lowerer, binding.binder.ty, binding.binder.span)?;
    if binding_type == closure_type {
        return Ok(closure);
    }
    if binding_type == erased_shape() {
        let erased = lowerer.fresh(binding_type);
        assignments.push(Assignment {
            destination: erased,
            kind: AssignmentKind::RepresentationCast {
                destination: erased,
                value: closure,
                reference: Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                },
            },
            span,
        });
        return Ok(erased);
    }
    Err(lowering_error(
        binding.binder.span,
        "recursive local function has an incompatible closure representation",
    ))
}

fn scalar_shape(
    lowerer: &FunctionLowerer<'_>,
    ty: psrs_core::TypeId,
    span: TextRange,
) -> Result<ValueShape, Vec<BackendError>> {
    scalar_type(
        lowerer.module,
        ty,
        span,
        lowerer.enum_types,
        lowerer.aggregate_types,
        lowerer.newtype_ids,
        lowerer.array_types,
        lowerer.record_types,
        lowerer.function_types,
    )
}

fn closure_shape(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

fn aggregate_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

fn erased_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn lowering_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}
