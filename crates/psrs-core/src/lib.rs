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
    /// A generalized type variable; quantifiers are stored at each binding site.
    Variable(TypeVariableId),
    I32,
    Boolean,
    String,
    Unit,
    Constructor(TypeConstructor),
    Application(TypeId, TypeId),
    Record(Vec<(String, TypeId)>),
    Function {
        parameter: TypeId,
        result: TypeId,
    },
}

/// A data constructor known to the module, mirrored from THIR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorInfo {
    pub symbol: SymbolId,
    pub type_id: HirTypeId,
    pub tag: u32,
    pub field_count: usize,
    pub field_types: Vec<TypeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub types: Vec<Type>,
    /// Nominal newtypes that are represented by their single field below Core.
    /// This is representation metadata, not a change to the source type.
    pub newtype_ids: Vec<HirTypeId>,
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
            Intrinsic::BoolTrue
            | Intrinsic::BoolFalse
            | Intrinsic::ArrayLength
            | Intrinsic::ArrayIndex
            | Intrinsic::ArrayUpdate => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Local(LocalId),
    Global(SymbolId),
    /// A data constructor application; aggregate constructors carry field expressions.
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Expr>,
    },
    Integer(i32),
    Boolean(bool),
    String(String),
    Array {
        elements: Vec<Expr>,
    },
    Record {
        fields: Vec<(String, Expr)>,
    },
    RecordUpdate {
        record: Box<Expr>,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        record: Box<Expr>,
        field: String,
    },
    ArrayLength(Box<Expr>),
    ArrayIndex {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    ArrayUpdate {
        array: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
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
    Var {
        id: LocalId,
        ty: TypeId,
    },
    /// A constructor pattern, with one nested pattern per field.
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pattern>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    /// The source module owning the declaration; linked Core maps diagnostics back to it.
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
            match ty {
                Type::Function { parameter, result } | Type::Application(parameter, result) => {
                    verify_type(*parameter, self, self.id, self.span, &mut errors);
                    verify_type(*result, self, self.id, self.span, &mut errors);
                }
                Type::Record(fields) => {
                    for (_, field) in fields {
                        verify_type(*field, self, self.id, self.span, &mut errors);
                    }
                }
                _ => {}
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
        ExprKind::Array { elements } => {
            for element in elements {
                verify_expr(element, module, owner, globals, locals, errors);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                verify_expr(value, module, owner, globals, locals, errors);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            verify_expr(record, module, owner, globals, locals, errors);
            for (_, value) in fields {
                verify_expr(value, module, owner, globals, locals, errors);
            }
        }
        ExprKind::FieldAccess { record, .. } => {
            verify_expr(record, module, owner, globals, locals, errors);
        }
        ExprKind::ArrayLength(value) => verify_expr(value, module, owner, globals, locals, errors),
        ExprKind::ArrayIndex { array, index } => {
            verify_expr(array, module, owner, globals, locals, errors);
            verify_expr(index, module, owner, globals, locals, errors);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            verify_expr(array, module, owner, globals, locals, errors);
            verify_expr(index, module, owner, globals, locals, errors);
            verify_expr(value, module, owner, globals, locals, errors);
        }
        ExprKind::Constructor { symbol, arguments } => {
            if let Some(constructor) = module
                .constructors
                .iter()
                .find(|constructor| constructor.symbol == *symbol)
            {
                if constructor.field_count != arguments.len() {
                    errors.push(VerifyError {
                        module: owner,
                        span: expression.span,
                        message: "constructor application has the wrong field count",
                    });
                }
            } else {
                errors.push(VerifyError {
                    module: owner,
                    span: expression.span,
                    message: "constructor reference is not declared",
                });
            }
            for argument in arguments {
                verify_expr(argument, module, owner, globals, locals, errors);
            }
        }
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
        PatternKind::Var { id, ty } => {
            verify_type(*ty, module, owner, pattern.span, errors);
            locals.insert(*id);
        }
        PatternKind::Constructor { symbol, arguments } => {
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
            for argument in arguments {
                verify_pattern(argument, module, owner, locals, errors);
            }
        }
    }
}

fn remove_pattern_locals(pattern: &Pattern, locals: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Var { id, .. } => {
            locals.remove(id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                remove_pattern_locals(argument, locals);
            }
        }
        PatternKind::Wildcard => {}
    }
}

#[cfg(test)]
mod tests;
