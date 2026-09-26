use super::super::classification::{param_kind, result_kind, source_signature, unsupported_shape};
use super::super::{SourceType, WasiParamKind, WasiResultKind};
use psrs_core::Module as CoreModule;
use psrs_hir::{BuiltinType, ModuleId, Type as HirType, TypeKind as HirTypeKind};
use psrs_span::TextRange;
use wit_parser::Resolve;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn hir(kind: HirTypeKind) -> HirType {
    HirType { kind, span: span() }
}

fn empty_core() -> CoreModule {
    CoreModule {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    }
}

fn array(element: BuiltinType) -> HirType {
    hir(HirTypeKind::Application(
        Box::new(hir(HirTypeKind::Constructor(BuiltinType::Array))),
        Box::new(hir(HirTypeKind::Constructor(element))),
    ))
}

#[test]
fn maps_an_array_of_supported_elements_and_rejects_nested_arrays() {
    let core = empty_core();
    let strings = source_signature(&core, &array(BuiltinType::String))
        .expect("Array String should be a source array");
    assert_eq!(
        strings.result,
        SourceType::Array {
            element: Box::new(SourceType::String)
        }
    );
    let ints = source_signature(&core, &array(BuiltinType::Int)).expect("Array Int");
    assert_eq!(
        ints.result,
        SourceType::Array {
            element: Box::new(SourceType::Int)
        }
    );
    assert!(
        source_signature(&core, &array_of_array()).is_none(),
        "Array (Array Int) is not a canonical list element"
    );
}

fn array_of_array() -> HirType {
    hir(HirTypeKind::Application(
        Box::new(hir(HirTypeKind::Constructor(BuiltinType::Array))),
        Box::new(array(BuiltinType::Int)),
    ))
}

fn function_named<'a>(resolve: &'a Resolve, name: &str) -> &'a wit_parser::Function {
    let package = resolve.packages.iter().next().expect("package").0;
    let interface = resolve.packages[package]
        .interfaces
        .values()
        .next()
        .unwrap();
    resolve.interfaces[*interface].functions.get(name).unwrap()
}

fn resolve_wit(wit: &str) -> Resolve {
    let mut resolve = Resolve::default();
    resolve.push_str("lists.wit", wit).expect("wit");
    resolve
}

#[test]
fn classifies_scalar_and_string_lists_and_rejects_aggregates() {
    let resolve = resolve_wit(
        "package test:lists@0.1.0; interface lists { \
         record item { n: s32 } \
         take-ints: func(values: list<s32>); \
         take-strings: func(values: list<string>); \
         take-bytes: func(values: list<u8>); \
         take-nested: func(values: list<list<s32>>); \
         resource file; \
         take-items: func(values: list<item>); \
         take-handles: func(values: list<own<file>>); \
         take-options: func(values: list<option<s32>>); \
         ints: func() -> list<s32>; \
         strings: func() -> list<string>; }",
    );
    let ints = function_named(&resolve, "take-ints");
    assert!(matches!(
        param_kind(&resolve, &ints.params[0].ty),
        WasiParamKind::ValueList { element } if matches!(element.as_ref(), WasiParamKind::Integer32)
    ));
    assert!(unsupported_shape(&resolve, ints, &WasiResultKind::None).is_none());

    let strings = function_named(&resolve, "take-strings");
    assert!(matches!(
        param_kind(&resolve, &strings.params[0].ty),
        WasiParamKind::ValueList { element } if matches!(element.as_ref(), WasiParamKind::List)
    ));

    let bytes = function_named(&resolve, "take-bytes");
    assert_eq!(
        param_kind(&resolve, &bytes.params[0].ty),
        WasiParamKind::List
    );

    for name in ["take-nested", "take-items", "take-handles", "take-options"] {
        let function = function_named(&resolve, name);
        assert!(
            unsupported_shape(&resolve, function, &WasiResultKind::None).is_some(),
            "{name} should be rejected"
        );
    }

    let returned = function_named(&resolve, "strings");
    assert!(matches!(
        result_kind(&resolve, returned.result.as_ref().unwrap()),
        WasiResultKind::ValueList { .. }
    ));
    assert!(
        unsupported_shape(
            &resolve,
            returned,
            &result_kind(&resolve, returned.result.as_ref().unwrap())
        )
        .is_none()
    );
}
