use super::*;

#[test]
fn lexes_keywords_operators_comments_and_newlines() {
    let (tokens, errors) = lex("module Main where\n  answer = 40 + 2 -- ok\n");
    assert!(errors.is_empty(), "{errors:?}");
    let labels: Vec<_> = tokens.iter().map(|token| token.kind.label()).collect();
    assert_eq!(
        labels,
        [
            "Module",
            "UpperIdent",
            "Where",
            "Newline",
            "LowerIdent",
            "Equals",
            "Integer",
            "Operator",
            "Integer",
            "Newline",
            "Eof",
        ]
    );
    assert_eq!(tokens[7].kind, RawTokenKind::Operator("+".into()));
}

#[test]
fn reports_invalid_characters_with_spans() {
    let (_, errors) = lex("main = \u{1}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].span, TextRange::new(7, 8));
}

#[test]
fn decodes_common_string_and_character_escapes() {
    let (tokens, errors) = lex(r#"main = "line\nnext""#);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::String("line\nnext".into()))
    );

    let (tokens, errors) = lex(r#"main = '\t'"#);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::Char('\t'))
    );
}

#[test]
fn accepts_a_supplementary_scalar_as_one_character() {
    for literal in [r"'\x10000'", r"'\x1F600'", "'😀'"] {
        let (tokens, errors) = lex(literal);
        assert!(errors.is_empty(), "{errors:?}");
        let character = tokens
            .iter()
            .find_map(|token| match token.kind {
                RawTokenKind::Char(character) => Some(character),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no character token in {literal}"));
        assert!(character as u32 > 0xFFFF, "{literal}: {character:?}");
    }
}

#[test]
fn rejects_an_unpaired_surrogate_escape() {
    for literal in [
        r#"main = "\xD834""#,
        r#"main = "\xDF06""#,
        r#"main = "a\xD834z""#,
    ] {
        let (_, errors) = lex(literal);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("unpaired surrogate")),
            "{literal}: {errors:?}"
        );
    }

    for literal in [r"main = '\xD834'", r"main = '\xDF06'"] {
        let (_, errors) = lex(literal);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("unpaired surrogate")),
            "{literal}: {errors:?}"
        );
    }
}

/// A type-level string is lexed by the same path as a value string, so an
/// unpaired surrogate escape cannot become a `Symbol` type literal either.
#[test]
fn rejects_an_unpaired_surrogate_escape_in_a_type_literal() {
    for literal in [
        r#"main :: Proxy "\xD834""#,
        r#"main :: Proxy "\xDF06""#,
        r#"main :: Proxy "a\xDF06z""#,
    ] {
        let (_, errors) = lex(literal);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("unpaired surrogate")),
            "{literal}: {errors:?}"
        );
    }
}

#[test]
fn decodes_a_escaped_surrogate_pair_as_one_scalar() {
    let paired = r#"main = "\xD834\xDF06""#;
    let direct = r#"main = "\x1D306""#;
    let (tokens, errors) = lex(paired);
    assert!(errors.is_empty(), "{errors:?}");
    let value = tokens
        .iter()
        .find_map(|token| match &token.kind {
            RawTokenKind::String(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(value, "\u{1D306}");

    let (tokens, errors) = lex(direct);
    assert!(errors.is_empty(), "{errors:?}");
    let other = tokens
        .iter()
        .find_map(|token| match &token.kind {
            RawTokenKind::String(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(value, other);
}

#[test]
fn lexes_unicode_identifiers_and_operators() {
    let (tokens, errors) = lex("f asgård = 1 ∘ 2\n");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::LowerIdent("asgård".into()))
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::Operator("∘".into()))
    );
}

#[test]
fn lexes_hex_escapes_and_block_strings() {
    let (tokens, errors) = lex(r#"main = "\x1D306""#);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::String("𝌆".into()))
    );

    let (tokens, errors) = lex("main = \"\"\"foo\"\"\"\n");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::String("foo".into()))
    );
}
