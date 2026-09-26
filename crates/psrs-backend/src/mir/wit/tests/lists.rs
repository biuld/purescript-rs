use super::common::{RecordingLowerer, reference, signature};
use super::*;
use crate::abi::{WasiParamKind, WasiResultKind};
use crate::mir::ListDirection;
use crate::types::DefinedTypeId;
use psrs_hir::{ModuleId, SymbolId};

fn import(result_kind: WasiResultKind, param_kinds: Vec<WasiParamKind>) -> WasiImport {
    let retptr = matches!(&result_kind, WasiResultKind::ValueList { .. });
    WasiImport {
        symbol: SymbolId::new(ModuleId(0), 1),
        module: "test:lists".into(),
        name: "values".into(),
        parameters: vec![crate::types::ValueType::I32; param_kinds.len() * 2],
        param_kinds,
        result: None,
        result_kind,
        unsupported: None,
        retptr,
        flat_slots: Vec::new(),
    }
}

#[test]
fn a_list_of_strings_result_lowers_to_an_array() {
    let mut lowerer = RecordingLowerer::default();
    let destination = ValueId(0);
    lowerer.array_types.insert(destination, DefinedTypeId(1));
    let element = WasiParamKind::List;
    lower(
        &mut lowerer,
        &import(
            WasiResultKind::ValueList {
                element: Box::new(element),
            },
            Vec::new(),
        ),
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
                element: crate::abi::ListElement::String,
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
        &import(
            WasiResultKind::None,
            vec![WasiParamKind::ValueList {
                element: Box::new(WasiParamKind::Integer32),
            }],
        ),
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
                element: crate::abi::ListElement::Word,
                ..
            }
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
        &import(
            WasiResultKind::None,
            vec![WasiParamKind::ValueList {
                element: Box::new(WasiParamKind::Enum {
                    cases: vec!["red".into(), "green".into()],
                }),
            }],
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
                element: crate::abi::ListElement::Narrow {
                    bits: 8,
                    signed: false,
                },
                ..
            }
        )
    }));
}
