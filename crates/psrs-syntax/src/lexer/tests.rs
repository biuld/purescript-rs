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

    let (tokens, errors) = lex(r#"main = '\x1F600'"#);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == RawTokenKind::Char('\u{1F600}'))
    );
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
