use crate::types::RefType;
use crate::types::ValueType;
use crate::wasm::convert::heap_type;
use psrs_core::Primitive;
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

pub(super) fn linear_load(ty: ValueType, offset: u32) -> Instruction<'static> {
    match ty {
        ValueType::I32 | ValueType::Boolean => Instruction::I32Load(memory(offset)),
        ValueType::F64 => Instruction::F64Load(memory_with_align(offset, 3)),
        _ => unreachable!("MIR verifier rejects unsupported linear load types"),
    }
}

pub(super) fn linear_store(ty: ValueType, offset: u32) -> Instruction<'static> {
    match ty {
        ValueType::I32 | ValueType::Boolean => Instruction::I32Store(memory(offset)),
        ValueType::F64 => Instruction::F64Store(memory_with_align(offset, 3)),
        _ => unreachable!("MIR verifier rejects unsupported linear store types"),
    }
}

pub(super) fn primitive(op: Primitive) -> Instruction<'static> {
    match op {
        Primitive::Add => Instruction::I32Add,
        Primitive::Sub => Instruction::I32Sub,
        Primitive::Mul => Instruction::I32Mul,
        Primitive::DivS => Instruction::I32DivS,
        Primitive::RemS => Instruction::I32RemS,
        Primitive::Eq => Instruction::I32Eq,
        Primitive::Ne => Instruction::I32Ne,
        Primitive::LtS => Instruction::I32LtS,
        Primitive::LeS => Instruction::I32LeS,
        Primitive::GtS => Instruction::I32GtS,
        Primitive::GeS => Instruction::I32GeS,
    }
}
