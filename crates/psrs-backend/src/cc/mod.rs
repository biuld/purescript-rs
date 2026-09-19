use crate::BackendError;
use psrs_core::{Expr, ExprKind, Module as CoreModule, Primitive, Type};
use psrs_hir::{ExternalKind, ExternalSymbol, LocalId, RuntimeFunction, SymbolId};
use psrs_span::TextRange;
use std::collections::HashMap;

mod verify;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueType {
    I32,
    Boolean,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDecl {
    pub id: ValueId,
    pub ty: ValueType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub functions: Vec<Function>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub parameters: Vec<ValueId>,
    pub values: Vec<ValueDecl>,
    pub assignments: Vec<Assignment>,
    pub result: ValueId,
    pub result_type: ValueType,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub destination: ValueId,
    pub kind: AssignmentKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentKind {
    Constant(i32),
    StringConstant(String),
    Copy(ValueId),
    Primitive {
        op: Primitive,
        left: ValueId,
        right: ValueId,
    },
    DirectCall {
        function: SymbolId,
        arguments: Vec<ValueId>,
    },
    If {
        condition: ValueId,
        then_assignments: Vec<Assignment>,
        then_value: ValueId,
        else_assignments: Vec<Assignment>,
        else_value: ValueId,
    },
}

#[derive(Clone, Copy)]
pub(super) struct Signature {
    arity: usize,
    result: ValueType,
}

pub(super) fn runtime_signature(kind: ExternalKind) -> Option<Signature> {
    match kind {
        ExternalKind::Runtime(RuntimeFunction::ConsoleLog) => Some(Signature {
            arity: 1,
            result: ValueType::I32,
        }),
        ExternalKind::Intrinsic(_) => None,
    }
}

pub fn lower_module(module: CoreModule) -> Result<Module, Vec<BackendError>> {
    if let Err(errors) = module.verify() {
        return Err(errors
            .into_iter()
            .map(|error| BackendError::new("P8 Core verification", error.span, error.message))
            .collect());
    }
    let mut signatures = HashMap::new();
    for declaration in &module.declarations {
        let (arity, result_ty) = declaration_shape(declaration, &module)?;
        signatures.insert(
            declaration.symbol,
            Signature {
                arity,
                result: result_ty,
            },
        );
    }
    for external in &module.externals {
        if let Some(signature) = runtime_signature(external.kind) {
            signatures.insert(external.symbol, signature);
        }
    }
    let mut functions = Vec::with_capacity(module.declarations.len());
    for declaration in &module.declarations {
        functions.push(lower_function(declaration, &module, &signatures)?);
    }
    let cc = Module {
        name: module.name,
        externals: module.externals,
        functions,
        span: module.span,
    };
    verify::verify_module(&cc)?;
    Ok(cc)
}

fn declaration_shape(
    declaration: &psrs_core::Declaration,
    module: &CoreModule,
) -> Result<(usize, ValueType), Vec<BackendError>> {
    if !declaration.quantified.is_empty() {
        return Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.name_span,
            "polymorphic declarations are not supported by the first backend slice",
        )]);
    }
    let mut ty = declaration.ty;
    let mut arity = 0;
    let mut value = &declaration.value;
    while let ExprKind::Lambda { binder, body } = &value.kind {
        let Some(Type::Function { parameter, result }) = module.types.get(ty.0 as usize) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                binder.span,
                "lambda binder does not have a function type",
            )]);
        };
        if *parameter != binder.ty {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                binder.span,
                "lambda binder type differs from the function parameter type",
            )]);
        }
        ty = *result;
        arity += 1;
        value = body;
    }
    match module.types.get(ty.0 as usize) {
        Some(Type::I32 | Type::String | Type::Unit) => Ok((arity, ValueType::I32)),
        Some(Type::Boolean) => Ok((arity, ValueType::Boolean)),
        Some(Type::Variable(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.name_span,
            "polymorphic declarations are not supported by the first backend slice",
        )]),
        Some(Type::Function { .. }) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "the first backend slice cannot return a function value",
        )]),
        None => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "declaration type is outside the Core type table",
        )]),
    }
}

fn lower_function(
    declaration: &psrs_core::Declaration,
    module: &CoreModule,
    signatures: &HashMap<SymbolId, Signature>,
) -> Result<Function, Vec<BackendError>> {
    let mut state = FunctionLowerer {
        next_value: 0,
        values: Vec::new(),
        locals: HashMap::new(),
        signatures,
        module,
    };
    let mut value = &declaration.value;
    let mut parameters = Vec::new();
    while let ExprKind::Lambda { binder, body } = &value.kind {
        let ty = scalar_type(module, binder.ty, binder.span)?;
        let id = state.fresh(ty);
        state.locals.insert(binder.id, id);
        parameters.push(id);
        value = body;
    }
    let mut assignments = Vec::new();
    let result = state.lower_value(value, &mut assignments)?;
    let result_type = scalar_type(module, value.ty, value.span)?;
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
    verify::verify_function(&function, signatures)?;
    Ok(function)
}

struct FunctionLowerer<'a> {
    next_value: u32,
    values: Vec<ValueDecl>,
    locals: HashMap<LocalId, ValueId>,
    signatures: &'a HashMap<SymbolId, Signature>,
    module: &'a CoreModule,
}

impl FunctionLowerer<'_> {
    fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
    }

    fn lower_value(
        &mut self,
        expression: &Expr,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ty = scalar_type(self.module, expression.ty, expression.span)?;
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

fn scalar_type(
    module: &CoreModule,
    id: psrs_core::TypeId,
    span: TextRange,
) -> Result<ValueType, Vec<BackendError>> {
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::String | Type::Unit) => Ok(ValueType::I32),
        Some(Type::Boolean) => Ok(ValueType::Boolean),
        Some(Type::Variable(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "polymorphic values are not supported by the first backend slice",
        )]),
        Some(Type::Function { .. }) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "function values are supported only as top-level direct-call targets",
        )]),
        None => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "expression type is outside the Core type table",
        )]),
    }
}
