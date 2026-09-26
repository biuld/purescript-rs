use super::common::{RecordingLowerer, signature};
use super::*;
use crate::abi::{WasiParamKind, WasiResultKind};
use crate::cc::ValueShape;
use psrs_hir::{ModuleId, SymbolId};
#[test]
fn scalar_f64_results_are_called_directly() {
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:interface".into(),
        name: "number".into(),
        parameters: Vec::new(),
        param_kinds: Vec::<WasiParamKind>::new(),
        result: Some(ValueType::F64),
        result_kind: WasiResultKind::Scalar,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(7);

    lower(
        &mut lowerer,
        &import,
        &signature(Vec::new()),
        destination,
        &[],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("F64 scalar results should lower directly");

    assert!(matches!(
        lowerer.instructions.as_slice(),
        [Instruction::Call {
            destination: actual,
            function,
            ..
        }] if *actual == destination && *function == import.symbol
    ));
}

#[test]
fn char_arguments_and_results_use_direct_i32_values() {
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:interface".into(),
        name: "char-roundtrip".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::Char],
        result: Some(ValueType::I32),
        result_kind: WasiResultKind::Char,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(7);
    let argument = ValueId(3);

    lower(
        &mut lowerer,
        &import,
        &signature(vec![ValueShape::Integer]),
        destination,
        &[argument],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("WIT Char uses the canonical i32 representation");

    assert!(matches!(
        lowerer.instructions.as_slice(),
        [Instruction::Call {
            destination: actual_destination,
            function,
            arguments,
            ..
        }] if *actual_destination == destination
            && *function == import.symbol
            && arguments == &[argument]
    ));
}

#[test]
fn enum_arguments_and_results_keep_the_validated_i32_tags() {
    let cases = vec!["Red".to_string(), "GreenBlue".to_string()];
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:enums".into(),
        name: "convert".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::Enum {
            cases: cases.clone(),
        }],
        result: Some(ValueType::I32),
        result_kind: WasiResultKind::Enum { cases },
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(7);
    let argument = ValueId(3);

    lower(
        &mut lowerer,
        &import,
        &signature(vec![ValueShape::Integer]),
        destination,
        &[argument],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("validated enum tags should pass through the canonical i32 ABI");

    assert!(matches!(
        lowerer.instructions.as_slice(),
        [Instruction::Call {
            destination: actual_destination,
            function,
            arguments,
            ..
        }] if *actual_destination == destination
            && *function == import.symbol
            && arguments == &[argument]
    ));
}

#[test]
fn f32_arguments_and_results_are_adapted_to_source_numbers() {
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:interface".into(),
        name: "f32-roundtrip".into(),
        parameters: vec![ValueType::F32],
        param_kinds: vec![WasiParamKind::Float32],
        result: Some(ValueType::F32),
        result_kind: WasiResultKind::Scalar,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(7);
    let argument = ValueId(3);

    lower(
        &mut lowerer,
        &import,
        &signature(vec![ValueShape::Number]),
        destination,
        &[argument],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("WIT f32 should adapt at the ABI boundary");

    assert!(matches!(
        lowerer.instructions.as_slice(),
        [
            Instruction::UnaryPrimitive {
                destination: ValueId(0),
                op: UnaryOp::F64ToF32,
                value,
                ..
            },
            Instruction::Call {
                destination: ValueId(1),
                function,
                arguments,
                ..
            },
            Instruction::UnaryPrimitive {
                destination: actual_destination,
                op: UnaryOp::F32ToF64,
                value: ValueId(1),
                ..
            }
        ] if *value == argument
            && *function == import.symbol
            && arguments == &[ValueId(0)]
            && *actual_destination == destination
    ));
}
