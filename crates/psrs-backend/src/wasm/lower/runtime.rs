use crate::types::{DataId, DefinedTypeId};
use crate::wasm::{DataIndex, DataMode, DataSegment, Global, GlobalIndex, GlobalInit};
use std::collections::{BTreeMap, HashMap};
use wasm_encoder::{HeapType, RefType, ValType};

/// Encodes each static string literal as a passive data segment of little-endian
/// UTF-16 code units. `array.new_data` materializes a segment into a GC string,
/// so a literal never passes through linear memory. Returns the segments in
/// `DataId` order and the code-unit count of each.
pub(super) fn collect_strings(
    module: &crate::mir::Module,
) -> (Vec<DataSegment>, HashMap<DataId, u32>) {
    let mut data = Vec::with_capacity(module.strings.len());
    let mut counts = HashMap::new();
    for (index, text) in module.strings.iter().enumerate() {
        let units = text.encode_utf16().collect::<Vec<_>>();
        let mut bytes = Vec::with_capacity(units.len() * 2);
        for unit in &units {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let id = DataId(index as u32);
        counts.insert(id, units.len() as u32);
        data.push(DataSegment {
            id,
            index: DataIndex(index as u32),
            mode: DataMode::Passive,
            bytes,
        });
    }
    (data, counts)
}

/// The module globals that intern string literals, plus the `DataId` lookup the
/// structurer uses to read one.
pub(super) struct LiteralGlobals {
    pub(super) globals: Vec<Global>,
    pub(super) indices: HashMap<DataId, GlobalIndex>,
}

/// Allocates one mutable global per distinct string literal that a MIR function
/// actually materializes. A literal is created on first use and stored in its
/// global; later uses reuse the same GC string. Globals are indexed from zero in
/// `DataId` order so the encoder is reproducible, and the global's type is the
/// nullable form of the literal's GC string type.
pub(super) fn collect_literal_globals(module: &crate::mir::Module) -> LiteralGlobals {
    let mut used = BTreeMap::<DataId, DefinedTypeId>::new();
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let crate::mir::Instruction::ArrayNewData {
                    data_index,
                    type_index,
                    ..
                } = instruction
                {
                    used.entry(*data_index).or_insert(*type_index);
                }
            }
        }
    }
    let mut globals = Vec::with_capacity(used.len());
    let mut indices = HashMap::with_capacity(used.len());
    for (position, (id, string_type)) in used.into_iter().enumerate() {
        let index = GlobalIndex(position as u32);
        let heap = HeapType::Concrete(string_type.0);
        indices.insert(id, index);
        globals.push(Global {
            index,
            mutable: true,
            ty: ValType::Ref(RefType {
                nullable: true,
                heap_type: heap,
            }),
            init: GlobalInit::RefNull(heap),
        });
    }
    LiteralGlobals { globals, indices }
}
