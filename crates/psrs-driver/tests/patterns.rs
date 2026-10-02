use std::collections::BTreeMap;

use psrs_ast::{Guard, Module, Pattern, PatternKind, TypeKind};
use psrs_span::{SourceFile, TextRange};
#[path = "pattern_support/support.rs"]
mod support;
use support::{type_contains_wildcard, visit_expr_guards, visit_expr_patterns};

macro_rules! corpus_fixtures {
    ($($path:literal),* $(,)?) => {
        [$(
            ($path, include_str!(concat!("../../../tests/upstream/", $path))),
        )*]
    };
}

const ISSUE_85_P2_BASELINE: &[(&str, &str)] = &corpus_fixtures![
    "passing/1570.purs",
    "passing/1664.purs",
    "passing/1991.purs",
    "passing/2049.purs",
    "passing/2626.purs",
    "passing/2689.purs",
    "passing/BigFunction.purs",
    "passing/BindersInFunctions.purs",
    "passing/Console.purs",
    "passing/DeepCase.purs",
    "passing/EmptyDataDecls.purs",
    "passing/EmptyTypeClass.purs",
    "passing/ExtendedInfixOperators.purs",
    "passing/IntAndChar.purs",
    "passing/Let.purs",
    "passing/Let2.purs",
    "passing/MinusConstructor.purs",
    "passing/MutRec.purs",
    "passing/NamedPatterns.purs",
    "passing/NegativeBinder.purs",
    "passing/ObjectUpdate.purs",
    "passing/ParensInTypedBinder.purs",
    "passing/PartialFunction.purs",
    "passing/Patterns.purs",
    "passing/Recursion.purs",
    "passing/ReservedWords.purs",
    "passing/RuntimeScopeIssue.purs",
    "passing/ShadowedTCOLet.purs",
    "passing/TCOCase.purs",
    "passing/TopLevelCase.purs",
    "passing/TypeDecl.purs",
    "passing/TypeSynonymInData.purs",
    "passing/TypedBinders.purs",
    "passing/Where.purs",
];

// These were called out separately in issue 85. Ten already occur in the
// 34-file baseline; keeping the named set here makes that overlap explicit.
const ISSUE_85_NAMED_PASSING: &[(&str, &str)] = &corpus_fixtures![
    "passing/1185.purs",
    "passing/2049.purs",
    "passing/IntAndChar.purs",
    "passing/NamedPatterns.purs",
    "passing/Patterns.purs",
    "passing/ExtendedInfixOperators.purs",
    "passing/MinusConstructor.purs",
    "passing/NegativeBinder.purs",
    "passing/BindersInFunctions.purs",
    "passing/LetPattern.purs",
    "passing/TypedBinders.purs",
    "passing/ParensInTypedBinder.purs",
];

const EXTRA_PATTERN_SHAPES: &[(&str, &str)] =
    &corpus_fixtures!["passing/Rank2Object.purs", "passing/Stream.purs",];

const ANNOTATED_BINDER_FAILURES: &[(&str, &str)] = &corpus_fixtures![
    "failing/DuplicateDeclarationsInLet.purs",
    "failing/DuplicateDeclarationsInLet2.purs",
    "failing/DuplicateDeclarationsInLet3.purs",
    "failing/OverlappingArguments.purs",
    "failing/OverlappingBinders.purs",
];

const NESTED_RECORD_PATTERN: &str = include_str!("fixtures/patterns/nested_records.purs");
const PATTERN_GUARD_FIRST_WINS: &str = include_str!("fixtures/patterns/guard_first_wins.purs");

#[test]
fn issue_85_p2_pattern_corpus_lowers_to_ast_without_loading_imports() {
    assert_eq!(ISSUE_85_P2_BASELINE.len(), 34);

    let mut fixtures = BTreeMap::new();
    for &(path, source) in ISSUE_85_P2_BASELINE
        .iter()
        .chain(ISSUE_85_NAMED_PASSING)
        .chain(EXTRA_PATTERN_SHAPES)
    {
        fixtures.insert(path, source);
    }

    assert_eq!(fixtures.len(), 38, "the maintained source set changed");
    for (path, source) in fixtures {
        let module = lower_to_ast(path, source)
            .unwrap_or_else(|failure| panic!("{path} did not lower through P2: {failure}"));
        assert!(
            !module.declarations.is_empty(),
            "{path} has no AST value declarations"
        );
    }
}

#[test]
fn p2_ast_keeps_literal_array_named_typed_nested_and_multifield_patterns() {
    let module = lower_to_ast("nested_records.purs", NESTED_RECORD_PATTERN)
        .expect("the nested record pattern fixture should lower to AST");
    let choose = module
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "choose")
        .expect("the fixture should contain `choose`");
    let mut patterns = Vec::new();
    visit_expr_patterns(&choose.value, &mut patterns);
    assert!(!patterns.is_empty(), "the fixture should contain patterns");
    assert!(
        patterns
            .iter()
            .all(|pattern| pattern.span.start < pattern.span.end)
    );

    let named_root = patterns
        .iter()
        .find_map(|pattern| match &pattern.kind {
            PatternKind::Named { binder, pattern } if binder.name == "whole" => {
                Some(pattern.as_ref())
            }
            _ => None,
        })
        .expect("the AST should retain the named pattern");
    let PatternKind::Record { fields, mode } = &named_root.kind else {
        panic!("the named pattern should wrap a record pattern");
    };
    assert_eq!(*mode, psrs_ast::RecordPatternMode::Partial);
    assert_eq!(fields.len(), 2, "the outer record has two named fields");

    let outer = field_pattern(fields, "outer");
    let PatternKind::Record {
        fields: inner_fields,
        mode: inner_mode,
    } = &outer.kind
    else {
        panic!("the `outer` field should retain its nested record pattern");
    };
    assert_eq!(*inner_mode, psrs_ast::RecordPatternMode::Partial);
    assert_eq!(inner_fields.len(), 2, "the nested record has two fields");
    assert!(matches!(
        &field_pattern(inner_fields, "enabled").kind,
        PatternKind::Boolean(true)
    ));

    let items = field_pattern(fields, "items");
    let PatternKind::Array { elements } = &items.kind else {
        panic!("the `items` field should retain its array pattern");
    };
    assert_eq!(elements.len(), 2);
    assert!(matches!(
        &elements[1].kind,
        PatternKind::Integer(value) if value == "0"
    ));

    let character = module
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "character")
        .expect("the fixture should contain `character`");
    let mut character_patterns = Vec::new();
    visit_expr_patterns(&character.value, &mut character_patterns);
    assert!(
        character_patterns
            .iter()
            .any(|pattern| matches!(&pattern.kind, PatternKind::Char('x')))
    );

    let typed = module
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "typed")
        .expect("the fixture should contain `typed`");
    let mut typed_patterns = Vec::new();
    visit_expr_patterns(&typed.value, &mut typed_patterns);
    let typed_pattern = typed_patterns
        .iter()
        .find_map(|pattern| match &pattern.kind {
            PatternKind::Typed { pattern, ty } => Some((pattern, ty)),
            _ => None,
        })
        .expect("the typed binder should retain its AST type");
    let PatternKind::Var(binder) = &typed_pattern.0.kind else {
        panic!("the typed pattern should wrap the `input` binder");
    };
    assert_eq!(binder.name, "input");
    assert_eq!(binder.span, span_in_pattern_source("input :: Int", "input"));
    let TypeKind::Name(type_name) = &typed_pattern.1.kind else {
        panic!("the typed pattern should retain the `Int` type name");
    };
    assert_eq!(type_name.text, "Int");
    assert_eq!(
        type_name.span,
        span_in_pattern_source("input :: Int", "Int")
    );
}

#[test]
fn p2_ast_retains_type_wildcards_inside_typed_patterns() {
    let source = corpus_fixtures!["passing/1664.purs"][0].1;
    let module = lower_to_ast("passing/1664.purs", source)
        .expect("typed patterns with a type wildcard should lower through P2");
    let has_wildcard = module.declarations.iter().any(|declaration| {
        let mut patterns = Vec::new();
        visit_expr_patterns(&declaration.value, &mut patterns);
        patterns.iter().any(|pattern| match &pattern.kind {
            PatternKind::Typed { ty, .. } => type_contains_wildcard(ty),
            _ => false,
        })
    });
    assert!(
        has_wildcard,
        "1664.purs should keep its `Identity _` annotation"
    );
}

#[test]
fn annotated_pattern_failures_match_their_official_error_codes() {
    for &(path, source) in ANNOTATED_BINDER_FAILURES {
        let expected = annotation_codes(source);
        assert!(
            !expected.is_empty(),
            "{path} must retain its official annotation"
        );

        let diagnostics = psrs_driver::check_source(path, source)
            .expect_err("the annotated failing fixture should be rejected");
        let mut actual = diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut expected = expected;
        actual.sort();
        expected.sort();
        actual.dedup();
        expected.dedup();
        assert_eq!(
            actual, expected,
            "{path} diagnostics disagree with annotations"
        );

        if expected.iter().any(|code| code == "OverlappingArgNames") {
            let binder_name = if path == "failing/OverlappingBinders.purs" {
                "y"
            } else {
                "x"
            };
            let expected_span = span_of_occurrence(source, binder_name, 1);
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == Some("OverlappingArgNames")
                        && diagnostic.stage == "P2 surface lowering"
                        && diagnostic.span == expected_span
                }),
                "{path} should report the second `{binder_name}` at P2"
            );
        }
    }
}

#[test]
fn lambda_array_and_as_pattern_duplicates_report_overlapping_arg_names() {
    for (name, source) in [
        ("lambda-array", "module Main where\nf = \\[x, x] -> x\n"),
        (
            "lambda-as-pattern",
            "module Main where\ndata Box a = Box a\nf = \\(x@(Box x)) -> x\n",
        ),
    ] {
        let diagnostics = psrs_driver::check_source(name, source)
            .expect_err("a lambda pattern with duplicate binders should be rejected");
        let expected_span = if name == "lambda-as-pattern" {
            span_after_fragment(source, "Box x", "x")
        } else {
            span_of_occurrence(source, "x", 1)
        };
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some("OverlappingArgNames")
                    && diagnostic.stage == "P2 surface lowering"
                    && diagnostic.span == expected_span
            }),
            "{name} should report the second `x` during P2: {diagnostics:?}"
        );
    }
}

#[test]
fn pattern_guard_keeps_duplicate_binders_and_checks_the_first_binding_type() {
    let module = lower_to_ast("guard_first_wins.purs", PATTERN_GUARD_FIRST_WINS)
        .expect("the pattern guard should lower even when its pattern repeats a name");
    let select = module
        .declarations
        .iter()
        .find(|declaration| declaration.name.text == "select")
        .expect("the fixture should contain `select`");
    let mut guards = Vec::new();
    visit_expr_guards(&select.value, &mut guards);
    let repeated = guards.iter().find_map(|guard| match guard {
        Guard::Pattern { pattern, .. } => Some(pattern),
        _ => None,
    });
    let pattern = repeated.expect("the AST should retain a Guard::Pattern");
    assert!(pattern.span.start < pattern.span.end);
    let PatternKind::Constructor { arguments, .. } = &pattern.kind else {
        panic!("the guard pattern should retain its constructor");
    };
    assert!(matches!(
        (&arguments[0].kind, &arguments[1].kind),
        (PatternKind::Var(first), PatternKind::Var(second)) if first.name == "x" && second.name == "x"
    ));

    psrs_driver::check_source("guard_first_wins.purs", PATTERN_GUARD_FIRST_WINS)
        .expect("the first Int binder should determine the result type");
}

fn lower_to_ast(source_name: &str, source_text: &str) -> Result<Module, String> {
    let source = SourceFile::new(source_name, source_text);
    let (tokens, errors) = psrs_syntax::lex(source.text());
    if !errors.is_empty() {
        return Err(format!("P0 lex errors: {errors:?}"));
    }
    let layout = psrs_syntax::add_layout(&source, &tokens);
    let cst = psrs_syntax::parse_module(&layout)
        .map_err(|error| format!("P1 parse error: {}", error.message))?;
    psrs_ast::lower_module(cst).map_err(|errors| format!("P2 surface errors: {errors:?}"))
}

fn annotation_codes(source: &str) -> Vec<String> {
    source
        .lines()
        .take_while(|line| line.trim_start().starts_with("--"))
        .filter_map(|line| {
            line.trim_start()
                .strip_prefix("-- @shouldFailWith ")
                .map(str::to_owned)
        })
        .collect()
}

fn span_of_occurrence(source: &str, needle: &str, occurrence: usize) -> TextRange {
    let (start, _) = source
        .match_indices(needle)
        .nth(occurrence)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of `{needle}`"));
    TextRange::new(start as u32, (start + needle.len()) as u32)
}

fn span_after_fragment(source: &str, fragment: &str, needle: &str) -> TextRange {
    let fragment_start = source
        .find(fragment)
        .unwrap_or_else(|| panic!("missing fragment `{fragment}`"));
    let needle_offset = fragment
        .rfind(needle)
        .unwrap_or_else(|| panic!("missing `{needle}` in fragment `{fragment}`"));
    let start = fragment_start + needle_offset;
    TextRange::new(start as u32, (start + needle.len()) as u32)
}

fn span_in_pattern_source(pattern: &str, needle: &str) -> TextRange {
    let source_offset = NESTED_RECORD_PATTERN
        .find(pattern)
        .unwrap_or_else(|| panic!("missing pattern fragment `{pattern}`"));
    let fragment_offset = pattern
        .find(needle)
        .unwrap_or_else(|| panic!("missing `{needle}` in pattern fragment"));
    let start = source_offset + fragment_offset;
    TextRange::new(start as u32, (start + needle.len()) as u32)
}

fn field_pattern<'a>(fields: &'a [(String, Pattern)], name: &str) -> &'a Pattern {
    fields
        .iter()
        .find_map(|(field, pattern)| (field == name).then_some(pattern))
        .unwrap_or_else(|| panic!("record pattern is missing field `{name}`"))
}
