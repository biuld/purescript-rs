use super::*;
use psrs_hir::SymbolId;
#[test]
fn linear_wit_record_projection_uses_the_planned_field_offset() {
    use crate::cc::{RefShape, Reference, ValueShape};
    use crate::mir::planner::{LinearField, LinearMemoryLayout, LinearRepresentation};
    use crate::types::{TableSlot, ValueDecl};
    use std::collections::HashMap;

    let record = crate::cc::ReprId(0);
    let layout = LinearMemoryLayout {
        representations: HashMap::from([(
            record,
            LinearRepresentation::Product {
                fields: vec![
                    LinearField {
                        offset: 0,
                        value: ValueShape::Integer,
                    },
                    LinearField {
                        offset: 8,
                        value: ValueShape::Number,
                    },
                ],
                size: 16,
                alignment: 8,
            },
        )]),
        signatures: HashMap::new(),
        types: Vec::new(),
        next_offset: 16,
    };
    let shapes = HashMap::from([(
        ValueId(0),
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(record),
        }),
    )]);
    let imports = HashMap::new();
    let scalar_helpers = crate::mir::scalar_helpers::ScalarHelpers::default();
    let table_slots = HashMap::<SymbolId, TableSlot>::new();
    let mut lowerer = super::super::super::lower_linear::LinearFunctionLowerer {
        next_block: 1,
        blocks: vec![crate::mir::BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        }],
        values: vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        shapes,
        next_value: 1,
        layout: &layout,
        wit_imports: &imports,
        scalar_helpers: &scalar_helpers,
        table_slots: &table_slots,
    };
    let field = WitCallLowerer::wit_product_field(
        &mut lowerer,
        BlockId(0),
        ValueId(0),
        1,
        TextRange::new(0, 1),
    )
    .expect("the second product field should load as a number");

    assert_eq!(field, ValueId(1));
    assert_eq!(lowerer.values[1].ty, ValueType::F64);
    assert!(matches!(
        lowerer.blocks[0].instructions.as_slice(),
        [Instruction::LinearLoad {
            destination: ValueId(1),
            address: ValueId(0),
            offset: 8,
            object_bytes: 16,
            alignment: 8,
            ty: ValueType::F64,
            ..
        }]
    ));
}
