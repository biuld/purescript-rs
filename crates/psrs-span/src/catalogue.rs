//! Source units survive semantic transformations without changing their identity.
use crate::{SourceFile, SourceId, TextRange};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceCatalogue {
    // Retain duplicate IDs so merging cannot silently choose a different file.
    sources: Vec<(SourceId, Arc<SourceFile>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLocation<'a> {
    pub name: &'a str,
    pub line: usize,
    pub column: usize,
}

impl SourceCatalogue {
    pub fn insert(&mut self, id: SourceId, file: SourceFile) {
        self.sources.push((id, Arc::new(file)));
    }

    pub fn extend(&mut self, other: Self) {
        self.sources.extend(other.sources);
    }

    /// Validate the complete range before the convenience line mapper can clamp
    /// an invalid offset or slice through a UTF-8 code point.
    pub fn location(&self, span: TextRange) -> Result<SourceLocation<'_>, &'static str> {
        let id = span.source().ok_or("source range has no source identity")?;
        let mut matches = self.sources.iter().filter(|(source, _)| *source == id);
        let (_, file) = matches.next().ok_or("source identity has no source unit")?;
        if matches.next().is_some() {
            return Err("source identity has multiple source units");
        }
        let start = span.start as usize;
        let end = span.end as usize;
        if start > end || end > file.text().len() {
            return Err("source range is outside its source unit");
        }
        if !file.text().is_char_boundary(start) || !file.text().is_char_boundary(end) {
            return Err("source range is not on UTF-8 boundaries");
        }
        let (line, column) = file.line_column(span.start);
        Ok(SourceLocation {
            name: file.name(),
            line,
            column,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_identity_bounds_and_unicode_before_mapping() {
        let mut files = SourceCatalogue::default();
        files.insert(SourceId(4), SourceFile::new("Helper", "α\nvalue"));
        let range = |start, end| TextRange::new(start, end).in_source(SourceId(4));
        assert_eq!(
            files.location(range(3, 8)),
            Ok(SourceLocation {
                name: "Helper",
                line: 2,
                column: 1
            })
        );
        for invalid in [
            range(9, 9),
            range(5, 4),
            range(1, 2),
            range(0, 1),
            TextRange::empty(0),
            TextRange::empty(0).in_source(SourceId(9)),
        ] {
            assert!(files.location(invalid).is_err(), "{invalid:?}");
        }
        let mut other = SourceCatalogue::default();
        other.insert(SourceId(4), SourceFile::new("Other", "different"));
        files.extend(other);
        assert!(files.location(range(3, 8)).is_err());
    }
}
