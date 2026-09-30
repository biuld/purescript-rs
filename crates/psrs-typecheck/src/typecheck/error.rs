use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheckErrorKind {
    InvalidHir,
    TypeMismatch,
    OccursCheck,
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

    pub fn error_code(&self) -> Option<&'static str> {
        match self.kind {
            TypeCheckErrorKind::InvalidCoercibleInstanceDeclaration => {
                Some("InvalidCoercibleInstanceDeclaration")
            }
            _ => None,
        }
    }
}
