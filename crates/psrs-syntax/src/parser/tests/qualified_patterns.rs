use super::*;

#[test]
fn qualified_pattern_operators_preserve_names_spans_and_constructor_operands() {
    for (pattern, expected, constructor) in [
        ("head L.: tail", "L.:", false),
        ("Wrap head Foo.Bar.: tail", "Foo.Bar.:", true),
        ("L.Wrap head Foo.Bar... tail", "Foo.Bar...", true),
    ] {
        let source = format!("module Main where\nmain = case value of\n  {pattern} -> 42\n");
        let module = parse(&source).unwrap();
        let ExprKind::Case { alternatives, .. } =
            &plain_value(as_value(&module.declarations[0])).kind
        else {
            panic!("expected a case");
        };
        let PatternKind::OperatorChain {
            operands,
            operators,
        } = &alternatives[0].patterns[0].kind
        else {
            panic!("expected an operator chain");
        };
        assert_eq!(operands.len(), 2);
        assert_eq!(operators.len(), 1);
        assert_eq!(operators[0].text, expected);
        let span = operators[0].span;
        assert_eq!(&source[span.start as usize..span.end as usize], expected);
        if constructor {
            let PatternKind::Constructor { arguments, .. } = &operands[0].kind else {
                panic!("expected the left constructor operand");
            };
            assert_eq!(arguments.len(), 1);
        }
    }
}

#[test]
fn qualified_pattern_operator_before_unicode_guards_parses() {
    parse("module Main where\nmain = case value of\n  t L.: ts\n    | t == tag → true\n    | otherwise → go ts\n  L.Nil → false\n").unwrap();
}
