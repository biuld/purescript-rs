#![allow(dead_code)]
//! Conservative instruction effect summaries shared by optimization and semantic verification.

use crate::mir::Instruction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct InstructionEffects {
    pub(super) may_trap: bool,
    pub(super) reads_memory: bool,
    pub(super) writes_memory: bool,
    pub(super) may_call: bool,
}

impl InstructionEffects {
    pub(super) fn is_pure_and_total(self) -> bool {
        !self.may_trap && !self.reads_memory && !self.writes_memory && !self.may_call
    }
}

pub(super) fn classify(instruction: &Instruction) -> InstructionEffects {
    use Instruction as I;

    match instruction {
        I::Primitive {
            op: crate::mir::NumericOp::I32DivS | crate::mir::NumericOp::I32RemS,
            ..
        } => InstructionEffects {
            may_trap: true,
            ..InstructionEffects::default()
        },
        I::Call { .. } | I::CallVoid { .. } | I::CallRef { .. } | I::ClosureCall { .. } => {
            InstructionEffects {
                may_trap: true,
                reads_memory: true,
                writes_memory: true,
                may_call: true,
            }
        }
        I::RefCast { .. }
        | I::I31GetS { .. }
        | I::StructNew { .. }
        | I::StructGet { .. }
        | I::ClosureGetCapture { .. }
        | I::ArrayNew { .. }
        | I::ArrayNewData { .. }
        | I::ArrayNewDefault { .. }
        | I::ArrayNewSized { .. }
        | I::ArrayNewFilled { .. }
        | I::ArrayGet { .. }
        | I::ArrayGetU { .. }
        | I::ArrayClone { .. }
        | I::ArrayLen { .. }
        | I::StringToBytes { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: matches!(
                instruction,
                I::StructGet { .. }
                    | I::ClosureGetCapture { .. }
                    | I::ArrayGet { .. }
                    | I::ArrayGetU { .. }
                    | I::ArrayClone { .. }
                    | I::ArrayLen { .. }
            ),
            writes_memory: matches!(
                instruction,
                I::StructNew { .. }
                    | I::ArrayNew { .. }
                    | I::ArrayNewData { .. }
                    | I::ArrayNewDefault { .. }
                    | I::ArrayNewSized { .. }
                    | I::ArrayNewFilled { .. }
                    | I::ArrayClone { .. }
            ),
            ..InstructionEffects::default()
        },
        I::StaticCell { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: true,
            writes_memory: true,
            ..InstructionEffects::default()
        },
        I::ClosureNew { .. } => InstructionEffects {
            may_trap: true,
            writes_memory: true,
            ..InstructionEffects::default()
        },
        I::StructSet { .. }
        | I::ArraySet { .. }
        | I::Store { .. }
        | I::Store8 { .. }
        | I::Store16 { .. }
        | I::StoreI64 { .. }
        | I::StoreF32 { .. }
        | I::StoreF64 { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: matches!(instruction, I::StructSet { .. } | I::ArraySet { .. }),
            writes_memory: true,
            ..InstructionEffects::default()
        },
        I::Load { .. }
        | I::Load8U { .. }
        | I::Load16U { .. }
        | I::LoadI64 { .. }
        | I::LoadF32 { .. }
        | I::LoadF64 { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: true,
            ..InstructionEffects::default()
        },
        I::TrapIf { .. } | I::Unreachable { .. } => InstructionEffects {
            may_trap: true,
            ..InstructionEffects::default()
        },
        I::ListCopy { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: true,
            writes_memory: true,
            ..InstructionEffects::default()
        },
        I::Copy { .. }
        | I::ControlEvidence { .. }
        | I::Constant { .. }
        | I::NumberConstant { .. }
        | I::Primitive { .. }
        | I::UnaryPrimitive { .. }
        | I::RefFunc { .. }
        | I::RefNull { .. }
        | I::RefIsNull { .. }
        | I::RefTest { .. }
        | I::I31New { .. }
        | I::HighI64 { .. }
        | I::JoinI64 { .. }
        | I::WrapI64 { .. }
        | I::WidenI64 { .. } => InstructionEffects::default(),
    }
}

pub(super) fn is_pure_and_total(instruction: &Instruction) -> bool {
    classify(instruction).is_pure_and_total()
}

/// Share the conservative call boundary with semantic IR verification.
pub(in crate::mir) fn may_invoke(instruction: &Instruction) -> bool {
    classify(instruction).may_call
}

/// Total local bodies: exclude fallible instructions, CFG cycles and recursive calls.
pub(in crate::mir) fn total_functions(
    module: &crate::mir::Module,
) -> std::collections::HashSet<psrs_hir::SymbolId> {
    use crate::mir::Terminator;
    use std::collections::{HashMap, HashSet};
    let mut dependencies = HashMap::<psrs_hir::SymbolId, HashSet<psrs_hir::SymbolId>>::new();
    for function in &module.functions {
        let mut calls = HashSet::new();
        let mut safe = acyclic(function);
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Call { function, .. } | Instruction::CallVoid { function, .. } => {
                        calls.insert(*function);
                    }
                    _ if !is_pure_and_total(instruction) => {
                        safe = false;
                        break;
                    }
                    _ => {}
                }
            }
            match &block.terminator {
                Some(Terminator::ReturnCall { function, .. }) => {
                    calls.insert(*function);
                }
                Some(
                    Terminator::ReturnCallRef { .. }
                    | Terminator::Trap { .. }
                    | Terminator::ThrowRef { .. }
                    | Terminator::Throw { .. },
                ) => {
                    safe = false;
                }
                _ => {}
            }
        }
        if safe {
            dependencies.insert(function.symbol, calls);
        }
    }
    let mut safe = HashSet::new();
    loop {
        let previous = safe.len();
        for (symbol, calls) in &dependencies {
            if calls.iter().all(|callee| safe.contains(callee)) {
                safe.insert(*symbol);
            }
        }
        if safe.len() == previous {
            return safe;
        }
    }
}

fn acyclic(function: &crate::mir::Function) -> bool {
    fn visit(
        id: crate::mir::BlockId,
        blocks: &std::collections::HashMap<crate::mir::BlockId, &crate::mir::BasicBlock>,
        active: &mut std::collections::HashSet<crate::mir::BlockId>,
        done: &mut std::collections::HashSet<crate::mir::BlockId>,
    ) -> bool {
        if done.contains(&id) {
            return true;
        }
        if !active.insert(id) {
            return false;
        }
        let Some(block) = blocks.get(&id) else {
            return false;
        };
        let Some(terminator) = &block.terminator else {
            return false;
        };
        if !crate::mir::cfg::successors(terminator)
            .into_iter()
            .all(|next| visit(next, blocks, active, done))
        {
            return false;
        }
        active.remove(&id);
        done.insert(id);
        true
    }
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect();
    visit(
        function.entry,
        &blocks,
        &mut Default::default(),
        &mut Default::default(),
    )
}
