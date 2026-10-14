//! MIR dispatch negatives and provenance positives.
use super::*;

#[test]
fn mir_dispatch_requires_a_trapping_default() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let module = crate::compile_main_stages(SOURCE).unwrap().mir;
    for role in [R::Executor, R::CloneSequence, R::RebuildSequence] {
        let (symbol, _) = binding(&input.cc.representations, role);
        let mut broken = module.clone();
        let function = broken
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        let span = function.span;
        let default = function
            .blocks
            .iter()
            .find_map(|block| match &block.terminator {
                Some(T::Switch { cases, default, .. })
                    if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>() == required(role) =>
                {
                    Some(*default)
                }
                _ => None,
            })
            .expect("input dispatch");
        let block = function
            .blocks
            .iter_mut()
            .find(|block| block.id == default)
            .unwrap();
        // Break the trap without leaving the block, so only the trap
        // requirement is violated.
        block.instructions.clear();
        block.terminator = Some(T::Jump {
            target: default,
            arguments: vec![],
            span,
        });
        let errors = psrs_backend::mir::verify_module(&broken).expect_err("non-trapping default");
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("must trap on unsupported alternatives")),
            "{role:?}: {errors:?}"
        );
    }
}

#[test]
fn mir_dispatch_requires_an_immutable_i32_tag() {
    use psrs_backend::types::{CompositeType, StorageType};
    let module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let mut broken = module.clone();
    let mut changed = false;
    for group in &mut broken.types {
        for ty in &mut group.0 {
            let CompositeType::Struct(fields) = &mut ty.composite else {
                continue;
            };
            let Some(first) = fields.first_mut() else {
                continue;
            };
            if first.storage == StorageType::I32 && !first.mutable {
                first.mutable = true;
                changed = true;
            }
        }
    }
    assert!(changed, "no immutable i32 tag to mutate");
    let errors = psrs_backend::mir::verify_module(&broken).expect_err("mutable tag");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("immutable i32 tag field")),
        "{errors:?}"
    );
}

#[test]
fn mir_dispatch_resolves_a_reference_copy_of_the_input() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let module = crate::compile_main_stages(SOURCE).unwrap().mir;
    for role in [R::Executor, R::CloneSequence, R::RebuildSequence] {
        let (symbol, _) = binding(&input.cc.representations, role);
        let mut broken = module.clone();
        let function = broken
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        let span = function.span;
        let tag = function
            .blocks
            .iter()
            .find_map(|block| match &block.terminator {
                Some(T::Switch { value, cases, .. })
                    if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>() == required(role) =>
                {
                    Some(*value)
                }
                _ => None,
            })
            .expect("input dispatch");
        let (block_index, cast) = function
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block_index, block)| {
                block
                    .instructions
                    .iter()
                    .find_map(|instruction| match instruction {
                        I::StructGet {
                            destination,
                            field: 0,
                            value,
                            ..
                        } if *destination == tag => Some((block_index, *value)),
                        _ => None,
                    })
            })
            .expect("tag projection");
        let cast_index = function.blocks[block_index]
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction, I::RefCast { destination, .. } if *destination == cast)
            })
            .expect("tag cast");
        let source = match &function.blocks[block_index].instructions[cast_index] {
            I::RefCast { value, .. } => *value,
            _ => unreachable!(),
        };
        let ty = function
            .values
            .iter()
            .find(|value| value.id == source)
            .unwrap()
            .ty;
        let alias = ValueId(
            function
                .values
                .iter()
                .map(|value| value.id.0)
                .max()
                .unwrap()
                + 1,
        );
        function
            .values
            .push(psrs_backend::types::ValueDecl { id: alias, ty });
        let block = &mut function.blocks[block_index];
        block.instructions.insert(
            cast_index,
            I::Copy {
                destination: alias,
                value: source,
                span,
            },
        );
        let I::RefCast { value, .. } = &mut block.instructions[cast_index + 1] else {
            unreachable!()
        };
        *value = alias;
        psrs_backend::mir::verify_module(&broken)
            .expect("a reference copy must preserve dispatch provenance");
    }
}

#[test]
fn mir_dispatch_rejects_a_projection_of_the_wrong_case() {
    use psrs_backend::types::{CompositeType, DefinedTypeId, HeapType, RefType, ValueType};
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let module = crate::compile_main_stages(SOURCE).unwrap().mir;
    for role in [R::Executor, R::CloneSequence, R::RebuildSequence] {
        let (symbol, _) = binding(&input.cc.representations, role);
        let mut broken = module.clone();
        let function = broken
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        let tag = function
            .blocks
            .iter()
            .find_map(|block| match &block.terminator {
                Some(T::Switch { value, cases, .. })
                    if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>() == required(role) =>
                {
                    Some(*value)
                }
                _ => None,
            })
            .expect("input dispatch");
        let base = function
            .blocks
            .iter()
            .find_map(|block| {
                block
                    .instructions
                    .iter()
                    .find_map(|instruction| match instruction {
                        I::StructGet {
                            destination,
                            field: 0,
                            type_index,
                            ..
                        } if *destination == tag => Some(*type_index),
                        _ => None,
                    })
            })
            .expect("tag projection");
        let flat = broken
            .types
            .iter()
            .flat_map(|group| group.0.iter().cloned())
            .collect::<Vec<_>>();
        let cases = flat
            .iter()
            .enumerate()
            .filter(|(_, ty)| ty.supertype == Some(base))
            .map(|(index, _)| DefinedTypeId(index as u32))
            .collect::<Vec<_>>();
        let storage = |id: DefinedTypeId, field: u32| match &flat.get(id.0 as usize)?.composite {
            CompositeType::Struct(fields) => fields.get(field as usize).map(|field| field.storage),
            _ => None,
        };
        let mut mutation = None;
        'search: for (block_index, block) in function.blocks.iter().enumerate() {
            for (index, instruction) in block.instructions.iter().enumerate() {
                let I::StructGet {
                    type_index,
                    field,
                    value,
                    ..
                } = instruction
                else {
                    continue;
                };
                if *field == 0 || !cases.contains(type_index) {
                    continue;
                }
                let Some(current) = storage(*type_index, *field) else {
                    continue;
                };
                let Some(alternative) = cases
                    .iter()
                    .copied()
                    .find(|case| case != type_index && storage(*case, *field) == Some(current))
                else {
                    continue;
                };
                if block.instructions.iter().any(
                    |producer| matches!(producer, I::RefCast { destination, .. } if destination == value),
                ) {
                    mutation = Some((block_index, index, *value, alternative));
                    break 'search;
                }
            }
        }
        let Some((block_index, index, cast, alternative)) = mutation else {
            panic!("{role:?}: no projection to mutate");
        };
        let cast_index = function.blocks[block_index]
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction, I::RefCast { destination, .. } if *destination == cast)
            })
            .expect("case cast");
        let I::RefCast { reference, .. } =
            &mut function.blocks[block_index].instructions[cast_index]
        else {
            unreachable!()
        };
        *reference = RefType {
            nullable: false,
            heap: HeapType::Index(alternative),
        };
        function
            .values
            .iter_mut()
            .find(|declaration| declaration.id == cast)
            .unwrap()
            .ty = ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(alternative),
        });
        let I::StructGet { type_index, .. } = &mut function.blocks[block_index].instructions[index]
        else {
            unreachable!()
        };
        *type_index = alternative;
        let errors = psrs_backend::mir::verify_module(&broken).expect_err("wrong projection");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("control projection must match")),
            "{role:?}: {errors:?}"
        );
    }
}
