use crate::types::DataId;
use crate::wasm::{DataIndex, DataMode, DataSegment};
use std::collections::HashMap;

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
