//! Publish machine bindings before source lowering; query immutable plans by symbol.
use super::{ControlLayout, plan};
use crate::cc::SignatureId;
use psrs_hir::SymbolId;

#[derive(Clone, Copy)]
pub(in crate::cc) struct ControlFunctions {
    pub(super) emitted: bool,
    pub(super) executor: SymbolId,
    pub(super) run: SymbolId,
    pub(super) resume: SymbolId,
    pub(super) clone_sequence: SymbolId,
    pub(super) rebuild_sequence: SymbolId,
    pub(super) start: SymbolId,
    pub(super) check: SymbolId,
    pub(super) close: SymbolId,
    pub(super) once: SymbolId,
    pub(super) cancel: SymbolId,
}

impl ControlFunctions {
    pub(super) fn contracts(
        self,
        layout: ControlLayout,
    ) -> [(SymbolId, SignatureId, crate::cc::ControlHelperRole); 9] {
        use crate::cc::ControlHelperRole::*;
        [
            (self.executor, layout.executor, Executor),
            (self.run, layout.run, Run),
            (self.clone_sequence, layout.clone_sequence, CloneSequence),
            (
                self.rebuild_sequence,
                layout.rebuild_sequence,
                RebuildSequence,
            ),
            (self.start, layout.start, Start),
            (self.check, layout.boundary_check, Check),
            (self.close, layout.boundary_check, Close),
            (self.once, layout.once, Once),
            (self.cancel, layout.cancel, Cancel),
        ]
    }

    pub(super) fn signature(self, layout: ControlLayout, function: SymbolId) -> SignatureId {
        self.contracts(layout)
            .into_iter()
            .find(|(symbol, _, _)| *symbol == function)
            .map(|(_, signature, _)| signature)
            .expect("predeclared control function")
    }
}

pub(in crate::cc) fn plan_control_helpers(
    layout: &mut crate::cc::layout::TypeLayout,
    allocator: &std::rc::Rc<std::cell::RefCell<super::super::GeneratedSymbolAllocator>>,
) -> Option<ControlFunctions> {
    let control = layout.control_layout?;
    let mut allocator = allocator.borrow_mut();
    let owner = layout.control_owner?;
    let functions = ControlFunctions {
        emitted: false,
        executor: allocator.fresh(owner),
        run: allocator.fresh(owner),
        resume: allocator.fresh(owner),
        clone_sequence: allocator.fresh(owner),
        rebuild_sequence: allocator.fresh(owner),
        start: allocator.fresh(owner),
        check: allocator.fresh(owner),
        close: allocator.fresh(owner),
        once: allocator.fresh(owner),
        cancel: allocator.fresh(owner),
    };
    let resumption = crate::cc::ControlResumptionContract {
        function: functions.resume,
        signature: control.continuation,
    };
    let schema = crate::cc::ControlSchema {
        computation: control.computation,
        sequence: control.sequence,
        context: control.context,
        extent: control.boundary,
        status: control.status,
        owner: control.owner,
        callback: control.callback,
        result_pair: control.pair,
        replay_result: control.replay_result,
    };
    let helpers = functions
        .contracts(control)
        .into_iter()
        .map(
            |(function, signature, role)| crate::cc::ControlHelperContract {
                role,
                function,
                signature,
            },
        )
        .collect();
    layout
        .representations
        .freeze_control_plan(helpers, resumption, schema)
        .expect("machine planning must satisfy its named schema");
    Some(functions)
}

/// The checked plan shape for a machine function symbol, if it is one.
pub(crate) fn plan_shape_for(
    symbol: psrs_hir::SymbolId,
    table: &crate::cc::RepresentationTable,
) -> Option<plan::Shape> {
    plan_for_symbol(symbol, table).map(|plan| plan::shape_of(plan.exec()))
}

pub(super) fn plan_for_symbol(
    symbol: psrs_hir::SymbolId,
    table: &crate::cc::RepresentationTable,
) -> Option<plan::CheckedPlan> {
    use crate::cc::ControlHelperRole as R;
    let role = table
        .control_helpers()
        .iter()
        .find(|helper| helper.function() == symbol)
        .map(|helper| helper.role());
    let resumption = table
        .control_resumption()
        .is_some_and(|raw| raw.function() == symbol);
    let program = match role {
        Some(R::Executor) => plan::Program::Executor,
        Some(R::Run) => plan::Program::Run,
        Some(R::CloneSequence) => plan::Program::CloneSequence,
        Some(R::RebuildSequence) => plan::Program::RebuildSequence,
        Some(R::Start) => plan::Program::Start,
        Some(R::Check) => plan::Program::Check,
        Some(R::Close) => plan::Program::Close,
        Some(R::Once) => plan::Program::Once,
        Some(R::Cancel) => plan::Program::Cancel,
        None if resumption => plan::Program::RawResume,
        None => return None,
    };
    Some(plan::build(program))
}

/// The external helper symbols a machine function tail-calls, per its plan.
///
/// Self tail calls are loopified into loop back-edges by P9, so they are
/// excluded; the MIR boundary check compares the rest against actual
/// `ReturnCall` targets.
pub(crate) fn plan_external_targets(
    symbol: psrs_hir::SymbolId,
    table: &crate::cc::RepresentationTable,
) -> Option<std::collections::HashSet<psrs_hir::SymbolId>> {
    use crate::cc::ControlHelperRole;
    fn collect(exec: &plan::PlanExec, out: &mut std::collections::HashSet<ControlHelperRole>) {
        match exec {
            plan::PlanExec::Steps { next, .. } => collect(next, out),
            plan::PlanExec::Switch { arms, .. } => {
                for (_, arm) in arms {
                    collect(arm, out);
                }
            }
            plan::PlanExec::Advance { .. } => {
                out.insert(ControlHelperRole::Executor);
            }
            plan::PlanExec::Tail { role, .. } => {
                out.insert(*role);
            }
            _ => {}
        }
    }
    let plan = plan_for_symbol(symbol, table)?;
    let mut wanted = std::collections::HashSet::new();
    collect(plan.exec(), &mut wanted);
    let mut targets = std::collections::HashSet::new();
    for role in wanted {
        if let Some(helper) = table.control_helper(role)
            && helper.function() != symbol
        {
            targets.insert(helper.function());
        }
    }
    Some(targets)
}
