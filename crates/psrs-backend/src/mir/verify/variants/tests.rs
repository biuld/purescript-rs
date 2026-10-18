//! The constructor contract applies to ordinary variants without control metadata.
use crate::TargetCapabilities;
use crate::cc::{Representation, RepresentationTable, VariantCase};
use crate::mir::layout::PlannedLayout;
use crate::mir::{BasicBlock, BlockId, Function, Instruction as I, Module, Terminator as T};
use crate::types::{FunctionId, HeapType, RefType, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn constructor(tag: i32, copy: bool) -> Module {
    let span = TextRange::new(0, 1);
    let table = RepresentationTable::from_parts(
        vec![Representation::Variant {
            cases: vec![
                VariantCase {
                    tag: 7,
                    fields: vec![],
                },
                VariantCase {
                    tag: 13,
                    fields: vec![],
                },
            ],
        }],
        Default::default(),
        Default::default(),
    );
    let layout = PlannedLayout::plan(&table, TargetCapabilities::default()).unwrap();
    let type_index = layout.variant_index(crate::cc::ReprId(0), 7).unwrap();
    let result_type = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(type_index),
    });
    let mut instructions = vec![I::Constant {
        destination: ValueId(0),
        value: tag,
        span,
    }];
    if copy {
        instructions.push(I::Copy {
            destination: ValueId(1),
            value: ValueId(0),
            span,
        });
    }
    instructions.push(I::StructNew {
        destination: ValueId(2),
        type_index,
        arguments: vec![ValueId(u32::from(copy))],
        span,
    });
    let symbol = SymbolId::new(ModuleId(0), 0);
    Module {
        tags: Vec::new(),
        name: "NominalConstructor".into(),
        types: layout.types().to_vec(),
        layout: Some(layout),
        strings: vec![],
        imports: vec![],
        entry: Some(symbol),
        span,
        functions: vec![Function {
            id: FunctionId(0),
            symbol,
            name: "constructor".into(),
            parameters: vec![],
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: result_type,
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: vec![],
                instructions,
                terminator: Some(T::Return {
                    value: ValueId(2),
                    span,
                }),
            }],
            result: ValueId(2),
            result_type,
            frame_weight: 0,
            span,
        }],
    }
}

#[test]
fn ordinary_variants_require_their_own_tag() {
    for copy in [false, true] {
        crate::mir::verify_module(&constructor(7, copy)).expect("matching nominal tag");
        for wrong in [0, 13] {
            let errors = crate::mir::verify_module(&constructor(wrong, copy)).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("initialize its planned case tag")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn ordinary_variant_base_cannot_bypass_case_construction() {
    let mut module = constructor(7, false);
    let base = module
        .layout
        .as_ref()
        .unwrap()
        .repr_index(crate::cc::ReprId(0))
        .unwrap();
    let I::StructNew { type_index, .. } = &mut module.functions[0].blocks[0].instructions[1] else {
        unreachable!()
    };
    *type_index = base;
    let base_type = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(base),
    });
    module.functions[0]
        .values
        .iter_mut()
        .find(|value| value.id == ValueId(2))
        .unwrap()
        .ty = base_type;
    module.functions[0].result_type = base_type;
    let errors = crate::mir::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("constructed at a planned case type")),
        "{errors:?}"
    );
}

#[test]
fn ordinary_variant_tag_cannot_become_mutable() {
    let mut module = constructor(7, false);
    let I::StructNew { type_index, .. } = module.functions[0].blocks[0].instructions[1] else {
        unreachable!()
    };
    let definition = module
        .types
        .iter_mut()
        .flat_map(|group| &mut group.0)
        .nth(type_index.0 as usize)
        .unwrap();
    let crate::types::CompositeType::Struct(fields) = &mut definition.composite else {
        unreachable!()
    };
    fields[0].mutable = true;
    let errors = crate::mir::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("tag must remain immutable i32")),
        "{errors:?}"
    );
}

#[test]
fn nominal_tag_proof_accepts_equal_constants_at_a_join() {
    for alternative in [7, 13] {
        let mut module = constructor(7, false);
        let function = &mut module.functions[0];
        let span = function.span;
        function.values.extend([
            ValueDecl {
                id: ValueId(3),
                ty: ValueType::Boolean,
            },
            ValueDecl {
                id: ValueId(4),
                ty: ValueType::I32,
            },
        ]);
        let mut construct = function.blocks[0].instructions.pop().unwrap();
        let I::StructNew { arguments, .. } = &mut construct else {
            unreachable!()
        };
        arguments[0] = ValueId(4);
        function.blocks = vec![
            BasicBlock {
                id: BlockId(0),
                parameters: vec![],
                instructions: vec![I::Constant {
                    destination: ValueId(3),
                    value: 1,
                    span,
                }],
                terminator: Some(T::Branch {
                    condition: ValueId(3),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span,
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: vec![],
                instructions: vec![I::Constant {
                    destination: ValueId(0),
                    value: 7,
                    span,
                }],
                terminator: Some(T::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(0)],
                    span,
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: vec![],
                instructions: vec![I::Constant {
                    destination: ValueId(1),
                    value: alternative,
                    span,
                }],
                terminator: Some(T::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(1)],
                    span,
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: vec![ValueId(4)],
                instructions: vec![construct],
                terminator: Some(T::Return {
                    value: ValueId(2),
                    span,
                }),
            },
        ];
        let result = crate::mir::verify_module(&module);
        if alternative == 7 {
            result.expect("equal tag constants at a normal join");
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|error| error.message.contains("initialize its planned case tag"))
            );
        }
    }
}
