use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheckErrorKind {
    InvalidHir,
    TypeMismatch,
    /// A binding would give a type a kind its recorded kind does not admit, so
    /// the kind equation `bind_type_variable` solves has no solution.
    KindsDoNotUnify,
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
    /// A recursive declaration has constraints it cannot discharge, and
    /// generalizing them would admit polymorphic recursion over a constraint the
    /// recursive uses never proved. `purs` raises this in
    /// `TypeChecker.Types.typesOf`, where it is the counterpart of retaining a
    /// non-recursive group's residual constraints.
    CannotGeneralizeRecursiveFunction,
    InvalidCoercibleInstanceDeclaration,
    /// An instance head the solver cannot match against, such as one that
    /// contains a type wildcard. `purs` raises this in
    /// `TypeChecker.checkTypeClassInstance`; see `failing/TypeWildcards3.purs`.
    InvalidInstanceHead,
    /// An inferred public value mentions a local type omitted from exports.
    TransitiveExport,
    /// `e @T` where `e`'s type has no quantifier left to apply `T` to.
    /// `purs` raises this in `TypeChecker.Types.infer'` for
    /// `VisibleTypeApp`, where the operand's type is reported against the
    /// written argument.
    CannotApplyExpressionOfTypeOnType,
    /// `e @_` where `e`'s type has no quantifier left to skip.
    CannotSkipTypeApplication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeCheckError {
    pub kind: TypeCheckErrorKind,
    pub span: TextRange,
    message: String,
}

/// A warning produced while checking a module. Warnings are kept separate from
/// errors so a successful typecheck can return both its checked module and the
/// reports that callers must surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeCheckWarning {
    pub span: TextRange,
    pub message: String,
}

impl TypeCheckWarning {
    /// The official PureScript `errorCode` for a user-defined warning.
    pub fn error_code(&self) -> &'static str {
        "UserDefinedWarning"
    }
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
    pub fn error_code(&self) -> Option<&'static str> {
        self.kind.error_code()
    }
}

impl TypeCheckErrorKind {
    /// Whether this error already reports why one wanted constraint failed to
    /// resolve. Callers must not add a second `NoInstanceFound` at an enclosing
    /// instance or deferral boundary.
    pub(in crate::typecheck) fn reports_constraint_failure(self) -> bool {
        matches!(
            self,
            TypeCheckErrorKind::NoInstance
                | TypeCheckErrorKind::OverlappingInstances
                | TypeCheckErrorKind::TypeMismatch
        )
    }

    /// The official PureScript `errorCode` this kind raises, when it maps to one.
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
        Some(match self {
            TypeCheckErrorKind::TypeMismatch => "TypesDoNotUnify",
            TypeCheckErrorKind::KindsDoNotUnify => "KindsDoNotUnify",
            TypeCheckErrorKind::OccursCheck => "InfiniteType",
            TypeCheckErrorKind::SkolemEscape => "EscapedSkolem",
            TypeCheckErrorKind::IntegerOutOfRange => "IntOutOfRange",
            TypeCheckErrorKind::NoInstance => "NoInstanceFound",
            TypeCheckErrorKind::MissingInstanceMethod => "MissingClassMember",
            TypeCheckErrorKind::OverlappingInstances => "OverlappingInstances",
            TypeCheckErrorKind::AmbiguousConstraint => "AmbiguousTypeVariables",
            TypeCheckErrorKind::CannotGeneralizeRecursiveFunction => {
                "CannotGeneralizeRecursiveFunction"
            }
            TypeCheckErrorKind::InvalidCoercibleInstanceDeclaration => {
                "InvalidCoercibleInstanceDeclaration"
            }
            TypeCheckErrorKind::InvalidInstanceHead => "InvalidInstanceHead",
            TypeCheckErrorKind::TransitiveExport => "TransitiveExportError",
            TypeCheckErrorKind::CannotApplyExpressionOfTypeOnType => {
                "CannotApplyExpressionOfTypeOnType"
            }
            TypeCheckErrorKind::CannotSkipTypeApplication => "CannotSkipTypeApplication",
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
