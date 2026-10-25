use super::*;

fn observable_module() -> Module {
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
    module
}

#[test]
fn observable_recursive_initializer_is_not_delayed_until_a_call() {
    let backend = crate::cc::lower_module(observable_module())
        .expect("observable recursive initializers use eager memo getters");
    let main = &backend.cc.functions[0];
    assert!(
        main.assignments
            .iter()
            .filter(|a| matches!(a.kind, AssignmentKind::IndirectCall { .. }))
            .count()
            >= 2
    );
    assert!(
        backend
            .cc
            .functions
            .iter()
            .any(|f| f.name.contains("_memo_"))
    );
    crate::mir::lower_module(backend.cc).expect("memo getters lower through P9");
}

#[test]
fn initialization_errors_do_not_fabricate_missing_source_locations() {
    let mut module = observable_module();
    module.sources = Default::default();
    let errors = crate::cc::lower_module(module).unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == crate::BackendErrorKind::InvalidCompilerIr
                && error.message.contains("source identity has no source unit")
        ),
        "{errors:?}"
    );
}
