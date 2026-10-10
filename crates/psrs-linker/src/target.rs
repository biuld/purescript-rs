//! Linker-owned target records.
//!
//! The backend converts checked IR requirements into these records. They carry
//! no compiler IR nodes, language `TypeId`s, or MIR bodies, so plan and
//! composition tests can construct artifacts without compiling PureScript.

/// A requirement identity assigned by the producer. The backend keeps the map
/// from this identity to a source symbol and span for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequirementId(pub u32);

/// The checked core-Wasm value contract, independent of module-local indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreType {
    I32,
    I64,
    F32,
    F64,
    V128,
    Ref(crate::core_types::CoreReference),
}

/// A core-Wasm function signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSignature {
    pub parameters: Vec<CoreType>,
    pub result: Option<CoreType>,
}

/// The boundary a requirement crosses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// A private core-module import, closed inside the component.
    RawCore { module: String, field: String },
    /// A WIT interface import resolved through the world.
    ResolvedWit { interface: String, function: String },
}

/// The selected implementation for a requirement.
///
/// A runtime operation names an identity and version. Planning selects the
/// unique compatible unit; a caller-chosen artifact is not a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provider {
    /// A compiler-generated local operation or helper; no import results.
    Generated,
    /// Resolve the unique offered unit that provides this operation.
    RuntimeOperation { name: String, version: String },
    /// A host interface retained in the component's external world.
    HostInterface { interface: String },
}

/// One live executable requirement and its selected provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingRequirement {
    pub id: RequirementId,
    /// A stable origin label for diagnostics, for example `numberToString` or
    /// `wasi:cli/stdout.get-stdout`.
    pub origin: String,
    pub boundary: Boundary,
    /// The expected raw core signature, when the requirement crosses a raw or
    /// WIT boundary. `None` for a generated helper that never leaves the module.
    pub expected: Option<CoreSignature>,
    pub provider: Provider,
}

/// Whether an artifact is a core module or a component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    CoreModule,
    /// A typed guest component provider. Use `guest::plan_components`; a
    /// component cannot satisfy a raw-core artifact contract.
    Component,
}

/// The kind of a declared artifact import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportKind {
    /// A shared memory import. Limits are part of the pinned contract.
    Memory {
        minimum: u64,
        maximum: Option<u64>,
    },
    /// An `i32` global import.
    Global {
        mutable: bool,
    },
    Function(CoreSignature),
}

/// An import the artifact's contract declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredImport {
    pub module: String,
    pub field: String,
    pub kind: ImportKind,
}

/// An export the artifact's contract declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredExport {
    pub name: String,
    pub kind: ExportKind,
    /// The function signature when `kind` is `Func`.
    pub signature: Option<CoreSignature>,
}

/// The kind of a declared export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Global,
}

/// A table the artifact's contract declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredTable {
    pub element: String,
    pub minimum: u32,
    pub maximum: Option<u32>,
}

/// A global the artifact's contract declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredGlobal {
    pub mutable: bool,
    /// The constant `i32` initial value.
    pub initial: u32,
}

/// An explicitly declared active function-table initializer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredElement {
    pub table: u32,
    pub offset: u32,
    pub functions: Vec<u32>,
}

/// A declared private execution-storage region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRegion {
    pub owner: String,
    pub start: u32,
    pub end: u32,
}

/// An artifact's private execution storage and allocator boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageContract {
    pub static_data: StorageRegion,
    pub stack: StorageRegion,
    pub heap_start: u32,
    pub minimum_pages: u64,
    /// The index of the mutable stack-pointer global the artifact uses.
    pub stack_pointer_global: u32,
    /// A reviewed upper bound on stack bytes for the supported calling pattern.
    pub stack_bound_bytes: u32,
    /// Where the stack bound comes from; a measured static bound names the
    /// analysis, while a reviewed build assumption names its pending evidence.
    pub stack_bound_evidence: String,
}

/// Constraints on an artifact's initialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializationContract {
    pub start_forbidden: bool,
    /// Every active data segment must lie in this half-open range.
    pub data_range: (u32, u32),
}

/// The declared contract an executable artifact must satisfy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactContract {
    pub id: String,
    pub kind: ArtifactKind,
    /// The private core-module import identity the artifact uses.
    pub module_name: String,
    /// SHA-256 of the artifact bytes.
    pub sha256: String,
    /// Human-readable pinned provenance (dependency, toolchain, recipe).
    pub provenance: String,
    pub required_features: Vec<String>,
    pub imports: Vec<DeclaredImport>,
    pub exports: Vec<DeclaredExport>,
    pub tables: Vec<DeclaredTable>,
    pub elements: Vec<DeclaredElement>,
    pub globals: Vec<DeclaredGlobal>,
    pub storage: Option<StorageContract>,
    pub initialization: InitializationContract,
    /// Whether the artifact must be instantiated after the call shims.
    pub instantiate_after_shims: bool,
}

/// A pinned artifact and the contract it must satisfy.
#[derive(Clone, Debug)]
pub struct ArtifactReference {
    pub contract: ArtifactContract,
    pub bytes: Vec<u8>,
}

/// The target policy the backend converts from its capability profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetPolicy {
    /// Canonical ids of the host interfaces the selected target profile permits.
    /// The backend derives these from the resolved world and the capability
    /// families, so the linker never parses backend capability types.
    pub permitted_host_interfaces: Vec<String>,
}

/// Growth owner while the backend still synthesizes `cabi_realloc`.
///
/// The canonical realloc adapter replaces this name with one allocator runtime
/// unit. Planning accepts that unit when the demand names its identity.
pub const GENERATED_GROWTH_OWNER: &str = "psrs:allocator/generated";

/// Identity of the one shared application memory.
pub const APPLICATION_MEMORY: &str = "application";

/// One operation an offered runtime unit provides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferedOperation {
    pub name: String,
    pub version: String,
    pub export: String,
    pub signature: CoreSignature,
}

/// An operation an offered unit requires from another provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredOperation {
    pub name: String,
    pub version: String,
    pub signature: CoreSignature,
}

/// A candidate runtime unit and the one variant it advertises.
#[derive(Clone, Debug)]
pub struct RuntimeUnitOffer {
    pub id: String,
    pub semantic_contract_version: String,
    pub provided: Vec<OfferedOperation>,
    pub required: Vec<RequiredOperation>,
    pub artifact: ArtifactReference,
    /// Private state owner. `None` means the unit owns no private state.
    pub state_owner: Option<String>,
    /// The unit grows the shared memory. This conflicts with any other owner.
    pub grows_memory: bool,
}

/// The canonical ABI memory layout demanded by the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryDemand {
    /// The reserved canonical scratch region `[start, end)`.
    pub canonical_scratch: (u32, u32),
    /// The allocator-state region `[start, end)`.
    pub allocator_state: (u32, u32),
    /// The application's allocator start before any artifact reservation.
    pub base_heap_start: u32,
    /// The allocator block granularity the heap start must satisfy.
    pub heap_alignment: u32,
    /// The one owner allowed to grow the shared memory.
    ///
    /// The backend currently supplies [`GENERATED_GROWTH_OWNER`]. A selected
    /// runtime unit may take this role when the demand names that unit.
    pub growth_owner: String,
    /// Declared maximum page count. `None` means the wasm32 limit of 65536.
    pub maximum_pages: Option<u64>,
}

/// The complete checked input to [`crate::plan::plan`].
#[derive(Clone, Debug)]
pub struct TargetLinkInput {
    pub requirements: Vec<BindingRequirement>,
    /// Candidate runtime units. Planning selects only the units a live
    /// requirement or a selected unit's dependency reaches.
    pub units: Vec<RuntimeUnitOffer>,
    pub policy: TargetPolicy,
    pub memory: MemoryDemand,
}
