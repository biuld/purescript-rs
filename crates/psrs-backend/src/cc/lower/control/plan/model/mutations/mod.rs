//! The model must detect semantically wrong plans that still pass logical
//! type/scope, action coverage and terminal-shape checking.
mod flows;
mod lifetime;
mod replay;

use super::super::{
    build, check,
    ir::{Case, Exec, Op, Operand, State},
};
use super::{
    compare::equivalent, fixtures::Fixture, heap::*, interpret::Interpreter, reference::Reference,
};
use crate::control::{
    execution::Program,
    schema::{ComputationCase as C, ContextCase as X, SequenceCase as Q},
};
use std::rc::Rc;

fn edit(plan: &mut Exec, change: &mut impl FnMut(&mut Op) -> bool) -> bool {
    match plan {
        Exec::Steps { ops, next } => ops.iter_mut().any(&mut *change) || edit(next, change),
        Exec::Switch { arms, .. } => arms.iter_mut().any(|(_, arm)| edit(arm, change)),
        _ => false,
    }
}

fn arm(plan: &mut Exec, tag: C) -> &mut Exec {
    let Exec::Switch { arms, .. } = plan else {
        panic!("executor begins with computation selection");
    };
    &mut arms
        .iter_mut()
        .find(|(case, _)| *case == tag.tag())
        .unwrap()
        .1
}

fn detects(f: Fixture, program: Program, args: Vec<Value>, change: impl FnOnce(&mut Exec)) {
    let mut interpreter = Interpreter::new(f.heap.clone());
    let mut plan = build(program).exec().clone();
    change(&mut plan);
    check::check_for(program,&plan).expect("the negative must preserve the checked plan's logical types, scope, action inventory and terminal roles");
    interpreter.plans.insert(program, Rc::new(plan));
    let actual = interpreter.run(program, args.clone());
    let mut expected_heap = f.heap.clone();
    let expected = Reference::new(&mut expected_heap).run(program, &args);
    assert!(
        !matches!(actual, Err(Fault::InvalidModel | Fault::Exhausted)),
        "mutation must have a defined model execution: {actual:?}"
    );
    assert!(
        !equivalent(
            &interpreter.heap,
            &actual,
            &expected_heap,
            &expected,
            f.heap.objects.len()
        ),
        "model accepted a semantically wrong, structurally checked {program:?} plan"
    );
}
