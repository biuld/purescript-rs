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
        | I::ArrayNewDefault { .. }
        | I::ArrayGet { .. }
        | I::ArrayClone { .. }
        | I::ArrayLen { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: matches!(
                instruction,
                I::StructGet { .. }
                    | I::ClosureGetCapture { .. }
                    | I::ArrayGet { .. }
                    | I::ArrayClone { .. }
                    | I::ArrayLen { .. }
            ),
            writes_memory: matches!(
                instruction,
                I::StructNew { .. }
                    | I::ArrayNew { .. }
                    | I::ArrayNewDefault { .. }
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
        I::Load { .. } | I::Load8U { .. } => InstructionEffects {
            may_trap: true,
            reads_memory: true,
            ..InstructionEffects::default()
        },
        I::TrapIf { .. } | I::Unreachable { .. } => InstructionEffects {
            may_trap: true,
            ..InstructionEffects::default()
        },
        I::Copy { .. }
        | I::Constant { .. }
        | I::NumberConstant { .. }
        | I::StringConstant { .. }
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
