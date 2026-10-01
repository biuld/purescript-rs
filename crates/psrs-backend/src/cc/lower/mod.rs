use super::layout::{scalar_type, unquantified_type};
use super::{
    Assignment, AssignmentKind, Function, ReprId, RepresentationTable, Signature, SignatureId,
    ValueDecl, ValueId, ValueShape,
};
use crate::{BackendError, BackendWarning};
use psrs_core::{Expr, ExprKind, Module as CoreModule, dictionary::ClassLayout};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod array;
mod call;
mod constructor;
mod conversion;
mod dictionary;
mod erased;
mod global;
mod lambda;
mod letrec;
mod record;
mod scalar;
mod string_bytes;
mod symbols;
use call::ApplicationLowering;
pub(in crate::cc) use conversion::VariantFieldConversion;
use global::GlobalLowering;
use lambda::LambdaLowering;
use letrec::LetLowering;
use scalar::{lower_binary_op, lower_unary_op};
pub(in crate::cc) use symbols::GeneratedSymbolAllocator;

pub(super) struct LoweringContext<'a> {
    pub(super) module: &'a CoreModule,
    pub(super) signatures: &'a HashMap<SymbolId, Signature>,
    pub(super) representations: &'a RepresentationTable,
    pub(super) enum_types: &'a HashSet<HirTypeId>,
    pub(super) aggregate_types: &'a HashSet<HirTypeId>,
    pub(super) newtype_ids: &'a HashSet<HirTypeId>,
    pub(super) boxed_integer_type: Option<ReprId>,
    pub(super) boxed_number_type: Option<ReprId>,
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    pub(super) record_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    pub(super) constructor_tags: &'a HashMap<SymbolId, u32>,
    pub(super) constructors_by_type: &'a HashMap<HirTypeId, Vec<(SymbolId, u32)>>,
    pub(super) constructor_types: &'a HashMap<SymbolId, ReprId>,
    pub(super) function_types: &'a HashMap<psrs_core::TypeId, SignatureId>,
    pub(super) function_wrappers: &'a HashMap<SymbolId, SymbolId>,
    pub(super) generated_symbols: Rc<RefCell<GeneratedSymbolAllocator>>,
}

pub(super) fn lower_function(
    declaration: &psrs_core::Declaration,
    context: &LoweringContext<'_>,
) -> Result<(Function, Vec<Function>, Vec<BackendWarning>), Vec<BackendError>> {
    let module = context.module;
    let mut state = FunctionLowerer {
        next_value: 0,
        values: Vec::new(),
        locals: HashMap::new(),
        local_types: HashMap::new(),
        signatures: context.signatures,
        representations: context.representations,
        module,
        enum_types: context.enum_types,
        aggregate_types: context.aggregate_types,
        newtype_ids: context.newtype_ids,
        boxed_integer_type: context.boxed_integer_type,
        boxed_number_type: context.boxed_number_type,
        array_types: context.array_types,
        record_types: context.record_types,
        constructor_tags: context.constructor_tags,
        constructors_by_type: context.constructors_by_type,
        constructor_types: context.constructor_types,
        function_types: context.function_types,
        function_wrappers: context.function_wrappers,
        generated_symbols: Rc::clone(&context.generated_symbols),
        owner: declaration.symbol.module,
        warnings: Vec::new(),
        generated: Vec::new(),
    };
    let mut value = &declaration.value;
    let mut declaration_type = unquantified_type(module, declaration.ty);
    let mut parameters = Vec::new();
    if let Some((closure_parameters, _)) = psrs_core::closure_parts(&module.types, declaration_type)
    {
        // When the value is the closure function, its fixed parameter list
        // belongs to this function and the result is returned as a value. An
        // alias of an existing closure is returned unchanged.
        let count = closure_parameters.len();
        let mut peeled = 0;
        for _ in 0..count {
            let ExprKind::Lambda { binder, body } = &value.kind else {
                break;
            };
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
            state.local_types.insert(binder.id, binder.ty);
            parameters.push(id);
            value = body;
            peeled += 1;
        }
        if peeled != 0 && peeled != count {
            return Err(vec![
                BackendError::new(
                    "P8 closure conversion",
                    declaration.span,
                    "closure declaration is missing a parameter",
                )
                .with_module(declaration.symbol.module),
            ]);
        }
    }
    while let ExprKind::Lambda { binder, body } = &value.kind {
        // Peel only ordinary function arrows. A closure in the result is a
        // value of this function, lowered as a nested closure by `lower_value`.
        if psrs_core::closure_parts(&module.types, declaration_type).is_some() {
            break;
        }
        let Some((_, result)) = psrs_core::arrow_parts(&module.types, declaration_type) else {
            break;
        };
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
        state.local_types.insert(binder.id, binder.ty);
        parameters.push(id);
        declaration_type = result;
        value = body;
        // A quantified result is returned as its own polymorphic closure. Do
        // not peel a syntactic lambda in that result into this function's
        // parameter list.
        if psrs_core::forall_parts(&module.types, declaration_type).is_some() {
            break;
        }
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
    super::verify::verify_function(&function, context.signatures, context.representations)?;
    let mut generated = state.generated;
    generated.push(lambda::make_wrapper(&function, declaration, context));
    let warnings = state
        .warnings
        .into_iter()
        .map(|warning| warning.with_module(declaration.symbol.module))
        .collect();
    Ok((function, generated, warnings))
}

pub(super) struct FunctionLowerer<'a> {
    pub(super) next_value: u32,
    pub(super) values: Vec<ValueDecl>,
    pub(super) locals: HashMap<LocalId, ValueId>,
    /// The declared source type of every local binder in scope. Erased
    /// adaptation is derived from this scope rather than from a side table:
    /// a use of a local whose lowered representation is erased is adapted from
    /// the binder's declared type to the use type.
    pub(super) local_types: HashMap<LocalId, psrs_core::TypeId>,
    pub(super) signatures: &'a HashMap<SymbolId, Signature>,
    pub(super) representations: &'a RepresentationTable,
    pub(super) module: &'a CoreModule,
    pub(super) enum_types: &'a HashSet<HirTypeId>,
    pub(super) aggregate_types: &'a HashSet<HirTypeId>,
    pub(super) newtype_ids: &'a HashSet<HirTypeId>,
    pub(super) boxed_integer_type: Option<ReprId>,
    pub(super) boxed_number_type: Option<ReprId>,
    pub(super) array_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    pub(super) record_types: &'a HashMap<psrs_core::TypeId, ReprId>,
    pub(super) constructor_tags: &'a HashMap<SymbolId, u32>,
    pub(super) constructors_by_type: &'a HashMap<HirTypeId, Vec<(SymbolId, u32)>>,
    pub(super) constructor_types: &'a HashMap<SymbolId, ReprId>,
    pub(super) function_types: &'a HashMap<psrs_core::TypeId, SignatureId>,
    pub(super) function_wrappers: &'a HashMap<SymbolId, SymbolId>,
    pub(super) generated_symbols: Rc<RefCell<GeneratedSymbolAllocator>>,
    pub(super) owner: ModuleId,
    pub(super) warnings: Vec<BackendWarning>,
    pub(super) generated: Vec<Function>,
}

impl FunctionLowerer<'_> {
    pub(super) fn fresh(&mut self, ty: ValueShape) -> ValueId {
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
        if self.module.is_record_type(expression.ty) {
            let layout =
                ClassLayout::from_record_type(self.module, expression.ty).map_err(|message| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        message,
                    )]
                })?;
            return self.lower_dictionary_value(expression, &layout, ty, assignments);
        }
        self.lower_value_inner(expression, ty, assignments)
    }

    pub(super) fn lower_value_inner(
        &mut self,
        expression: &Expr,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        match &expression.kind {
            ExprKind::Local(local) => {
                let value = self.locals.get(local).copied().ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "local value is unavailable before its binding is lowered",
                    )]
                })?;
                self.adapt_erased_function_use(
                    *local,
                    value,
                    expression.ty,
                    expression.span,
                    assignments,
                )
            }
            ExprKind::Global(function) => self.lower_global(expression, *function, ty, assignments),
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
            ExprKind::Array { elements } => self.lower_array(expression, elements, ty, assignments),
            ExprKind::Record { .. } => Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "record value bypassed its checked class layout",
            )]),
            ExprKind::RecordUpdate { record, fields } => {
                self.lower_record_update(expression, record, fields, ty, assignments)
            }
            ExprKind::FieldAccess { record, field } => {
                self.lower_field_access(expression, record, field, ty, assignments)
            }
            ExprKind::RepresentationCast {
                value,
                source_type,
                target_type,
            } => {
                if *source_type != value.ty || *target_type != expression.ty {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "representation cast boundary does not match its typed value",
                    )]);
                }
                let source_shape = self.value_shape(*source_type, expression.span)?;
                let value = self.lower_value(value, assignments)?;
                let conversion = self.typed_conversion(
                    *source_type,
                    *target_type,
                    source_shape,
                    ty,
                    expression.span,
                )?;
                Ok(self.emit_conversion(
                    value,
                    source_shape,
                    ty,
                    conversion,
                    expression.span,
                    assignments,
                ))
            }
            ExprKind::ArrayIndex { array, index } => {
                let Some(representation) = self.array_types.get(&array.ty).copied() else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "array expression has no representation requirement",
                    )]);
                };
                let array = self.lower_value(array, assignments)?;
                let index = self.lower_value(index, assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::ArrayGet {
                        destination,
                        representation,
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
            ExprKind::StringToBytes(value) => {
                self.lower_string_to_bytes(expression, value, ty, assignments)
            }
            ExprKind::BytesToString(value) => {
                self.lower_bytes_to_string(expression, value, ty, assignments)
            }
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
                self.lower_constructor(expression, *symbol, arguments, ty, assignments)
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
                        op: lower_binary_op(*op),
                        left,
                        right,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::UnaryPrimitive { op, value } => {
                let value = self.lower_value(value, assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::Unary {
                        op: lower_unary_op(*op),
                        value,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            ExprKind::Application(_, _) => self.lower_application(expression, ty, assignments),
            ExprKind::Let { bindings, body } => {
                self.lower_let(bindings, body, expression.span, assignments)
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
