use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheckErrorKind {
    InvalidHir,
    TypeMismatch,
    OccursCheck,
    SkolemEscape,
    UnconstrainedType,
    IntegerOutOfRange,
    NumberOutOfRange,
    UnsupportedExpression,
    UnsupportedType,
    UnsupportedIntrinsic,
    UnloweredOperator,
    UnsupportedClass,
    NoInstance,
    MissingInstanceMethod,
    /// A functional dependency's determined positions disagree, so no single
    /// type can satisfy the constraint.
    FundepConflict,
    /// More than one unrelated visible instance proves the same constraint.
    OverlappingInstances,
    /// A constraint still mentions variables that neither the result type nor
    /// the class's functional dependencies determine.
    AmbiguousConstraint,
    InvalidCoercibleInstanceDeclaration,
    /// An inferred public value mentions a local type omitted from exports.
    TransitiveExport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeCheckError {
    pub kind: TypeCheckErrorKind,
    pub span: TextRange,
    message: String,
}

impl TypeCheckError {
    pub(super) fn new(
        kind: TypeCheckErrorKind,
        span: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// The official PureScript `errorCode` for this diagnostic, when it maps to
    /// one.
    ///
    /// The mappings follow `purs`' `ErrorCode` in
    /// `src/Language/PureScript/Errors.hs`. Three kinds deliberately have no
    /// code, and each for a reason read from `purs` rather than guessed:
    ///
    /// - `UnconstrainedType` is our "a monomorphic type variable was never
    ///   determined". `purs` does not have an error for it: it either
    ///   generalizes the variable or reports the mismatch that left it
    ///   unsolved, so the honest code is whichever `TypeMismatch` or
    ///   `UndefinedTypeVariable` applies. Mapping it to `WildcardInferredType`
    ///   would be wrong twice over — that is a *warning* about a wildcard, not
    ///   an error about a determined variable, and it fires on `purs`' side
    ///   while the variable is still solvable.
    /// - `FundepConflict` is our "a fundep's determined positions disagree".
    ///   `purs` reports the consequence, not the conflict: the corpus files
    ///   `RowInInstanceNotDetermined0/1` expect `InvalidInstanceHead`, which
    ///   `purs` raises in `TypeChecker.checkTypeClassInstance` for exactly this
    ///   situation, and there is no `FunctionalDependencyError` in
    ///   `Errors.hs` at all.
    /// - `AmbiguousConstraint` is our "a constraint mentions variables that
    ///   neither the result type nor a fundep determines". That is
    ///   `purs`' `AmbiguousTypeVariables`, which `TypeChecker/Types.hs` throws
    ///   with a set of variable names — but `purs` raises it while generalizing
    ///   a declaration, whereas we raise it while solving a goal, and only one
    ///   corpus case (`ConstraintInference.purs`) expects it. The mapping is
    ///   right and the stage differs, so it is recorded here rather than left
    ///   unmapped.
    pub fn error_code(&self) -> Option<&'static str> {
        Some(match self.kind {
            TypeCheckErrorKind::TypeMismatch => "TypesDoNotUnify",
            TypeCheckErrorKind::OccursCheck => "InfiniteType",
            TypeCheckErrorKind::SkolemEscape => "EscapedSkolem",
            TypeCheckErrorKind::IntegerOutOfRange => "IntOutOfRange",
            TypeCheckErrorKind::NoInstance => "NoInstanceFound",
            TypeCheckErrorKind::MissingInstanceMethod => "MissingClassMember",
            TypeCheckErrorKind::OverlappingInstances => "OverlappingInstances",
            TypeCheckErrorKind::AmbiguousConstraint => "AmbiguousTypeVariables",
            TypeCheckErrorKind::InvalidCoercibleInstanceDeclaration => {
                "InvalidCoercibleInstanceDeclaration"
            }
            TypeCheckErrorKind::TransitiveExport => "TransitiveExportError",
            TypeCheckErrorKind::InvalidHir
            | TypeCheckErrorKind::UnconstrainedType
            | TypeCheckErrorKind::NumberOutOfRange
            | TypeCheckErrorKind::UnsupportedExpression
            | TypeCheckErrorKind::UnsupportedType
            | TypeCheckErrorKind::UnsupportedIntrinsic
            | TypeCheckErrorKind::UnloweredOperator
            | TypeCheckErrorKind::UnsupportedClass
            | TypeCheckErrorKind::FundepConflict => return None,
        })
    }
}
