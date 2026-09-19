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
