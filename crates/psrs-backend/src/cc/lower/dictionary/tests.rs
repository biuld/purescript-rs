use super::super::super::{AssignmentKind, Representation, ValueShape};
use psrs_hir::{LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;
use psrs_thir as thir;

#[test]
fn typed_thir_instance_and_superclass_evidence_lower_to_ordinary_products() {
    let thir = dictionary_evidence_module();
    let core = psrs_core::lower_module(thir).expect("typed evidence should lower to Core");
    core.verify()
        .expect("dictionary Core should remain well typed");

    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .expect("main declaration");
    let psrs_core::ExprKind::Lambda { body, .. } = &main.value.kind else {
        panic!("expected the given dictionary parameter");
    };
    let psrs_core::ExprKind::Lambda { body, .. } = &body.kind else {
        panic!("expected the method argument");
    };
    let psrs_core::ExprKind::Application(method, _) = &body.kind else {
        panic!("expected the selected method call");
    };
    let psrs_core::ExprKind::FieldAccess { record, field } = &method.kind else {
        panic!("expected method selection to remain an ordinary Core projection");
    };
    assert_eq!(field, "isPositive");
    let psrs_core::ExprKind::Application(thunk, unit) = &record.kind else {
        panic!("expected superclass selection to force its thunk");
    };
    assert!(matches!(unit.kind, psrs_core::ExprKind::Unit));
    let psrs_core::ExprKind::FieldAccess { record, field } = &thunk.kind else {
        panic!("expected superclass selection to project its thunk");
    };
    assert_eq!(field, "super");
    assert!(matches!(
        record.kind,
        psrs_core::ExprKind::Application(_, _)
    ));

    let backend = crate::cc::lower_module(core).expect("dictionary Core should lower to CC");
    let cc = &backend.cc;
    let make_ord = cc
        .functions
        .iter()
        .find(|function| function.name == "makeOrd")
        .expect("instance constructor function");
    let (ord_repr, ord_arguments) = make_ord
        .assignments
        .iter()
        .find_map(|assignment| match &assignment.kind {
            AssignmentKind::ProductNew {
                representation,
                arguments,
                ..
            } if arguments.len() == 3 => Some((*representation, arguments)),
            _ => None,
        })
        .expect("instance dictionary must use ordinary ProductNew");
    let Some(Representation::Product { fields }) = cc.representations.representation(ord_repr)
    else {
        panic!("instance dictionary layout must be a product");
    };
    assert_eq!(fields.len(), 3);
    assert!(fields[..2].iter().all(|field| matches!(
        field,
        ValueShape::Reference(reference)
            if matches!(reference.heap, crate::cc::RefShape::Closure(_))
    )));
    for (argument, field) in ord_arguments.iter().zip(fields) {
        assert_eq!(value_shape(make_ord, *argument), Some(*field));
    }
    let ValueShape::Reference(crate::cc::Reference {
        heap: crate::cc::RefShape::Closure(thunk),
        ..
    }) = fields[2]
    else {
        panic!("superclass field must be a callable thunk");
    };
    let thunk = cc.representations.signature(thunk).unwrap();
    assert_eq!(thunk.parameters, vec![ValueShape::Integer]);
    assert_eq!(
        thunk.result,
        make_ord.values[make_ord.parameters[0].0 as usize].ty
    );

    let main = cc
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("lowered main function");
    assert!(main.assignments.iter().any(|assignment| {
        matches!(
            assignment.kind,
            AssignmentKind::ProductGet {
                representation,
                field: 2,
                ..
            } if representation == ord_repr
        )
    }));
    let eq_repr = main
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::ProductGet {
                representation,
                field: 0,
                ..
            } if representation != ord_repr => Some(representation),
            _ => None,
        })
        .expect("method projection must use the superclass dictionary product");
    assert!(
        main.assignments
            .iter()
            .any(|assignment| { matches!(assignment.kind, AssignmentKind::IndirectCall { .. }) })
    );

    let (mir, _) = crate::mir::lower_module(cc.clone())
        .expect("dictionary CC should lower through the normal MIR product path");
    let main = mir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("MIR main function");
    let instructions = main
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .collect::<Vec<_>>();
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        crate::mir::Instruction::StructGet { field: 2, .. }
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        crate::mir::Instruction::StructGet { field: 0, .. }
    )));
    assert!(mir.functions.iter().any(|function| {
        function.name == "makeOrd"
            && function.blocks.iter().any(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(
                        instruction,
                        crate::mir::Instruction::StructNew { arguments, .. }
                            if arguments.len() == 3
                    )
                })
            })
    }));
    assert!(cc.representations.representation(eq_repr).is_some());
}

fn value_shape(function: &crate::cc::Function, id: crate::cc::ValueId) -> Option<ValueShape> {
    function
        .values
        .iter()
        .find(|value| value.id == id)
        .map(|value| value.ty)
}

fn dictionary_evidence_module() -> thir::Module {
    let module_id = ModuleId(0);
    let entry = SymbolId::new(module_id, 0);
    let make_ord = SymbolId::new(module_id, 1);
    let is_positive = SymbolId::new(module_id, 2);
    let eq_class = psrs_hir::TypeId::new(module_id, 0);
    let ord_class = psrs_hir::TypeId::new(module_id, 1);
    let span = TextRange::new(0, 32);
    let integer = thir::TypeId(0);
    let boolean = thir::TypeId(1);
    let mut types = vec![
        thir::Type::Constructor(thir::TypeConstructor::Int),
        thir::Type::Constructor(thir::TypeConstructor::Boolean),
    ];
    let method = push_arrow(&mut types, integer, boolean);
    let eq_dictionary = push_record(&mut types, vec![("isPositive".into(), method)]);
    let unit = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Constructor(thir::TypeConstructor::Unit));
    let superclass_thunk = push_arrow(&mut types, unit, eq_dictionary);
    let ord_dictionary = push_record(
        &mut types,
        vec![
            ("compare".into(), method),
            ("rank".into(), method),
            ("super".into(), superclass_thunk),
        ],
    );
    let make_ord_type = push_arrow(&mut types, eq_dictionary, ord_dictionary);
    let main_body = push_arrow(&mut types, integer, boolean);
    let main_type = push_arrow(&mut types, eq_dictionary, main_body);

    let given = thir::Evidence {
        kind: thir::EvidenceKind::Given(LocalId(0)),
        class_id: eq_class,
        ty: eq_dictionary,
        span,
    };
    let instance = thir::Evidence {
        kind: thir::EvidenceKind::Instance {
            constructor: make_ord,
            constructor_type: make_ord_type,
            context: vec![given],
        },
        class_id: ord_class,
        ty: ord_dictionary,
        span,
    };
    let superclass = thir::Evidence {
        kind: thir::EvidenceKind::Superclass {
            parent: Box::new(instance),
            field: "super".into(),
        },
        class_id: eq_class,
        ty: eq_dictionary,
        span,
    };
    let selected_method = typed(
        thir::ExprKind::FieldAccess {
            expression: Box::new(typed(
                thir::ExprKind::Evidence(superclass),
                eq_dictionary,
                span,
            )),
            field: "isPositive".into(),
        },
        method,
        span,
    );
    let call = typed(
        thir::ExprKind::Application(
            Box::new(selected_method),
            Box::new(typed(thir::ExprKind::Local(LocalId(1)), integer, span)),
        ),
        boolean,
        span,
    );
    let main = typed(
        thir::ExprKind::Lambda {
            binder: binder(0, "given", eq_dictionary, span),
            body: Box::new(typed(
                thir::ExprKind::Lambda {
                    binder: binder(1, "value", integer, span),
                    body: Box::new(call),
                },
                main_body,
                span,
            )),
        },
        main_type,
        span,
    );
    let make_ord_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(2, "eq", eq_dictionary, span),
            body: Box::new(typed(
                thir::ExprKind::Record(vec![
                    (
                        "compare".into(),
                        typed(thir::ExprKind::Global(is_positive), method, span),
                    ),
                    (
                        "rank".into(),
                        typed(thir::ExprKind::Global(is_positive), method, span),
                    ),
                    (
                        "super".into(),
                        typed(
                            thir::ExprKind::Lambda {
                                binder: binder(4, "superclass_unit", unit, span),
                                body: Box::new(typed(
                                    thir::ExprKind::Local(LocalId(2)),
                                    eq_dictionary,
                                    span,
                                )),
                            },
                            superclass_thunk,
                            span,
                        ),
                    ),
                ]),
                ord_dictionary,
                span,
            )),
        },
        make_ord_type,
        span,
    );
    let method_value = typed(
        thir::ExprKind::Lambda {
            binder: binder(3, "value", integer, span),
            body: Box::new(typed(thir::ExprKind::Boolean(true), boolean, span)),
        },
        method,
        span,
    );

    thir::Module {
        type_names: Vec::new(),
        id: module_id,
        name: "DictionaryLayout".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(entry, "main", main_type, main, span),
            declaration(make_ord, "makeOrd", make_ord_type, make_ord_value, span),
            declaration(is_positive, "isPositive", method, method_value, span),
        ],
        span,
    }
}

fn typed(kind: thir::ExprKind, ty: thir::TypeId, span: TextRange) -> thir::Expr {
    thir::Expr { kind, ty, span }
}

fn push_arrow(
    types: &mut Vec<thir::Type>,
    parameter: thir::TypeId,
    result: thir::TypeId,
) -> thir::TypeId {
    let head = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Constructor(thir::TypeConstructor::Function));
    let inner = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Application(head, parameter));
    let outer = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Application(inner, result));
    outer
}

fn push_record(
    types: &mut Vec<thir::Type>,
    mut fields: Vec<(String, thir::TypeId)>,
) -> thir::TypeId {
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    let row_empty = thir::TypeId(types.len() as u32);
    types.push(thir::Type::RowEmpty);
    let mut tail = row_empty;
    for (label, ty) in fields.into_iter().rev() {
        let id = thir::TypeId(types.len() as u32);
        types.push(thir::Type::RowExtend { label, ty, tail });
        tail = id;
    }
    let head = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Constructor(thir::TypeConstructor::Record));
    let id = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Application(head, tail));
    id
}

fn binder(id: u32, name: &str, ty: thir::TypeId, span: TextRange) -> thir::Binder {
    thir::Binder {
        id: LocalId(id),
        name: name.into(),
        ty,
        span,
    }
}

fn declaration(
    symbol: SymbolId,
    name: &str,
    ty: thir::TypeId,
    value: thir::Expr,
    span: TextRange,
) -> thir::Declaration {
    thir::Declaration {
        symbol,
        name: name.into(),
        name_span: span,
        quantified: Vec::new(),
        ty,
        value,
        span,
    }
}
