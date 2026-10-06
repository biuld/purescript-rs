pub mod dictionary;
pub mod effect;
mod instantiation;
mod link;
mod locals;
mod lower;
pub mod opt;
mod pattern;
pub mod primitive;
mod records;
pub use instantiation::Instantiation;
mod types;
mod verify;

pub use link::{link, prune_unreachable};
pub use pattern::{Literal, Pattern, PatternKind};
pub use records::{record_row, row_fields};
pub use types::{
    Type, TypeConstructor, TypeId, arrow_parts, closure_parts, forall_parts, scheme_parts,
};

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

/// The checked, synonym-expanded signature of a source foreign value import.
/// Bootstrap intrinsics use registry-owned contracts and do not appear here;
/// explicit primitive bindings retain their source signature until linking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalType {
    pub symbol: SymbolId,
    /// Source module that declared this import. External symbols themselves
    /// live in the reserved intrinsic namespace, so their symbol ID cannot
    /// carry diagnostic origin.
    pub source_module: ModuleId,
    pub ty: TypeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    /// Checked source foreign signatures projected from THIR. Backend binding and
    /// representation lowering must consume these schemes rather than
    /// reconstructing types from raw HIR annotations.
    pub external_types: Vec<ExternalType>,
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
    /// The one `Unit` value. `Unit` is a builtin type with no constructor
    /// table, so the value is a compiler primitive rather than a nullary
    /// constructor application. Its canonical runtime value is the integer `0`
    /// ([scalars and primitives](../../design/backend/fp/scalars-and-primitives.md)).
    Unit,
    /// The one value of the compiler-owned opaque state token an `Effect`
    /// closure takes. Only effect lowering produces it; it is threaded through
    /// the chain and never inspected, the `State# RealWorld` analogue of
    /// [effects](../../design/backend/fp/effects.md). Its runtime shape is a
    /// scalar, but it is not an `Int`.
    StateToken,
    /// An expression that never produces its value: the guest traps. Core
    /// carries it so the effect interface can supply an `Effect Unit` that
    /// escapes instead of returning, which is what an uncaught failure is on
    /// this target. It is a target control-flow edge, not a value, so the
    /// verifier checks its type is the one its context wants and nothing else.
    Trap,
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
    /// A call to a compiler intrinsic, saturated to its descriptor's arity. The
    /// per-intrinsic handling lives in the Core intrinsic module, so Core's
    /// traversals match this one node rather than one variant per operation.
    IntrinsicCall {
        intrinsic: Intrinsic,
        arguments: Vec<Expr>,
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
        verify::module(self, None)
    }

    /// Verifies this physical module while checking declaration instantiation
    /// against `source` whenever both type ids still exist there.
    ///
    /// Representation lowering may replace a source application with a closure
    /// at the same type id. Those relations stay on the immutable source
    /// module. Types allocated only in this module, including synthesized
    /// operation closures, are checked against this module's own type table.
    pub fn verify_with_source(&self, source: &Module) -> Result<(), Vec<VerifyError>> {
        verify::module(self, Some(source))
    }

    /// Checks a declaration use with the same relation as Core verification,
    /// retaining its solved constructor bindings without changing acceptance.
    pub fn checked_instantiation(
        &self,
        scheme: TypeId,
        quantified: &[TypeVariableId],
        instance: TypeId,
    ) -> Option<Instantiation<'_>> {
        verify::instantiation(self, scheme, quantified, instance)
    }

    /// Compares types with the same semantic relation used by Core verification,
    /// including separately interned alpha-equivalent quantified types.
    pub fn types_equivalent(&self, left: TypeId, right: TypeId) -> bool {
        verify::equivalent_types(left, right, self)
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
