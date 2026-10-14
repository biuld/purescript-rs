//! Same-shaped replacements cannot bypass the Bind or Resume machine steps.
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::{AssignmentKind as A, ControlHelperRole as R};
use psrs_backend::mir::{Instruction as I, Terminator as T};

#[derive(Clone, Copy)]
enum Mutation {
    Body,
    Queue,
    Callback,
    Answer,
    Extent,
    AppendTail,
}

fn message(mutation: Mutation) -> &'static str {
    match mutation {
        Mutation::Body | Mutation::Callback => {
            "Bind must enter its body with its own callback and current queue"
        }
        Mutation::Queue => "execution step must retain its pending queue",
        Mutation::Answer => "Resume must return its current saved answer",
        Mutation::Extent => "Resume must use its replayed queue and activation",
        Mutation::AppendTail => "Resume must append replay to its current queue",
    }
}

fn cc_step(tag: i32, mutation: Mutation) {
    let mut module = crate::lower_main_to_cc(SOURCE).unwrap().cc;
    let (symbol, _) = binding(&module.representations, R::Executor);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let p = function.parameters.clone();
    let A::TagSwitch { cases, .. } = &mut function.assignments.last_mut().unwrap().kind else {
        panic!("input dispatch")
    };
    let case = cases.iter_mut().find(|case| case.tag == tag).unwrap();
    let next = case
        .assignments
        .iter()
        .find_map(|a| match &a.kind {
            A::DirectCall {
                function,
                arguments,
                ..
            } if *function == symbol => Some(arguments.clone()),
            _ => None,
        })
        .unwrap();
    match mutation {
        Mutation::Body | Mutation::Queue | Mutation::Extent => {
            let A::DirectCall { arguments, .. } = &mut case
                .assignments
                .iter_mut()
                .find(|a| a.destination == case.value)
                .unwrap()
                .kind
            else {
                panic!("transition")
            };
            let index = match mutation {
                Mutation::Body => 2,
                Mutation::Queue => 3,
                _ => 4,
            };
            arguments[index] = p[index];
        }
        Mutation::Callback | Mutation::AppendTail => {
            let A::VariantNew { fields, .. } = &mut case
                .assignments
                .iter_mut()
                .find(|a| a.destination == next[3])
                .unwrap()
                .kind
            else {
                panic!("pending queue")
            };
            match mutation {
                Mutation::Callback => fields[0] = p[1],
                _ => fields[1] = fields[0],
            }
        }
        Mutation::Answer => {
            let A::VariantNew { fields, .. } = &mut case
                .assignments
                .iter_mut()
                .find(|a| a.destination == next[2])
                .unwrap()
                .kind
            else {
                panic!("returned answer")
            };
            fields[0] = p[0];
        }
    }
    let errors = psrs_backend::cc::verify_module(&module).expect_err("broken execution step");
    assert!(
        errors.iter().any(|e| e.message.contains(message(mutation))),
        "{errors:?}"
    );
}

fn mir_step(tag: i32, mutation: Mutation) {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let target = function
        .blocks
        .iter()
        .find_map(|b| match &b.terminator {
            Some(T::Switch { cases, .. })
                if cases.iter().map(|(t, _)| *t).collect::<Vec<_>>() == vec![0, 1, 2, 3, 4, 5] =>
            {
                Some(cases.iter().find(|(t, _)| *t == tag).unwrap().1)
            }
            _ => None,
        })
        .unwrap();
    let p = function.parameters.clone();
    let header = match function
        .blocks
        .iter()
        .find(|b| b.id == function.entry)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
    {
        T::Jump { target, .. } => *target,
        _ => panic!("loop preheader"),
    };
    let mut pending = vec![target];
    let mut seen = std::collections::HashSet::new();
    let mut transition = None;
    while let Some(id) = pending.pop() {
        if id == header || !seen.insert(id) {
            continue;
        }
        let block = function.blocks.iter().find(|b| b.id == id).unwrap();
        match block.terminator.as_ref().unwrap() {
            T::Jump {
                target, arguments, ..
            } if *target == header => {
                transition = Some((id, arguments.clone()));
                break;
            }
            T::Jump { target, .. } => pending.push(*target),
            T::Branch {
                then_block,
                else_block,
                ..
            } => pending.extend([*then_block, *else_block]),
            T::Switch { cases, default, .. } => {
                pending.extend(cases.iter().map(|(_, target)| *target));
                pending.push(*default);
            }
            _ => {}
        }
    }
    let (transition, next) = transition.expect("step loop transition");
    let aliases = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i {
            I::Copy {
                destination, value, ..
            }
            | I::RefCast {
                destination, value, ..
            } => Some((*destination, *value)),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    let origin = |mut value| {
        for _ in 0..aliases.len() {
            if let Some(source) = aliases.get(&value) {
                value = *source;
            } else {
                break;
            }
        }
        value
    };
    match mutation {
        Mutation::Body | Mutation::Queue | Mutation::Extent => {
            let T::Jump { arguments, .. } = function
                .blocks
                .iter_mut()
                .find(|b| b.id == transition)
                .unwrap()
                .terminator
                .as_mut()
                .unwrap()
            else {
                unreachable!()
            };
            let index = match mutation {
                Mutation::Body => 2,
                Mutation::Queue => 3,
                _ => 4,
            };
            arguments[index] = p[index];
        }
        Mutation::Callback | Mutation::AppendTail => {
            let I::StructNew { arguments, .. } = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| i.destination() == Some(origin(next[3])))
                .unwrap()
            else {
                panic!("pending queue")
            };
            match mutation {
                Mutation::Callback => arguments[1] = p[1],
                _ => arguments[2] = arguments[1],
            }
        }
        Mutation::Answer => {
            let I::StructNew { arguments, .. } = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| i.destination() == Some(origin(next[2])))
                .unwrap()
            else {
                panic!("returned answer")
            };
            arguments[1] = p[0];
        }
    }
    let errors = psrs_backend::mir::verify_module(&module).expect_err("broken execution step");
    assert!(
        errors.iter().any(|e| e.message.contains(message(mutation))),
        "{errors:?}"
    );
}

#[test]
fn cc_bind_requires_its_body_callback_and_queue() {
    for mutation in [Mutation::Body, Mutation::Callback, Mutation::Queue] {
        cc_step(2, mutation);
    }
}
#[test]
fn mir_bind_requires_its_body_callback_and_queue() {
    for mutation in [Mutation::Body, Mutation::Callback, Mutation::Queue] {
        mir_step(2, mutation);
    }
}
#[test]
fn cc_resume_requires_its_answer_replayed_extent_and_pending_queue() {
    for mutation in [
        Mutation::Answer,
        Mutation::Extent,
        Mutation::Queue,
        Mutation::AppendTail,
    ] {
        cc_step(3, mutation);
    }
}
#[test]
fn mir_resume_requires_its_answer_replayed_extent_and_pending_queue() {
    for mutation in [
        Mutation::Answer,
        Mutation::Extent,
        Mutation::Queue,
        Mutation::AppendTail,
    ] {
        mir_step(3, mutation);
    }
}
