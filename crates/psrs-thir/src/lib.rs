use psrs_hir::{ExternalSymbol, LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};
use psrs_span::TextRange;

mod evidence;

pub use evidence::{Evidence, EvidenceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// A type constructor reference. `Array` is the only built-in constructor the
/// current type system elaborates; user constructors are identified by their
/// resolved HIR declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeConstructor {
    Array,
    User(HirTypeId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// A generalized type variable. See [`Declaration::quantified`] and
    /// [`Binding::quantified`] for the variables bound at each site.
    Variable(TypeVariableId),
    I32,
    F64,
    Boolean,
    String,
    Char,
    Unit,
    Constructor(TypeConstructor),
    Application(TypeId, TypeId),
    Record(Vec<(String, TypeId)>),
    Function {
        parameter: TypeId,
        result: TypeId,
    },
}

/// A data constructor known to the module. `tag` is its zero-based position in
/// the declaration; `field_count` is the number of fields it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorInfo {
    pub symbol: SymbolId,
    /// Source constructor name, retained for diagnostics and external type mapping.
    pub name: String,
    pub type_id: HirTypeId,
    pub tag: u32,
    pub field_count: usize,
    /// The elaborated field types, in constructor order. Keeping these in
    /// THIR lets later representations choose a runtime layout without
    /// consulting HIR again.
    pub field_types: Vec<TypeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub types: Vec<Type>,
    /// Nominal types whose single constructor is erased at runtime. The type
    /// checker keeps these types distinct; later lowering uses this metadata
    /// to pass their field value through without allocating a wrapper.
    pub newtype_ids: Vec<HirTypeId>,
    /// Foreign data declarations. Their type node is still
    /// `Constructor(User(id))`; this set, with an empty constructor list, is
    /// what keeps the type opaque. It is not a runtime layout.
    pub opaque_ids: Vec<HirTypeId>,
    pub constructors: Vec<ConstructorInfo>,
    pub declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub quantified: Vec<TypeVariableId>,
    pub ty: TypeId,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binder {
    pub id: LocalId,
    pub name: String,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub binder: Binder,
    pub quantified: Vec<TypeVariableId>,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(i32),
    Number(String),
    Boolean(bool),
    String(String),
    Char(char),
    Array(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    RecordUpdate {
        expression: Box<Expr>,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        expression: Box<Expr>,
        field: String,
    },
    /// Selected class evidence. Core lowering erases this to ordinary values,
    /// applications, and record projections.
    Evidence(Evidence),
    Application(Box<Expr>, Box<Expr>),
    Lambda {
        binder: Binder,
        body: Box<Expr>,
    },
    Let {
        bindings: Vec<Binding>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Case {
        scrutinee: Box<Expr>,
        branches: Vec<CaseBranch>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseBranch {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Var {
        id: LocalId,
        ty: TypeId,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pattern>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    pub span: TextRange,
    pub message: &'static str,
}

impl Module {
    pub fn verify(&self) -> Result<(), Vec<VerifyError>> {
        let mut errors = Vec::new();
        for ty in &self.types {
            match ty {
                Type::Function { parameter, result } | Type::Application(parameter, result) => {
                    verify_type_id(*parameter, self.types.len(), self.span, &mut errors);
                    verify_type_id(*result, self.types.len(), self.span, &mut errors);
                }
                Type::Record(fields) => {
                    for (_, field) in fields {
                        verify_type_id(*field, self.types.len(), self.span, &mut errors);
                    }
                }
                _ => {}
            }
        }
        for constructor in &self.constructors {
            if self.opaque_ids.contains(&constructor.type_id) {
                errors.push(VerifyError {
                    span: self.span,
                    message: "an opaque type has no constructors",
                });
            }
        }
        for declaration in &self.declarations {
            verify_type_id(
                declaration.ty,
                self.types.len(),
                declaration.name_span,
                &mut errors,
            );
            verify_expr(&declaration.value, &self.types, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn verify_expr(expression: &Expr, types: &[Type], errors: &mut Vec<VerifyError>) {
    verify_type_id(expression.ty, types.len(), expression.span, errors);
    match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        ExprKind::Array(elements) => {
            for element in elements {
                verify_expr(element, types, errors);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                verify_expr(value, types, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            verify_expr(expression, types, errors);
            for (_, value) in fields {
                verify_expr(value, types, errors);
            }
        }
        ExprKind::FieldAccess { expression, .. } => verify_expr(expression, types, errors),
        ExprKind::Evidence(evidence) => verify_evidence(evidence, types, errors),
        ExprKind::Application(function, argument) => {
            verify_expr(function, types, errors);
            verify_expr(argument, types, errors);
        }
        ExprKind::Lambda { binder, body } => {
            verify_type_id(binder.ty, types.len(), binder.span, errors);
            verify_expr(body, types, errors);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                verify_type_id(binding.binder.ty, types.len(), binding.binder.span, errors);
                verify_expr(&binding.value, types, errors);
            }
            verify_expr(body, types, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, types, errors);
            verify_expr(then_branch, types, errors);
            verify_expr(else_branch, types, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            verify_expr(scrutinee, types, errors);
            for branch in branches {
                verify_pattern(&branch.pattern, types.len(), errors);
                verify_expr(&branch.value, types, errors);
            }
        }
    }
}

fn verify_evidence(evidence: &Evidence, types: &[Type], errors: &mut Vec<VerifyError>) {
    verify_type_id(evidence.ty, types.len(), evidence.span, errors);
    match &evidence.kind {
        EvidenceKind::Given(_) | EvidenceKind::Global(_) => {}
        EvidenceKind::Superclass { parent, field } => {
            verify_evidence(parent, types, errors);
            let Some(Type::Record(fields)) = types.get(parent.ty.0 as usize) else {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "superclass evidence parent is not a dictionary record",
                });
                return;
            };
            match fields.iter().find(|(label, _)| label == field) {
                Some((_, field_ty)) if *field_ty == evidence.ty => {}
                _ => errors.push(VerifyError {
                    span: evidence.span,
                    message: "superclass evidence field has the wrong type",
                }),
            }
        }
        EvidenceKind::Instance {
            constructor_type,
            context,
            ..
        } => {
            verify_type_id(*constructor_type, types.len(), evidence.span, errors);
            let mut result = *constructor_type;
            for argument in context {
                verify_evidence(argument, types, errors);
                let Some(Type::Function {
                    parameter,
                    result: next,
                }) = types.get(result.0 as usize)
                else {
                    errors.push(VerifyError {
                        span: argument.span,
                        message: "instance dictionary constructor takes too few context arguments",
                    });
                    return;
                };
                if *parameter != argument.ty {
                    errors.push(VerifyError {
                        span: argument.span,
                        message: "instance evidence does not match its context parameter",
                    });
                }
                result = *next;
            }
            if result != evidence.ty {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "instance evidence result has the wrong dictionary type",
                });
            }
        }
    }
}

fn verify_pattern(pattern: &Pattern, type_count: usize, errors: &mut Vec<VerifyError>) {
    verify_type_id(pattern.ty, type_count, pattern.span, errors);
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var { ty, .. } => verify_type_id(*ty, type_count, pattern.span, errors),
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                verify_pattern(argument, type_count, errors);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                verify_pattern(field, type_count, errors);
            }
        }
    }
}

fn verify_type_id(id: TypeId, type_count: usize, span: TextRange, errors: &mut Vec<VerifyError>) {
    if id.0 as usize >= type_count {
        errors.push(VerifyError {
            span,
            message: "type reference is outside the THIR type table",
        });
    }
}

#[cfg(test)]
mod tests;
