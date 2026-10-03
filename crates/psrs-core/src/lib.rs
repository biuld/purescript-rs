pub mod dictionary;
pub mod effect;
mod link;
mod lower;
pub mod opt;
mod pattern;
mod records;
mod types;
mod verify;

pub use link::{link, prune_unreachable};
pub use pattern::{Literal, Pattern, PatternKind};
pub use records::{record_row, row_fields};
pub use types::{Type, TypeConstructor, TypeId, arrow_parts, closure_parts, forall_parts};

use psrs_hir::{
    CaseBranchCoverage, ExternalSymbol, Intrinsic, LocalId, ModuleId, SymbolId,
    TypeId as HirTypeId, TypeVariableId,
};
use psrs_span::TextRange;

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
    /// The declaration's ordered type parameters, as the variables its
    /// `field_types` templates refer to. The variable at `parameters[i]` stands
    /// for the resolved application's argument `i`, letting the ABI instantiate
    /// a parameterized constructor's field templates.
    pub parameters: Vec<TypeVariableId>,
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
    /// Type constructors whose application has a callable closure
    /// representation, registered by the trusted elaboration through their
    /// resolved type identity. Each entry names the number of hidden
    /// calling-convention parameters; the call result is the application's last
    /// type argument. This is representation metadata, not a type.
    pub callable_types: Vec<(HirTypeId, u32)>,
    pub constructors: Vec<ConstructorInfo>,
    pub declarations: Vec<Declaration>,
    /// Qualified names of the type declarations in this module, keyed by their
    /// stable id. Retained from HIR so the backend can recognize well-known
    /// library types after names are otherwise dropped.
    pub type_names: Vec<(HirTypeId, String)>,
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
            | Intrinsic::IntToChar
            | Intrinsic::StringToBytes
            | Intrinsic::BytesToString
            | Intrinsic::Coerce
            | Intrinsic::Undefined => return None,
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
    /// A representational conversion authorized by checked frontend evidence.
    /// The backend applies its normal typed value-conversion protocol.
    RepresentationCast {
        value: Box<Expr>,
        source_type: TypeId,
        target_type: TypeId,
    },
    ArrayLength(Box<Expr>),
    /// A source `String`'s canonical UTF-8 bytes as an `Array Int`. A source
    /// string is a sequence of Unicode scalar values, so the conversion is
    /// lossless ([DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
    StringToBytes(Box<Expr>),
    /// An `Array Int` read as a source `String`. Every element must be a
    /// canonical byte and the bytes must be well-formed UTF-8; either
    /// violation traps instead of producing replacement text.
    BytesToString(Box<Expr>),
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
    pub coverage: CaseBranchCoverage,
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

    /// The hidden calling-convention parameter count registered for a callable
    /// type constructor identity, or `None` when the constructor is not
    /// callable.
    pub fn callable_parameters(&self, type_id: HirTypeId) -> Option<u32> {
        self.callable_types
            .iter()
            .find(|(id, _)| *id == type_id)
            .map(|(_, parameters)| *parameters)
    }

    /// Decomposes an applied type into its head constructor and the arguments
    /// applied to it, in order. A non-constructor head yields `None`.
    pub fn applied_constructor(&self, mut id: TypeId) -> Option<(TypeConstructor, Vec<TypeId>)> {
        let mut arguments = Vec::new();
        while let Some(Type::Application(function, argument)) = self.types.get(id.0 as usize) {
            arguments.push(*argument);
            id = *function;
        }
        arguments.reverse();
        let Some(Type::Constructor(constructor)) = self.types.get(id.0 as usize) else {
            return None;
        };
        Some((*constructor, arguments))
    }

    /// The name and arguments of a callable type-constructor application, when its
    /// head constructor has a registered closure representation.
    pub fn callable_application(&self, id: TypeId) -> Option<(HirTypeId, Vec<TypeId>)> {
        let (constructor, arguments) = self.applied_constructor(id)?;
        let TypeConstructor::User(type_id) = constructor else {
            return None;
        };
        self.callable_parameters(type_id)?;
        Some((type_id, arguments))
    }
}

#[cfg(test)]
mod tests;
