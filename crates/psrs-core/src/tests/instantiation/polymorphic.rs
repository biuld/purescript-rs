//! Explicit quantified arguments survive checked nominal instantiation.

use super::*;

#[test]
fn a_polymorphic_nominal_argument_cannot_contain_the_parameter_being_solved() {
    let parameter = TypeVariableId(10);
    let bound = TypeVariableId(11);
    let mut types = vec![Type::Variable(parameter), Type::Variable(bound)];
    let body = arrow(&mut types, TypeId(0), TypeId(1));
    let poly = forall(&mut types, vec![bound], body);
    let scheme = nominal(&mut types, 1, TypeId(0));
    let instance = nominal(&mut types, 1, poly);
    let module = bare(types);
    assert!(
        module
            .checked_instantiation(scheme, &[parameter], instance)
            .is_none()
    );
    assert!(
        module
            .checked_instantiation(instance, &[parameter], scheme)
            .is_none()
    );
}

#[test]
fn nominal_parameters_retain_explicit_polymorphic_arguments() {
    let parameter = TypeVariableId(10);
    let bound = TypeVariableId(11);
    let renamed = TypeVariableId(12);
    let mut types = vec![
        Type::Variable(parameter),
        Type::Variable(bound),
        Type::Variable(renamed),
        Type::Constructor(TypeConstructor::Int),
    ];
    let body = arrow(&mut types, TypeId(1), TypeId(1));
    let poly = forall(&mut types, vec![bound], body);
    let body = arrow(&mut types, TypeId(2), TypeId(2));
    let alpha = forall(&mut types, vec![renamed], body);
    let mono = arrow(&mut types, TypeId(3), TypeId(3));
    let abstract_box = nominal(&mut types, 1, TypeId(0));
    let poly_box = nominal(&mut types, 1, poly);
    let mono_box = nominal(&mut types, 1, mono);
    let scheme = arrow(&mut types, abstract_box, abstract_box);
    let alpha_box = nominal(&mut types, 1, alpha);
    let instance = arrow(&mut types, poly_box, alpha_box);
    let inconsistent = arrow(&mut types, poly_box, mono_box);
    let module = bare(types);

    let evidence = module
        .checked_instantiation(scheme, &[parameter], instance)
        .expect("an explicitly polymorphic argument is retained under a nominal constructor");
    assert!(module.types_equivalent(evidence.replacements[&parameter], poly));
    assert!(
        module
            .checked_instantiation(scheme, &[parameter], inconsistent)
            .is_none()
    );
    assert!(
        module
            .checked_instantiation(abstract_box, &[], poly_box)
            .is_none()
    );
    assert!(
        module
            .checked_instantiation(poly_box, &[], mono_box)
            .is_none()
    );
    assert!(
        module
            .checked_instantiation(mono_box, &[], poly_box)
            .is_none()
    );
}

#[test]
fn constructor_fields_preserve_an_explicit_universal_argument() {
    let parameter = TypeVariableId(10);
    let bound = TypeVariableId(11);
    let mut types = vec![
        Type::Variable(parameter),
        Type::Variable(bound),
        Type::Constructor(TypeConstructor::Int),
    ];
    let body = arrow(&mut types, TypeId(1), TypeId(1));
    let poly = forall(&mut types, vec![bound], body);
    let mono = arrow(&mut types, TypeId(2), TypeId(2));
    let boxed = nominal(&mut types, 1, poly);
    let constructor = SymbolId::new(ModuleId(0), 1);
    for (field, accepted) in [(poly, true), (mono, false)] {
        let mut types = types.clone();
        let ty = arrow(&mut types, field, boxed);
        let mut module = bare(types);
        module.constructors.push(ConstructorInfo {
            symbol: constructor,
            name: "Box".into(),
            type_id: HirTypeId::new(ModuleId(0), 1),
            tag: 0,
            field_count: 1,
            field_types: vec![TypeId(0)],
            parameters: vec![parameter],
        });
        module.declarations.push(Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "box".into(),
            name_span: SPAN,
            quantified: Vec::new(),
            ty,
            span: SPAN,
            value: Expr {
                ty,
                span: SPAN,
                kind: ExprKind::Lambda {
                    binder: Binder {
                        id: LocalId(0),
                        name: "value".into(),
                        ty: field,
                        span: SPAN,
                    },
                    body: Box::new(Expr {
                        ty: boxed,
                        span: SPAN,
                        kind: ExprKind::Constructor {
                            symbol: constructor,
                            arguments: vec![Expr {
                                kind: ExprKind::Local(LocalId(0)),
                                ty: field,
                                span: SPAN,
                            }],
                        },
                    }),
                },
            },
        });
        assert_eq!(module.verify().is_ok(), accepted, "{module:?}");
    }
}
