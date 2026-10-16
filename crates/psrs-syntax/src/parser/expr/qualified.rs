use super::*;

impl Parser<'_> {
    /// Recognizes a contiguous module qualifier followed by a symbolic name.
    /// The lexer maximally munches the final dot with the operator (`A.!!`).
    pub(super) fn qualified_operator(&self) -> Option<(CstName, u8, usize)> {
        let LayoutTokenKind::Raw(RawTokenKind::UpperIdent(first)) = &self.current().kind else {
            return None;
        };
        let start = self.current().span.start;
        let mut end = self.current().span.end;
        let mut qualifier = first.clone();
        let mut offset = 1;
        loop {
            let token = self.peek(offset);
            if token.span.start != end {
                return None;
            }
            if token.kind == LayoutTokenKind::Raw(RawTokenKind::Dot) {
                let part = self.peek(offset + 1);
                if let LayoutTokenKind::Raw(RawTokenKind::UpperIdent(name)) = &part.kind {
                    if part.span.start != token.span.end {
                        return None;
                    }
                    qualifier.push('.');
                    qualifier.push_str(name);
                    end = part.span.end;
                    offset += 2;
                    continue;
                }
            }
            let text = match &token.kind {
                LayoutTokenKind::Raw(RawTokenKind::Operator(text)) => text.as_str(),
                LayoutTokenKind::Raw(RawTokenKind::DotDot) => "..",
                _ => return None,
            };
            let operator = text.strip_prefix('.')?;
            if operator.is_empty() || operator == "@" {
                return None;
            }
            return Some((
                CstName::new(
                    format!("{qualifier}.{operator}"),
                    TextRange::new(start, token.span.end),
                ),
                precedence(operator),
                offset + 1,
            ));
        }
    }
}
