//! Checkpoint identities are checked independently of physical reference types.
use psrs_backend::cc::{
    Assignment, AssignmentKind as A, ControlHelperId, ControlHelperRole as R, ValueId,
};
use psrs_backend::mir::{Instruction as I, Terminator as T};

pub(super) const SOURCE: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
main :: Int
main = (C.run 0 handler (C.perform (Op \x -> x)))._2
";

fn cc_visit(body: &mut [Assignment], mutate: &mut impl FnMut(&mut A) -> bool) -> bool {
    for assignment in body {
        if mutate(&mut assignment.kind) {
            return true;
        }
        match &mut assignment.kind {
            A::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    if cc_visit(&mut case.assignments, mutate) {
                        return true;
                    }
                }
                if cc_visit(default_assignments, mutate) {
                    return true;
                }
            }
            A::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                if cc_visit(then_assignments, mutate) || cc_visit(else_assignments, mutate) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub(super) fn binding(
    table: &psrs_backend::cc::RepresentationTable,
    role: R,
) -> (psrs_hir::SymbolId, ControlHelperId) {
    let index = table
        .control_helpers
        .iter()
        .position(|helper| helper.role() == role)
        .unwrap();
    (
        table.control_helpers[index].function(),
        ControlHelperId(index as u32),
    )
}

fn cc_error(input: &psrs_backend::BackendInput, message: &str) {
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{errors:?}"
    );
}

pub(super) fn mir_error(module: &psrs_backend::mir::Module, message: &str) {
    let errors = psrs_backend::mir::verify_module(module).unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{errors:?}"
    );
}

fn mir_parameters(function: &psrs_backend::mir::Function) -> Vec<ValueId> {
    let target = function
        .blocks
        .iter()
        .find(|block| block.id == function.entry)
        .and_then(|block| match &block.terminator {
            Some(T::Jump {
                target, arguments, ..
            }) if arguments == &function.parameters => Some(*target),
            _ => None,
        });
    function
        .blocks
        .iter()
        .find(|block| Some(block.id) == target)
        .map_or_else(
            || function.parameters.clone(),
            |block| block.parameters.clone(),
        )
}

#[test]
fn cc_rejects_missing_raw_resumption_binding() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    input.cc.representations.control_resumption = None;
    cc_error(&input, "no valid raw resumption binding");
}

#[test]
fn cc_rejects_capturing_execution_extent_as_dispatch_checkpoint() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let raw = input
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let executor = binding(&input.cc.representations, R::Executor).0;
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let wrong = function.parameters[4];
    assert!(cc_visit(
        &mut function.assignments,
        &mut |kind| match kind {
            A::FunctionRef {
                function, captures, ..
            } if *function == raw => {
                captures[1] = wrong;
                true
            }
            _ => false,
        }
    ));
    cc_error(&input, "current dispatch context checkpoint");
}

#[test]
fn mir_rejects_capturing_execution_extent_as_dispatch_checkpoint() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let raw = stages
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let executor = binding(&stages.cc.representations, R::Executor).0;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let wrong = mir_parameters(function)[4];
    let captures = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match instruction {
            I::ClosureNew {
                function, captures, ..
            } if *function == raw => Some(captures),
            _ => None,
        })
        .unwrap();
    captures[1] = wrong;
    mir_error(&stages.mir, "current dispatch context checkpoint");
}

#[test]
fn cc_rejects_a_raw_frame_checkpoint_from_another_capture_slot() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let raw = input
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .unwrap();
    let index = function
        .assignments
        .iter_mut()
        .find_map(|assignment| match &mut assignment.kind {
            A::ClosureGetCapture { index, .. } if *index == 1 => Some(index),
            _ => None,
        })
        .unwrap();
    *index = 2;
    // Keep the ordinary dense capture contract valid, so this exercises identity.
    let checkpoint = function
        .assignments
        .iter()
        .find_map(|assignment| {
            matches!(assignment.kind, A::ClosureGetCapture { index: 2, .. })
                .then_some(assignment.destination)
        })
        .unwrap();
    let ty = function
        .values
        .iter()
        .find(|declaration| declaration.id == checkpoint)
        .unwrap()
        .ty;
    let unused = ValueId(
        function
            .values
            .iter()
            .map(|declaration| declaration.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function
        .values
        .push(psrs_backend::cc::ValueDecl { id: unused, ty });
    function.assignments.insert(
        0,
        Assignment {
            destination: unused,
            kind: A::ClosureGetCapture {
                closure: function.parameters[0],
                index: 1,
            },
            span: function.span,
        },
    );
    cc_error(&input, "its own queue and checkpoint capture slots");
}

#[test]
fn mir_rejects_a_raw_frame_checkpoint_from_another_capture_slot() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let raw = stages
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .unwrap();
    let index = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match instruction {
            I::ClosureGetCapture { index, .. } if *index == 1 => Some(index),
            _ => None,
        })
        .unwrap();
    *index = 2;
    mir_error(&stages.mir, "its own queue and checkpoint capture slots");
}

#[test]
fn cc_rejects_a_raw_frame_built_without_its_checkpoint_check() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let raw = input
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let check = binding(&input.cc.representations, R::Check).1;
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .unwrap();
    let count = function.assignments.len();
    function.assignments.retain(|assignment| !matches!(assignment.kind, A::DirectCall { contract: Some(id), .. } if id == check));
    assert_eq!(function.assignments.len(), count - 1);
    cc_error(
        &input,
        "check its saved checkpoint before construction or replay",
    );
}

#[test]
fn mir_rejects_a_raw_frame_built_without_its_checkpoint_check() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let raw = stages
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let check = binding(&stages.cc.representations, R::Check).1;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .unwrap();
    let mut removed = 0;
    for block in &mut function.blocks {
        block.instructions.retain(|instruction| {
            let remove = matches!(instruction, I::Call { contract: Some(id), .. } if *id == check);
            removed += usize::from(remove);
            !remove
        });
    }
    assert_eq!(removed, 1);
    mir_error(
        &stages.mir,
        "check its saved checkpoint before construction or replay",
    );
}

#[test]
fn cc_rejects_replay_checking_current_extent_instead_of_saved_checkpoint() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let executor = binding(&input.cc.representations, R::Executor).0;
    let check = binding(&input.cc.representations, R::Check).1;
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let wrong = function.parameters[4];
    assert!(cc_visit(
        &mut function.assignments,
        &mut |kind| match kind {
            A::DirectCall {
                contract: Some(id),
                arguments,
                ..
            } if *id == check => {
                arguments[0] = wrong;
                true
            }
            _ => false,
        }
    ));
    cc_error(
        &input,
        "check its saved checkpoint before construction or replay",
    );
}

#[test]
fn mir_rejects_replay_checking_current_extent_instead_of_saved_checkpoint() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let executor = binding(&stages.cc.representations, R::Executor).0;
    let check = binding(&stages.cc.representations, R::Check).1;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let wrong = mir_parameters(function)[4];
    let arguments = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match instruction {
            I::Call {
                contract: Some(id),
                arguments,
                ..
            } if *id == check => Some(arguments),
            _ => None,
        })
        .unwrap();
    arguments[0] = wrong;
    mir_error(
        &stages.mir,
        "check its saved checkpoint before construction or replay",
    );
}

#[test]
fn checked_checkpoint_reference_aliases_survive_physical_lowering() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let raw = input
        .cc
        .representations
        .control_resumption
        .as_ref()
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .unwrap();
    let checkpoint = function
        .assignments
        .iter()
        .find_map(|assignment| {
            matches!(assignment.kind, A::ClosureGetCapture { index: 1, .. })
                .then_some(assignment.destination)
        })
        .unwrap();
    let declaration = function
        .values
        .iter()
        .find(|declaration| declaration.id == checkpoint)
        .unwrap()
        .clone();
    let alias = ValueId(
        function
            .values
            .iter()
            .map(|declaration| declaration.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(psrs_backend::cc::ValueDecl {
        id: alias,
        ty: declaration.ty,
    });
    let position = function
        .assignments
        .iter()
        .position(|assignment| assignment.destination == checkpoint)
        .unwrap()
        + 1;
    function.assignments.insert(
        position,
        Assignment {
            destination: alias,
            kind: A::AggregateConvert {
                destination: alias,
                value: checkpoint,
                conversion: psrs_backend::cc::AggregateConvert {
                    source: declaration.ty,
                    destination: declaration.ty,
                    plan: psrs_backend::cc::ValueConversion::Identity,
                },
            },
            span: function.span,
        },
    );
    for assignment in &mut function.assignments[position + 1..] {
        match &mut assignment.kind {
            A::DirectCall { arguments, .. } => {
                for argument in arguments {
                    if *argument == checkpoint {
                        *argument = alias
                    }
                }
            }
            A::VariantNew { fields, .. } => {
                for field in fields {
                    if *field == checkpoint {
                        *field = alias
                    }
                }
            }
            _ => {}
        }
    }
    psrs_backend::cc::verify_module(&input.cc).unwrap();
    psrs_backend::compile_cc(input).unwrap();
}
