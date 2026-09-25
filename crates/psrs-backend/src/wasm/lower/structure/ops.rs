use crate::mir::NumericOp;
use crate::types::RefType;
use crate::wasm::convert::heap_type;
use wasm_encoder::{Instruction, MemArg};

pub(super) fn ref_test(reference: RefType) -> Instruction<'static> {
    if reference.nullable {
        Instruction::RefTestNullable(heap_type(reference.heap))
    } else {
        Instruction::RefTestNonNull(heap_type(reference.heap))
    }
}

pub(super) fn ref_cast(reference: RefType) -> Instruction<'static> {
    if reference.nullable {
        Instruction::RefCastNullable(heap_type(reference.heap))
    } else {
        Instruction::RefCastNonNull(heap_type(reference.heap))
    }
}

pub(super) fn memory(offset: u32) -> MemArg {
    memory_with_align(offset, 2)
}

pub(super) fn memory_with_align(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align,
        memory_index: 0,
    }
}

pub(super) fn primitive(op: NumericOp) -> Instruction<'static> {
    match op {
        NumericOp::I32Add => Instruction::I32Add,
        NumericOp::I32Sub => Instruction::I32Sub,
        NumericOp::I32Mul => Instruction::I32Mul,
        NumericOp::I32DivS => Instruction::I32DivS,
        NumericOp::I32RemS => Instruction::I32RemS,
        NumericOp::I32And | NumericOp::BoolAnd => Instruction::I32And,
        NumericOp::I32Or | NumericOp::BoolOr => Instruction::I32Or,
        NumericOp::I32Xor => Instruction::I32Xor,
        NumericOp::I32Shl => Instruction::I32Shl,
        NumericOp::I32ShrS => Instruction::I32ShrS,
        NumericOp::I32ShrU => Instruction::I32ShrU,
        NumericOp::I32Eq => Instruction::I32Eq,
        NumericOp::I32Ne => Instruction::I32Ne,
        NumericOp::BoolEq => Instruction::I32Eq,
        NumericOp::BoolNe => Instruction::I32Ne,
        NumericOp::I32LtS => Instruction::I32LtS,
        NumericOp::I32LeS => Instruction::I32LeS,
        NumericOp::I32GtS => Instruction::I32GtS,
        NumericOp::I32GeS => Instruction::I32GeS,
        NumericOp::F64Add => Instruction::F64Add,
        NumericOp::F64Sub => Instruction::F64Sub,
        NumericOp::F64Mul => Instruction::F64Mul,
        NumericOp::F64Div => Instruction::F64Div,
        NumericOp::F64Eq => Instruction::F64Eq,
        NumericOp::F64Ne => Instruction::F64Ne,
        NumericOp::F64Lt => Instruction::F64Lt,
        NumericOp::F64Le => Instruction::F64Le,
        NumericOp::F64Gt => Instruction::F64Gt,
        NumericOp::F64Ge => Instruction::F64Ge,
    }
}
