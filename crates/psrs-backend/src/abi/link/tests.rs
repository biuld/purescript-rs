use super::*;
use psrs_hir::{ModuleId, TypeField};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module() -> CoreModule {
    CoreModule {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            CoreType::I32,
            CoreType::F64,
            CoreType::Record(vec![
                ("first".into(), CoreTypeId(0)),
                ("secondValue".into(), CoreTypeId(1)),
            ]),
            CoreType::Unit,
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
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
    assert_eq!(id, CoreTypeId(4));
    assert_eq!(
        module.types[4],
        CoreType::Function {
            parameter: CoreTypeId(2),
            result: CoreTypeId(3),
        }
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
