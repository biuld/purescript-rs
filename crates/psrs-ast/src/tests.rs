use super::*;
use psrs_cst::{
    CstName, Declaration as CstDeclaration, Module as CstModule, Pattern, PatternKind,
    TypeVarBinder, ValueDeclaration, ValueRhs,
};
use psrs_span::TextRange;

fn name(text: &str, start: u32) -> CstName {
    CstName {
        text: text.into(),
        span: TextRange::new(start, start + text.len() as u32),
    }
}

fn pattern_var(text: &str, start: u32) -> Pattern {
    Pattern {
        kind: PatternKind::Var(name(text, start)),
        span: TextRange::new(start, start + text.len() as u32),
    }
}

fn type_var_binder(text: &str, start: u32) -> TypeVarBinder {
    TypeVarBinder {
        name: name(text, start),
        kind: None,
        span: TextRange::new(start, start + text.len() as u32),
    }
}

fn cst_module(declaration: CstDeclaration) -> CstModule {
    CstModule {
        module_keyword_span: TextRange::new(0, 6),
        name: name("Main", 7),
        exports: None,
        where_keyword_span: TextRange::new(12, 17),
        imports: Vec::new(),
        declarations: vec![declaration],
        span: TextRange::new(0, 40),
    }
}

fn value_declaration(
    name_text: &str,
    name_start: u32,
    parameters: Vec<Pattern>,
    equals_span: TextRange,
    value: cst::Expr,
    span_end: u32,
    annotation: Option<cst::TypeExpr>,
) -> CstDeclaration {
    CstDeclaration::Value(ValueDeclaration {
        name: name(name_text, name_start),
        parameters,
        rhs: ValueRhs::Plain { equals_span, value },
        where_block: None,
        span: TextRange::new(name_start, span_end),
        annotation,
    })
}

#[test]
fn function_parameters_become_nested_lambdas_and_names_stay_unresolved() {
    let value = cst::Expr {
        kind: CstExprKind::Operator {
            operator: name("+", 33),
            left: Box::new(cst::Expr {
                kind: CstExprKind::Name(name("x", 31)),
                span: TextRange::new(31, 32),
            }),
            right: Box::new(cst::Expr {
                kind: CstExprKind::Name(name("y", 35)),
                span: TextRange::new(35, 36),
            }),
        },
        span: TextRange::new(31, 36),
    };
    let declaration = value_declaration(
        "add",
        19,
        vec![pattern_var("x", 23), pattern_var("y", 25)],
        TextRange::new(27, 28),
        value,
        36,
        None,
    );
    let module = lower_module(cst_module(declaration)).unwrap();
    let ExprKind::Lambda { binder, body } = &module.declarations[0].value.kind else {
        panic!("expected the first normalized lambda");
    };
    assert_eq!(binder.name, "x");
    let ExprKind::Lambda { binder, body } = &body.kind else {
        panic!("expected the second normalized lambda");
    };
    assert_eq!(binder.name, "y");
    let ExprKind::OperatorChain {
        operands,
        operators,
    } = &body.kind
    else {
        panic!("expected the source operator chain to remain unresolved");
    };
    assert_eq!(operators.len(), 1);
    assert_eq!(operators[0].name.text, "+");
    assert!(matches!(&operands[0].kind, ExprKind::Name(name) if name.text == "x"));
    assert!(matches!(&operands[1].kind, ExprKind::Name(name) if name.text == "y"));
}

#[test]
fn parentheses_are_removed_without_losing_the_expression_range() {
    let declaration = value_declaration(
        "main",
        18,
        Vec::new(),
        TextRange::new(23, 24),
        cst::Expr {
            kind: CstExprKind::Parens {
                open_paren_span: TextRange::new(25, 26),
                expression: Box::new(cst::Expr {
                    kind: CstExprKind::Integer("42".into()),
                    span: TextRange::new(26, 28),
                }),
                close_paren_span: TextRange::new(28, 29),
            },
            span: TextRange::new(25, 29),
        },
        29,
        None,
    );
    let module = lower_module(cst_module(declaration)).unwrap();
    let value = &module.declarations[0].value;
    assert!(matches!(value.kind, ExprKind::Integer(ref value) if value == "42"));
    assert_eq!(value.span, TextRange::new(25, 29));
}

#[test]
fn lowers_forall_types_and_removes_parentheses() {
    let annotation = cst::TypeExpr {
        kind: cst::TypeExprKind::Forall {
            forall_span: TextRange::new(0, 6),
            variables: vec![type_var_binder("a", 7)],
            dot_span: TextRange::new(8, 9),
            body: Box::new(cst::TypeExpr {
                kind: cst::TypeExprKind::Parens {
                    open_paren_span: TextRange::new(10, 11),
                    expression: Box::new(cst::TypeExpr {
                        kind: cst::TypeExprKind::Name(name("a", 11)),
                        span: TextRange::new(11, 12),
                    }),
                    close_paren_span: TextRange::new(12, 13),
                },
                span: TextRange::new(10, 13),
            }),
        },
        span: TextRange::new(0, 13),
    };
    let declaration = value_declaration(
        "id",
        19,
        vec![pattern_var("x", 23)],
        TextRange::new(25, 26),
        cst::Expr {
            kind: CstExprKind::Name(name("x", 27)),
            span: TextRange::new(27, 28),
        },
        28,
        Some(annotation),
    );
    let module = lower_module(cst_module(declaration)).unwrap();
    let annotation = module.declarations[0].annotation.as_ref().unwrap();
    let TypeKind::Forall { variables, body } = &annotation.kind else {
        panic!("expected a lowered forall");
    };
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].name.text, "a");
    assert!(variables[0].kind.is_none());
    assert!(matches!(&body.kind, TypeKind::Name(name) if name.text == "a"));
    assert_eq!(body.span, TextRange::new(10, 13));
}

fn cst_expr(kind: CstExprKind, start: u32, end: u32) -> cst::Expr {
    cst::Expr {
        kind,
        span: TextRange::new(start, end),
    }
}

fn do_expr(statements: Vec<cst::DoStatement>, start: u32, end: u32) -> cst::Expr {
    cst::Expr {
        kind: CstExprKind::Do {
            do_keyword_span: TextRange::new(start, start + 2),
            layout_start_span: TextRange::empty(start + 2),
            statements,
            layout_end_span: TextRange::empty(end),
            in_keyword_span: None,
            result: None,
        },
        span: TextRange::new(start, end),
    }
}

fn lower_do_blocks(statements: Vec<cst::DoStatement>) -> Result<Expr, Vec<LowerError>> {
    let value = do_expr(statements, 30, 90);
    let declaration = value_declaration(
        "main",
        18,
        Vec::new(),
        TextRange::new(24, 25),
        value,
        90,
        None,
    );
    Ok(lower_module(cst_module(declaration))?
        .declarations
        .remove(0)
        .value)
}

#[test]
fn lowers_a_do_bind_to_bind_and_a_continuation_lambda() {
    let statements = vec![
        cst::DoStatement::Bind {
            pattern: pattern_var("x", 40),
            left_arrow_span: TextRange::new(42, 44),
            value: cst_expr(CstExprKind::Name(name("effect", 45)), 45, 51),
        },
        cst::DoStatement::Discard(cst_expr(CstExprKind::Name(name("rest", 55)), 55, 59)),
    ];
    let value = lower_do_blocks(statements).unwrap();
    let ExprKind::Application(partial, continuation) = &value.kind else {
        panic!("expected a bind application, got {:?}", value.kind);
    };
    let ExprKind::Application(function, argument) = &partial.kind else {
        panic!("expected the bind function to be applied to the effect");
    };
    assert!(matches!(&function.kind, ExprKind::Name(name) if name.text == "bind"));
    assert!(matches!(&argument.kind, ExprKind::Name(name) if name.text == "effect"));
    let ExprKind::Lambda { binder, body } = &continuation.kind else {
        panic!("expected a continuation lambda");
    };
    assert_eq!(binder.name, "x");
    assert!(matches!(&body.kind, ExprKind::Name(name) if name.text == "rest"));
    assert_eq!(value.span, TextRange::new(30, 90));
}

#[test]
fn lowers_a_do_discard_to_discard_and_a_wildcard_lambda() {
    let statements = vec![
        cst::DoStatement::Discard(cst_expr(CstExprKind::Name(name("effect", 40)), 40, 46)),
        cst::DoStatement::Discard(cst_expr(CstExprKind::Name(name("rest", 48)), 48, 52)),
    ];
    let value = lower_do_blocks(statements).unwrap();
    let ExprKind::Application(partial, continuation) = &value.kind else {
        panic!("expected a discard application");
    };
    let ExprKind::Application(function, _) = &partial.kind else {
        panic!("expected the discard function to be applied to the effect");
    };
    assert!(matches!(&function.kind, ExprKind::Name(name) if name.text == "discard"));
    let ExprKind::Lambda { binder, .. } = &continuation.kind else {
        panic!("expected a wildcard continuation lambda");
    };
    assert!(binder.name.starts_with("__psrs_do_wildcard_"));
}

#[test]
fn a_final_do_value_is_the_result_without_pure() {
    let statements = vec![cst::DoStatement::Discard(cst_expr(
        CstExprKind::Name(name("result", 40)),
        40,
        46,
    ))];
    let value = lower_do_blocks(statements).unwrap();
    assert!(matches!(&value.kind, ExprKind::Name(name) if name.text == "result"));
    assert_eq!(value.span, TextRange::new(30, 90));
}

#[test]
fn lowers_a_non_variable_do_binder_through_a_case() {
    let constructor = Pattern {
        kind: PatternKind::Constructor {
            name: name("Unit", 40),
            arguments: Vec::new(),
        },
        span: TextRange::new(40, 44),
    };
    let statements = vec![
        cst::DoStatement::Bind {
            pattern: constructor,
            left_arrow_span: TextRange::new(45, 47),
            value: cst_expr(CstExprKind::Name(name("effect", 48)), 48, 54),
        },
        cst::DoStatement::Discard(cst_expr(CstExprKind::Name(name("rest", 56)), 56, 60)),
    ];
    let value = lower_do_blocks(statements).unwrap();
    let ExprKind::Application(_, continuation) = &value.kind else {
        panic!("expected a bind application");
    };
    let ExprKind::Lambda { binder, body } = &continuation.kind else {
        panic!("expected a continuation lambda");
    };
    assert!(binder.name.starts_with("$psrs_pattern_"));
    let ExprKind::Case {
        scrutinee,
        branches,
    } = &body.kind
    else {
        panic!("expected a case for the non-variable binder");
    };
    assert!(matches!(&scrutinee.kind, ExprKind::Name(name) if name.text == binder.name));
    assert_eq!(branches.len(), 1);
    assert!(matches!(
        &branches[0].pattern.kind,
        crate::PatternKind::Constructor { name, arguments }
            if name.text == "Unit" && arguments.is_empty()
    ));
}

#[test]
fn lowers_a_do_let_to_a_let_around_the_rest() {
    let binding = value_declaration(
        "y",
        40,
        Vec::new(),
        TextRange::new(42, 43),
        cst_expr(CstExprKind::Integer("1".into()), 44, 45),
        45,
        None,
    );
    let statements = vec![
        cst::DoStatement::Let {
            let_keyword_span: TextRange::new(32, 35),
            declarations: vec![binding],
            layout_start_span: TextRange::empty(36),
            layout_end_span: TextRange::empty(45),
        },
        cst::DoStatement::Discard(cst_expr(CstExprKind::Name(name("rest", 48)), 48, 52)),
    ];
    let value = lower_do_blocks(statements).unwrap();
    let ExprKind::Let { declarations, body } = &value.kind else {
        panic!("expected a let around the rest");
    };
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].name.text, "y");
    assert!(matches!(&body.kind, ExprKind::Name(name) if name.text == "rest"));
}

#[test]
fn rejects_an_empty_do_block() {
    let error = lower_do_blocks(Vec::new()).unwrap_err();
    assert_eq!(error[0].message, "an empty `do` block is not allowed");
    assert!(error[0].code.is_none());
}

#[test]
fn rejects_a_do_block_ending_in_a_bind() {
    let statements = vec![cst::DoStatement::Bind {
        pattern: pattern_var("x", 40),
        left_arrow_span: TextRange::new(42, 44),
        value: cst_expr(CstExprKind::Name(name("effect", 45)), 45, 51),
    }];
    let error = lower_do_blocks(statements).unwrap_err();
    assert_eq!(error[0].code, Some("InvalidDoBind"));
}

fn declaration_block(
    declarations: Vec<CstDeclaration>,
    start: u32,
    end: u32,
) -> cst::DeclarationBlock {
    cst::DeclarationBlock {
        where_keyword_span: TextRange::new(start, start + 5),
        layout_start_span: TextRange::empty(start + 5),
        declarations,
        layout_end_span: TextRange::empty(end),
        span: TextRange::new(start, end),
    }
}

#[test]
fn lowers_a_value_where_block_to_a_let_inside_the_parameters() {
    let binding = value_declaration(
        "y",
        40,
        Vec::new(),
        TextRange::new(42, 43),
        cst_expr(CstExprKind::Integer("1".into()), 44, 45),
        45,
        None,
    );
    let declaration = CstDeclaration::Value(ValueDeclaration {
        name: name("f", 30),
        parameters: vec![pattern_var("x", 32)],
        rhs: ValueRhs::Plain {
            equals_span: TextRange::new(34, 35),
            value: cst_expr(CstExprKind::Name(name("y", 36)), 36, 37),
        },
        where_block: Some(declaration_block(vec![binding], 38, 50)),
        span: TextRange::new(30, 50),
        annotation: None,
    });
    let module = lower_module(cst_module(declaration)).unwrap();
    let ExprKind::Lambda { binder, body } = &module.declarations[0].value.kind else {
        panic!("expected the parameter lambda");
    };
    assert_eq!(binder.name, "x");
    let ExprKind::Let { declarations, body } = &body.kind else {
        panic!("expected the where declarations as a let inside the parameter");
    };
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].name.text, "y");
    assert!(matches!(&body.kind, ExprKind::Name(name) if name.text == "y"));
}

#[test]
fn lowers_a_case_where_block_to_a_let_in_the_branch() {
    let binding = value_declaration(
        "y",
        40,
        Vec::new(),
        TextRange::new(42, 43),
        cst_expr(CstExprKind::Name(name("x", 44)), 44, 45),
        45,
        None,
    );
    let alternative = cst::CaseAlternative {
        patterns: vec![pattern_var("x", 33)],
        rhs: cst::CaseRhs::Plain {
            arrow_span: TextRange::new(35, 37),
            value: cst_expr(CstExprKind::Name(name("y", 38)), 38, 39),
            where_block: Some(declaration_block(vec![binding], 40, 50)),
        },
        span: TextRange::new(33, 50),
    };
    let case = cst_expr(
        CstExprKind::Case {
            case_keyword_span: TextRange::new(20, 24),
            scrutinees: vec![cst_expr(CstExprKind::Name(name("value", 25)), 25, 30)],
            of_keyword_span: TextRange::new(31, 33),
            layout_start_span: TextRange::empty(33),
            alternatives: vec![alternative],
            layout_end_span: TextRange::empty(50),
        },
        20,
        50,
    );
    let declaration = value_declaration(
        "main",
        18,
        Vec::new(),
        TextRange::new(24, 25),
        case,
        50,
        None,
    );
    let module = lower_module(cst_module(declaration)).unwrap();
    let ExprKind::Case { branches, .. } = &module.declarations[0].value.kind else {
        panic!("expected a case expression");
    };
    assert_eq!(branches.len(), 1);
    let ExprKind::Let { declarations, body } = &branches[0].value.kind else {
        panic!("expected the where declarations as a let around the branch body");
    };
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].name.text, "y");
    assert!(matches!(&body.kind, ExprKind::Name(name) if name.text == "y"));
}

#[test]
fn rejects_a_do_block_ending_in_a_let() {
    let binding = value_declaration(
        "y",
        40,
        Vec::new(),
        TextRange::new(42, 43),
        cst_expr(CstExprKind::Integer("1".into()), 44, 45),
        45,
        None,
    );
    let statements = vec![cst::DoStatement::Let {
        let_keyword_span: TextRange::new(32, 35),
        declarations: vec![binding],
        layout_start_span: TextRange::empty(36),
        layout_end_span: TextRange::empty(45),
    }];
    let error = lower_do_blocks(statements).unwrap_err();
    assert_eq!(error[0].code, Some("InvalidDoLet"));
}
