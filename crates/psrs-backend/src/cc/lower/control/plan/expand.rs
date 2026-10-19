//! Expansion of the checked plan into ordinary CC.

use super::super::builder::{Machine, repr};
use super::ir::{Exec, Op, Operand, Product, Shape, state_index};
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, ControlHelperRole, ValueId};
use crate::control::schema::SequenceCase;

impl Machine<'_> {
    /// Emit ordinary CC from the checked plan. `params` are the enclosing
    /// function's parameters; `Operand::State` indexes the executor's six named
    /// slots and `Operand::Param` a helper's explicit parameters.
    pub(in crate::cc::lower::control) fn expand(
        &mut self,
        plan: &super::CheckedPlan,
        params: &[ValueId],
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        self.expand_exec(plan.exec(), &mut Vec::new(), params, body)
    }

    fn expand_exec(
        &mut self,
        exec: &Exec,
        env: &mut Vec<ValueId>,
        params: &[ValueId],
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        match exec {
            Exec::Steps { ops, next } => {
                let base = env.len();
                for op in ops {
                    let value = self.expand_op(op, env, params, body);
                    env.push(value);
                }
                let result = self.expand_exec(next, env, params, body);
                env.truncate(base);
                result
            }
            Exec::Switch { source, case, arms } => {
                let value = self.operand(source, env, params);
                let value = self.cast(value, repr(case.repr(self.layout)), body);
                let mut cases = Vec::new();
                for (tag, arm) in arms {
                    let mut arm_body = Vec::new();
                    let mut arm_env = env.clone();
                    let result = self.expand_exec(arm, &mut arm_env, params, &mut arm_body);
                    cases.push(super::super::executor::arm(*tag, arm_body, result));
                }
                self.switch(value, case.repr(self.layout), cases, body)
            }
            Exec::Advance {
                state,
                handler,
                computation,
                sequence,
                extent,
                context,
            } => {
                let state = self.operand(state, env, params);
                let handler = self.operand(handler, env, params);
                let computation = self.operand(computation, env, params);
                let sequence = self.operand(sequence, env, params);
                let extent = self.operand(extent, env, params);
                let context = self.operand(context, env, params);
                let function = self.functions.executor;
                self.call(
                    function,
                    vec![state, handler, computation, sequence, extent, context],
                    body,
                )
            }
            Exec::Publish { result } | Exec::Return { value: result } => {
                self.operand(result, env, params)
            }
            Exec::Tail { role, args } => {
                let function = self.function_for(*role);
                let args = args
                    .iter()
                    .map(|operand| self.operand(operand, env, params))
                    .collect();
                self.call(function, args, body)
            }
            Exec::Reject { shape } => {
                let shape = self.planned_shape(*shape);
                self.emit(shape, body, |_| AssignmentKind::Unreachable)
            }
        }
    }

    fn expand_op(
        &mut self,
        op: &Op,
        env: &[ValueId],
        params: &[ValueId],
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        match op {
            Op::Protect {
                action: _,
                ops,
                result,
                cleanup,
            } => {
                let mut protected = Vec::new();
                let mut body_env = env.to_vec();
                for op in ops {
                    let value = self.expand_op(op, &body_env, params, &mut protected);
                    body_env.push(value);
                }
                let value = self.operand(result, &body_env, params);
                let mut handler = Vec::new();
                let mut handler_env = env.to_vec();
                for op in cleanup {
                    let value = if let Op::CloseBoundary { extent } = op {
                        let extent = self.operand(extent, &handler_env, params);
                        self.close_boundary_inline(extent, &mut handler)
                    } else {
                        self.expand_op(op, &handler_env, params, &mut handler)
                    };
                    handler_env.push(value);
                }
                use super::super::fields::ControlEmitter;
                let exception =
                    self.fresh_value(crate::cc::ValueShape::Reference(crate::cc::Reference {
                        nullable: false,
                        heap: crate::cc::RefShape::Exception,
                    }));
                self.emit(self.value_shape(value), body, |_| AssignmentKind::Protect {
                    body: crate::cc::Region {
                        assignments: protected,
                        exit: crate::cc::RegionExit::Yield(value),
                    },
                    exception,
                    handler: crate::cc::Region {
                        assignments: handler,
                        exit: crate::cc::RegionExit::Propagate(exception),
                    },
                })
            }
            Op::Project {
                source,
                case,
                field,
            } => {
                let source = self.operand(source, env, params);
                self.field(source, case.repr(self.layout), case.tag(), *field, body)
            }
            Op::Node { case, fields } => {
                let fields = fields
                    .iter()
                    .map(|operand| self.operand(operand, env, params))
                    .collect();
                self.node(case.repr(self.layout), case.tag(), fields, body)
            }
            Op::Capture {
                closure,
                index,
                case,
            } => {
                let closure = self.operand(closure, env, params);
                let shape = repr(case.repr(self.layout));
                self.capture(closure, *index, shape, body)
            }
            Op::Apply { callee, args } => {
                let callee = self.operand(callee, env, params);
                let args = args
                    .iter()
                    .map(|operand| self.operand(operand, env, params))
                    .collect::<Vec<_>>();
                self.apply(callee, &args, body)
            }
            Op::Helper { role, args } => {
                let function = self.function_for(*role);
                let args = args
                    .iter()
                    .map(|operand| self.operand(operand, env, params))
                    .collect();
                self.call(function, args, body)
            }
            Op::ResumeClosure { captures } => {
                let captures = captures
                    .iter()
                    .map(|operand| self.operand(operand, env, params))
                    .collect();
                let function = self.functions.resume;
                let signature = self.layout.continuation;
                self.closure(function, signature, captures, body)
            }
            Op::Pair { state, value } => {
                let state = self.operand(state, env, params);
                let value = self.operand(value, env, params);
                self.pair(state, value, body)
            }
            Op::PairField { pair, field } => {
                let pair = self.operand(pair, env, params);
                self.pair_field(pair, *field, body)
            }
            Op::ReplayField { result, field } => {
                let result = self.operand(result, env, params);
                self.replay_field(result, *field, body)
            }
            Op::FreshExtent { parent } => {
                let parent = parent
                    .as_ref()
                    .map(|operand| self.operand(operand, env, params));
                self.new_boundary(parent, body)
            }
            Op::ContextExtent { context } => {
                let context = self.operand(context, env, params);
                self.context_extent(context, body)
            }
            Op::CheckBoundary { extent } => {
                let extent = self.operand(extent, env, params);
                self.check_boundary(extent, body)
            }
            Op::CloseBoundary { extent } => {
                let extent = self.operand(extent, env, params);
                self.close_boundary(extent, body)
            }
            Op::GuardPending { cell } => {
                let cell = self.operand(cell, env, params);
                self.guard_pending(cell, body);
                cell
            }
            Op::SetStatus { cell, value } => {
                let cell = self.operand(cell, env, params);
                self.set_status(cell, *value, body);
                cell
            }
            Op::ReadCallback { cell } => {
                let cell = self.operand(cell, env, params);
                self.read_cell(self.layout.callback, erased_shape(), cell, body)
            }
            Op::WriteCallback { cell, value } => {
                let cell = self.operand(cell, env, params);
                let value = self.operand(value, env, params);
                self.write_cell(self.layout.callback, cell, value, body);
                cell
            }
            Op::NewCallbackCell { value } => {
                let value = self.operand(value, env, params);
                let value = self.cast(value, erased_shape(), body);
                self.new_cell(self.layout.callback, value, body)
            }
            Op::NewStatus => self.new_status(body),
            Op::Product { product, fields } => {
                let representation = self.planned_product(*product);
                let fields = fields
                    .iter()
                    .map(|operand| self.operand(operand, env, params))
                    .collect();
                self.emit(repr(representation), body, |destination| {
                    AssignmentKind::ProductNew {
                        destination,
                        representation,
                        arguments: fields,
                    }
                })
            }
            Op::EmptySequence => {
                let representation = self.layout.sequence;
                self.node(representation, SequenceCase::Empty.tag(), Vec::new(), body)
            }
        }
    }

    fn operand(&self, operand: &Operand, env: &[ValueId], params: &[ValueId]) -> ValueId {
        match operand {
            Operand::State(slot) => params[state_index(*slot)],
            Operand::Param(index) => params[*index],
            Operand::Temp(index) => env[*index],
        }
    }

    fn planned_shape(&self, shape: Shape) -> crate::cc::ValueShape {
        match shape {
            Shape::ReplayResult => repr(self.layout.replay_result),
        }
    }

    fn planned_product(&self, product: Product) -> crate::cc::ReprId {
        match product {
            Product::ReplayResult => self.layout.replay_result,
        }
    }

    fn function_for(&self, role: ControlHelperRole) -> psrs_hir::SymbolId {
        match role {
            ControlHelperRole::Executor => self.functions.executor,
            ControlHelperRole::Run => self.functions.run,
            ControlHelperRole::CloneSequence => self.functions.clone_sequence,
            ControlHelperRole::RebuildSequence => self.functions.rebuild_sequence,
            ControlHelperRole::Start => self.functions.start,
            ControlHelperRole::Check => self.functions.check,
            ControlHelperRole::Close => self.functions.close,
            ControlHelperRole::Once => self.functions.once,
            ControlHelperRole::Cancel => self.functions.cancel,
        }
    }
}
