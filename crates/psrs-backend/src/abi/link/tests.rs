use super::*;
use psrs_core::ConstructorInfo;
use psrs_hir::{ModuleId, TypeField};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module() -> CoreModule {
    CoreModule {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
            CoreType::Constructor(psrs_core::TypeConstructor::Number),
            CoreType::RowEmpty,
            CoreType::RowExtend {
                label: "secondValue".into(),
                ty: CoreTypeId(1),
                tail: CoreTypeId(2),
            },
            CoreType::RowExtend {
                label: "first".into(),
                ty: CoreTypeId(0),
                tail: CoreTypeId(3),
            },
            CoreType::Constructor(psrs_core::TypeConstructor::Record),
            CoreType::Application(CoreTypeId(5), CoreTypeId(4)),
            CoreType::Constructor(psrs_core::TypeConstructor::Unit),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    }
}

fn field(label: &str, kind: BuiltinType) -> TypeField {
    TypeField {
        label: label.into(),
        label_span: span(),
        ty: HirType {
            kind: HirTypeKind::Constructor(kind),
            span: span(),
        },
        span: span(),
    }
}

fn record_function() -> HirType {
    HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(HirType {
                kind: HirTypeKind::Record {
                    fields: vec![
                        field("secondValue", BuiltinType::Number),
                        field("first", BuiltinType::Int),
                    ],
                    tail: None,
                },
                span: span(),
            }),
            result: Box::new(HirType {
                kind: HirTypeKind::Constructor(BuiltinType::Unit),
                span: span(),
            }),
        },
        span: span(),
    }
}

#[test]
fn reuses_a_structurally_equal_record_and_appends_the_function_type() {
    let mut module = module();
    let id = intern_source_type(&mut module, &record_function()).expect("supported signature");
    // The arrow is interned as the application spine
    // `Application(Application(Constructor(Function), record), unit)`.
    assert_eq!(id, CoreTypeId(10));
    assert_eq!(
        module.types[8],
        CoreType::Constructor(TypeConstructor::Function)
    );
    assert_eq!(
        module.types[9],
        CoreType::Application(CoreTypeId(8), CoreTypeId(6))
    );
    assert_eq!(
        module.types[10],
        CoreType::Application(CoreTypeId(9), CoreTypeId(7))
    );
}

#[test]
fn interning_an_equal_type_returns_the_same_id() {
    let mut module = module();
    let first = intern_source_type(&mut module, &record_function()).expect("supported signature");
    let length = module.types.len();
    let again = intern_source_type(&mut module, &record_function()).expect("supported signature");
    assert_eq!(first, again);
    assert_eq!(module.types.len(), length);
}

#[test]
fn rejects_an_open_record_signature() {
    let mut module = module();
    let open = HirType {
        kind: HirTypeKind::Record {
            fields: vec![field("first", BuiltinType::Int)],
            tail: Some(Box::new(HirType {
                kind: HirTypeKind::Variable("r".into()),
                span: span(),
            })),
        },
        span: span(),
    };
    assert!(intern_source_type(&mut module, &open).is_none());
}

#[test]
fn interns_an_array_of_a_nullary_enum() {
    use psrs_hir::{SymbolId, TypeId as HirTypeId};
    let mut module = module();
    let type_id = HirTypeId::new(ModuleId(0), 0);
    module.constructors = ["Red", "Green"]
        .into_iter()
        .enumerate()
        .map(|(tag, name)| ConstructorInfo {
            symbol: SymbolId::new(ModuleId(0), tag as u32),
            name: name.into(),
            type_id,
            tag: tag as u32,
            field_count: 0,
            field_types: Vec::new(),
            parameters: Vec::new(),
        })
        .collect();
    let array = HirType {
        kind: HirTypeKind::Application(
            Box::new(HirType {
                kind: HirTypeKind::Constructor(BuiltinType::Array),
                span: span(),
            }),
            Box::new(HirType {
                kind: HirTypeKind::Named(type_id),
                span: span(),
            }),
        ),
        span: span(),
    };
    assert!(
        intern_source_type(&mut module, &array).is_some(),
        "list<enum> should intern"
    );
}

/// `Array (Resource Pollable)` must intern: a resource handle in a non-byte
/// list element is an applied newtype, not a bare scalar. The element's
/// conformance against the canonical list element is validated later.
#[test]
fn interns_an_array_of_a_resource_newtype() {
    use psrs_hir::TypeId as HirTypeId;
    let mut module = module();
    let resource = HirTypeId::new(ModuleId(0), 0);
    let pollable = HirTypeId::new(ModuleId(0), 1);
    module.newtype_ids.push(resource);
    module.opaque_ids.push(pollable);
    module
        .type_names
        .push((resource, "WASI.Resource.Resource".into()));
    let array = HirType {
        kind: HirTypeKind::Application(
            Box::new(HirType {
                kind: HirTypeKind::Constructor(BuiltinType::Array),
                span: span(),
            }),
            Box::new(HirType {
                kind: HirTypeKind::Application(
                    Box::new(HirType {
                        kind: HirTypeKind::Named(resource),
                        span: span(),
                    }),
                    Box::new(HirType {
                        kind: HirTypeKind::Opaque(pollable),
                        span: span(),
                    }),
                ),
                span: span(),
            }),
        ),
        span: span(),
    };
    assert!(
        intern_source_type(&mut module, &array).is_some(),
        "list<Resource a> should intern"
    );
}
