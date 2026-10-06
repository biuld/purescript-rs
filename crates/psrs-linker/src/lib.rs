//! Independent checked target linker.
//!
//! This crate owns WIT definition loading and the resolved-world identity,
//! provider closure and artifact contract validation, the immutable checked
//! link plan, and component composition. It consumes linker-owned target
//! records and the `psrs-runtime` catalog; it depends on no compiler IR crate.
//!
//! The conceptual API is:
//!
//! ```text
//! resolve_definitions(...) -> ResolvedWorldContext
//! plan(context, TargetLinkInput) -> CheckedLinkPlan
//! compose(context, plan, encoded_application) -> LinkedArtifact
//! ```

mod compose;
mod definitions;
mod digest;
mod error;
pub mod plan;
pub mod runtime;
mod stack;
mod target;
mod verify;

pub use compose::{LinkedArtifact, compose};
pub use definitions::{ResolvedWorldContext, resolve_default_definitions, resolve_definitions};
pub use digest::sha256_hex;
pub use error::{LinkError, LinkErrors, LinkStage};
pub use plan::{CheckedLinkPlan, MemoryPlan, ResolvedBinding, plan};
pub use stack::{StackBound, measure_stack_bound};
pub use target::{
    ArtifactContract, ArtifactKind, ArtifactReference, BindingRequirement, Boundary, CoreSignature,
    CoreType, DeclaredExport, DeclaredGlobal, DeclaredImport, DeclaredTable, ExportKind,
    ImportKind, InitializationContract, MemoryDemand, Provider, RequirementId, StorageContract,
    StorageRegion, TargetLinkInput, TargetPolicy,
};
pub use verify::{VerifiedArtifact, verify_artifact};
