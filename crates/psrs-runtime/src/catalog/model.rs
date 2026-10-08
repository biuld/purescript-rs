//! Plain metadata records consumed by the linker and backend.

use crate::abi::{RawFunctionAbi, RawType};

/// Reviewed provenance for a pinned compiler-owned runtime artifact.
#[derive(Clone, Copy, Debug)]
pub struct ArtifactProvenance {
    /// The pinned dependency whose behavior the artifact implements.
    pub dependency: &'static str,
    /// The exact dependency revision.
    pub dependency_revision: &'static str,
    /// The Rust toolchain that produced the artifact.
    pub rust_toolchain: &'static str,
    /// The Rust target triple.
    pub target: &'static str,
    /// The Cargo profile.
    pub profile: &'static str,
    /// The linker flags and preparation recipe.
    pub recipe: &'static str,
    /// SHA-256 of the prepared artifact bytes.
    pub sha256: &'static str,
}

/// A function export the artifact declares.
#[derive(Clone, Copy, Debug)]
pub struct RawExport {
    pub name: &'static str,
    pub parameters: &'static [RawType],
    pub result: Option<RawType>,
}

/// The kind of a raw artifact import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawImportKind {
    /// A memory import and its declared limits.
    Memory { minimum: u64, maximum: Option<u64> },
    /// A scalar function import.
    Function {
        parameters: &'static [RawType],
        result: Option<RawType>,
    },
}

/// An import the artifact declares.
#[derive(Clone, Copy, Debug)]
pub struct RawImport {
    pub module: &'static str,
    pub field: &'static str,
    pub kind: RawImportKind,
}

/// A table the artifact declares.
#[derive(Clone, Copy, Debug)]
pub struct RawTable {
    pub element: &'static str,
    pub minimum: u32,
    pub maximum: Option<u32>,
}

/// An active initializer for a private function table.
#[derive(Clone, Copy, Debug)]
pub struct RawElement {
    pub table: u32,
    pub offset: u32,
    pub functions: &'static [u32],
}

/// Private execution storage and the allocator boundary an artifact requires.
#[derive(Clone, Copy, Debug)]
pub struct RawStorage {
    pub static_data: (u32, u32),
    pub stack: (u32, u32),
    pub heap_start: u32,
    pub minimum_pages: u64,
    /// Absolute index of the mutable stack-pointer global, counting imports.
    pub stack_pointer_global: u32,
    /// A reviewed upper bound on stack bytes for supported calling behavior.
    pub stack_bound_bytes: u32,
    /// Where the bound comes from, including pending evidence.
    pub stack_bound_evidence: &'static str,
}

/// A pinned core-Wasm artifact and its declared raw contract.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeArtifact {
    pub id: &'static str,
    /// The private core-module import identity the artifact uses.
    pub module_name: &'static str,
    /// The artifact's core-Wasm bytes.
    pub bytes: &'static [u8],
    pub provenance: ArtifactProvenance,
    pub required_features: &'static [&'static str],
    pub imports: &'static [RawImport],
    pub function_exports: &'static [RawExport],
    pub global_exports: &'static [&'static str],
    pub tables: &'static [RawTable],
    pub elements: &'static [RawElement],
    /// Declared globals as `(mutable, constant i32 initial)`.
    pub globals: &'static [(bool, u32)],
    pub storage: RawStorage,
    pub start_forbidden: bool,
    pub data_range: (u32, u32),
    pub instantiate_after_shims: bool,
}

/// An operation a unit provides, with the raw export that implements it.
#[derive(Clone, Copy)]
pub struct ProvidedOperation {
    pub name: &'static str,
    pub version: &'static str,
    pub abi: &'static RawFunctionAbi,
}

/// An operation a unit requires from exactly one other provider.
#[derive(Clone, Copy)]
pub struct RequiredOperation {
    pub name: &'static str,
    pub version: &'static str,
    pub parameters: &'static [crate::RawType],
    pub result: Option<crate::RawType>,
}

/// Who owns a unit's private state and whether it may grow shared memory.
#[derive(Clone, Copy)]
pub struct StateOwnership {
    /// Stable owner identity. `None` means the unit has no private state.
    pub owner: Option<&'static str>,
    /// Exclusive growth of the planned shared memory.
    pub grows_memory: bool,
}

/// One runtime implementation unit and the single variant it advertises.
#[derive(Clone, Copy)]
pub struct RuntimeUnit {
    pub id: &'static str,
    pub semantic_contract_version: &'static str,
    pub provided: &'static [ProvidedOperation],
    pub required: &'static [RequiredOperation],
    /// The produced core-module variant. A second variant needs an explicit
    /// selection rule before it is added here.
    pub variant: &'static RuntimeArtifact,
    pub state: StateOwnership,
}

/// A runtime package identity and the units it publishes.
#[derive(Clone, Copy)]
pub struct RuntimePackage {
    pub id: &'static str,
    pub version: &'static str,
    pub provenance: &'static str,
    pub units: &'static [&'static RuntimeUnit],
}
