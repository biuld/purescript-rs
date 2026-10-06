//! Immutable target catalog consumed by the linker and backend.
//!
//! The catalog returns plain data: WIT source paths and bytes, a world
//! identity, and a pinned artifact's bytes plus declared raw contract and
//! provenance. It never returns a `wit_parser::Resolve`, compiler IR, or
//! link-plan value, and it compiles no executable target code, so a native
//! consumer can read it without linking the formatter.

use crate::RawType;

/// One pinned WIT source file: its catalog path and exact contents.
#[derive(Clone, Copy, Debug)]
pub struct WitSource {
    pub path: &'static str,
    pub contents: &'static str,
}

/// The default world the command artifact implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldIdentity {
    pub package_namespace: &'static str,
    pub package_name: &'static str,
    pub world: &'static str,
}

/// The `psrs:app/command` default world.
pub const DEFAULT_WORLD: WorldIdentity = WorldIdentity {
    package_namespace: "psrs",
    package_name: "app",
    world: "command",
};

/// Vendored WASI 0.2.12 WIT in dependency order.
pub const WASI_WIT: &[WitSource] = &[
    WitSource {
        path: "wasi/io.wit",
        contents: include_str!("../wit/deps/io.wit"),
    },
    WitSource {
        path: "wasi/clocks.wit",
        contents: include_str!("../wit/deps/clocks.wit"),
    },
    WitSource {
        path: "wasi/random.wit",
        contents: include_str!("../wit/deps/random.wit"),
    },
    WitSource {
        path: "wasi/filesystem.wit",
        contents: include_str!("../wit/deps/filesystem.wit"),
    },
    WitSource {
        path: "wasi/sockets.wit",
        contents: include_str!("../wit/deps/sockets.wit"),
    },
    WitSource {
        path: "wasi/cli.wit",
        contents: include_str!("../wit/deps/cli.wit"),
    },
];

/// The application world source.
pub const APP_WIT: WitSource = WitSource {
    path: "psrs-app.wit",
    contents: include_str!("../wit/psrs-app.wit"),
};

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

/// An import the artifact declares.
#[derive(Clone, Copy, Debug)]
pub struct RawImport {
    pub module: &'static str,
    pub field: &'static str,
}

/// A table the artifact declares.
#[derive(Clone, Copy, Debug)]
pub struct RawTable {
    pub element: &'static str,
    pub minimum: u32,
    pub maximum: Option<u32>,
}

/// Private execution storage and the allocator boundary an artifact requires.
#[derive(Clone, Copy, Debug)]
pub struct RawStorage {
    pub static_data: (u32, u32),
    pub stack: (u32, u32),
    pub heap_start: u32,
    pub minimum_pages: u64,
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
    /// Declared globals as `(mutable, constant i32 initial)`.
    pub globals: &'static [(bool, u32)],
    pub storage: RawStorage,
    pub start_forbidden: bool,
    pub data_range: (u32, u32),
    pub instantiate_after_shims: bool,
}

/// The `numberToString` formatter artifact, produced by `tools/build.sh`.
pub const NUMBER_FORMATTER: RuntimeArtifact = RuntimeArtifact {
    id: "psrs:runtime-number-format",
    module_name: crate::MODULE_NAME,
    bytes: include_bytes!("../artifact/psrs_runtime.wasm"),
    provenance: ArtifactProvenance {
        dependency: "ryu-js",
        dependency_revision: "1.0.2",
        rust_toolchain: "1.99.0 (b940084d7 2026-09-28)",
        target: "wasm32-unknown-unknown",
        profile: "target-runtime",
        recipe: "tools/build.sh: --import-memory --global-base=65536 \
                 -zstack-size=65536 --export=__heap_base, then package",
        sha256: "6ed6f666ff48d5ce12d9ffef106edd4cb2fd0629b2ebd033e0b549414c664694",
    },
    required_features: &[
        "mutable-globals",
        "sign-extension",
        "saturating-float-to-int",
        "multi-value",
        "bulk-memory",
        "reference-types",
    ],
    imports: &[RawImport {
        module: crate::MEMORY_MODULE,
        field: crate::MEMORY_FIELD,
    }],
    function_exports: &[RawExport {
        name: crate::NUMBER_EXPORT,
        parameters: &[RawType::F64, RawType::I32, RawType::I32],
        result: Some(RawType::I32),
    }],
    global_exports: &[crate::HEAP_BASE_EXPORT],
    tables: &[RawTable {
        element: "funcref",
        minimum: 1,
        maximum: Some(1),
    }],
    globals: &[(true, crate::HEAP_START), (false, crate::HEAP_START)],
    storage: RawStorage {
        static_data: (crate::RESERVED_START, crate::STACK_BOTTOM),
        stack: (crate::STACK_BOTTOM, crate::HEAP_START),
        heap_start: crate::HEAP_START,
        minimum_pages: 3,
        stack_bound_bytes: 4096,
        stack_bound_evidence: "reviewed pinned nonrecursive build assumption; stress evidence pending",
    },
    start_forbidden: true,
    data_range: (crate::RESERVED_START, crate::STACK_BOTTOM),
    instantiate_after_shims: false,
};
