use super::super::layout::{
    function_arrow_parameters, function_type_signature, scalar_type, unquantified_type,
};
use super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, ValueId, ValueShape,
};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Expr, ExprKind};
use std::collections::HashMap;

mod captures;
mod wrapper;

pub(super) use captures::collect_captures;
use captures::lambda_captures;
pub(super) use wrapper::make_wrapper;

pub(super) trait LambdaLowering {
    fn lower_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>>;

    fn child_lowerer(&self) -> FunctionLowerer<'_>;
}

impl LambdaLowering for FunctionLowerer<'_> {
    fn lower_lambda(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ExprKind::Lambda { binder, body } = &expression.kind else {
            unreachable!("lambda lowering received another expression");
        };
        let Some(signature) =
            function_type_signature(self.module, self.function_types, expression.ty)
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "lambda has no runtime function type",
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
        // A lambda chain `\x -> \y -> e` is one callable value. Peel binders
        // while the current lambda's result is itself an arrow, so the lifted
        // function's arity matches its flattened runtime signature exactly as a
        // top-level declaration is peeled. A lambda at a callable-constructor
        // boundary is that value's hidden context closure: it keeps its own
        // binder and is returned as the value, even when the value is a
        // function.
        let mut binders = vec![binder];
        let mut body = body;
        let mut lambda_type = unquantified_type(self.module, expression.ty);
        while let Some((_, result)) = psrs_core::arrow_parts(&self.module.types, lambda_type) {
            let ExprKind::Lambda { binder, body: rest } = &body.kind else {
                break;
            };
            if psrs_core::arrow_parts(&self.module.types, result).is_none() {
                break;
            }
            binders.push(binder);
            lambda_type = result;
            body = rest;
        }
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
        // When the lambda returns a function value rather than binding every
        // arrow syntactically (`\x -> g x`), expose the full flattened arity by
        // applying the remaining parameters to the returned closure.
        let signature_definition = self.representations.signature(signature).cloned();
        let body_signature = function_type_signature(self.module, self.function_types, body.ty);
        // The body can be a generic function value (an erased reference), such
        // as a callable value returned by `pure`. It still has a runtime call
        // signature, so the remaining parameters are applied to it after a
        // representation cast.
        let eta_expand = matches!(
            (&signature_definition, body_signature),
            (Some(signature_definition), Some(body_signature))
                if signature_definition.parameters.len() > binders.len()
                    && (nested_result_type == closure_value_type_for(body_signature)
                        || is_erased_reference(nested_result_type))
        );
        let mut nested = self.child_lowerer();
        let closure_parameter = nested.fresh(closure_value_type());
        let mut parameters = vec![closure_parameter];
        let source_parameter_types = function_arrow_parameters(self.module, expression.ty).0;
        let mut nested_assignments = Vec::new();
        let mut binder_adaptations = Vec::with_capacity(binders.len());
        for (index, binder) in binders.iter().enumerate() {
            let binder_shape = scalar_type(
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
            let parameter_shape = signature_definition
                .as_ref()
                .and_then(|signature| signature.parameters.get(index))
                .copied()
                .unwrap_or(binder_shape);
            let parameter = nested.fresh(parameter_shape);
            nested.local_types.insert(binder.id, binder.ty);
            let source_type = source_parameter_types
                .get(index)
                .copied()
                .unwrap_or(binder.ty);
            binder_adaptations.push((
                binder.id,
                parameter,
                source_type,
                binder.ty,
                parameter_shape,
                binder_shape,
                binder.span,
            ));
            parameters.push(parameter);
        }
        let mut extra_parameters = Vec::new();
        if eta_expand {
            let signature_definition = signature_definition
                .as_ref()
                .expect("an eta-expanded lambda has an interned signature");
            for parameter_type in &signature_definition.parameters[binders.len()..] {
                let parameter = nested.fresh(*parameter_type);
                parameters.push(parameter);
                extra_parameters.push(parameter);
            }
        }
        for (local, parameter, source_type, binder_type, parameter_shape, binder_shape, span) in
            binder_adaptations
        {
            let conversion = nested.typed_conversion(
                source_type,
                binder_type,
                parameter_shape,
                binder_shape,
                span,
            )?;
            let bound_value = nested.emit_conversion(
                parameter,
                parameter_shape,
                binder_shape,
                conversion,
                span,
                &mut nested_assignments,
            );
            nested.locals.insert(local, bound_value);
        }
        for (index, capture) in captures.into_iter().enumerate() {
            let Some(outer_value) = self.locals.get(&capture).copied() else {
                return Err(capture_error(expression));
            };
            let Some(capture_type) = self
                .values
                .iter()
                .find(|decl| decl.id == outer_value)
                .map(|decl| decl.ty)
            else {
                return Err(capture_error(expression));
            };
            let destination = nested.fresh(capture_type);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: closure_parameter,
                    index: index as u32,
                },
                span: expression.span,
            });
            // A captured generalized local keeps its erased source type, so a
            // use at an instantiated type inside the closure can restore the
            // concrete call signature.
            if let Some(source_type) = self.local_types.get(&capture).copied() {
                nested.local_types.insert(capture, source_type);
            }
            nested.locals.insert(capture, destination);
        }
        let result = nested
            .lower_value(body, &mut nested_assignments)
            .map_err(|_| capture_error(expression))?;
        let (final_result, final_result_type) = if eta_expand {
            let signature_definition = signature_definition
                .as_ref()
                .expect("an eta-expanded lambda has an interned signature");
            let body_signature =
                body_signature.expect("an eta-expanded lambda has a body signature");
            // Recover the erased function value into its closure shape so the
            // indirect call can name the body signature.
            let callable = if is_erased_reference(nested_result_type) {
                let closure = nested.fresh(closure_value_type_for(body_signature));
                nested_assignments.push(Assignment {
                    destination: closure,
                    kind: AssignmentKind::RepresentationCast {
                        destination: closure,
                        value: result,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Closure(body_signature),
                        },
                    },
                    span: expression.span,
                });
                closure
            } else {
                result
            };
            let final_result = nested.fresh(signature_definition.result);
            nested_assignments.push(Assignment {
                destination: final_result,
                kind: AssignmentKind::IndirectCall {
                    function: callable,
                    signature: body_signature,
                    arguments: extra_parameters,
                },
                span: expression.span,
            });
            (final_result, signature_definition.result)
        } else {
            (result, nested_result_type)
        };
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let nested_function = Function {
            symbol,
            name: format!("lambda_{}", expression.span.start),
            parameters,
            values: nested.values,
            assignments: nested_assignments,
            result: final_result,
            result_type: final_result_type,
            span: expression.span,
        };
        super::super::verify::verify_function(
            &nested_function,
            self.signatures,
            self.representations,
        )?;
        let FunctionLowerer {
            generated: nested_generated,
            warnings: nested_warnings,
            ..
        } = nested;
        self.generated.extend(nested_generated);
        self.warnings.extend(nested_warnings);
        self.generated.push(nested_function);
        let closure_result = self.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(signature),
        }));
        assignments.push(Assignment {
            destination: closure_result,
            kind: AssignmentKind::FunctionRef {
                function: symbol,
                signature,
                captures: capture_values,
            },
            span: expression.span,
        });
        if result_type
            == ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            })
        {
            let destination = self.fresh(result_type);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::RepresentationCast {
                    destination,
                    value: closure_result,
                    reference: Reference {
                        nullable: false,
                        heap: RefShape::Erased,
                    },
                },
                span: expression.span,
            });
            Ok(destination)
        } else {
            Ok(closure_result)
        }
    }

    fn child_lowerer(&self) -> FunctionLowerer<'_> {
        FunctionLowerer {
            next_value: 0,
            values: Vec::new(),
            locals: HashMap::new(),
            signatures: self.signatures,
            representations: self.representations,
            module: self.module,
            enum_types: self.enum_types,
            aggregate_types: self.aggregate_types,
            newtype_ids: self.newtype_ids,
            boxed_integer_type: self.boxed_integer_type,
            boxed_number_type: self.boxed_number_type,
            array_types: self.array_types,
            record_types: self.record_types,
            constructor_tags: self.constructor_tags,
            constructors_by_type: self.constructors_by_type,
            constructor_types: self.constructor_types,
            function_types: self.function_types,
            function_wrappers: self.function_wrappers,
            generated_symbols: std::rc::Rc::clone(&self.generated_symbols),
            owner: self.owner,
            warnings: Vec::new(),
            local_types: HashMap::new(),
            generated: Vec::new(),
        }
    }
}

pub(super) fn is_erased_reference(shape: ValueShape) -> bool {
    matches!(
        shape,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        })
    )
}

pub(super) fn closure_value_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

pub(super) fn closure_value_type_for(signature: super::super::SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

fn capture_error(expression: &Expr) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 closure conversion",
        expression.span,
        "closure capture is not representable in the current runtime slice",
    )]
}
