use super::*;

fn verify(table: RepresentationTable) -> Result<(), Vec<crate::BackendError>> {
    super::super::verify_module(&super::super::Module {
        name: "RecordSchema".into(),
        externals: Vec::new(),
        representations: table,
        functions: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 1),
    })
}

#[test]
fn a_record_schema_keeps_its_identity_and_reuses_the_array_owner() {
    let mut table = RepresentationTable::default();
    let existing_values =
        super::super::layout::protocols::array(&mut table, super::super::payload::erased_shape());
    let first = table.ensure_record_storage();
    assert_eq!(first.values(), existing_values);
    let size = table.representations.len();
    assert_eq!(table.ensure_record_storage(), first);
    assert_eq!(table.representations.len(), size);
    verify(table).expect("registered schema passes the public CC verifier");
}

#[test]
fn malformed_record_schemas_fail_at_the_public_cc_boundary() {
    let mut baseline = RepresentationTable::default();
    let schema = baseline.ensure_record_storage();
    let mutations: &[fn(&mut RepresentationTable, RecordStorage)] = &[
        |table, schema| {
            table.set(
                schema.labels(),
                Representation::Array {
                    element: ValueShape::Integer,
                },
            )
        },
        |table, schema| {
            table.set(
                schema.values(),
                Representation::Array {
                    element: ValueShape::String,
                },
            )
        },
        |table, _| table.record.storage.as_mut().unwrap().labels = ReprId(u32::MAX),
        |table, schema| {
            table.set(
                schema.representation(),
                Representation::Product {
                    fields: vec![reference(schema.values()), reference(schema.labels())],
                },
            )
        },
        |table, schema| {
            table.set(
                schema.representation(),
                Representation::Product {
                    fields: vec![reference(schema.labels())],
                },
            )
        },
        |table, schema| {
            table.set(
                schema.representation(),
                Representation::Product {
                    fields: vec![
                        ValueShape::Reference(Reference {
                            nullable: true,
                            heap: RefShape::Repr(schema.labels()),
                        }),
                        reference(schema.values()),
                    ],
                },
            )
        },
        |table, schema| {
            table.set_product_labels(
                schema.representation(),
                vec!["labels".into(), "values".into()],
            )
        },
        |table, schema| {
            table.set(
                schema.representation(),
                Representation::Array {
                    element: reference(schema.labels()),
                },
            )
        },
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut table = baseline.clone();
        mutate(&mut table, schema);
        // Registration must not replace corrupted published metadata with an
        // apparently valid fresh schema and hide the malformed owner contract.
        let published = table.record_storage().unwrap();
        assert_eq!(table.ensure_record_storage(), published);
        let errors = verify(table).expect_err("malformed record schema must be rejected");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("record storage")),
            "mutation {index}: {errors:?}"
        );
    }
}
