use super::*;

/// Clone and equality follow a deep application spine on a heap stack.
/// Ordinary recursive destruction also fits the worker stack at this depth.
#[test]
fn deep_application_clone_equality_and_drop_stay_within_a_worker_stack() {
    let mut expression = Expr {
        kind: ExprKind::Boolean(true),
        ty: TypeId(0),
        span: TextRange::default(),
    };
    for _ in 0..800 {
        expression = Expr {
            kind: ExprKind::Application(
                Box::new(Expr {
                    kind: ExprKind::Boolean(false),
                    ty: TypeId(0),
                    span: TextRange::default(),
                }),
                Box::new(expression),
            ),
            ty: TypeId(0),
            span: TextRange::default(),
        };
    }
    let cloned = expression.clone();
    assert_eq!(expression, cloned);
    drop(cloned);
    drop(expression);
}
