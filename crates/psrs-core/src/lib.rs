mod lower;

use psrs_hir::{ExternalSymbol, Intrinsic, LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::HashSet;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    Add,
    Sub,
    Mul,
    DivS,
    RemS,
    Eq,
    Ne,
    LtS,
    LeS,
    GtS,
    GeS,
}

impl Primitive {
    fn from_intrinsic(intrinsic: Intrinsic) -> Option<Self> {
        Some(match intrinsic {
            Intrinsic::I32Add => Self::Add,
            Intrinsic::I32Sub => Self::Sub,
            Intrinsic::I32Mul => Self::Mul,
            Intrinsic::I32DivS => Self::DivS,
            Intrinsic::I32RemS => Self::RemS,
            Intrinsic::I32Eq => Self::Eq,
            Intrinsic::I32Ne => Self::Ne,
            Intrinsic::I32LtS => Self::LtS,
            Intrinsic::I32LeS => Self::LeS,
            Intrinsic::I32GtS => Self::GtS,
            Intrinsic::I32GeS => Self::GeS,
            Intrinsic::BoolTrue | Intrinsic::BoolFalse => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(i32),
    Boolean(bool),
    Primitive {
        op: Primitive,
        left: Box<Expr>,
        right: Box<Expr>,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    pub span: TextRange,
    pub message: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerError {
    pub span: TextRange,
    pub message: &'static str,
}

pub fn lower_module(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    lower::lower_module(module)
}

impl Module {
    pub fn verify(&self) -> Result<(), Vec<VerifyError>> {
        let globals = self
            .declarations
            .iter()
            .map(|declaration| declaration.symbol)
            .chain(self.externals.iter().map(|external| external.symbol))
            .collect::<HashSet<_>>();
        let mut errors = Vec::new();
        for declaration in &self.declarations {
            verify_type(declaration.ty, self, declaration.name_span, &mut errors);
            let mut locals = HashSet::new();
            verify_expr(&declaration.value, self, &globals, &mut locals, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn verify_type(id: TypeId, module: &Module, span: TextRange, errors: &mut Vec<VerifyError>) {
    if id.0 as usize >= module.types.len() {
        errors.push(VerifyError {
            span,
            message: "type reference is outside the Core type table",
        });
    }
}

fn verify_expr(
    expression: &Expr,
    module: &Module,
    globals: &HashSet<SymbolId>,
    locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    verify_type(expression.ty, module, expression.span, errors);
    match &expression.kind {
        ExprKind::Local(id) if !locals.contains(id) => errors.push(VerifyError {
            span: expression.span,
            message: "local reference is not in scope",
        }),
        ExprKind::Global(id) if !globals.contains(id) => errors.push(VerifyError {
            span: expression.span,
            message: "global reference is not declared",
        }),
        ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Integer(_) | ExprKind::Boolean(_) => {}
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            verify_expr(left, module, globals, locals, errors);
            verify_expr(right, module, globals, locals, errors);
        }
        ExprKind::Lambda { binder, body } => {
            verify_type(binder.ty, module, binder.span, errors);
            locals.insert(binder.id);
            verify_expr(body, module, globals, locals, errors);
            locals.remove(&binder.id);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                verify_type(binding.binder.ty, module, binding.binder.span, errors);
                locals.insert(binding.binder.id);
            }
            for binding in bindings {
                verify_expr(&binding.value, module, globals, locals, errors);
            }
            verify_expr(body, module, globals, locals, errors);
            for binding in bindings {
                locals.remove(&binding.binder.id);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, module, globals, locals, errors);
            verify_expr(then_branch, module, globals, locals, errors);
            verify_expr(else_branch, module, globals, locals, errors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::{ModuleId, SymbolId};

    #[test]
    fn verifier_rejects_out_of_range_types() {
        let module = Module {
            id: ModuleId(0),
            name: "Main".into(),
            externals: Vec::new(),
            types: vec![Type::I32],
            declarations: vec![Declaration {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: "main".into(),
                name_span: TextRange::new(0, 4),
                ty: TypeId(1),
                value: Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: TextRange::new(7, 8),
                },
                span: TextRange::new(0, 8),
            }],
            span: TextRange::new(0, 8),
        };
        assert_eq!(
            module.verify().unwrap_err()[0].message,
            "type reference is outside the Core type table"
        );
    }
}
