//! Pinned numeric artifact and the operations it provides.

use super::model::*;
use super::{VERSION, op};
use crate::abi::RawType;

/// Numeric formatting and decimal conversion, produced by `tools/build.sh`.
pub const NUMBER_RUNTIME: RuntimeArtifact = RuntimeArtifact {
    id: "psrs:runtime-number",
    module_name: crate::abi::MODULE_NAME,
    bytes: include_bytes!("../../artifact/psrs_runtime.wasm"),
    provenance: ArtifactProvenance {
        dependency: "ryu-js, libm, and Rust core::num::dec2flt",
        dependency_revision: "ryu-js 1.0.2; libm 0.2.15; Rust 1.99.0",
        rust_toolchain: "1.99.0 (b940084d7 2026-09-28)",
        target: "wasm32-unknown-unknown",
        profile: "target-runtime",
        recipe: "tools/build.sh: RUSTFLAGS=-C no-redzone=yes; --import-memory --global-base=65536 \
                 -zstack-size=65536 --export=__heap_base, then package",
        sha256: "458a7df7eb27038c4d6b22aa52fc8ea283c6dca41120b0c0bf31f6a851aa0260",
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
        module: crate::abi::MEMORY_MODULE,
        field: crate::abi::MEMORY_FIELD,
        kind: RawImportKind::Memory {
            minimum: 3,
            maximum: None,
        },
    }],
    function_exports: &[
        RawExport {
            name: crate::abi::NUMBER_EXPORT,
            parameters: &[RawType::F64, RawType::I32, RawType::I32],
            result: Some(RawType::I32),
        },
        RawExport {
            name: crate::abi::DECIMAL_EXPORT,
            parameters: &[RawType::I32, RawType::I32],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::ACOS_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::ASIN_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::ATAN_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::ATAN2_EXPORT,
            parameters: &[RawType::F64, RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::SIN_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::COS_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::TAN_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::EXP_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::LOG_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::POW_EXPORT,
            parameters: &[RawType::F64, RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::MIN_EXPORT,
            parameters: &[RawType::F64, RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::MAX_EXPORT,
            parameters: &[RawType::F64, RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::SIGN_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::REMAINDER_EXPORT,
            parameters: &[RawType::F64, RawType::F64],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::IS_NAN_EXPORT,
            parameters: &[RawType::F64],
            result: Some(RawType::I32),
        },
        RawExport {
            name: crate::abi::NAN_EXPORT,
            parameters: &[],
            result: Some(RawType::F64),
        },
        RawExport {
            name: crate::abi::INFINITY_EXPORT,
            parameters: &[],
            result: Some(RawType::F64),
        },
    ],
    global_exports: &[crate::abi::HEAP_BASE_EXPORT],
    tables: &[RawTable {
        element: "funcref",
        minimum: 2,
        maximum: Some(2),
    }],
    elements: &[RawElement {
        table: 0,
        offset: 1,
        functions: &[54],
    }],
    globals: &[
        (true, crate::abi::HEAP_START),
        (false, crate::abi::HEAP_START),
    ],
    storage: RawStorage {
        static_data: (crate::abi::RESERVED_START, crate::abi::STACK_BOTTOM),
        stack: (crate::abi::STACK_BOTTOM, crate::abi::HEAP_START),
        heap_start: crate::abi::HEAP_START,
        minimum_pages: 3,
        stack_pointer_global: 0,
        stack_bound_bytes: 1680,
        stack_bound_evidence: "static call-graph frame analysis of the pinned artifact (psrs-linker::measure_stack_bound)",
    },
    start_forbidden: true,
    data_range: (crate::abi::RESERVED_START, crate::abi::STACK_BOTTOM),
    instantiate_after_shims: false,
};

/// `number_to_string`.
pub const NUMBER_TO_STRING_OP: ProvidedOperation =
    op("psrs:number/to-string", &crate::abi::NUMBER_FORMAT);
/// `number_from_decimal`.
pub const NUMBER_FROM_DECIMAL_OP: ProvidedOperation =
    op("psrs:number/from-decimal", &crate::abi::NUMBER_PARSE);
/// `number_acos`.
pub const NUMBER_ACOS_OP: ProvidedOperation = op("psrs:number/acos", &crate::abi::NUMBER_ACOS);
/// `number_asin`.
pub const NUMBER_ASIN_OP: ProvidedOperation = op("psrs:number/asin", &crate::abi::NUMBER_ASIN);
/// `number_atan`.
pub const NUMBER_ATAN_OP: ProvidedOperation = op("psrs:number/atan", &crate::abi::NUMBER_ATAN);
/// `number_atan2`.
pub const NUMBER_ATAN2_OP: ProvidedOperation = op("psrs:number/atan2", &crate::abi::NUMBER_ATAN2);
/// `number_sin`.
pub const NUMBER_SIN_OP: ProvidedOperation = op("psrs:number/sin", &crate::abi::NUMBER_SIN);
/// `number_cos`.
pub const NUMBER_COS_OP: ProvidedOperation = op("psrs:number/cos", &crate::abi::NUMBER_COS);
/// `number_tan`.
pub const NUMBER_TAN_OP: ProvidedOperation = op("psrs:number/tan", &crate::abi::NUMBER_TAN);
/// `number_exp`.
pub const NUMBER_EXP_OP: ProvidedOperation = op("psrs:number/exp", &crate::abi::NUMBER_EXP);
/// `number_log`.
pub const NUMBER_LOG_OP: ProvidedOperation = op("psrs:number/log", &crate::abi::NUMBER_LOG);
/// `number_pow`.
pub const NUMBER_POW_OP: ProvidedOperation = op("psrs:number/pow", &crate::abi::NUMBER_POW);
/// `number_min`.
pub const NUMBER_MIN_OP: ProvidedOperation = op("psrs:number/min", &crate::abi::NUMBER_MIN);
/// `number_max`.
pub const NUMBER_MAX_OP: ProvidedOperation = op("psrs:number/max", &crate::abi::NUMBER_MAX);
/// `number_sign`.
pub const NUMBER_SIGN_OP: ProvidedOperation = op("psrs:number/sign", &crate::abi::NUMBER_SIGN);
/// `number_remainder`.
pub const NUMBER_REMAINDER_OP: ProvidedOperation =
    op("psrs:number/remainder", &crate::abi::NUMBER_REMAINDER);
/// `number_is_nan`.
pub const NUMBER_IS_NAN_OP: ProvidedOperation =
    op("psrs:number/is-nan", &crate::abi::NUMBER_IS_NAN);
/// `number_nan`.
pub const NUMBER_NAN_OP: ProvidedOperation = op("psrs:number/nan", &crate::abi::NUMBER_NAN);
/// `number_infinity`.
pub const NUMBER_INFINITY_OP: ProvidedOperation =
    op("psrs:number/infinity", &crate::abi::NUMBER_INFINITY);

const NUMBER_OPERATIONS: &[ProvidedOperation] = &[
    NUMBER_TO_STRING_OP,
    NUMBER_FROM_DECIMAL_OP,
    NUMBER_ACOS_OP,
    NUMBER_ASIN_OP,
    NUMBER_ATAN_OP,
    NUMBER_ATAN2_OP,
    NUMBER_SIN_OP,
    NUMBER_COS_OP,
    NUMBER_TAN_OP,
    NUMBER_EXP_OP,
    NUMBER_LOG_OP,
    NUMBER_POW_OP,
    NUMBER_MIN_OP,
    NUMBER_MAX_OP,
    NUMBER_SIGN_OP,
    NUMBER_REMAINDER_OP,
    NUMBER_IS_NAN_OP,
    NUMBER_NAN_OP,
    NUMBER_INFINITY_OP,
];

/// Numeric formatting, parsing, and elementary operations.
///
/// This unit owns its static data and stack. It imports the application memory
/// and does not grow it. The demand names the allocator that owns growth.
pub const NUMBER_UNIT: RuntimeUnit = RuntimeUnit {
    id: "psrs:runtime/number",
    semantic_contract_version: VERSION,
    provided: NUMBER_OPERATIONS,
    required: &[],
    variant: &NUMBER_RUNTIME,
    state: StateOwnership {
        owner: Some("psrs:runtime/number"),
        grows_memory: false,
    },
};
