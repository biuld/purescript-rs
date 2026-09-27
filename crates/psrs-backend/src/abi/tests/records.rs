use super::canonical::{CanonicalField, CanonicalType, flatten as canonical_flatten};
use super::test_support::import;
use super::*;
use psrs_core::{Type as CoreType, TypeId as CoreTypeId};
use psrs_hir::{BuiltinType, ModuleId, Type as HirType, TypeField, TypeKind as HirTypeKind};
use psrs_span::TextRange;
use wit_parser::abi::WasmType;

fn field(name: &str, ty: CanonicalType) -> CanonicalField {
    CanonicalField {
        name: name.into(),
        ty,
    }
}

#[test]
fn maps_closed_source_records_to_direct_wit_record_parameters() {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "record.wit",
            "package test:records@0.1.0; interface records { record sample { second-value: f64, first: s32 } take: func(value: sample); }",
        )
        .expect("the WIT record fixture should resolve");
    let interface = resolve.packages[package].interfaces["records"];
    let function = &resolve.interfaces[interface].functions["take"];
    let resolved =
        super::canonical::resolve(&resolve, &function.params[0].ty).expect("record resolves");
    let CanonicalType::Record(fields) = &resolved else {
        panic!("scalar WIT record fields should have a direct record shape");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["second-value", "first"]
    );
    let canonical = resolve.wasm_signature(AbiVariant::GuestImport, function);
    assert_eq!(canonical.params, vec![WasmType::F64, WasmType::I32]);
    assert!(!canonical.indirect_params);

    let span = TextRange::new(0, 1);
    let source_field = |label: &str, ty| TypeField {
        label: label.into(),
        label_span: span,
        ty: HirType {
            kind: HirTypeKind::Constructor(ty),
            span,
        },
        span,
    };
    let record = HirType {
        kind: HirTypeKind::Record {
            fields: vec![
                source_field("secondValue", BuiltinType::Number),
                source_field("first", BuiltinType::Int),
            ],
            tail: None,
        },
        span,
    };
    let unit = HirType {
        kind: HirTypeKind::Constructor(BuiltinType::Unit),
        span,
    };
    let mut core = empty_core_module();
    core.types = vec![
        CoreType::I32,
        CoreType::F64,
        CoreType::Record(vec![
            ("first".into(), CoreTypeId(0)),
            ("secondValue".into(), CoreTypeId(1)),
        ]),
        CoreType::Unit,
    ];
    let function = HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(record),
            result: Box::new(unit),
        },
        span,
    };
    let type_id = crate::abi::intern_source_type(&mut core, &function)
        .expect("the record function type should intern");
    let import = import(
        psrs_hir::SymbolId::new(ModuleId(0), 0),
        "test:records",
        "take",
        vec![resolved],
        None,
    );
    crate::abi::link::validate_import_signature(&import, &core, type_id)
        .expect("source fields should match WIT names and types");

    let (bad_module, bad_record) =
        record_module(&[("first", CoreType::I32), ("secondValue", CoreType::Boolean)]);
    let mut bad_module = bad_module;
    let bad_unit = unit_type(&mut bad_module);
    assert!(
        validate_against(&import, bad_module, &[bad_record], bad_unit).is_err(),
        "a record field of the wrong type must be rejected"
    );

    let record_types = std::collections::HashMap::from([(CoreTypeId(2), crate::cc::ReprId(0))]);
    let abstract_signature = crate::cc::abstract_signature(
        Some(type_id),
        &core,
        &record_types,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    )
    .expect("the record representation should be selected from Core layout metadata");
    assert_eq!(
        abstract_signature.parameters,
        vec![crate::cc::ValueShape::Reference(crate::cc::Reference {
            nullable: false,
            heap: crate::cc::RefShape::Repr(crate::cc::ReprId(0)),
        })]
    );
}

#[test]
fn accepts_records_with_nested_byte_list_fields() {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "record-lists.wit",
            "package test:record-lists@0.1.0; interface messages { record metadata { note: string, revision: s32 } record message { metadata: metadata, body: list<u8>, code: s32 } take: func(value: message); }",
        )
        .expect("the nested byte-list WIT fixture should resolve");
    let interface = resolve.packages[package].interfaces["messages"];
    let function = &resolve.interfaces[interface].functions["take"];
    let ty = super::canonical::resolve(&resolve, &function.params[0].ty).expect("message resolves");
    let CanonicalType::Record(fields) = &ty else {
        panic!("records containing byte lists should remain directly flattenable");
    };
    assert_eq!(fields.len(), 3);
    assert!(matches!(fields[0].ty, CanonicalType::Record(_)));
    assert!(fields[1].ty.is_byte_list());

    let canonical = resolve.wasm_signature(AbiVariant::GuestImport, function);
    assert!(!canonical.indirect_params);
    assert_eq!(
        canonical.params,
        vec![
            WasmType::Pointer,
            WasmType::Length,
            WasmType::I32,
            WasmType::Pointer,
            WasmType::Length,
            WasmType::I32,
        ]
    );
    assert_eq!(
        canonical_flatten(&ty).len() + usize::from(canonical.retptr),
        canonical.params.len()
    );
    assert!(super::unsupported(&resolve, function).is_none());
}

#[test]
fn rejects_non_byte_lists_nested_in_records() {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "record-non-byte-list.wit",
            "package test:record-non-byte-list@0.1.0; interface messages { record message { values: list<s32> } take: func(value: message); }",
        )
        .expect("the non-byte-list WIT fixture should resolve");
    let interface = resolve.packages[package].interfaces["messages"];
    let function = &resolve.interfaces[interface].functions["take"];
    let ty = super::canonical::resolve(&resolve, &function.params[0].ty).expect("message resolves");
    assert!(matches!(ty, CanonicalType::Record(_)));
    assert_eq!(
        super::unsupported(&resolve, function).as_deref(),
        Some("non-byte WIT lists are not supported by the String ABI")
    );
}

#[test]
fn validates_a_list_of_records() {
    use crate::types::ValueType;
    use psrs_core::TypeConstructor;
    use psrs_hir::SymbolId;

    let mut module = empty_core_module();
    let x = intern_all(&mut module, vec![CoreType::I32])
        .pop()
        .expect("one integer");
    let y = intern_all(&mut module, vec![CoreType::F64])
        .pop()
        .expect("one number");
    let record = intern_all(
        &mut module,
        vec![CoreType::Record(vec![("x".into(), x), ("y".into(), y)])],
    )
    .pop()
    .expect("one record");
    let array_ctor = intern_all(
        &mut module,
        vec![CoreType::Constructor(TypeConstructor::Array)],
    )
    .pop()
    .expect("one array constructor");
    let array = intern_all(&mut module, vec![CoreType::Application(array_ctor, record)])
        .pop()
        .expect("one array");
    let unit = unit_type(&mut module);
    let import = import(
        SymbolId::new(ModuleId(0), 0),
        "test:records",
        "take",
        vec![CanonicalType::List(Box::new(CanonicalType::Record(vec![
            field(
                "x",
                CanonicalType::Int {
                    width: 32,
                    signed: true,
                },
            ),
            field("y", CanonicalType::Float { width: 64 }),
        ])))],
        None,
    );
    let _ = ValueType::I32;
    validate_against(&import, module, &[array], unit)
        .expect("list<record> should validate against the source record");
}
