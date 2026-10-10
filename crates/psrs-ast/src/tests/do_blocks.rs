use super::*;

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
