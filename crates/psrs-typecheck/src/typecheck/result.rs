//! The inferred result: declarations, expressions, and patterns with their
//! checked types and explicit evidence. This is what inference produces before
//! finalization lowers it into THIR.

use super::*;

/// A declaration whose body has been checked and whose scheme is known.
#[derive(Clone, Debug)]
pub(super) struct InferredDeclaration {
    pub(super) symbol: SymbolId,
    pub(super) name: String,
    pub(super) name_span: TextRange,
    pub(super) scheme: Scheme,
    pub(super) value: InferredExpr,
    pub(super) span: TextRange,
}

/// A bound variable with the scheme it is used at.
#[derive(Clone, Debug)]
pub(super) struct InferredBinder {
    pub(super) binder: LocalBinder,
    pub(super) scheme: Scheme,
}

/// One `let` binding, with its generalized scheme.
#[derive(Clone, Debug)]
pub(super) struct InferredBinding {
    pub(super) binder: InferredBinder,
    pub(super) value: InferredExpr,
    pub(super) span: TextRange,
}

/// A checked expression: what it is, the type it was checked at, and the range
/// it occupies in the source.
#[derive(Clone, Debug)]
pub(super) struct InferredExpr {
    pub(super) kind: InferredExprKind,
    pub(super) ty: InferType,
    pub(super) span: TextRange,
}

/// A checked expression node.
#[derive(Clone, Debug)]
pub(super) enum InferredExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(i32),
    Number(String),
    Boolean(bool),
    String(String),
    Char(char),
    Array(Vec<InferredExpr>),
    Record(Vec<(String, InferredExpr)>),
    RecordUpdate {
        expression: Box<InferredExpr>,
        fields: Vec<(String, InferredExpr)>,
    },
    FieldAccess {
        expression: Box<InferredExpr>,
        field: String,
    },
    /// A class method selected from the dictionary solved for `wanted`.
    Method {
        method: String,
        wanted: usize,
    },
    /// A constrained function applied to the dictionary solved for `wanted`.
    DictionaryApplication {
        function: Box<InferredExpr>,
        wanted: usize,
    },
    /// The `Safe.Coerce.coerce` function. Its type relation is checked before
    /// finalization, which closes this into a typed representation cast.
    CoerceFunction {
        wanted: usize,
        source: InferType,
        target: InferType,
    },
    /// An unchecked conversion with a recorded source or derivation authority.
    /// It has no `Coercible` wanted;
    /// finalization closes it into an unchecked representation cast.
    UnsafeCoerceFunction {
        source: InferType,
        target: InferType,
        origin: thir::UncheckedCoercionOrigin,
    },
    /// A dictionary solved for `wanted`, used directly (for example as an
    /// instance's superclass field).
    Evidence(usize),
    Application(Box<InferredExpr>, Box<InferredExpr>),
    Lambda {
        binder: InferredBinder,
        body: Box<InferredExpr>,
    },
    Let {
        bindings: Vec<InferredBinding>,
        body: Box<InferredExpr>,
    },
    If {
        condition: Box<InferredExpr>,
        then_branch: Box<InferredExpr>,
        else_branch: Box<InferredExpr>,
    },
    Case {
        scrutinee: Box<InferredExpr>,
        branches: Vec<InferredCaseBranch>,
    },
}

/// One `case` branch: the pattern it matched and the value it produced.
#[derive(Clone, Debug)]
pub(super) struct InferredCaseBranch {
    pub(super) pattern: InferredPattern,
    pub(super) value: InferredExpr,
    pub(super) span: TextRange,
    pub(super) coverage: hir::CaseBranchCoverage,
}

/// A checked pattern with the type it binds.
#[derive(Clone, Debug)]
pub(super) struct InferredPattern {
    pub(super) kind: InferredPatternKind,
    pub(super) ty: InferType,
    pub(super) span: TextRange,
}

/// A checked pattern node.
#[derive(Clone, Debug)]
pub(super) enum InferredPatternKind {
    Wildcard,
    Literal {
        literal: thir::PatternLiteral,
    },
    Array {
        elements: Vec<InferredPattern>,
    },
    Named {
        binder: LocalBinder,
        pattern: Box<InferredPattern>,
    },
    Var {
        binder: LocalBinder,
        ty: InferType,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<InferredPattern>,
    },
    Record {
        fields: Vec<(String, InferredPattern)>,
    },
}
