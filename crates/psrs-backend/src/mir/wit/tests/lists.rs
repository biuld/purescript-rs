use super::common::{RecordingLowerer, reference, signature};
use super::*;
use crate::abi::canonical::CanonicalType;
use crate::abi::test_support::import;
use crate::mir::ListDirection;
use crate::types::DefinedTypeId;
use psrs_hir::{ModuleId, SymbolId};

fn list_import(params: Vec<CanonicalType>, result: Option<CanonicalType>) -> WasiImport {
    import(
        SymbolId::new(ModuleId(0), 1),
        "test:lists",
        "values",
        params,
        result,
    )
}

fn int(width: u8, signed: bool) -> CanonicalType {
    CanonicalType::Int { width, signed }
}

#[test]
fn a_list_of_strings_result_lowers_to_an_array() {
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(0);
    lowerer.array_types.insert(destination, DefinedTypeId(1));
    let element = CanonicalType::String;
    lower(
        &mut lowerer,
        &list_import(Vec::new(), Some(CanonicalType::List(Box::new(element)))),
        &signature(Vec::new()),
        destination,
        &[],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("list<string> should lower to an array");
    assert!(lowerer.instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::ListCopy {
                direction: ListDirection::Load,
                element: CanonicalType::String,
                array,
                ..
            } if *array == destination
        )
    }));
}

#[test]
fn an_array_of_ints_lowers_to_a_list_parameter() {
    let mut lowerer = RecordingLowerer::default();
    let argument = ValueId(3);
    lowerer.array_types.insert(argument, DefinedTypeId(2));
    lower(
        &mut lowerer,
        &list_import(vec![CanonicalType::List(Box::new(int(32, true)))], None),
        &signature(vec![reference(0)]),
        ValueId(0),
        &[argument],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("Array Int should lower to list<s32>");
    assert!(lowerer.instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::ListCopy {
                direction: ListDirection::Store,
                element,
                ..
            } if *element == int(32, true)
        )
    }));
    assert!(lowerer.instructions.iter().any(|instruction| {
        matches!(instruction, Instruction::ArrayLen { value, .. } if *value == argument)
    }));
}

#[test]
fn an_array_of_enums_lowers_to_a_narrow_list_parameter() {
    let mut lowerer = RecordingLowerer::default();
    let argument = ValueId(3);
    lowerer.array_types.insert(argument, DefinedTypeId(2));
    lower(
        &mut lowerer,
        &list_import(
            vec![CanonicalType::List(Box::new(CanonicalType::Enum(vec![
                "red".into(),
                "green".into(),
            ])))],
            None,
        ),
        &signature(vec![reference(0)]),
        ValueId(0),
        &[argument],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("Array enum should lower to a list of discriminants");
    assert!(lowerer.instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::ListCopy {
                direction: ListDirection::Store,
                element: CanonicalType::Enum(_),
                ..
            }
        )
    }));
}
