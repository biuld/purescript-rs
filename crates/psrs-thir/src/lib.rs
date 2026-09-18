use psrs_hir::{ExternalSymbol, LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    I32,
    Boolean,
    Function { parameter: TypeId, result: TypeId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub types: Vec<Type>,
    pub declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
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
    Boolean(bool),
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
            if let Type::Function { parameter, result } = ty {
                verify_type_id(*parameter, self.types.len(), self.span, &mut errors);
                verify_type_id(*result, self.types.len(), self.span, &mut errors);
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
        ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Integer(_) | ExprKind::Boolean(_) => {}
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
            declarations: vec![Declaration {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: "main".into(),
                name_span: TextRange::new(0, 4),
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
