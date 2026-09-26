use super::*;
use crate::cc::{RefShape, Reference, Representation, RepresentationTable, ValueShape};
use crate::mir::layout::PlannedLayout;
use crate::types::ValueType;
use psrs_span::TextRange;

#[test]
fn gc_wit_record_projection_uses_the_planned_product_type() {
    let mut representations = RepresentationTable::default();
    let record = representations.reserve();
    representations.set(
        record,
        Representation::Product {
            fields: vec![ValueShape::Integer, ValueShape::Number],
        },
    );
    let layout = PlannedLayout::plan(&representations, crate::TargetCapabilities::default())
        .expect("the product requirement should receive a concrete GC layout");
    let record_shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(record),
    });
    let lowerer_span = TextRange::new(0, 1);
    let mut lowerer = FunctionLowerer {
        conversion_helpers: None,
        next_block: 1,
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        }],
        values: vec![ValueDecl {
            id: ValueId(0),
            ty: layout.value_type(&record_shape).expect("record value type"),
        }],
        next_value: 1,
        wit_imports: &HashMap::new(),
        scalar_helpers: &ScalarHelpers::default(),
        layout: &layout,
        literals: None,
        owned_handles: Vec::new(),
    };

    let field = lowerer
        .wit_product_field(BlockId(0), ValueId(0), 1, lowerer_span)
        .expect("the second record field should project as a number");
    assert_eq!(field, ValueId(1));
    assert_eq!(lowerer.values[1].ty, ValueType::F64);
    assert!(matches!(
        lowerer.blocks[0].instructions.as_slice(),
        [Instruction::StructGet {
            destination: ValueId(1),
            type_index: crate::types::DefinedTypeId(0),
            field: 1,
            value: ValueId(0),
            ..
        }]
    ));
}
