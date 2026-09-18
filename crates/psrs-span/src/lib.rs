use std::{fmt, sync::Arc};

/// A half-open byte range in a source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

impl TextRange {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub const fn empty(at: u32) -> Self {
        Self::new(at, at)
    }

    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: TextRange,
}

impl<T> Spanned<T> {
    pub const fn new(value: T, span: TextRange) -> Self {
        Self { value, span }
    }
}

/// Source text plus a line index used to turn byte offsets into human locations.
#[derive(Clone, Debug)]
pub struct SourceFile {
    name: Arc<str>,
    text: Arc<str>,
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(name: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        assert!(
            text.len() <= u32::MAX as usize,
            "source files are limited to 4 GiB"
        );
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push((offset + 1) as u32);
            }
        }
        Self {
            name: name.into(),
            text,
            line_starts,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns one-based (line, Unicode scalar column) for a byte offset.
    pub fn line_column(&self, offset: u32) -> (usize, usize) {
        let offset = offset.min(self.text.len() as u32);
        let line_index = self
            .line_starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line_index] as usize;
        let prefix = &self.text[line_start..offset as usize];
        (line_index + 1, prefix.chars().count() + 1)
    }

    pub fn line_start(&self, one_based_line: usize) -> Option<u32> {
        self.line_starts
            .get(one_based_line.checked_sub(1)?)
            .copied()
    }
}

impl fmt::Display for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_byte_offsets_to_unicode_columns() {
        let source = SourceFile::new("test", "α\nxyz");
        assert_eq!(source.line_column(2), (1, 2));
        assert_eq!(source.line_column(4), (2, 2));
    }
}
