//! Pinned canonical allocator artifact and its state ownership.

use super::model::*;
use super::{VERSION, op};
use crate::abi::RawType;

/// Canonical realloc, produced by `tools/build-allocator.sh`.
///
/// Static data and the stack occupy the hole below the numeric runtime. The
/// imported `get_heap_base` function carries the planned shared heap boundary,
/// so this artifact stays pinned when that boundary moves.
pub const ALLOCATOR_RUNTIME: RuntimeArtifact = RuntimeArtifact {
    id: "psrs:runtime-allocator",
    module_name: crate::abi::ALLOCATOR_MODULE,
    bytes: include_bytes!("../../artifact/psrs_allocator.wasm"),
    provenance: ArtifactProvenance {
        dependency: "dlmalloc",
        dependency_revision: "0.2.14 (ad5208a115eaba24916f7456929832e310a81518c641f93fee4f89aa93aa3675)",
        rust_toolchain: "1.99.0 (b940084d7 2026-09-28)",
        target: "wasm32-unknown-unknown",
        profile: "target-runtime",
        recipe: "tools/build-allocator.sh: remap-path-prefix, no-redzone, --import-memory \
                 --stack-first -zstack-size=32768 --global-base=32768; direct Rust output, \
                 imports __main_module__.get_heap_base, no executable start",
        sha256: "a6f556d0151e8c474c495ad4488b9a23d67bb105dda2dc84eff17eaba70bbf19",
    },
    required_features: &[
        "mutable-globals",
        "sign-extension",
        "multi-value",
        "bulk-memory",
        "reference-types",
    ],
    imports: &[
        RawImport {
            module: crate::abi::MEMORY_MODULE,
            field: crate::abi::MEMORY_FIELD,
            kind: RawImportKind::Memory {
                minimum: 1,
                maximum: None,
            },
        },
        RawImport {
            module: crate::abi::APPLICATION_MODULE,
            field: crate::abi::HEAP_BOUNDARY_IMPORT,
            kind: RawImportKind::Function {
                parameters: &[],
                result: Some(RawType::I32),
            },
        },
    ],
    function_exports: &[RawExport {
        name: crate::abi::REALLOC_EXPORT,
        parameters: &[RawType::I32, RawType::I32, RawType::I32, RawType::I32],
        result: Some(RawType::I32),
    }],
    global_exports: &[],
    tables: &[RawTable {
        element: "funcref",
        minimum: 1,
        maximum: Some(1),
    }],
    elements: &[],
    globals: &[(true, crate::abi::ALLOCATOR_STACK_TOP)],
    storage: RawStorage {
        static_data: (
            crate::abi::ALLOCATOR_STATIC_START,
            crate::abi::ALLOCATOR_STATIC_END,
        ),
        stack: (
            crate::abi::ALLOCATOR_STACK_BOTTOM,
            crate::abi::ALLOCATOR_STACK_TOP,
        ),
        heap_start: crate::abi::ALLOCATOR_HEAP_START,
        minimum_pages: 1,
        stack_pointer_global: 0,
        stack_bound_bytes: 80,
        stack_bound_evidence: "static call-graph frame analysis of the pinned artifact (psrs-linker::measure_stack_bound)",
    },
    start_forbidden: true,
    data_range: (
        crate::abi::ALLOCATOR_STATIC_START,
        crate::abi::ALLOCATOR_STATIC_END,
    ),
    instantiate_after_shims: false,
};
/// Canonical `cabi_realloc`.
pub const ALLOCATOR_REALLOC_OP: ProvidedOperation =
    op("psrs:allocator/realloc", &crate::abi::ALLOCATOR_REALLOC);

const ALLOCATOR_OPERATIONS: &[ProvidedOperation] = &[ALLOCATOR_REALLOC_OP];

/// The exclusive shared-memory growth owner.
///
/// It imports the application memory and the planned constant heap getter.
/// Instantiation does not call the canonical export; the first call initializes it.
pub const ALLOCATOR_UNIT: RuntimeUnit = RuntimeUnit {
    id: "psrs:runtime/allocator",
    semantic_contract_version: VERSION,
    provided: ALLOCATOR_OPERATIONS,
    required: &[],
    variant: &ALLOCATOR_RUNTIME,
    state: StateOwnership {
        owner: Some("psrs:runtime/allocator"),
        grows_memory: true,
    },
};
