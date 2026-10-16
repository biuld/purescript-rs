//! Structured link failures. The linker never publishes a partial plan or a
//! composed artifact when any check fails.

use std::fmt;

/// The linking stage that rejected an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStage {
    /// WIT definition loading and world resolution.
    Definitions,
    /// Requirement/provider closure.
    Requirements,
    /// Artifact contract validation.
    Verify,
    /// Memory reservation and instantiation planning.
    Memory,
    /// Component composition and final import closure.
    Compose,
}

impl LinkStage {
    fn label(self) -> &'static str {
        match self {
            LinkStage::Definitions => "definitions",
            LinkStage::Requirements => "requirements",
            LinkStage::Verify => "verify",
            LinkStage::Memory => "memory",
            LinkStage::Compose => "compose",
        }
    }
}

/// One structured link failure carrying the owning subject identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkError {
    pub stage: LinkStage,
    /// The requirement origin, artifact id, or interface the failure names.
    pub subject: Option<String>,
    pub message: String,
}

/// A nonempty collection of link failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkErrors(pub Vec<LinkError>);

impl LinkErrors {
    pub fn new(errors: Vec<LinkError>) -> Self {
        debug_assert!(!errors.is_empty(), "a link failure has at least one cause");
        Self(errors)
    }

    pub fn one(stage: LinkStage, subject: impl Into<String>, message: impl Into<String>) -> Self {
        Self(vec![LinkError {
            stage,
            subject: Some(subject.into()),
            message: message.into(),
        }])
    }

    pub fn plain(stage: LinkStage, message: impl Into<String>) -> Self {
        Self(vec![LinkError {
            stage,
            subject: None,
            message: message.into(),
        }])
    }
}

impl From<LinkError> for LinkErrors {
    fn from(error: LinkError) -> Self {
        Self(vec![error])
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.stage.label())?;
        if let Some(subject) = &self.subject {
            write!(f, "{subject}: ")?;
        }
        write!(f, "{}", self.message)
    }
}

impl fmt::Display for LinkErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for LinkErrors {}
