use super::common::{RecordingLowerer, source_signature};
use super::*;
use crate::abi::{SourceType, WasiParamKind, WasiResultKind};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;
#[test]
fn flags_arguments_pack_boolean_fields_in_wit_declaration_order() {
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 1),
        module: "test:flags".into(),
        name: "take".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::Flags {
            names: vec!["write".into(), "read".into()],
        }],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
    };
    let source = SourceType::Record {
        fields: vec![
            ("read".into(), Box::new(SourceType::Boolean)),
            ("write".into(), Box::new(SourceType::Boolean)),
        ],
    };
    let mut lowerer = RecordingLowerer {
        product_field_types: HashMap::from([(0, ValueType::Boolean), (1, ValueType::Boolean)]),
        ..RecordingLowerer::default()
    };
    lower(
        &mut lowerer,
        &import,
        &source_signature(vec![source], SourceType::Unit),
        ValueId(7),
        &[ValueId(9)],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("Boolean record fields should pack into a canonical flags word");

    assert_eq!(lowerer.product_fields, vec![1, 0]);
    assert_eq!(
        lowerer
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::UnaryPrimitive {
                    op: UnaryOp::BoolToI32,
                    ..
                }
            ))
            .count(),
        2
    );
    assert_eq!(
        lowerer
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::Primitive {
                    op: NumericOp::I32Shl,
                    ..
                }
            ))
            .count(),
        1
    );
    let packed = lowerer
        .instructions
        .iter()
        .rev()
        .find_map(|instruction| match instruction {
            Instruction::Primitive {
                destination,
                op: NumericOp::I32Or,
                ..
            } => Some(*destination),
            _ => None,
        })
        .expect("the flags fields should combine with integer OR");
    assert!(lowerer.instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::CallVoid { arguments, .. } if arguments == &[packed]
    )));
}

#[test]
fn flags_arguments_split_after_thirty_two_bits() {
    let names = (0..33)
        .map(|index| format!("flag{index:03}"))
        .collect::<Vec<_>>();
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 1),
        module: "test:flags".into(),
        name: "take".into(),
        parameters: vec![ValueType::I32, ValueType::I32],
        param_kinds: vec![WasiParamKind::Flags {
            names: names.clone(),
        }],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
    };
    let source = SourceType::Record {
        fields: names
            .iter()
            .map(|name| (name.clone(), Box::new(SourceType::Boolean)))
            .collect(),
    };
    let mut lowerer = RecordingLowerer {
        product_field_types: (0..33).map(|index| (index, ValueType::Boolean)).collect(),
        ..RecordingLowerer::default()
    };
    lower(
        &mut lowerer,
        &import,
        &source_signature(vec![source], SourceType::Unit),
        ValueId(7),
        &[ValueId(9)],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("33 Boolean fields should pack into two canonical flags words");

    let arguments = lowerer
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            Instruction::CallVoid { arguments, .. } => Some(arguments),
            _ => None,
        });
    assert_eq!(arguments.map(Vec::len), Some(2));
    assert_eq!(lowerer.product_fields, (0..33).collect::<Vec<_>>());
    assert_eq!(
        lowerer
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::Primitive {
                    op: NumericOp::I32Shl,
                    ..
                }
            ))
            .count(),
        31
    );
}
