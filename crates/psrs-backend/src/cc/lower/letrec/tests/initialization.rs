use super::*;

#[test]
fn observable_recursive_initializer_is_not_delayed_until_a_call() {
    let mut module = mutual_recursion_module();
    let ExprKind::Let { bindings, .. } = &mut module.declarations[0].value.kind else {
        panic!("fixture contains a recursive group");
    };
    let original = bindings[0].value.clone();
    let ty = original.ty;
    bindings[0].value.kind = ExprKind::If {
        condition: Box::new(expression(ExprKind::Boolean(true), TypeId(1), 20, 21)),
        then_branch: Box::new(expression(ExprKind::Trap, ty, 21, 22)),
        else_branch: Box::new(original),
    };
    let errors = crate::cc::lower_module(module)
        .expect_err("an observable recursive initializer requires eager evaluation");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("eager recursive value support")),
        "{errors:?}"
    );
}
