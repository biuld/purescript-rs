//! A source return or operation must apply its own scoped clause.
use super::control_checkpoints::{SOURCE, binding};
use super::control_dispatch::{carrier_repr, visit};
use psrs_backend::cc::{AssignmentKind as A, ControlHelperRole as R, ValueId};
use psrs_backend::mir::{BlockId, Function, Instruction as I, Terminator as T};
use psrs_backend::types::DefinedTypeId;
use std::collections::{HashSet, VecDeque};

fn successors(terminator: &T) -> Vec<BlockId> {
    match terminator {
        T::Jump { target, .. } => vec![*target],
        T::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        T::Switch { cases, default, .. } => cases
            .iter()
            .map(|(_, target)| *target)
            .chain(std::iter::once(*default))
            .collect(),
        _ => Vec::new(),
    }
}

/// Blocks of one input-dispatch arm, excluding the dispatch header itself.
fn arm(function: &Function, dispatch: BlockId, target: BlockId) -> HashSet<BlockId> {
    let mut seen = HashSet::new();
    let mut pending = VecDeque::from([target]);
    while let Some(id) = pending.pop_front() {
        if id == dispatch || !seen.insert(id) {
            continue;
        }
        if let Some(terminator) = function
            .blocks
            .iter()
            .find(|block| block.id == id)
            .and_then(|block| block.terminator.as_ref())
        {
            for successor in successors(terminator) {
                if successor != dispatch {
                    pending.push_back(successor);
                }
            }
        }
    }
    seen
}

/// The base type named by a switch's `i32` selector.
fn selector_base(function: &Function, value: ValueId) -> Option<DefinedTypeId> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            I::StructGet {
                destination,
                field: 0,
                type_index,
                ..
            } if *destination == value => Some(*type_index),
            _ => None,
        })
}

fn executor(function: &Function) -> (BlockId, Vec<(i32, BlockId)>) {
    function
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            Some(T::Switch { cases, .. }) => {
                let mut tags = cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>();
                tags.sort_unstable();
                (tags == vec![0, 1, 2, 3, 4, 5]).then(|| (block.id, cases.clone()))
            }
            _ => None,
        })
        .expect("executor input dispatch")
}

/// Interprets the source step `tag` and rewires its scoped clause projection
/// from `from` to `to`.
fn check_clause(source_tag: i32, from: u32, to: u32, message: &str) {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut broken = module.clone();
    let flat = broken
        .types
        .iter()
        .flat_map(|group| group.0.iter().cloned())
        .collect::<Vec<_>>();
    let function = broken
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let (dispatch, cases) = executor(function);
    let target = cases
        .iter()
        .find(|(tag, _)| *tag == source_tag)
        .map(|(_, block)| *block)
        .unwrap();
    let arm = arm(function, dispatch, target);
    // The scoped context clause is projected from the context case of the
    // context dispatch inside this arm.
    let scoped = function
        .blocks
        .iter()
        .filter(|block| arm.contains(&block.id))
        .find_map(|block| match &block.terminator {
            Some(T::Switch { value, cases, .. })
                if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>() == vec![0, 1] =>
            {
                let base = selector_base(function, *value)?;
                flat.iter()
                    .enumerate()
                    .filter(|(_, ty)| ty.supertype == Some(base))
                    .max_by_key(|(_, ty)| match &ty.composite {
                        psrs_backend::types::CompositeType::Struct(fields) => fields.len(),
                        _ => 0,
                    })
                    .map(|(index, _)| DefinedTypeId(index as u32))
            }
            _ => None,
        })
        .expect("scoped context case");
    let location = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block_index, block)| {
            if !arm.contains(&block.id) {
                return None;
            }
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(index, instruction)| match instruction {
                    I::StructGet {
                        type_index, field, ..
                    } if *type_index == scoped && *field == from => Some((block_index, index)),
                    _ => None,
                })
        })
        .expect("scoped clause projection");
    let I::StructGet { field, .. } = &mut function.blocks[location.0].instructions[location.1]
    else {
        unreachable!()
    };
    *field = to;
    let errors = psrs_backend::mir::verify_module(&broken).expect_err("wrong clause");
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{errors:?}"
    );
}

#[test]
fn cc_source_return_applies_its_own_return_clause() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let context = carrier_repr(table, R::Executor, 5);
    let (symbol, _) = binding(table, R::Executor);
    let mut broken = input.cc.clone();
    let function = broken
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let changed = visit(&mut function.assignments, &mut |assignment| {
        let A::VariantGet {
            representation,
            case: 1,
            field,
            ..
        } = &mut assignment.kind
        else {
            return false;
        };
        if *representation == context && *field == 0 {
            *field = 1;
            true
        } else {
            false
        }
    });
    assert!(changed, "return clause projection");
    let errors = psrs_backend::cc::verify_module(&broken).expect_err("wrong return clause");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("source return must apply its scoped return clause")),
        "{errors:?}"
    );
}

#[test]
fn cc_source_operation_applies_its_own_operation_clause() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let context = carrier_repr(table, R::Executor, 5);
    let (symbol, _) = binding(table, R::Executor);
    let mut broken = input.cc.clone();
    let function = broken
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let changed = visit(&mut function.assignments, &mut |assignment| {
        let A::VariantGet {
            representation,
            case: 1,
            field,
            ..
        } = &mut assignment.kind
        else {
            return false;
        };
        if *representation == context && *field == 1 {
            *field = 0;
            true
        } else {
            false
        }
    });
    assert!(changed, "operation clause projection");
    let errors = psrs_backend::cc::verify_module(&broken).expect_err("wrong operation clause");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("source operation must apply its scoped operation clause")),
        "{errors:?}"
    );
}

#[test]
fn mir_source_return_applies_its_own_return_clause() {
    check_clause(0, 1, 2, "source return must apply its scoped return clause");
}

#[test]
fn mir_source_operation_applies_its_own_operation_clause() {
    check_clause(
        1,
        2,
        1,
        "source operation must apply its scoped operation clause",
    );
}
