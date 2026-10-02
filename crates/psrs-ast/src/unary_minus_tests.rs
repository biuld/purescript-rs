use crate::{ExprKind, cst, lower_module};
use psrs_cst::{
    CstName, Declaration as CstDeclaration, ExprKind as CstExprKind, Module as CstModule,
    ValueDeclaration, ValueRhs,
};
use psrs_span::TextRange;

fn name(text: &str, start: u32) -> CstName {
    CstName {
        text: text.into(),
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
    equals_span: TextRange,
    value: cst::Expr,
    span_end: u32,
) -> CstDeclaration {
    CstDeclaration::Value(ValueDeclaration {
        name: name(name_text, name_start),
        parameters: Vec::new(),
        rhs: ValueRhs::Plain { equals_span, value },
        where_block: None,
        span: TextRange::new(name_start, span_end),
        annotation: None,
    })
}

#[test]
fn unary_minus_survives_ast_lowering_with_both_source_ranges() {
    let expression = cst::Expr {
        kind: CstExprKind::Negate {
            minus_span: TextRange::new(24, 25),
            expression: Box::new(cst::Expr {
                kind: CstExprKind::Integer("5".into()),
                span: TextRange::new(25, 26),
            }),
        },
        span: TextRange::new(24, 26),
    };
    let declaration = value_declaration("value", 18, TextRange::new(23, 24), expression, 26);

    let module = lower_module(cst_module(declaration)).unwrap();
    let value = &module.declarations[0].value;
    let ExprKind::Negate {
        minus_span,
        expression,
    } = &value.kind
    else {
        panic!("expected unary minus to remain an unresolved surface form");
    };
    assert_eq!(*minus_span, TextRange::new(24, 25));
    assert_eq!(expression.span, TextRange::new(25, 26));
    assert_eq!(value.span, TextRange::new(24, 26));
}
