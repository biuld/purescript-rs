//! Explicit, in-memory composition of pinned guest WIT interface providers.
mod graph;
pub use graph::{compose_component, plan_components};

/// A pinned, executable component; definitions alone are not providers.
#[derive(Clone, Debug)]
pub struct ComponentReference {
    pub id: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// Bind the whole imported interface to one export of one guest instance.
/// This preserves a resource's constructor/method/destructor identity.
#[derive(Clone, Debug)]
pub struct GuestBinding {
    pub interface: String,
    pub artifact: String,
    pub export: String,
}

/// Target-only entry point for an already typed application component.
/// Core-module callers still use `plan` and `compose` before this boundary.
#[derive(Clone, Debug)]
pub struct ComponentLinkInput {
    pub application: ComponentReference,
    pub guests: Vec<ComponentReference>,
    pub bindings: Vec<GuestBinding>,
    pub policy: crate::TargetPolicy,
    /// The selected target's validator feature policy, applied to every input.
    pub features: wasmparser::WasmFeatures,
}

/// One live whole-interface edge in the executable graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedGuestBinding {
    pub consumer: String,
    pub interface: String,
    pub provider: String,
    pub export: String,
}

/// A checked, closed component graph; construction is private to planning.
/// Component memories stay inside their owners and have no core memory plan.
#[derive(Clone, Debug)]
pub struct CheckedComponentPlan {
    pub(crate) application_digest: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) artifacts: Vec<(String, String)>,
    pub(crate) external_world: Vec<String>,
    pub(crate) bindings: Vec<ResolvedGuestBinding>,
    pub(crate) features: wasmparser::WasmFeatures,
}

impl CheckedComponentPlan {
    /// Selected component identities and pins, including the root application.
    pub fn component_artifacts(&self) -> &[(String, String)] {
        &self.artifacts
    }
    pub fn bindings(&self) -> &[ResolvedGuestBinding] {
        &self.bindings
    }
    pub fn features(&self) -> wasmparser::WasmFeatures {
        self.features
    }
    /// Exact residual host interface imports after graph closure.
    pub fn external_world(&self) -> &[String] {
        &self.external_world
    }
}
