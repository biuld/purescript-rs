use super::*;
use crate::cc::{Function, Module as CcModule, Reference, Signature, ValueDecl, VariantCase};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

#[test]
fn planner_owns_the_gc_closure_and_capture_layouts() {
    let mut table = RepresentationTable::default();
    table.add_signature(Signature {
        parameters: vec![CcValueShape::Integer],
        result: CcValueShape::Integer,
    });

    let layout = PlannedLayout::plan(&table, TargetCapabilities::default())
        .expect("planning a closure signature");
    let definitions = &layout.types[0].0;

    assert!(matches!(definitions[0].composite, CompositeType::Array(_)));
    assert!(matches!(definitions[1].composite, CompositeType::Struct(_)));
    assert!(matches!(
        definitions[2].composite,
        CompositeType::Func { .. }
    ));
    assert_eq!(
        layout.closure_layout().unwrap(),
        (DefinedTypeId(1), DefinedTypeId(0))
    );
}
#[test]
fn signatures_that_lower_to_the_same_wasm_type_share_one_definition() {
    let mut table = RepresentationTable::default();
    table.add_signature(Signature {
        parameters: vec![CcValueShape::Boolean],
        result: CcValueShape::Boolean,
    });
    table.add_signature(Signature {
        parameters: vec![CcValueShape::Integer],
        result: CcValueShape::Integer,
    });

    let layout = PlannedLayout::plan(&table, TargetCapabilities::default())
        .expect("planning Boolean and Integer signatures");
    assert_eq!(
        layout.signature_index(SignatureId(0)).unwrap(),
        layout.signature_index(SignatureId(1)).unwrap(),
        "Boolean and Integer both lower to i32 and must share a function type"
    );
    let function_types = layout.types[0]
        .0
        .iter()
        .filter(|definition| matches!(definition.composite, CompositeType::Func { .. }))
        .count();
    assert_eq!(
        function_types, 1,
        "the shared signature emits one function type"
    );
}

#[test]
fn planner_rejects_a_dangling_closure_signature() {
    let table = RepresentationTable {
        representations: vec![Representation::Product {
            fields: vec![CcValueShape::Reference(Reference {
                nullable: false,
                heap: CcRefShape::Closure(SignatureId(0)),
            })],
        }],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };
    assert!(matches!(
        PlannedLayout::plan(&table, TargetCapabilities::default()),
        Err(LayoutError::UnknownSignature)
    ));
}
#[test]
fn gc_planner_rejects_an_mvp_only_target() {
    let table = RepresentationTable {
        representations: vec![Representation::Product { fields: Vec::new() }],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };

    let mvp_only = TargetCapabilities {
        gc: false,
        ..TargetCapabilities::default()
    };
    assert!(matches!(
        PlannedLayout::plan(&table, mvp_only),
        Err(LayoutError::UnsupportedGcTarget)
    ));
}
#[test]
fn module_planner_omits_unreachable_requirements() {
    let mut table = RepresentationTable::default();
    let reachable = table.reserve();
    table.set(reachable, Representation::Product { fields: Vec::new() });
    let unreachable = table.reserve();
    table.set(unreachable, Representation::Product { fields: Vec::new() });
    let value = crate::cc::ValueId(0);
    let value_shape = CcValueShape::Reference(Reference {
        nullable: false,
        heap: CcRefShape::Repr(reachable),
    });
    let module = CcModule {
        name: "reachable-layout".into(),
        externals: Vec::new(),
        representations: table,
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: vec![value],
            values: vec![ValueDecl {
                id: value,
                ty: value_shape,
            }],
            assignments: Vec::new(),
            result: value,
            result_type: value_shape,
            span: TextRange::new(0, 1),
        }],
        entry: None,
        span: TextRange::new(0, 1),
    };

    let layout = PlannedLayout::plan_module(&module, TargetCapabilities::default())
        .expect("planning reachable requirements");
    assert_eq!(layout.types[0].0.len(), 1);
    assert_eq!(layout.repr_index(reachable).unwrap(), DefinedTypeId(0));
    assert!(matches!(
        layout.repr_index(unreachable),
        Err(LayoutError::UnknownRepresentation)
    ));
}

#[test]
fn maps_every_cc_value_shape_to_its_specified_mir_type() {
    let mut table = RepresentationTable::default();
    let product = table.reserve();
    table.set(
        product,
        Representation::Product {
            fields: vec![
                CcValueShape::Integer,
                CcValueShape::Boolean,
                CcValueShape::Number,
            ],
        },
    );
    let signature = table.add_signature(Signature {
        parameters: vec![CcValueShape::Integer],
        result: CcValueShape::Boolean,
    });

    let layout = PlannedLayout::plan(&table, TargetCapabilities::default())
        .expect("planning every scalar and reference shape");
    let definitions = &layout.types[0].0;
    let product_index = layout.repr_index(product).unwrap();
    let closure_index = layout.closure_layout().unwrap().0;

    let scalar_cases = [
        (CcValueShape::Integer, ValueType::I32),
        (CcValueShape::Boolean, ValueType::Boolean),
        (CcValueShape::Number, ValueType::F64),
        (CcValueShape::String, ValueType::I32),
    ];
    for (shape, expected) in scalar_cases {
        assert_eq!(layout.value_type(&shape).unwrap(), expected, "{shape:?}");
    }

    let reference_cases = [
        (
            Reference {
                nullable: false,
                heap: CcRefShape::Repr(product),
            },
            RefType {
                nullable: false,
                heap: HeapType::Index(product_index),
            },
        ),
        (
            Reference {
                nullable: true,
                heap: CcRefShape::Aggregate,
            },
            RefType {
                nullable: true,
                heap: HeapType::Struct,
            },
        ),
        (
            Reference {
                nullable: false,
                heap: CcRefShape::Closure(signature),
            },
            RefType {
                nullable: false,
                heap: HeapType::Index(closure_index),
            },
        ),
        (
            Reference {
                nullable: true,
                heap: CcRefShape::Erased,
            },
            RefType {
                nullable: true,
                heap: HeapType::Eq,
            },
        ),
    ];
    for (reference, expected) in reference_cases {
        assert_eq!(
            layout
                .value_type(&CcValueShape::Reference(reference))
                .unwrap(),
            ValueType::Ref(expected),
            "{reference:?}"
        );
    }

    let CompositeType::Struct(fields) = &definitions[product_index.0 as usize].composite else {
        panic!("a product must plan as a struct");
    };
    assert_eq!(
        fields.iter().map(|field| field.storage).collect::<Vec<_>>(),
        vec![StorageType::I32, StorageType::I32, StorageType::F64]
    );
    assert!(
        fields.iter().all(|field| !field.mutable),
        "product fields are immutable"
    );
}

#[test]
fn plans_mutually_recursive_requirements_in_one_recursion_group() {
    let mut table = RepresentationTable::default();
    let variant = table.reserve();
    let product = table.reserve();
    let array = table.reserve();
    table.set(
        product,
        Representation::Product {
            fields: vec![CcValueShape::Reference(Reference {
                nullable: false,
                heap: CcRefShape::Repr(variant),
            })],
        },
    );
    table.set(
        variant,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![CcValueShape::Reference(Reference {
                    nullable: false,
                    heap: CcRefShape::Repr(product),
                })],
            }],
        },
    );
    table.set(
        array,
        Representation::Array {
            element: CcValueShape::Reference(Reference {
                nullable: false,
                heap: CcRefShape::Repr(variant),
            }),
        },
    );

    let layout =
        PlannedLayout::plan(&table, TargetCapabilities::default()).expect("planning recursion");
    assert_eq!(
        layout.types.len(),
        1,
        "all definitions share one recursion group"
    );
    let definitions = &layout.types[0].0;
    assert_eq!(
        definitions.len(),
        4,
        "three reserved types plus one case subtype"
    );

    let product_index = layout.repr_index(product).unwrap();
    let variant_index = layout.repr_index(variant).unwrap();
    let array_index = layout.repr_index(array).unwrap();
    assert_eq!(
        [product_index, variant_index, array_index]
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3,
        "each ReprId maps to a distinct DefinedTypeId"
    );

    assert!(!definitions[variant_index.0 as usize].final_type);
    let case_index = layout.variant_index(variant, 0).unwrap();
    assert!(
        case_index.0 > variant_index.0,
        "a case subtype follows its supertype"
    );
    assert_eq!(
        definitions[case_index.0 as usize].supertype,
        Some(variant_index)
    );

    for definition in definitions {
        for field in match &definition.composite {
            CompositeType::Struct(fields) => fields.clone(),
            CompositeType::Array(field) => vec![*field],
            CompositeType::Func { parameters, .. } => parameters
                .iter()
                .filter_map(|parameter| match parameter {
                    ValueType::Ref(reference) => Some(FieldType {
                        storage: StorageType::Ref(*reference),
                        mutable: false,
                    }),
                    _ => None,
                })
                .collect(),
        } {
            if let StorageType::Ref(RefType {
                heap: HeapType::Index(index),
                ..
            }) = field.storage
            {
                assert!(
                    index.0 < definitions.len() as u32,
                    "forward reference {index:?} resolves within the group"
                );
            }
        }
    }
}

#[test]
fn plans_field_bearing_variants_as_tag_carrying_supertypes() {
    let mut table = RepresentationTable::default();
    let variant = table.reserve();
    table.set(
        variant,
        Representation::Variant {
            cases: vec![
                VariantCase {
                    tag: 0,
                    fields: vec![CcValueShape::Integer],
                },
                VariantCase {
                    tag: 1,
                    fields: vec![CcValueShape::Number, CcValueShape::Boolean],
                },
            ],
        },
    );
    let layout =
        PlannedLayout::plan(&table, TargetCapabilities::default()).expect("planning a mixed sum");
    let definitions = &layout.types[0].0;
    let supertype = layout.repr_index(variant).unwrap();
    assert!(
        !definitions[supertype.0 as usize].final_type,
        "the tag supertype must stay open"
    );
    assert_eq!(
        definitions[supertype.0 as usize].composite,
        CompositeType::Struct(vec![FieldType {
            storage: StorageType::I32,
            mutable: false,
        }])
    );

    let case0 = layout.variant_index(variant, 0).unwrap();
    let case1 = layout.variant_index(variant, 1).unwrap();
    assert_ne!(case0, case1);
    for (case, expected) in [
        (case0, vec![StorageType::I32, StorageType::I32]),
        (
            case1,
            vec![StorageType::I32, StorageType::F64, StorageType::I32],
        ),
    ] {
        let definition = &definitions[case.0 as usize];
        assert!(definition.final_type, "each case subtype is final");
        assert_eq!(definition.supertype, Some(supertype));
        let CompositeType::Struct(fields) = &definition.composite else {
            panic!("a variant case must plan as a struct");
        };
        assert_eq!(
            fields.iter().map(|field| field.storage).collect::<Vec<_>>(),
            expected,
            "field 0 is the tag and case fields follow"
        );
    }
}

#[test]
fn plans_the_uniform_closure_and_capture_array_layout() {
    let mut table = RepresentationTable::default();
    table.add_signature(Signature {
        parameters: vec![CcValueShape::Integer],
        result: CcValueShape::Number,
    });
    let layout =
        PlannedLayout::plan(&table, TargetCapabilities::default()).expect("planning a closure");
    let definitions = &layout.types[0].0;
    let (closure, capture) = layout.closure_layout().unwrap();

    let CompositeType::Array(element) = &definitions[capture.0 as usize].composite else {
        panic!("the capture environment must be an array");
    };
    assert!(element.mutable, "the capture array is mutable");
    assert_eq!(
        element.storage,
        StorageType::Ref(RefType {
            nullable: true,
            heap: HeapType::Eq,
        })
    );

    let CompositeType::Struct(fields) = &definitions[closure.0 as usize].composite else {
        panic!("the closure must be a struct");
    };
    assert_eq!(
        fields[0].storage,
        StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Func,
        })
    );
    assert_eq!(
        fields[1].storage,
        StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(capture),
        })
    );
    assert!(fields.iter().all(|field| !field.mutable));
}
