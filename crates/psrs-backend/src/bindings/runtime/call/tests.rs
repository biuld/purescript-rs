use super::*;

#[test]
fn canonical_runtime_call_rejects_malformed_cc_signatures() {
    let binding = RuntimeBinding {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(7), 1),
        source_module: psrs_hir::ModuleId(7),
        module: psrs_runtime::STORAGE_MODULE.into(),
        function: "array_read".into(),
        type_id: None,
        span: psrs_span::TextRange::new(0, 1),
    };
    let mut table = cc::RepresentationTable::default();
    let array = table.reserve();
    table.set(
        array,
        cc::Representation::Array {
            element: cc::payload::erased_shape(),
        },
    );
    let mut module = cc::Module {
        name: "Probe".into(),
        externals: vec![cc::External {
            symbol: binding.symbol,
            signature: Some(cc::Signature {
                parameters: vec![
                    cc::ValueShape::Reference(cc::Reference {
                        nullable: false,
                        heap: cc::RefShape::Repr(array),
                    }),
                    cc::ValueShape::Integer,
                ],
                result: cc::payload::erased_shape(),
            }),
            projection: None,
        }],
        representations: table,
        functions: Vec::new(),
        entry: None,
        span: binding.span,
    };
    checked(&binding, &module).unwrap();
    for change in 0..5 {
        let mut wrong = module.clone();
        match change {
            0 => wrong.externals[0].signature = None,
            1 => {
                wrong.externals[0]
                    .signature
                    .as_mut()
                    .unwrap()
                    .parameters
                    .pop();
            }
            2 => wrong.externals[0].signature.as_mut().unwrap().result = cc::ValueShape::Integer,
            3 => {
                wrong.externals[0].signature.as_mut().unwrap().parameters[0] =
                    cc::payload::erased_shape()
            }
            _ => wrong.representations.set(
                array,
                cc::Representation::Array {
                    element: cc::ValueShape::Integer,
                },
            ),
        }
        let errors = checked(&binding, &wrong).expect_err("malformed runtime CC signature");
        assert!(
            errors
                .iter()
                .all(|error| error.module == Some(binding.source_module))
        );
    }
    module.externals.clear();
    assert!(checked(&binding, &module).is_err());
}

#[test]
fn canonical_record_call_packs_closed_record_arguments_through_shared_storage() {
    let binding = RuntimeBinding {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(7), 2),
        source_module: psrs_hir::ModuleId(7),
        module: psrs_runtime::RECORD_STORAGE_MODULE.into(),
        function: "length".into(),
        type_id: None,
        span: psrs_span::TextRange::new(0, 1),
    };
    let mut table = cc::RepresentationTable::default();
    let storage = table.ensure_record_storage();
    let closed_record = table.reserve();
    table.set(
        closed_record,
        cc::Representation::Product {
            fields: vec![cc::ValueShape::Integer],
        },
    );
    let source = cc::Signature {
        parameters: vec![cc::ValueShape::Reference(cc::Reference {
            nullable: false,
            heap: cc::RefShape::Repr(closed_record),
        })],
        result: cc::ValueShape::Integer,
    };
    let canonical = canonical_signature(&binding, &source, &table)
        .expect("a checked closed record is adapted through the shared owner");
    assert_eq!(
        canonical.parameters,
        vec![cc::ValueShape::Reference(cc::Reference {
            nullable: false,
            heap: cc::RefShape::Repr(storage.representation()),
        })]
    );
    assert_eq!(canonical.result, cc::ValueShape::Integer);
}
