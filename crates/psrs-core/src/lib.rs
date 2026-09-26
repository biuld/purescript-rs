pub mod dictionary;
mod link;
mod lower;
pub mod opt;
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
    /// Source constructor name, retained for external enum mapping.
    pub name: String,
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
    /// Foreign data declarations. The type node stays
    /// `Constructor(User(HirTypeId))`; this set records that the type is opaque
    /// and has no constructors. It is not a calling-convention layout.
    pub opaque_ids: Vec<HirTypeId>,
    pub constructors: Vec<ConstructorInfo>,
    pub declarations: Vec<Declaration>,
    /// The declaration used as the program entry point, if one was selected.
    /// The backend lowers this symbol rather than inferring identity from a
    /// source name. See `docs/design/backend/wasm/encoding-and-structuring.md`.
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
pub enum UnaryPrimitive {
    IntNeg,
    IntComplement,
    NumberNeg,
    BooleanNot,
    IntToNumber,
    NumberToInt,
    BooleanToInt,
    IntToBoolean,
    CharToInt,
    IntToChar,
}

impl UnaryPrimitive {
    fn from_intrinsic(intrinsic: Intrinsic) -> Option<Self> {
        Some(match intrinsic {
            Intrinsic::IntNeg => Self::IntNeg,
            Intrinsic::IntComplement => Self::IntComplement,
            Intrinsic::NumberNeg => Self::NumberNeg,
            Intrinsic::BooleanNot => Self::BooleanNot,
            Intrinsic::IntToNumber => Self::IntToNumber,
            Intrinsic::NumberToInt => Self::NumberToInt,
            Intrinsic::BooleanToInt => Self::BooleanToInt,
            Intrinsic::IntToBoolean => Self::IntToBoolean,
            Intrinsic::CharToInt => Self::CharToInt,
            Intrinsic::IntToChar => Self::IntToChar,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    IntAdd,
    IntSub,
    IntMul,
    IntQuot,
    IntRem,
    IntDiv,
    IntMod,
    IntAnd,
    IntOr,
    IntXor,
    IntShl,
    IntShr,
    IntZshr,
    IntEq,
    IntNe,
    IntLt,
    IntLe,
    IntGt,
    IntGe,
    NumberAdd,
    NumberSub,
    NumberMul,
    NumberDiv,
    NumberEq,
    NumberNe,
    NumberLt,
    NumberLe,
    NumberGt,
    NumberGe,
    BooleanAnd,
    BooleanOr,
    BooleanEq,
    BooleanNe,
    CharEq,
    CharNe,
    CharLt,
    CharLe,
    CharGt,
    CharGe,
}

impl Primitive {
    fn from_intrinsic(intrinsic: Intrinsic) -> Option<Self> {
        Some(match intrinsic {
            Intrinsic::I32Add => Self::IntAdd,
            Intrinsic::I32Sub => Self::IntSub,
            Intrinsic::I32Mul => Self::IntMul,
            Intrinsic::I32DivS => Self::IntQuot,
            Intrinsic::I32RemS => Self::IntRem,
            Intrinsic::I32Eq => Self::IntEq,
            Intrinsic::I32Ne => Self::IntNe,
            Intrinsic::I32LtS => Self::IntLt,
            Intrinsic::I32LeS => Self::IntLe,
            Intrinsic::I32GtS => Self::IntGt,
            Intrinsic::I32GeS => Self::IntGe,
            Intrinsic::IntDiv => Self::IntDiv,
            Intrinsic::IntMod => Self::IntMod,
            Intrinsic::IntAnd => Self::IntAnd,
            Intrinsic::IntOr => Self::IntOr,
            Intrinsic::IntXor => Self::IntXor,
            Intrinsic::IntShl => Self::IntShl,
            Intrinsic::IntShr => Self::IntShr,
            Intrinsic::IntZshr => Self::IntZshr,
            Intrinsic::NumberAdd => Self::NumberAdd,
            Intrinsic::NumberSub => Self::NumberSub,
            Intrinsic::NumberMul => Self::NumberMul,
            Intrinsic::NumberDiv => Self::NumberDiv,
            Intrinsic::NumberEq => Self::NumberEq,
            Intrinsic::NumberNe => Self::NumberNe,
            Intrinsic::NumberLt => Self::NumberLt,
            Intrinsic::NumberLe => Self::NumberLe,
            Intrinsic::NumberGt => Self::NumberGt,
            Intrinsic::NumberGe => Self::NumberGe,
            Intrinsic::BooleanAnd => Self::BooleanAnd,
            Intrinsic::BooleanOr => Self::BooleanOr,
            Intrinsic::BooleanEq => Self::BooleanEq,
            Intrinsic::BooleanNe => Self::BooleanNe,
            Intrinsic::CharEq => Self::CharEq,
            Intrinsic::CharNe => Self::CharNe,
            Intrinsic::CharLt => Self::CharLt,
            Intrinsic::CharLe => Self::CharLe,
            Intrinsic::CharGt => Self::CharGt,
            Intrinsic::CharGe => Self::CharGe,
            Intrinsic::BoolTrue
            | Intrinsic::BoolFalse
            | Intrinsic::ArrayLength
            | Intrinsic::ArrayIndex
            | Intrinsic::ArrayUpdate
            | Intrinsic::IntNeg
            | Intrinsic::IntComplement
            | Intrinsic::NumberNeg
            | Intrinsic::BooleanNot
            | Intrinsic::IntToNumber
            | Intrinsic::NumberToInt
            | Intrinsic::BooleanToInt
            | Intrinsic::IntToBoolean
            | Intrinsic::CharToInt
            | Intrinsic::IntToChar => return None,
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
    UnaryPrimitive {
        op: UnaryPrimitive,
        value: Box<Expr>,
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
