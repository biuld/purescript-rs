//! Module-level pool of static string literals.
//!
//! A literal is interned once and referenced by `DataId` from `ArrayNewData`;
//! P10 turns the pool into passive UTF-16 code-unit data segments.

use crate::types::DataId;

#[derive(Default)]
pub(super) struct StringLiterals {
    strings: Vec<String>,
}

impl StringLiterals {
    /// Interns `text`, returning a stable index reused for equal literals.
    pub(super) fn intern(&mut self, text: &str) -> DataId {
        if let Some(index) = self.strings.iter().position(|existing| existing == text) {
            return DataId(index as u32);
        }
        let index = self.strings.len();
        self.strings.push(text.to_string());
        DataId(index as u32)
    }

    pub(super) fn into_strings(self) -> Vec<String> {
        self.strings
    }
}
