use super::*;

#[test]
fn copies_before_updating_a_linear_array() {
    let mut representations = RepresentationTable::default();
    let array = representations.reserve();
    representations.set(
        array,
        Representation::Array {
            element: ValueShape::Integer,
        },
    );
    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        (ValueId(0), ValueShape::Integer),
        (ValueId(1), ValueShape::Integer),
        (ValueId(2), ValueShape::Integer),
        (
            ValueId(3),
            ValueShape::Reference(Reference {
                nullable: false,
                heap: crate::cc::RefShape::Repr(array),
            }),
        ),
        (
            ValueId(4),
            ValueShape::Reference(Reference {
                nullable: false,
                heap: crate::cc::RefShape::Repr(array),
            }),
        ),
        (ValueId(5), ValueShape::Integer),
    ];
    let module = CcModule {
        name: "LinearArrayClone".into(),
        externals: Vec::new(),
        representations,
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: values
                .into_iter()
                .map(|(id, ty)| crate::cc::ValueDecl { id, ty })
                .collect(),
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::Constant(4),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::Constant(9),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(3),
                        representation: array,
                        elements: vec![ValueId(0), ValueId(0)],
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(4),
                    kind: AssignmentKind::ArrayClone {
                        destination: ValueId(4),
                        representation: array,
                        value: ValueId(3),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(4),
                    kind: AssignmentKind::ArraySet {
                        destination: ValueId(4),
                        representation: array,
                        value: ValueId(4),
                        index: ValueId(2),
                        new_value: ValueId(1),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(5),
                    kind: AssignmentKind::ArrayGet {
                        destination: ValueId(5),
                        representation: array,
                        value: ValueId(3),
                        index: ValueId(2),
                    },
                    span: span(),
                },
            ],
            result: ValueId(5),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    run_linear(module, "4");
}
