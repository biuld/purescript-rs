use super::layout::scalar_type;
use super::{
    Assignment, AssignmentKind, Function, ReprId, RepresentationTable, Signature, SignatureId,
    ValueDecl, ValueId, ValueShape,
};
use crate::{BackendError, BackendWarning};
use psrs_core::{Expr, ExprKind, Module as CoreModule};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod array;
mod call;
mod constructor;
mod erased;
mod global;
mod lambda;
mod letrec;
mod record;
mod scalar;
use call::ApplicationLowering;
use global::GlobalLowering;
use lambda::LambdaLowering;
use letrec::LetLowering;
use scalar::{lower_binary_op, lower_unary_op};

/// Allocates generated callable symbols without relying on source offsets.
///
/// Linked Core keeps source declaration symbols from each input module but
/// lowers generated functions after linking. A shared allocator therefore
/// reserves every existing callable symbol and allocates within the
/// originating source module so diagnostics retain their source ownership.
pub(super) struct GeneratedSymbolAllocator {
    used: HashSet<SymbolId>,
    next: HashMap<ModuleId, u32>,
}

impl GeneratedSymbolAllocator {
    pub(super) fn new(module: &CoreModule) -> Self {
        let used = module
            .declarations
            .iter()
            .map(|declaration| declaration.symbol)
            .chain(module.externals.iter().map(|external| external.symbol))
            .collect();
        Self {
            used,
            next: HashMap::new(),
        }
    }

    pub(super) fn fresh(&mut self, module: ModuleId) -> SymbolId {
        let next = self.next.entry(module).or_default();
        loop {
            let symbol = SymbolId::new(module, *next);
            *next = next
                .checked_add(1)
                .expect("generated symbol index space exhausted");
            if self.used.insert(symbol) {
                return symbol;
            }
        }
    }
}

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
    pub(super) erased_function_types: HashMap<ValueId, psrs_core::TypeId>,
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
        match &expression.kind {
            ExprKind::Local(local) => self.locals.get(local).copied().ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "local value is unavailable before its binding is lowered",
                )]
            }),
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
            ExprKind::Record { fields } => self.lower_record(expression, fields, ty, assignments),
            ExprKind::RecordUpdate { record, fields } => {
                self.lower_record_update(expression, record, fields, ty, assignments)
            }
            ExprKind::FieldAccess { record, field } => {
                self.lower_field_access(expression, record, field, ty, assignments)
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
