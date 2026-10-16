use super::super::{
    Expr, ExprKind, Module, TextRange, Type, TypeConstructor, TypeId, arrow_type,
    single_declaration,
};
use psrs_hir::TypeVariableId;

fn projected_identity(reverse: bool, inconsistent: bool) -> Module {
    let mut types = vec![
        Type::Variable(TypeVariableId(0)),
        Type::Variable(TypeVariableId(1)),
        Type::Constructor(TypeConstructor::Array),
        Type::Constructor(TypeConstructor::Int),
    ];
    let general_body = arrow_type(
        &mut types,
        if inconsistent { TypeId(3) } else { TypeId(0) },
        TypeId(0),
    );
    let general = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![TypeVariableId(0)],
        body: general_body,
    });
    let array = TypeId(types.len() as u32);
    types.push(Type::Application(TypeId(2), TypeId(1)));
    let array_body = arrow_type(&mut types, array, array);
    let specialized = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![TypeVariableId(1)],
        body: array_body,
    });
    let (field, selected) = if reverse {
        (specialized, general)
    } else {
        (general, specialized)
    };
    let record = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    let empty = TypeId(types.len() as u32);
    types.push(Type::RowEmpty);
    let row = TypeId(types.len() as u32);
    types.push(Type::RowExtend {
        label: "identity".into(),
        ty: field,
        tail: empty,
    });
    let dictionary = TypeId(types.len() as u32);
    types.push(Type::Application(record, row));
    let span = TextRange::new(0, 1);
    single_declaration(
        types,
        selected,
        Expr {
            kind: ExprKind::FieldAccess {
                record: Box::new(Expr {
                    kind: ExprKind::Trap,
                    ty: dictionary,
                    span,
                }),
                field: "identity".into(),
            },
            ty: selected,
            span,
        },
    )
}

#[test]
fn a_universal_dictionary_method_can_specialize_under_another_universal() {
    projected_identity(false, false).verify().unwrap();
}

#[test]
fn universal_specialization_cannot_generalize_or_change_its_input_relation() {
    for (reverse, inconsistent) in [(true, false), (false, true)] {
        let errors = projected_identity(reverse, inconsistent)
            .verify()
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message
                    == "Core expression type is inconsistent with its context")
        );
    }
}
