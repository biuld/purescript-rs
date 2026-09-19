use super::{NEWLINE_ADDR, NWRITTEN_ADDR, SCRATCH_END};
use crate::mir::{self, Instruction as MirInstruction};
use crate::wasm::{Body, DataSegment, Op};
use psrs_hir::{ExternalKind, RuntimeFunction as HirRuntimeFunction, SymbolId};
use std::collections::HashMap;
use wasm_encoder::{Instruction, MemArg};

pub(super) fn console_log_symbol(module: &mir::Module) -> Option<SymbolId> {
    module.externals.iter().find_map(|external| {
        (external.kind == ExternalKind::Runtime(HirRuntimeFunction::ConsoleLog))
            .then_some(external.symbol)
    })
}

pub(super) fn collect_strings(module: &mir::Module) -> (HashMap<String, u32>, Vec<DataSegment>) {
    let mut offsets = HashMap::new();
    let mut data = Vec::new();
    let mut next = SCRATCH_END;
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let MirInstruction::StringConstant { bytes, .. } = instruction
                    && !offsets.contains_key(bytes)
                {
                    let offset = next.next_multiple_of(4);
                    offsets.insert(bytes.clone(), offset);
                    let mut segment = (bytes.len() as u32).to_le_bytes().to_vec();
                    segment.extend_from_slice(bytes.as_bytes());
                    data.push(DataSegment {
                        offset,
                        bytes: segment,
                    });
                    next = offset + 4 + bytes.len() as u32;
                }
            }
        }
    }
    (offsets, data)
}

/// Writes the string pointer's bytes followed by a newline, then returns unit
/// (zero). The runtime ABI tracks PureScript's `console.log`, which terminates
/// each write with a newline. Each `fd_write` uses a single iovec because a
/// host is allowed to complete only a partial write of a multi-entry vector.
pub(super) fn log_body(fd_write_index: u32) -> Body {
    let mem = MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    };
    let write = |body: &mut Body, cursor: Vec<Op>| {
        body.extend(cursor);
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(NWRITTEN_ADDR as i32)));
        body.push(Op::Leaf(Instruction::Call(fd_write_index)));
        body.push(Op::Leaf(Instruction::Drop));
    };
    let mut body = Body::new();
    // iovec = { buffer: pointer + 4, length: *pointer }
    write(
        &mut body,
        vec![
            Op::Leaf(Instruction::I32Const(0)),
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::I32Const(4)),
            Op::Leaf(Instruction::I32Add),
            Op::Leaf(Instruction::I32Store(mem)),
            Op::Leaf(Instruction::I32Const(4)),
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::I32Load(mem)),
            Op::Leaf(Instruction::I32Store(mem)),
        ],
    );
    // iovec = { buffer: newline, length: 1 }
    write(
        &mut body,
        vec![
            Op::Leaf(Instruction::I32Const(0)),
            Op::Leaf(Instruction::I32Const(NEWLINE_ADDR as i32)),
            Op::Leaf(Instruction::I32Store(mem)),
            Op::Leaf(Instruction::I32Const(4)),
            Op::Leaf(Instruction::I32Const(1)),
            Op::Leaf(Instruction::I32Store(mem)),
        ],
    );
    body.push(Op::Leaf(Instruction::I32Const(0)));
    body
}
