use super::SCRATCH_END;
use crate::mir::Instruction as MirInstruction;
use crate::wasm::DataSegment;
use std::collections::HashMap;

/// Collects string literals into length-prefixed data segments after the
/// scratch region, returning their addresses and the first free offset.
pub(super) fn collect_strings(
    module: &crate::mir::Module,
) -> (HashMap<String, u32>, Vec<DataSegment>, u32) {
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
    (offsets, data, next)
}
