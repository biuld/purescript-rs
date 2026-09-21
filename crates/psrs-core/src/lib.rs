mod link;
mod lower;
mod pattern;
mod verify;

pub use link::{link, prune_unreachable};
pub use pattern::{Pattern, PatternKind};

use psrs_hir::{
    ExternalSymbol, Intrinsic, LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId,
};
use psrs_span::TextRange;

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
    Number(String),
    Boolean(bool),
    String(String),
    Char(char),
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
        verify::module(self)
    }
}

#[cfg(test)]
mod tests;
