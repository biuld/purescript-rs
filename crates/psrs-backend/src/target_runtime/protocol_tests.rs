use super::*;
use psrs_runtime::{RawCallProtocol, RawFunctionAbi, RawType};

fn provider(intrinsic: Intrinsic, abi: &'static RawFunctionAbi) -> ArtifactImplementation {
    ArtifactImplementation {
        intrinsic,
        symbol: NUMBER_FORMAT.symbol,
        abi,
        artifact: &psrs_runtime::NUMBER_RUNTIME,
    }
}

#[test]
fn scalar_and_void_protocols_require_matching_closed_language_schemes() {
    const SCALAR: RawFunctionAbi = RawFunctionAbi {
        export: "scalar",
        parameters: &[RawType::F64],
        result: Some(RawType::F64),
        protocol: RawCallProtocol::Scalars,
    };
    const VOID: RawFunctionAbi = RawFunctionAbi {
        export: "void",
        parameters: &[],
        result: None,
        protocol: RawCallProtocol::Scalars,
    };
    provider(Intrinsic::NumberAbs, &SCALAR)
        .validate_protocol()
        .unwrap();
    provider(Intrinsic::Unit, &VOID)
        .validate_protocol()
        .unwrap();
    assert!(
        provider(Intrinsic::NumberAbs, &VOID)
            .validate_protocol()
            .is_err()
    );
    assert!(
        provider(Intrinsic::NumberToString, &SCALAR)
            .validate_protocol()
            .is_err()
    );
    assert!(
        provider(Intrinsic::ArrayLength, &SCALAR)
            .validate_protocol()
            .is_err()
    );
}

#[test]
fn buffer_protocols_require_matching_shapes_raw_types_and_bounded_capacity() {
    const UNBOUNDED: RawFunctionAbi = RawFunctionAbi {
        export: "format",
        parameters: &[RawType::F64, RawType::I32, RawType::I32],
        result: Some(RawType::I32),
        protocol: RawCallProtocol::Utf8Output { capacity: 0 },
    };
    const WRONG_WIDTH: RawFunctionAbi = RawFunctionAbi {
        export: "parse",
        parameters: &[RawType::I32, RawType::I64],
        result: Some(RawType::F64),
        protocol: RawCallProtocol::Utf8Input,
    };
    assert!(
        provider(Intrinsic::NumberToString, &UNBOUNDED)
            .validate_protocol()
            .is_err()
    );
    assert!(
        provider(Intrinsic::NumberFromDecimal, &WRONG_WIDTH)
            .validate_protocol()
            .is_err()
    );
    assert!(
        provider(Intrinsic::NumberAbs, &psrs_runtime::NUMBER_PARSE)
            .validate_protocol()
            .is_err()
    );
}
