use crate::{
    Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeConstructor, TypeId, VerifyError,
};
use psrs_hir::{LocalId, SymbolId, TypeVariableId};
use std::collections::HashMap;

mod matching;

pub(super) fn types_equal(left: TypeId, right: TypeId, module: &Module) -> bool {
    matching::equivalent(left, right, module)
}

#[derive(Clone)]
struct Scheme {
    ty: TypeId,
    quantified: Vec<TypeVariableId>,
}

pub(super) fn verify_module(module: &Module) -> Vec<VerifyError> {
    let globals = module
        .declarations
        .iter()
        .map(|declaration| {
            (
                declaration.symbol,
                Scheme {
                    ty: declaration.ty,
                    quantified: declaration.quantified.clone(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut errors = Vec::new();
    for declaration in &module.declarations {
        let mut context = Context {
            module,
            globals: &globals,
            locals: HashMap::new(),
            errors: &mut errors,
        };
        context.expr(&declaration.value, Some(declaration.ty));
    }
    errors
}

struct Context<'a> {
    module: &'a Module,
    globals: &'a HashMap<SymbolId, Scheme>,
    locals: HashMap<LocalId, Scheme>,
    errors: &'a mut Vec<VerifyError>,
}

impl Context<'_> {
    fn expr(&mut self, expression: &Expr, expected: Option<TypeId>) {
        if let Some(expected) = expected {
            self.compatible(expression.ty, expected, expression.span);
        }
        match &expression.kind {
            ExprKind::Local(id) => match self.locals.get(id) {
                Some(scheme)
                    if matching::scheme_instance(
                        scheme.ty,
                        &scheme.quantified,
                        expression.ty,
                        self.module,
                    ) => {}
                Some(_) => self.error(
                    expression.span,
                    "local reference is not a valid scheme instance",
                ),
                None => self.error(expression.span, "local reference is not in scope"),
            },
            ExprKind::Global(symbol) => {
                if let Some(scheme) = self.globals.get(symbol)
                    && !matching::scheme_instance(
                        scheme.ty,
                        &scheme.quantified,
                        expression.ty,
                        self.module,
                    )
                {
                    self.error(
                        expression.span,
                        "global reference is not a valid scheme instance",
                    );
                }
            }
            ExprKind::Integer(_) => self.primitive_shape(expression, TypeConstructor::Int),
            ExprKind::Number(_) => self.primitive_shape(expression, TypeConstructor::Number),
            ExprKind::Boolean(_) => self.primitive_shape(expression, TypeConstructor::Boolean),
            ExprKind::String(_) => self.primitive_shape(expression, TypeConstructor::String),
            ExprKind::Char(_) => self.primitive_shape(expression, TypeConstructor::Char),
            ExprKind::Array(elements) => {
                if let Some(element) = array_element(self.module, expression.ty) {
                    for value in elements {
                        self.expr(value, Some(element));
                    }
                } else {
                    self.error(
                        expression.span,
                        "array expression does not have an Array type",
                    );
                }
            }
            ExprKind::Record(fields) => {
                let Some(expected_fields) = crate::record_fields(&self.module.types, expression.ty)
                else {
                    self.error(
                        expression.span,
                        "record expression does not have a record type",
                    );
                    return;
                };
                for (label, value) in fields {
                    let field_type = expected_fields
                        .iter()
                        .find(|(expected_label, _)| expected_label == label)
                        .map(|(_, ty)| *ty);
                    self.expr(value, field_type);
                    if field_type.is_none() {
                        self.error(value.span, "record field is absent from its type");
                    }
                }
            }
            ExprKind::RecordUpdate {
                expression: record,
                fields,
            } => {
                self.expr(record, None);
                for (label, value) in fields {
                    let source_field = record_field(self.module, record.ty, label);
                    let target_field = record_field(self.module, expression.ty, label);
                    if source_field.is_none() || target_field.is_none() {
                        self.error(value.span, "record update field is absent from its type");
                    }
                    self.expr(value, target_field);
                }
            }
            ExprKind::FieldAccess {
                expression: record,
                field,
            } => {
                self.expr(record, None);
                if let Some(field_type) = record_field(self.module, record.ty, field) {
                    self.compatible(field_type, expression.ty, expression.span);
                } else {
                    self.error(expression.span, "record field is not declared");
                }
            }
            ExprKind::Evidence(_) => {}
            ExprKind::Coerce {
                value,
                source_type,
                target_type,
                ..
            } => {
                self.expr(value, Some(*source_type));
                self.compatible(*target_type, expression.ty, expression.span);
            }
            ExprKind::Application(function, argument) => {
                self.expr(function, None);
                self.expr(argument, None);
                if !matching::application(function.ty, argument.ty, expression.ty, self.module) {
                    self.error(
                        expression.span,
                        "application argument or result type is inconsistent",
                    );
                }
            }
            ExprKind::Lambda { binder, body } => {
                let function_type = strip_leading_foralls(self.module, expression.ty);
                if let Some((parameter, result)) =
                    crate::arrow_parts(&self.module.types, function_type)
                {
                    self.compatible(binder.ty, parameter, binder.span);
                    let previous = self.locals.insert(
                        binder.id,
                        Scheme {
                            ty: binder.ty,
                            quantified: Vec::new(),
                        },
                    );
                    self.expr(body, Some(result));
                    restore_local(&mut self.locals, binder.id, previous);
                } else {
                    self.error(expression.span, "lambda does not have a function type");
                    self.expr(body, None);
                }
            }
            ExprKind::Let { bindings, body } => {
                let previous = bindings
                    .iter()
                    .map(|binding| {
                        (
                            binding.binder.id,
                            self.locals.insert(
                                binding.binder.id,
                                Scheme {
                                    ty: binding.binder.ty,
                                    quantified: binding.quantified.clone(),
                                },
                            ),
                        )
                    })
                    .collect::<Vec<_>>();
                for binding in bindings {
                    self.expr(&binding.value, Some(binding.binder.ty));
                }
                self.expr(
                    body,
                    Some(strip_leading_foralls(self.module, expression.ty)),
                );
                for (id, old) in previous.into_iter().rev() {
                    restore_local(&mut self.locals, id, old);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(
                    condition,
                    primitive_type(self.module, TypeConstructor::Boolean),
                );
                let result = strip_leading_foralls(self.module, expression.ty);
                self.expr(then_branch, Some(result));
                self.expr(else_branch, Some(result));
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                self.expr(scrutinee, None);
                let result = strip_leading_foralls(self.module, expression.ty);
                for branch in branches {
                    let mut child = Context {
                        module: self.module,
                        globals: self.globals,
                        locals: self.locals.clone(),
                        errors: self.errors,
                    };
                    child.pattern(&branch.pattern, scrutinee.ty);
                    child.expr(&branch.value, Some(result));
                }
            }
        }
    }

    fn pattern(&mut self, pattern: &Pattern, expected: TypeId) {
        self.compatible(pattern.ty, expected, pattern.span);
        match &pattern.kind {
            PatternKind::Wildcard => {}
            PatternKind::Literal { literal } => {
                let constructor = match literal {
                    crate::PatternLiteral::Integer(_) => TypeConstructor::Int,
                    crate::PatternLiteral::Number(_) => TypeConstructor::Number,
                    crate::PatternLiteral::String(_) => TypeConstructor::String,
                    crate::PatternLiteral::Char(_) => TypeConstructor::Char,
                    crate::PatternLiteral::Boolean(_) => TypeConstructor::Boolean,
                };
                if let Some(expected) = primitive_type(self.module, constructor) {
                    self.compatible(pattern.ty, expected, pattern.span);
                } else {
                    self.error(
                        pattern.span,
                        "primitive type is missing from the THIR type table",
                    );
                }
            }
            PatternKind::Array { elements } => {
                if let Some(element) = array_element(self.module, pattern.ty) {
                    for item in elements {
                        self.pattern(item, element);
                    }
                } else {
                    self.error(pattern.span, "array pattern does not have an Array type");
                }
            }
            PatternKind::Named {
                id,
                pattern: nested,
            } => {
                self.locals.insert(
                    *id,
                    Scheme {
                        ty: pattern.ty,
                        quantified: Vec::new(),
                    },
                );
                self.pattern(nested, pattern.ty);
            }
            PatternKind::Var { id, ty } => {
                self.compatible(*ty, pattern.ty, pattern.span);
                self.locals.insert(
                    *id,
                    Scheme {
                        ty: *ty,
                        quantified: Vec::new(),
                    },
                );
            }
            PatternKind::Constructor { arguments, .. } => {
                for argument in arguments {
                    self.pattern(argument, argument.ty);
                }
            }
            PatternKind::Record { fields } => {
                for (_, field) in fields {
                    self.pattern(field, field.ty);
                }
            }
        }
    }

    fn primitive_shape(&mut self, expression: &Expr, constructor: TypeConstructor) {
        if let Some(expected) = primitive_type(self.module, constructor) {
            self.compatible(expression.ty, expected, expression.span);
        } else {
            self.error(
                expression.span,
                "primitive type is missing from the THIR type table",
            );
        }
    }

    fn compatible(&mut self, actual: TypeId, expected: TypeId, span: psrs_span::TextRange) {
        if !matching::compatible(actual, expected, self.module) {
            self.error(
                span,
                "THIR expression type is inconsistent with its context",
            );
        }
    }

    fn error(&mut self, span: psrs_span::TextRange, message: &'static str) {
        self.errors.push(VerifyError { span, message });
    }
}

fn restore_local(locals: &mut HashMap<LocalId, Scheme>, id: LocalId, previous: Option<Scheme>) {
    if let Some(previous) = previous {
        locals.insert(id, previous);
    } else {
        locals.remove(&id);
    }
}

fn primitive_type(module: &Module, constructor: TypeConstructor) -> Option<TypeId> {
    module
        .types
        .iter()
        .position(|ty| *ty == Type::Constructor(constructor))
        .map(|index| TypeId(index as u32))
}

fn array_element(module: &Module, id: TypeId) -> Option<TypeId> {
    let id = strip_leading_foralls(module, id);
    let Type::Application(head, element) = module.types.get(id.0 as usize)? else {
        return None;
    };
    matches!(
        module.types.get(head.0 as usize),
        Some(Type::Constructor(TypeConstructor::Array))
    )
    .then_some(*element)
}

fn record_field(module: &Module, id: TypeId, label: &str) -> Option<TypeId> {
    crate::record_fields(&module.types, id)?
        .into_iter()
        .find(|(field, _)| field == label)
        .map(|(_, ty)| ty)
}

fn strip_leading_foralls(module: &Module, mut id: TypeId) -> TypeId {
    let mut seen = std::collections::HashSet::new();
    while let Some((_, body)) = crate::forall_parts(&module.types, id) {
        if !seen.insert(id) {
            break;
        }
        id = body;
    }
    id
}
