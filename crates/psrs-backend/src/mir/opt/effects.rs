//! Conservative instruction effect summaries used by dead-code elimination.

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

/// Greatest closed set of local bodies without writes or dynamic calls.
/// Static recursion is permitted; imported calls remain unproven.
pub(in crate::mir) fn read_only_functions(
    module: &crate::mir::Module,
) -> std::collections::HashSet<psrs_hir::SymbolId> {
    use crate::mir::Terminator;
    use std::collections::{HashMap, HashSet};
    let mut dependencies = HashMap::<psrs_hir::SymbolId, HashSet<psrs_hir::SymbolId>>::new();
    for function in &module.functions {
        let mut calls = HashSet::new();
        let mut safe = true;
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Call { function, .. } | Instruction::CallVoid { function, .. } => {
                        calls.insert(*function);
                    }
                    _ if classify(instruction).may_call || classify(instruction).writes_memory => {
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
                Some(Terminator::ReturnCallRef { .. }) => safe = false,
                _ => {}
            }
        }
        if safe {
            dependencies.insert(function.symbol, calls);
        }
    }
    let mut safe = dependencies.keys().copied().collect::<HashSet<_>>();
    loop {
        let previous = safe.clone();
        safe.retain(|symbol| {
            dependencies[symbol]
                .iter()
                .all(|callee| previous.contains(callee))
        });
        if safe == previous {
            return safe;
        }
    }
}
