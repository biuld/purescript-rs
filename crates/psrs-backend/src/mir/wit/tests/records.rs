use super::common::{RecordingLowerer, source_signature};
use super::*;
use crate::abi::{SourceType, WasiParamKind, WasiResultKind};
use psrs_hir::{ModuleId, SymbolId};
#[test]
fn record_arguments_flatten_in_wit_field_order() {
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:records".into(),
        name: "take".into(),
        parameters: vec![ValueType::F64, ValueType::I32],
        param_kinds: vec![WasiParamKind::Record {
            fields: vec![
                crate::abi::WasiField {
                    name: "second-value".into(),
                    kind: WasiParamKind::Float64,
                },
                crate::abi::WasiField {
                    name: "first".into(),
                    kind: WasiParamKind::Integer32,
                },
            ],
        }],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
    };
    let source = SourceType::Record {
        fields: vec![
            ("first".into(), Box::new(SourceType::Int)),
            ("secondValue".into(), Box::new(SourceType::Number)),
        ],
    };
    let mut lowerer = RecordingLowerer::default();
    lower(
        &mut lowerer,
        &import,
        &source_signature(vec![source], SourceType::Unit),
        ValueId(7),
        &[ValueId(9)],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("direct scalar record fields should flatten at the call boundary");

    assert_eq!(lowerer.product_fields, vec![1, 0]);
    assert!(matches!(
        lowerer.instructions.get(2),
        Some(Instruction::CallVoid { arguments, .. })
            if arguments == &[ValueId(0), ValueId(1)]
    ));
}

#[test]
fn nested_records_flatten_byte_lists_in_wit_field_order() {
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 1),
        module: "test:record-lists".into(),
        name: "take".into(),
        parameters: vec![ValueType::I32, ValueType::I32, ValueType::I32],
        param_kinds: vec![WasiParamKind::Record {
            fields: vec![
                crate::abi::WasiField {
                    name: "code".into(),
                    kind: WasiParamKind::Integer32,
                },
                crate::abi::WasiField {
                    name: "payload".into(),
                    kind: WasiParamKind::Record {
                        fields: vec![crate::abi::WasiField {
                            name: "text".into(),
                            kind: WasiParamKind::List,
                        }],
                    },
                },
            ],
        }],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
    };
    // Source record fields are normalized alphabetically. The nested source
    // product therefore projects by name while the ABI emits WIT declaration
    // order, then expands the String to pointer and length.
    let source = SourceType::Record {
        fields: vec![
            ("code".into(), Box::new(SourceType::Int)),
            (
                "payload".into(),
                Box::new(SourceType::Record {
                    fields: vec![("text".into(), Box::new(SourceType::String))],
                }),
            ),
        ],
    };
    let mut lowerer = RecordingLowerer::default();
    lower(
        &mut lowerer,
        &import,
        &source_signature(vec![source], SourceType::Unit),
        ValueId(15),
        &[ValueId(20)],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("nested records with byte-list fields should flatten");

    assert_eq!(lowerer.product_fields, vec![0, 1, 0]);
    assert!(lowerer.instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::CallVoid { arguments, .. }
            if arguments == &[ValueId(0), ValueId(6), ValueId(4)]
    )));
}
