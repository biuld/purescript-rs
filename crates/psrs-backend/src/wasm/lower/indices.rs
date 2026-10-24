use super::FunctionIndex;

/// The final Wasm function indices of the synthesized helpers. The encoder
/// emits the entry, then `cabi_realloc`, then the codec helpers in their
/// declared order, so every index has exactly one source here.
pub(super) struct SynthesizedIndices {
    pub(super) realloc: Option<FunctionIndex>,
    pub(super) string_to_bytes: Option<FunctionIndex>,
    pub(super) bytes_to_string: Option<FunctionIndex>,
    pub(super) validate_step: FunctionIndex,
}

impl SynthesizedIndices {
    pub(super) fn new(entry: FunctionIndex, needs_realloc: bool, needs_helpers: bool) -> Self {
        let realloc = needs_realloc.then_some(FunctionIndex(entry.0 + 1));
        let helper_base = entry.0 + 1 + u32::from(needs_realloc);
        if needs_helpers {
            Self {
                realloc,
                string_to_bytes: Some(FunctionIndex(helper_base)),
                bytes_to_string: Some(FunctionIndex(helper_base + 1)),
                validate_step: FunctionIndex(helper_base + 2),
            }
        } else {
            Self {
                realloc,
                string_to_bytes: None,
                bytes_to_string: None,
                validate_step: FunctionIndex(helper_base),
            }
        }
    }
}
