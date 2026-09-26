//! Low-level structured-instruction helpers for the string codec.

use crate::types::DefinedTypeId;
use crate::wasm::{Body, Op};
use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

const DEFAULT_ALIGN: u32 = 0;

pub(super) fn memarg(offset: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align: DEFAULT_ALIGN,
        memory_index: 0,
    }
}

pub(super) fn string_ref(string_type: DefinedTypeId) -> ValType {
    ValType::Ref(wasm_encoder::RefType {
        nullable: false,
        heap_type: wasm_encoder::HeapType::Concrete(string_type.0),
    })
}

pub(super) fn nullable_string_ref(string_type: DefinedTypeId) -> ValType {
    ValType::Ref(wasm_encoder::RefType {
        nullable: true,
        heap_type: wasm_encoder::HeapType::Concrete(string_type.0),
    })
}

/// A small structured-instruction builder that tracks label depth so branch
/// targets can be named instead of counted.
pub(super) struct Asm {
    body: Body,
    stack: Vec<u32>,
    next: u32,
}

impl Asm {
    pub(super) fn new() -> Self {
        Self {
            body: Vec::new(),
            stack: Vec::new(),
            next: 0,
        }
    }

    pub(super) fn into_body(self) -> Body {
        self.body
    }

    pub(super) fn label(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    pub(super) fn leaf(&mut self, instruction: Instruction<'static>) {
        self.body.push(Op::Leaf(instruction));
    }

    pub(super) fn block(&mut self, id: u32) {
        self.leaf(Instruction::Block(BlockType::Empty));
        self.stack.push(id);
    }

    pub(super) fn loop_(&mut self, id: u32) {
        self.leaf(Instruction::Loop(BlockType::Empty));
        self.stack.push(id);
    }

    pub(super) fn if_(&mut self, id: u32) {
        self.leaf(Instruction::If(BlockType::Empty));
        self.stack.push(id);
    }

    pub(super) fn else_(&mut self) {
        self.leaf(Instruction::Else);
    }

    pub(super) fn end(&mut self) {
        self.leaf(Instruction::End);
        self.stack.pop();
    }

    pub(super) fn br(&mut self, id: u32) {
        self.leaf(Instruction::Br(self.depth(id)));
    }

    pub(super) fn br_if(&mut self, id: u32) {
        self.leaf(Instruction::BrIf(self.depth(id)));
    }

    fn depth(&self, id: u32) -> u32 {
        let count = self.stack.len();
        for (position, candidate) in self.stack.iter().enumerate().rev() {
            if *candidate == id {
                return (count - 1 - position) as u32;
            }
        }
        panic!("branch target label is not on the structured stack");
    }
}

pub(super) fn get(asm: &mut Asm, index: u32) {
    asm.leaf(Instruction::LocalGet(index));
}

pub(super) fn set(asm: &mut Asm, index: u32) {
    asm.leaf(Instruction::LocalSet(index));
}

pub(super) fn constant(asm: &mut Asm, value: i32) {
    asm.leaf(Instruction::I32Const(value));
}

pub(super) fn advance(asm: &mut Asm, out: u32, amount: i32) {
    get(asm, out);
    constant(asm, amount);
    asm.leaf(Instruction::I32Add);
    set(asm, out);
}

/// Stores the constant byte `value` at `out + offset`.
pub(super) fn store_const(asm: &mut Asm, out: u32, offset: i32, value: i32) {
    get(asm, out);
    if offset != 0 {
        constant(asm, offset);
        asm.leaf(Instruction::I32Add);
    }
    constant(asm, value);
    asm.leaf(Instruction::I32Store8(memarg(0)));
}

/// Emits one byte of a multi-byte sequence: `base | ((value >> shift) & mask)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn store_derived(
    asm: &mut Asm,
    out: u32,
    offset: i32,
    base: i32,
    value: u32,
    shift: i32,
    mask: i32,
) {
    get(asm, out);
    if offset != 0 {
        constant(asm, offset);
        asm.leaf(Instruction::I32Add);
    }
    constant(asm, base);
    get(asm, value);
    constant(asm, shift);
    asm.leaf(Instruction::I32ShrU);
    constant(asm, mask);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::I32Store8(memarg(0)));
}
