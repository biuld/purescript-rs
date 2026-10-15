//! Execute the actual plan with concrete references, state cells and callbacks.
use super::super::ir::{Case, Exec, Operand, State};
use super::heap::*;
use crate::control::execution::{ActionId, Program, Selection};
use crate::control::schema::ControlHelperRole as R;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub(super) enum Flow {
    Exit(Step),
    Tail(Program, Vec<Value>),
}

pub(super) struct Interpreter {
    pub heap: Heap,
    pub actions: HashSet<ActionId>,
    pub plans: HashMap<Program, Rc<Exec>>,
    fuel: usize,
}

pub(super) fn program(role: R) -> Program {
    match role {
        R::Executor => Program::Executor,
        R::Run => Program::Run,
        R::CloneSequence => Program::CloneSequence,
        R::RebuildSequence => Program::RebuildSequence,
        R::Start => Program::Start,
        R::Check => Program::Check,
        R::Close => Program::Close,
        R::Once => Program::Once,
        R::Cancel => Program::Cancel,
    }
}

impl Interpreter {
    pub(super) fn new(heap: Heap) -> Self {
        let plans = [
            Program::Executor,
            Program::Run,
            Program::CloneSequence,
            Program::RebuildSequence,
            Program::Start,
            Program::Check,
            Program::Close,
            Program::Once,
            Program::Cancel,
            Program::RawResume,
        ]
        .into_iter()
        .map(|program| {
            (
                program,
                Rc::new(super::super::build(program).exec().clone()),
            )
        })
        .collect();
        Self {
            heap,
            plans,
            actions: HashSet::new(),
            fuel: 100_000,
        }
    }

    pub(super) fn tick(&mut self) -> Checked<()> {
        self.fuel = self.fuel.checked_sub(1).ok_or(Fault::Exhausted)?;
        Ok(())
    }
    pub(super) fn reset(&mut self, heap: Heap) {
        self.heap = heap;
        self.fuel = 100_000;
    }

    pub(super) fn run(&mut self, mut program: Program, mut params: Vec<Value>) -> Checked<Step> {
        loop {
            self.tick()?;
            let plan = self
                .plans
                .get(&program)
                .cloned()
                .ok_or(Fault::InvalidModel)?;
            let mut path = Vec::new();
            let flow = self.exec(&plan, &params, &mut Vec::new(), &mut path);
            if let Some(action) = ActionId::ALL
                .into_iter()
                .find(|action| action.selection() == (program, path.as_slice()))
            {
                self.actions.insert(action);
            }
            match flow? {
                Flow::Exit(step) => return Ok(step),
                Flow::Tail(target, args) => {
                    program = target;
                    params = args;
                }
            }
        }
    }

    pub(super) fn call(&mut self, target: Program, args: Vec<Value>) -> Checked<Value> {
        let mut step = self.run(target, args)?;
        loop {
            match step {
                Step::Return(value) => return Ok(value),
                Step::Publish(value) if target == Program::Executor => return Ok(value),
                Step::Advance(params) if target == Program::Executor => {
                    step = self.run(Program::Executor, params)?;
                }
                _ => return Err(Fault::InvalidModel),
            }
        }
    }

    pub(super) fn apply(&mut self, function: Value, args: Vec<Value>) -> Checked<Value> {
        if matches!(self.heap.get(function)?, Object::Raw { .. }) {
            self.heap.events.push(Event::Apply(function, args.clone()));
            let [value] = args.as_slice() else {
                return Err(Fault::InvalidModel);
            };
            self.call(Program::RawResume, vec![function, *value])
        } else {
            self.heap.invoke(function, args)
        }
    }

    pub(super) fn operand(operand: &Operand, params: &[Value], env: &[Value]) -> Checked<Value> {
        match operand {
            Operand::State(state) => params.get(match state {
                State::Value => 0,
                State::Handler => 1,
                State::Computation => 2,
                State::Sequence => 3,
                State::Extent => 4,
                State::Context => 5,
            }),
            Operand::Param(index) => params.get(*index),
            Operand::Temp(index) => env.get(*index),
        }
        .copied()
        .ok_or(Fault::InvalidModel)
    }

    pub(super) fn operands(
        values: &[Operand],
        params: &[Value],
        env: &[Value],
    ) -> Checked<Vec<Value>> {
        values
            .iter()
            .map(|value| Self::operand(value, params, env))
            .collect()
    }

    fn exec(
        &mut self,
        exec: &Exec,
        params: &[Value],
        env: &mut Vec<Value>,
        path: &mut Vec<Selection>,
    ) -> Checked<Flow> {
        self.tick()?;
        match exec {
            Exec::Steps { ops, next } => {
                for op in ops {
                    let value = self.op(op, params, env)?;
                    env.push(value);
                }
                self.exec(next, params, env, path)
            }
            Exec::Switch { source, case, arms } => {
                let source = Self::operand(source, params, env)?;
                let (family, tag) = match self.heap.get(source)? {
                    Object::Node(node) => (node.case().family(), node.case().tag()),
                    Object::Unknown(family, tag) => (*family, *tag),
                    _ => return Err(Fault::InvalidModel),
                };
                if family != case.family() {
                    return Err(Fault::InvalidModel);
                }
                let arm = arms
                    .iter()
                    .find(|(candidate, _)| *candidate == tag)
                    .ok_or(Fault::Trap)?;
                let actual = self.heap.get_node(source)?.case();
                path.push(match actual {
                    Case::Computation(case) => Selection::Computation(case),
                    Case::Sequence(case) => Selection::Sequence(case),
                    Case::Context(case) => Selection::Context(case),
                    Case::Extent(case) => Selection::Extent(case),
                    Case::Owner(_) => return Err(Fault::InvalidModel),
                });
                self.exec(&arm.1, params, &mut env.clone(), path)
            }
            Exec::Advance {
                state,
                handler,
                computation,
                sequence,
                extent,
                context,
            } => {
                let values = [state, handler, computation, sequence, extent, context]
                    .into_iter()
                    .map(|o| Self::operand(o, params, env))
                    .collect::<Checked<Vec<_>>>()?;
                Ok(Flow::Exit(Step::Advance(values)))
            }
            Exec::Publish { result } => Ok(Flow::Exit(Step::Publish(Self::operand(
                result, params, env,
            )?))),
            Exec::Return { value } => {
                Ok(Flow::Exit(Step::Return(Self::operand(value, params, env)?)))
            }
            Exec::Tail { role, args } => Ok(Flow::Tail(
                program(*role),
                Self::operands(args, params, env)?,
            )),
            Exec::Reject { .. } => Err(Fault::Trap),
        }
    }
}
