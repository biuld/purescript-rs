use crate::abi::{WasiImport, WasiParamKind};
use crate::mir::{Instruction, Module, NumericOp, UnaryOp};
use crate::types::ValueId;
use crate::wasm;

pub(super) fn assert_wit_shape(import: &WasiImport) {
    let cases = vec!["Red".to_string(), "Green".into(), "Blue".into()];
    let WasiParamKind::Record { fields } = &import.param_kinds[14] else {
        panic!("the final WIT parameter should classify as a record");
    };
    assert_eq!(
        fields[0].kind,
        WasiParamKind::Flags {
            names: vec![
                "write".into(),
                "read".into(),
                "audit".into(),
                "execute".into(),
                "debug".into(),
            ],
        }
    );
    let WasiParamKind::Record {
        fields: details_fields,
    } = &fields[1].kind
    else {
        panic!("the details field should remain a nested record");
    };
    assert_eq!(details_fields[0].kind, WasiParamKind::List);
    assert_eq!(
        details_fields[1].kind,
        WasiParamKind::Enum {
            cases: cases.clone(),
        }
    );
    assert_eq!(fields[2].kind, WasiParamKind::Enum { cases });
}

pub(super) fn assert_p9_layout(module: &Module, import: &WasiImport) {
    assert!(
        module
            .imports
            .iter()
            .any(|candidate| candidate.symbol == import.symbol)
    );
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("the fixture should have a main function");
    let instructions = main
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .collect::<Vec<_>>();
    assert!(instructions.iter().any(|instruction| matches!(instruction,
        Instruction::CallVoid { function, arguments, .. }
            if *function == import.symbol && arguments.len() == 1
    )));

    let product_gets = instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::StructGet {
                destination,
                value,
                field,
                ..
            } => Some((*destination, *value, *field)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let project = |parent: ValueId, field: u32| {
        product_gets
            .iter()
            .find_map(|(destination, value, index)| {
                (*value == parent && *index == field).then_some(*destination)
            })
            .expect("the source record field should be projected")
    };
    let projection_order = |parent| {
        product_gets
            .iter()
            .filter_map(|(_, value, field)| (*value == parent).then_some(*field))
            .collect::<Vec<_>>()
    };
    let access = project(ValueId(24), 0);
    let details = project(ValueId(24), 1);
    assert_eq!(projection_order(access), vec![4, 3, 0, 2, 1]);
    assert_eq!(projection_order(details), vec![1, 0]);

    let text = project(details, 1);
    let nested_enum = project(details, 0);
    let outer_enum = project(ValueId(24), 2);
    let list_length = instructions
        .iter()
        .find_map(|instruction| match instruction {
            Instruction::Load {
                destination,
                address,
                offset: 0,
                ..
            } if *address == text => Some(*destination),
            _ => None,
        })
        .expect("the list branch should load the String length");
    let list_pointer = instructions
        .iter()
        .find_map(|instruction| match instruction {
            Instruction::Primitive {
                destination,
                op: NumericOp::I32Add,
                left,
                right,
                ..
            } if *left == text
                && instructions.iter().any(|candidate| {
                    matches!(candidate,
                        Instruction::Constant { destination, value: 4, .. }
                            if destination == right
                    )
                }) =>
            {
                Some(*destination)
            }
            _ => None,
        })
        .expect("the list branch should skip the String length prefix");
    let stored_value = |offset| {
        instructions
            .iter()
            .find_map(|instruction| match instruction {
                Instruction::Store {
                    value, offset: at, ..
                } if *at == offset => Some(*value),
                _ => None,
            })
    };
    assert_eq!(stored_value(60), Some(list_pointer));
    assert_eq!(stored_value(64), Some(list_length));

    let stored_byte = |offset| {
        instructions
            .iter()
            .find_map(|instruction| match instruction {
                Instruction::Store8 {
                    value, offset: at, ..
                } if *at == offset => Some(*value),
                _ => None,
            })
    };
    assert_eq!(stored_byte(68), Some(nested_enum));
    assert_eq!(stored_byte(72), Some(outer_enum));
    let packed_flags = stored_byte(56).expect("flags should occupy the first byte");
    assert!(instructions.iter().any(|instruction| matches!(instruction,
        Instruction::Primitive {
            destination,
            op: NumericOp::I32Or,
            ..
        } if *destination == packed_flags
    )));
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::UnaryPrimitive {
                    op: UnaryOp::BoolToI32,
                    ..
                }
            ))
            .count(),
        5
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::Primitive {
                    op: NumericOp::I32Or,
                    ..
                }
            ))
            .count(),
        5
    );
    let shift_values = instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::Primitive {
                op: NumericOp::I32Shl,
                right,
                ..
            } => instructions.iter().find_map(|candidate| match candidate {
                Instruction::Constant {
                    destination, value, ..
                } if destination == right => Some(*value),
                _ => None,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shift_values, vec![1, 2, 3, 4]);

    let parameter_stores = instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::Store8 { offset, .. } => Some(("i32.store8", *offset)),
            Instruction::Store { offset, .. } => Some(("i32.store", *offset)),
            _ => None,
        })
        .filter(|(_, offset)| *offset >= 56)
        .collect::<Vec<_>>();
    assert_eq!(
        parameter_stores,
        vec![
            ("i32.store8", 56),
            ("i32.store", 60),
            ("i32.store", 64),
            ("i32.store8", 68),
            ("i32.store8", 72),
        ]
    );
    let realloc_size = instructions
        .iter()
        .find_map(|instruction| match instruction {
            Instruction::Call {
                function,
                arguments,
                ..
            } if *function == crate::abi::REALLOC_SYMBOL => arguments.get(3).copied(),
            _ => None,
        })
        .expect("the tuple layout should allocate a parameter area");
    assert!(instructions.iter().any(|instruction| matches!(instruction,
        Instruction::Constant { destination, value: 76, .. }
            if *destination == realloc_size
    )));
}

pub(super) fn assert_wasm_artifact(module: &wasm::Module, import: &WasiImport, binary: &[u8]) {
    let core_import = module
        .imports
        .iter()
        .find(|candidate| candidate.module == import.module && candidate.name == import.name)
        .expect("the encoded module should contain the composite core import");
    let function_type = core_import
        .type_index
        .0
        .checked_sub(module.defined_type_count())
        .and_then(|index| module.types.get(index as usize))
        .expect("the composite import should reference a function type");
    assert_eq!(
        function_type.parameters.as_slice(),
        &[wasm_encoder::ValType::I32],
        "the imported signature is exactly one pointer"
    );
    let wat = wasmprinter::print_bytes(binary).expect("the composite artifact should print as WAT");
    assert!(wat.contains("\"wasi:io/streams@0.2.12\" \"take-shapes\""));
    for expected_store in [
        "i32.store8 offset=56",
        "i32.store offset=60",
        "i32.store offset=64",
        "i32.store8 offset=68",
        "i32.store8 offset=72",
    ] {
        assert!(
            wat.contains(expected_store),
            "missing {expected_store}: {wat}"
        );
    }
    assert!(
        wat.contains("i32.shl"),
        "flags are packed by bit shifts: {wat}"
    );
    assert!(
        wat.contains("i32.or"),
        "flags are combined into words: {wat}"
    );
    assert!(
        wat.contains("i32.const 76"),
        "the tuple occupies 76 bytes: {wat}"
    );
}
