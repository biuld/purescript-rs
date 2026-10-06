use super::*;
use crate::{add_layout, lex};
use psrs_cst::{Declaration, ExprKind, PatternKind, TypeExprKind, ValueRhs};
use psrs_span::SourceFile;

fn parse(source: &str) -> Result<Module, ParseError> {
    let source_file = SourceFile::new("test.purs", source);
    let (tokens, errors) = lex(source);
    assert!(errors.is_empty(), "{errors:?}");
    parse_module(&add_layout(&source_file, &tokens))
}

fn as_value(declaration: &Declaration) -> &psrs_cst::ValueDeclaration {
    let Declaration::Value(declaration) = declaration else {
        panic!("expected a value declaration, found {declaration:?}");
    };
    declaration
}

fn plain_value(declaration: &psrs_cst::ValueDeclaration) -> &psrs_cst::Expr {
    let ValueRhs::Plain { value, .. } = &declaration.rhs else {
        panic!("expected a plain right-hand side");
    };
    value
}

fn pattern_names(declaration: &psrs_cst::ValueDeclaration) -> Vec<&str> {
    declaration
        .parameters
        .iter()
        .map(|pattern| {
            let PatternKind::Var(name) = &pattern.kind else {
                panic!("expected a variable pattern");
            };
            name.text.as_str()
        })
        .collect()
}

#[test]
fn parses_a_module_with_functions_and_local_let() {
    let module = parse("module Main where\n\nadd x y = x + y\n\nmain =\n  let\n    answer = add 40 2\n  in answer\n").unwrap();
    assert_eq!(module.name.text, "Main");
    assert_eq!(module.declarations.len(), 2);
    assert_eq!(pattern_names(as_value(&module.declarations[0])), ["x", "y"]);
    assert!(matches!(
        plain_value(as_value(&module.declarations[1])).kind,
        ExprKind::Let { .. }
    ));
}

#[test]
fn reports_a_useful_error_for_malformed_declarations() {
    let error = parse("module Main where\nmain 42\n").unwrap_err();
    assert!(error.message.contains("expected `=`") || error.message.contains("expected Equals"));
}

#[test]
fn parses_lambdas_conditionals_and_operator_precedence() {
    let module =
        parse("module Main where\nmain = \\x -> if x < 1 then x + 1 * 2 else x\n").unwrap();
    let declaration = as_value(&module.declarations[0]);
    let ExprKind::Lambda { body, .. } = &plain_value(declaration).kind else {
        panic!("expected lambda expression");
    };
    let ExprKind::If { then_branch, .. } = &body.kind else {
        panic!("expected conditional expression");
    };
    let ExprKind::Operator {
        operator, right, ..
    } = &then_branch.kind
    else {
        panic!("expected addition");
    };
    assert_eq!(operator.text, "+");
    assert!(matches!(right.kind, ExprKind::Operator { ref operator, .. } if operator.text == "*"));
}

#[test]
fn parses_a_qualified_name_between_backticks_as_one_operator() {
    let module = parse("module Main where\nzip xs ys = xs `A.zip` ys\n").unwrap();
    let ExprKind::Operator { operator, .. } = &plain_value(as_value(&module.declarations[0])).kind
    else {
        panic!("expected an infix operator");
    };
    assert_eq!(operator.text, "A.zip");
}

#[test]
fn distinguishes_parenthesized_negation_from_explicit_operator_sections() {
    let module = parse(
        "module Main where\nnegative = (-5)\nnegativeVariable x = (-x)\nsubtractOne = (_ - 1)\naddOne = (1 + _)\n",
    )
    .unwrap();

    for declaration in &module.declarations[..2] {
        let expression = plain_value(as_value(declaration));
        let ExprKind::Parens { expression, .. } = &expression.kind else {
            panic!("expected parenthesized negative expression, found {expression:?}");
        };
        assert!(matches!(expression.kind, ExprKind::Negate { .. }));
    }

    let subtract = plain_value(as_value(&module.declarations[2]));
    assert!(matches!(
        subtract.kind,
        ExprKind::OperatorSection {
            side: psrs_cst::OperatorSectionSide::Right,
            ..
        }
    ));
    let add = plain_value(as_value(&module.declarations[3]));
    assert!(matches!(
        add.kind,
        ExprKind::OperatorSection {
            side: psrs_cst::OperatorSectionSide::Left,
            ..
        }
    ));
}

#[test]
fn distinguishes_lowercase_record_fields_from_uppercase_qualified_values() {
    let module =
        parse("module Main where\nfieldAccess record = record.value\nqualified = Data.Array.map\n")
            .unwrap();

    let field_access = plain_value(as_value(&module.declarations[0]));
    let ExprKind::FieldAccess {
        expression, field, ..
    } = &field_access.kind
    else {
        panic!("expected lowercase dotted expression to be a record field access");
    };
    assert_eq!(field.text, "value");
    assert!(matches!(&expression.kind, ExprKind::Name(name) if name.text == "record"));

    let qualified = plain_value(as_value(&module.declarations[1]));
    assert!(matches!(&qualified.kind, ExprKind::Name(name) if name.text == "Data.Array.map"));
}

#[test]
fn parses_anonymous_record_field_accessors() {
    let module = parse("module Main where\nproject = _.value.nested\n").unwrap();
    let accessor = plain_value(as_value(&module.declarations[0]));
    let ExprKind::RecordAccessor {
        marker_span,
        fields,
    } = &accessor.kind
    else {
        panic!("expected `_ .field` to have its own CST node");
    };
    assert_eq!(marker_span, &TextRange::new(28, 29));
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].field.text, "value");
    assert_eq!(fields[1].field.text, "nested");
}

#[test]
fn parses_single_line_let_blocks() {
    let module = parse("module Main where\nmain = let x = 1 in x\n").unwrap();
    assert!(matches!(
        plain_value(as_value(&module.declarations[0])).kind,
        ExprKind::Let { .. }
    ));
}

#[test]
fn cst_retains_binder_and_concrete_token_ranges() {
    let source = "module Main where\nmain x = (x)\n";
    let module = parse(source).unwrap();
    let declaration = as_value(&module.declarations[0]);
    assert_eq!(
        &source[declaration.name.span.start as usize..declaration.name.span.end as usize],
        "main"
    );
    let PatternKind::Var(binder) = &declaration.parameters[0].kind else {
        panic!("expected a variable binder");
    };
    assert_eq!(
        &source[binder.span.start as usize..binder.span.end as usize],
        "x"
    );
    let ValueRhs::Plain { equals_span, value } = &declaration.rhs else {
        panic!("expected a plain right-hand side");
    };
    assert_eq!(
        &source[equals_span.start as usize..equals_span.end as usize],
        "="
    );
    let ExprKind::Parens {
        open_paren_span,
        close_paren_span,
        ..
    } = value.kind
    else {
        panic!("expected concrete parentheses in CST");
    };
    assert_eq!(
        &source[open_paren_span.start as usize..open_paren_span.end as usize],
        "("
    );
    assert_eq!(
        &source[close_paren_span.start as usize..close_paren_span.end as usize],
        ")"
    );
}

#[test]
fn parses_a_signed_declaration_with_a_function_type() {
    let module =
        parse("module Main where\nidentity :: forall a. a -> a\nidentity x = x\n").unwrap();
    let declaration = as_value(&module.declarations[0]);
    assert_eq!(declaration.name.text, "identity");
    assert_eq!(pattern_names(declaration), ["x"]);
    let annotation = declaration.annotation.as_ref().expect("annotation");
    let TypeExprKind::Forall {
        variables, body, ..
    } = &annotation.kind
    else {
        panic!("expected a forall type");
    };
    assert_eq!(
        variables
            .iter()
            .map(|variable| variable.name.text.as_str())
            .collect::<Vec<_>>(),
        ["a"]
    );
    assert!(matches!(body.kind, TypeExprKind::Function { .. }));
}

#[test]
fn parses_forall_with_multiple_variables() {
    let module =
        parse("module Main where\nchoose :: forall a b. a -> b -> a\nchoose x y = x\n").unwrap();
    let declaration = as_value(&module.declarations[0]);
    let annotation = declaration.annotation.as_ref().unwrap();
    let TypeExprKind::Forall { variables, .. } = &annotation.kind else {
        panic!("expected a forall type");
    };
    assert_eq!(
        variables
            .iter()
            .map(|variable| variable.name.text.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
}

#[test]
fn parses_empty_parentheses_as_an_empty_row_type() {
    let module = parse("module Main where\ntype Empty = ()\n").unwrap();
    let Declaration::TypeSynonym(declaration) = &module.declarations[0] else {
        panic!("expected a type synonym");
    };
    assert!(matches!(
        &declaration.body.kind,
        TypeExprKind::Row { fields, tail: None, .. } if fields.is_empty()
    ));
}

#[test]
fn type_arrows_are_right_associative() {
    let module = parse("module Main where\nf :: a -> b -> c\nf x = x\n").unwrap();
    let declaration = as_value(&module.declarations[0]);
    let annotation = declaration.annotation.as_ref().unwrap();
    let TypeExprKind::Function { right, .. } = &annotation.kind else {
        panic!("expected a function type");
    };
    assert!(matches!(right.kind, TypeExprKind::Function { .. }));
}

#[test]
fn parses_module_exports_and_imports() {
    let module = parse(
        "module Main (main, Foo(..)) where\nimport Prelude hiding (map)\nimport Data.Maybe as Maybe\nmain = 1\n",
    )
    .unwrap();
    assert!(module.exports.is_some());
    assert_eq!(module.imports.len(), 2);
    assert_eq!(module.imports[0].module.text, "Prelude");
    assert_eq!(module.imports[1].alias.as_ref().unwrap().text, "Maybe");
}

#[test]
fn parses_data_newtype_and_type_declarations() {
    let module = parse(
        "module Main where\ndata Person = Person String Boolean\nnewtype Age = Age Int\ntype Name = String\n",
    )
    .unwrap();
    assert!(matches!(module.declarations[0], Declaration::Data(_)));
    assert!(matches!(module.declarations[1], Declaration::Newtype(_)));
    assert!(matches!(
        module.declarations[2],
        Declaration::TypeSynonym(_)
    ));
}

#[test]
fn parses_class_and_instance_heads() {
    let module =
        parse(
            "module Main where\nclass Eq a <= Ord a where\n  compare :: a -> a -> Int\ninstance ordInt :: Ord Int where\n  compare x y = x\n",
        )
        .unwrap();
    assert!(matches!(module.declarations[0], Declaration::Class(_)));
    assert!(matches!(module.declarations[1], Declaration::Instance(_)));
}

#[test]
fn agrees_with_official_layout_fixture_parse_outcomes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/layout");
    let mut count = 0;
    for entry in std::fs::read_dir(root).expect("vendored layout corpus") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "purs") {
            let source = std::fs::read_to_string(&path).unwrap();
            // These layout fixtures are deliberately rejected by the official parser.
            let expected_ok = !["DoLet.purs", "LetGuards.purs", "InstanceChainElse.purs"]
                .iter()
                .any(|name| path.file_name().unwrap() == *name);
            assert_eq!(
                parse(&source).is_ok(),
                expected_ok,
                "{}: {:?}",
                path.display(),
                parse(&source)
            );
            count += 1;
        }
    }
    assert_eq!(count, 15);
}

#[test]
fn parses_keyword_record_labels_and_guarded_lambdas() {
    for source in [
        "module Main where\nx = { case: 1, do: 2, let: 3, where: 4, if: 5 }\n",
        "module Main where\nx = case a, b of\n  c, d | f (\\a -> a), true -> d\n",
        "module Main where\nx a\n  | do that\n       that = true\n  | otherwise = false\n",
    ] {
        assert!(parse(source).is_ok(), "{source}: {:?}", parse(source));
    }
}

#[test]
fn layout_closes_case_and_guard_blocks_at_their_terminators() {
    use crate::LayoutTokenKind::{LayoutEnd, Raw};
    use crate::RawTokenKind;

    for (source, terminator, ends) in [
        ("module Main where\nx = [case a of b -> c, d]\n", ",", 1),
        (
            "module Main where\nx = a `case _ of b -> const` c\n",
            "` c",
            1,
        ),
        (
            "module Main where\nx = case a of\n  b | do f -> c\n",
            "->",
            1,
        ),
        ("module Main where\nx a | do f = c\n", "= c", 1),
        ("module Main where\nx = [do do do f, g]\n", ",", 3),
    ] {
        let file = SourceFile::new("test.purs", source);
        let (tokens, errors) = lex(source);
        assert!(errors.is_empty(), "{errors:?}");
        let layout = add_layout(&file, &tokens);
        let at = source.find(terminator).unwrap() as u32;
        let index = layout
            .iter()
            .position(|token| token.span.start == at && matches!(token.kind, Raw(_)))
            .unwrap();
        assert_eq!(
            layout[..index]
                .iter()
                .rev()
                .take_while(|token| token.kind == LayoutEnd)
                .count(),
            ends,
            "{source}"
        );
        assert!(
            layout[index - ends..index]
                .iter()
                .all(|token| token.span == psrs_span::TextRange::empty(at))
        );
        assert!(matches!(
            layout[index].kind,
            Raw(RawTokenKind::Comma
                | RawTokenKind::Backtick
                | RawTokenKind::Arrow
                | RawTokenKind::Equals)
        ));
    }
}

#[test]
fn parses_negative_number_patterns_after_prior_alternative() {
    let source =
        "module Main where\nchoose value = case value of\n  0.0 -> 20\n  -0.0 -> 30\n  _ -> 0\n";
    let module = parse(source).unwrap();
    let ExprKind::Case { alternatives, .. } = &plain_value(as_value(&module.declarations[0])).kind
    else {
        panic!("expected a case expression");
    };
    assert_eq!(alternatives.len(), 3);
    assert!(
        matches!(alternatives[1].patterns[0].kind, PatternKind::Number(ref value) if value == "-0.0")
    );
}

#[test]
fn keeps_a_deeper_indented_minus_in_a_case_rhs_as_an_operator() {
    let source = "module Main where\nvalue = case input of\n  _ ->\n    20\n      - 1\n";
    let module = parse(source).unwrap();
    let ExprKind::Case { alternatives, .. } = &plain_value(as_value(&module.declarations[0])).kind
    else {
        panic!("expected a case expression");
    };
    let psrs_cst::CaseRhs::Plain { value, .. } = &alternatives[0].rhs else {
        panic!("expected a plain case alternative");
    };
    assert!(matches!(&value.kind, ExprKind::Operator { operator, .. } if operator.text == "-"));
}

#[test]
fn record_projection_and_update_bind_before_value_application() {
    let module = parse(
        "module Main where\nproject r = consume r.value\nupdate r = consume r { value = 42 }\n",
    )
    .unwrap();
    for (index, projection) in [(0, true), (1, false)] {
        let expression = plain_value(as_value(&module.declarations[index]));
        let ExprKind::Application(function, argument) = &expression.kind else {
            panic!("record postfix must belong to the argument: {expression:?}");
        };
        assert!(matches!(&function.kind, ExprKind::Name(name) if name.text == "consume"));
        if projection {
            assert!(
                matches!(&argument.kind, ExprKind::FieldAccess { expression, .. } if matches!(&expression.kind, ExprKind::Name(name) if name.text == "r"))
            );
        } else {
            assert!(
                matches!(&argument.kind, ExprKind::RecordUpdate { expression, .. } if matches!(&expression.kind, ExprKind::Name(name) if name.text == "r"))
            );
        }
    }
}
