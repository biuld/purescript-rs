use super::common::{RecordingLowerer, signature};
use super::*;
use crate::abi::canonical::CanonicalType;
use crate::abi::test_support::import;
use crate::cc::ValueShape;
use psrs_hir::{ModuleId, SymbolId};

#[test]
fn scalar_f64_results_are_called_directly() {
    let import = import(
        SymbolId::new(ModuleId(0), 0),
        "test:interface",
        "number",
        Vec::new(),
        Some(CanonicalType::Float { width: 64 }),
    );
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
    let import = import(
        SymbolId::new(ModuleId(0), 0),
        "test:interface",
        "char-roundtrip",
        vec![CanonicalType::Char],
        Some(CanonicalType::Char),
    );
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
    let cases = vec!["red".to_string(), "green-blue".to_string()];
    let enum_type = CanonicalType::Enum(cases);
    let import = import(
        SymbolId::new(ModuleId(0), 0),
        "test:enums",
        "convert",
        vec![enum_type.clone()],
        Some(enum_type),
    );
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
    let import = import(
        SymbolId::new(ModuleId(0), 0),
        "test:interface",
        "f32-roundtrip",
        vec![CanonicalType::Float { width: 32 }],
        Some(CanonicalType::Float { width: 32 }),
    );
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
