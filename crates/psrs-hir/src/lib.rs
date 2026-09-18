use psrs_span::TextRange;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModuleId(pub u32);

impl ModuleId {
    pub const INTRINSICS: Self = Self(u32::MAX);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolId {
    pub module: ModuleId,
    pub index: u32,
}

impl SymbolId {
    pub const fn new(module: ModuleId, index: u32) -> Self {
        Self { module, index }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Intrinsic {
    BoolTrue,
    BoolFalse,
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32RemS,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LeS,
    I32GtS,
    I32GeS,
}

impl Intrinsic {
    pub const fn symbol(self) -> SymbolId {
        SymbolId::new(ModuleId::INTRINSICS, self as u32)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalSymbol {
    pub symbol: SymbolId,
    pub name: String,
    pub intrinsic: Intrinsic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBinder {
    pub id: LocalId,
    pub name: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBinding {
    pub binder: LocalBinder,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(String),
    String(String),
    Char(char),
    Application(Box<Expr>, Box<Expr>),
    Operator {
        operator: SymbolId,
        operator_span: TextRange,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Lambda {
        binder: LocalBinder,
        body: Box<Expr>,
    },
    Let {
        bindings: Vec<LocalBinding>,
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
    /// Checks that every reference targets a declaration visible in this module.
    pub fn verify(&self) -> Result<(), Vec<VerifyError>> {
        let mut errors = Vec::new();
        let mut globals = HashSet::new();
        if self.id == ModuleId::INTRINSICS {
            errors.push(VerifyError {
                span: self.span,
                message: "module uses the reserved intrinsic module ID",
            });
        }
        for declaration in &self.declarations {
            if declaration.symbol.module != self.id {
                errors.push(VerifyError {
                    span: declaration.name_span,
                    message: "declaration symbol belongs to a different module",
                });
            }
            if !globals.insert(declaration.symbol) {
                errors.push(VerifyError {
                    span: declaration.name_span,
                    message: "duplicate declaration symbol ID",
                });
            }
        }
        for external in &self.externals {
            if external.symbol.module != ModuleId::INTRINSICS {
                errors.push(VerifyError {
                    span: self.span,
                    message: "external symbol does not use the intrinsic module ID",
                });
            }
            if !globals.insert(external.symbol) {
                errors.push(VerifyError {
                    span: self.span,
                    message: "duplicate global symbol ID",
                });
            }
        }

        let mut declared_locals = HashSet::new();
        for declaration in &self.declarations {
            let mut visible_locals = HashSet::new();
            verify_expr(
                &declaration.value,
                &globals,
                &mut visible_locals,
                &mut declared_locals,
                &mut errors,
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn verify_expr(
    expression: &Expr,
    globals: &HashSet<SymbolId>,
    visible_locals: &mut HashSet<LocalId>,
    declared_locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    match &expression.kind {
        ExprKind::Local(id) if !visible_locals.contains(id) => errors.push(VerifyError {
            span: expression.span,
            message: "local reference is not in scope",
        }),
        ExprKind::Global(id) if !globals.contains(id) => errors.push(VerifyError {
            span: expression.span,
            message: "global reference does not name a module declaration",
        }),
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        ExprKind::Application(function, argument) => {
            verify_expr(function, globals, visible_locals, declared_locals, errors);
            verify_expr(argument, globals, visible_locals, declared_locals, errors);
        }
        ExprKind::Operator {
            operator,
            left,
            right,
            ..
        } => {
            if !globals.contains(operator) {
                errors.push(VerifyError {
                    span: expression.span,
                    message: "operator symbol is not declared in the module or intrinsic set",
                });
            }
            verify_expr(left, globals, visible_locals, declared_locals, errors);
            verify_expr(right, globals, visible_locals, declared_locals, errors);
        }
        ExprKind::Lambda { binder, body } => {
            if !declared_locals.insert(binder.id) {
                errors.push(VerifyError {
                    span: binder.span,
                    message: "duplicate local ID",
                });
            }
            let inserted = visible_locals.insert(binder.id);
            verify_expr(body, globals, visible_locals, declared_locals, errors);
            if inserted {
                visible_locals.remove(&binder.id);
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut inserted = Vec::new();
            for binding in bindings {
                if !declared_locals.insert(binding.binder.id) {
                    errors.push(VerifyError {
                        span: binding.binder.span,
                        message: "duplicate local ID",
                    });
                }
                if visible_locals.insert(binding.binder.id) {
                    inserted.push(binding.binder.id);
                }
            }
            for binding in bindings {
                verify_expr(
                    &binding.value,
                    globals,
                    visible_locals,
                    declared_locals,
                    errors,
                );
            }
            verify_expr(body, globals, visible_locals, declared_locals, errors);
            for id in inserted {
                visible_locals.remove(&id);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, globals, visible_locals, declared_locals, errors);
            verify_expr(
                then_branch,
                globals,
                visible_locals,
                declared_locals,
                errors,
            );
            verify_expr(
                else_branch,
                globals,
                visible_locals,
                declared_locals,
                errors,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_rejects_references_to_out_of_scope_locals() {
        let module_id = ModuleId(0);
        let module = Module {
            id: module_id,
            name: "Main".into(),
            externals: Vec::new(),
            declarations: vec![Declaration {
                symbol: SymbolId::new(module_id, 0),
                name: "main".into(),
                name_span: TextRange::new(0, 4),
                value: Expr {
                    kind: ExprKind::Local(LocalId(9)),
                    span: TextRange::new(7, 8),
                },
                span: TextRange::new(0, 8),
            }],
            span: TextRange::new(0, 8),
        };

        let errors = module.verify().unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "local reference is not in scope");
    }
}
