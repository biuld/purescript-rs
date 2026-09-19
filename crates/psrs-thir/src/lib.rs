use psrs_hir::{ExternalSymbol, LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};
use psrs_span::TextRange;

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
        for declaration in &self.declarations {
            verify_type_id(
                declaration.ty,
                self.types.len(),
                declaration.name_span,
                &mut errors,
            );
            verify_expr(&declaration.value, self.types.len(), &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn verify_expr(expression: &Expr, type_count: usize, errors: &mut Vec<VerifyError>) {
    verify_type_id(expression.ty, type_count, expression.span, errors);
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
                verify_expr(element, type_count, errors);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                verify_expr(value, type_count, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            verify_expr(expression, type_count, errors);
            for (_, value) in fields {
                verify_expr(value, type_count, errors);
            }
        }
        ExprKind::FieldAccess { expression, .. } => verify_expr(expression, type_count, errors),
        ExprKind::Application(function, argument) => {
            verify_expr(function, type_count, errors);
            verify_expr(argument, type_count, errors);
        }
        ExprKind::Lambda { binder, body } => {
            verify_type_id(binder.ty, type_count, binder.span, errors);
            verify_expr(body, type_count, errors);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                verify_type_id(binding.binder.ty, type_count, binding.binder.span, errors);
                verify_expr(&binding.value, type_count, errors);
            }
            verify_expr(body, type_count, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, type_count, errors);
            verify_expr(then_branch, type_count, errors);
            verify_expr(else_branch, type_count, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            verify_expr(scrutinee, type_count, errors);
            for branch in branches {
                verify_pattern(&branch.pattern, type_count, errors);
                verify_expr(&branch.value, type_count, errors);
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
mod tests {
    use super::*;

    #[test]
    fn verifier_rejects_invalid_type_references() {
        let module = Module {
            id: ModuleId(0),
            name: "Main".into(),
            externals: Vec::new(),
            types: vec![Type::I32],
            newtype_ids: Vec::new(),
            constructors: Vec::new(),
            declarations: vec![Declaration {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: "main".into(),
                name_span: TextRange::new(0, 4),
                quantified: Vec::new(),
                ty: TypeId(2),
                value: Expr {
                    kind: ExprKind::Integer(1),
                    ty: TypeId(0),
                    span: TextRange::new(7, 8),
                },
                span: TextRange::new(0, 8),
            }],
            span: TextRange::new(0, 8),
        };

        let errors = module.verify().unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].message,
            "type reference is outside the THIR type table"
        );
    }
}
