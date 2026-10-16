use super::*;
use psrs_hir::TypeVariableId;

fn fixture(entry: TypeId) -> Module {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Unit),
        Type::Variable(TypeVariableId(7)),
    ];
    let runner_type = arrow_type(&mut types, TypeId(1), TypeId(0));
    let span = TextRange::new(0, 1);
    let mut module = single_declaration(
        types,
        entry,
        Expr {
            kind: ExprKind::Trap,
            ty: entry,
            span,
        },
    );
    module.declarations[0].quantified = vec![TypeVariableId(7)];
    let symbol = SymbolId::new(module.id, 1);
    module.declarations.push(Declaration {
        symbol,
        name: "runner".into(),
        name_span: span,
        quantified: vec![TypeVariableId(8)],
        ty: runner_type,
        value: Expr {
            kind: ExprKind::Trap,
            ty: runner_type,
            span,
        },
        span,
    });
    module
}

#[test]
fn closed_command_types_allow_quantifiers_used_only_inside_declarations() {
    for ty in [TypeId(0), TypeId(1)] {
        let mut module = fixture(ty);
        let entry = module.declarations[0].symbol;
        let runner = module.declarations[1].symbol;
        let normalized = crate::command::normalize_entry(&mut module, entry, runner).unwrap();
        assert_eq!(normalized == entry, ty == TypeId(0));
        module.verify().unwrap();
    }
}

#[test]
fn open_command_types_are_rejected_without_changing_the_module() {
    let mut module = fixture(TypeId(2));
    let entry = module.declarations[0].symbol;
    let runner = module.declarations[1].symbol;
    let before = module.clone();
    let errors = crate::command::normalize_entry(&mut module, entry, runner).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message == "command entry must be monomorphic")
    );
    assert_eq!(module, before);
}
