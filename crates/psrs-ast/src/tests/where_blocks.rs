use super::*;

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
