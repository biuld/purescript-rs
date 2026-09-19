mod link;
mod lower;

pub use link::{link, prune_unreachable};

use psrs_hir::{
    ExternalSymbol, Intrinsic, LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId,
};
use psrs_span::TextRange;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// A type constructor reference, mirrored from THIR.
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
    Boolean,
    String,
    Unit,
    Constructor(TypeConstructor),
    Application(TypeId, TypeId),
    Function {
        parameter: TypeId,
        result: TypeId,
    },
}

/// A data constructor known to the module, mirrored from THIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructorInfo {
    pub symbol: SymbolId,
    pub type_id: HirTypeId,
    pub tag: u32,
    pub field_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub types: Vec<Type>,
    pub constructors: Vec<ConstructorInfo>,
    pub declarations: Vec<Declaration>,
    /// The declaration used as the program entry point, if one was selected.
    /// The backend lowers this symbol rather than inferring identity from a
    /// source name. See `docs/design/D-02-wasm-lowering.md`.
    pub entry: Option<SymbolId>,
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
    /// A nullary data constructor value; its tag identifies it at runtime.
    Constructor(SymbolId),
    Integer(i32),
    Boolean(bool),
    String(String),
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
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Var(LocalId),
    /// A nullary constructor pattern, tagged by its value symbol.
    Constructor(SymbolId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    /// The source module that owns the declaration being verified. Linked Core
    /// keeps declaration symbols stable, so diagnostics can be mapped back to
    /// the original program input instead of defaulting to source zero.
    pub module: ModuleId,
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

/// Lowers a module without verifying the result, for a module that will be
/// linked with others. Verify the linked module instead.
pub fn lower_module_unverified(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    lower::lower_module_unverified(module)
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
        for ty in &self.types {
            if let Type::Function { parameter, result } | Type::Application(parameter, result) = ty
            {
                verify_type(*parameter, self, self.id, self.span, &mut errors);
                verify_type(*result, self, self.id, self.span, &mut errors);
            }
        }
        for declaration in &self.declarations {
            let owner = declaration.symbol.module;
            verify_type(
                declaration.ty,
                self,
                owner,
                declaration.name_span,
                &mut errors,
            );
            let mut locals = HashSet::new();
            verify_expr(
                &declaration.value,
                self,
                owner,
                &globals,
                &mut locals,
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

fn verify_type(
    id: TypeId,
    module: &Module,
    owner: ModuleId,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    if id.0 as usize >= module.types.len() {
        errors.push(VerifyError {
            module: owner,
            span,
            message: "type reference is outside the Core type table",
        });
    }
}

fn verify_expr(
    expression: &Expr,
    module: &Module,
    owner: ModuleId,
    globals: &HashSet<SymbolId>,
    locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    verify_type(expression.ty, module, owner, expression.span, errors);
    match &expression.kind {
        ExprKind::Local(id) if !locals.contains(id) => errors.push(VerifyError {
            module: owner,
            span: expression.span,
            message: "local reference is not in scope",
        }),
        ExprKind::Global(id) if !globals.contains(id) => errors.push(VerifyError {
            module: owner,
            span: expression.span,
            message: "global reference is not declared",
        }),
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_) => {}
        ExprKind::Constructor(symbol)
            if !module
                .constructors
                .iter()
                .any(|constructor| constructor.symbol == *symbol) =>
        {
            errors.push(VerifyError {
                module: owner,
                span: expression.span,
                message: "constructor reference is not declared",
            });
        }
        ExprKind::Constructor(_) => {}
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            verify_expr(left, module, owner, globals, locals, errors);
            verify_expr(right, module, owner, globals, locals, errors);
        }
        ExprKind::Lambda { binder, body } => {
            verify_type(binder.ty, module, owner, binder.span, errors);
            locals.insert(binder.id);
            verify_expr(body, module, owner, globals, locals, errors);
            locals.remove(&binder.id);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                verify_type(
                    binding.binder.ty,
                    module,
                    owner,
                    binding.binder.span,
                    errors,
                );
                locals.insert(binding.binder.id);
            }
            for binding in bindings {
                verify_expr(&binding.value, module, owner, globals, locals, errors);
            }
            verify_expr(body, module, owner, globals, locals, errors);
            for binding in bindings {
                locals.remove(&binding.binder.id);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, module, owner, globals, locals, errors);
            verify_expr(then_branch, module, owner, globals, locals, errors);
            verify_expr(else_branch, module, owner, globals, locals, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            verify_expr(scrutinee, module, owner, globals, locals, errors);
            for branch in branches {
                verify_pattern(&branch.pattern, module, owner, locals, errors);
                verify_expr(&branch.value, module, owner, globals, locals, errors);
                remove_pattern_locals(&branch.pattern, locals);
            }
        }
    }
}

fn verify_pattern(
    pattern: &Pattern,
    module: &Module,
    owner: ModuleId,
    locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var(id) => {
            locals.insert(*id);
        }
        PatternKind::Constructor(symbol) => {
            if !module
                .constructors
                .iter()
                .any(|constructor| constructor.symbol == *symbol)
            {
                errors.push(VerifyError {
                    module: owner,
                    span: pattern.span,
                    message: "pattern constructor is not declared",
                });
            }
        }
    }
}

fn remove_pattern_locals(pattern: &Pattern, locals: &mut HashSet<LocalId>) {
    if let PatternKind::Var(id) = &pattern.kind {
        locals.remove(id);
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
            constructors: Vec::new(),
            declarations: vec![Declaration {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: "main".into(),
                name_span: TextRange::new(0, 4),
                quantified: Vec::new(),
                ty: TypeId(1),
                value: Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: TextRange::new(7, 8),
                },
                span: TextRange::new(0, 8),
            }],
            entry: None,
            span: TextRange::new(0, 8),
        };
        assert_eq!(
            module.verify().unwrap_err()[0].message,
            "type reference is outside the Core type table"
        );
    }

    #[test]
    fn verifier_attributes_declaration_errors_to_their_source_module() {
        let owner = ModuleId(7);
        let module = Module {
            id: ModuleId(0),
            name: "Linked".into(),
            externals: Vec::new(),
            types: vec![Type::I32],
            constructors: Vec::new(),
            declarations: vec![Declaration {
                symbol: SymbolId::new(owner, 0),
                name: "broken".into(),
                name_span: TextRange::new(0, 6),
                quantified: Vec::new(),
                ty: TypeId(0),
                value: Expr {
                    kind: ExprKind::Global(SymbolId::new(owner, 99)),
                    ty: TypeId(0),
                    span: TextRange::new(9, 15),
                },
                span: TextRange::new(0, 15),
            }],
            entry: None,
            span: TextRange::new(0, 15),
        };
        let error = module.verify().unwrap_err().remove(0);
        assert_eq!(error.module, owner);
        assert_eq!(error.message, "global reference is not declared");
    }
}
