use super::canonical::{CanonicalType, resolve as canonical_resolve};
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
        type_names: Vec::new(),
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
fn interns_an_array_of_supported_elements_and_nested_arrays() {
    let mut core = empty_core();
    assert!(
        crate::abi::intern_source_type(&mut core, &array(BuiltinType::String)).is_some(),
        "Array String should intern"
    );
    assert!(
        crate::abi::intern_source_type(&mut core, &array(BuiltinType::Int)).is_some(),
        "Array Int should intern"
    );
    assert!(
        crate::abi::intern_source_type(&mut core, &array_of_array()).is_some(),
        "Array (Array Int) is a recursive list element"
    );
    assert!(
        crate::abi::intern_source_type(&mut core, &array(BuiltinType::Unit)).is_none(),
        "Array Unit has no canonical list element"
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

fn param(resolve: &Resolve, function: &wit_parser::Function) -> CanonicalType {
    canonical_resolve(resolve, &function.params[0].ty).expect("parameter should resolve")
}

fn int(width: u8, signed: bool) -> CanonicalType {
    CanonicalType::Int { width, signed }
}

#[test]
fn classifies_scalar_and_string_lists_and_rejects_aggregates() {
    let resolve = resolve_wit(
        "package test:lists@0.1.0; interface lists { \
         record item { n: s32 } \
         enum color { red, green } \
         take-ints: func(values: list<s32>); \
         take-strings: func(values: list<string>); \
         take-bytes: func(values: list<u8>); \
         take-enums: func(values: list<color>); \
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
        param(&resolve, ints),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::Int { .. })
    ));
    assert!(super::unsupported(&resolve, ints).is_none());

    let strings = function_named(&resolve, "take-strings");
    assert!(matches!(
        param(&resolve, strings),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::String)
    ));

    let bytes = function_named(&resolve, "take-bytes");
    assert_eq!(
        param(&resolve, bytes),
        CanonicalType::List(Box::new(int(8, false)))
    );

    let enums = function_named(&resolve, "take-enums");
    assert!(matches!(
        param(&resolve, enums),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::Enum(_))
    ));
    assert!(super::unsupported(&resolve, enums).is_none());

    let items = function_named(&resolve, "take-items");
    assert!(matches!(
        param(&resolve, items),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::Record(_))
    ));
    assert!(super::unsupported(&resolve, items).is_none());

    let handles = function_named(&resolve, "take-handles");
    assert!(matches!(
        param(&resolve, handles),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::Handle { .. })
    ));
    assert!(super::unsupported(&resolve, handles).is_none());

    let options = function_named(&resolve, "take-options");
    assert!(matches!(
        param(&resolve, options),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::Option(_))
    ));
    assert!(
        super::unsupported(&resolve, options).is_none(),
        "a list of options is admitted as a recursive element"
    );

    let nested = function_named(&resolve, "take-nested");
    assert!(matches!(
        param(&resolve, nested),
        CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::List(_))
    ));
    assert!(
        super::unsupported(&resolve, nested).is_none(),
        "a nested list is admitted as a recursive element"
    );

    let returned = function_named(&resolve, "strings");
    let returned = canonical_resolve(&resolve, returned.result.as_ref().unwrap())
        .expect("result should resolve");
    assert!(
        matches!(returned, CanonicalType::List(element) if matches!(element.as_ref(), CanonicalType::String))
    );
}

#[test]
fn admits_a_non_byte_fixed_length_list() {
    let resolve = resolve_wit(
        "package test:lists@0.1.0; interface lists { take: func(values: list<s32, 3>); }",
    );
    let function = function_named(&resolve, "take");
    let CanonicalType::FixedList { element, length } = param(&resolve, function) else {
        panic!("list<s32, 3> should be a fixed-length list");
    };
    assert!(matches!(*element, CanonicalType::Int { .. }));
    assert_eq!(length, 3);
    assert!(
        super::unsupported(&resolve, function).is_none(),
        "a non-byte fixed-length list is admitted"
    );
}

#[test]
fn maps_a_list_of_tuples_to_a_record_list() {
    let resolve = resolve_wit(
        "package test:lists@0.1.0; interface lists { take: func(values: list<tuple<string, string>>); }",
    );
    let function = function_named(&resolve, "take");
    let CanonicalType::List(element) = param(&resolve, function) else {
        panic!("list<tuple<...>> should be a value list");
    };
    let CanonicalType::Record(fields) = element.as_ref() else {
        panic!("a tuple element should map to a record");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| (field.name.as_str(), &field.ty))
            .collect::<Vec<_>>(),
        vec![
            ("_1", &CanonicalType::String),
            ("_2", &CanonicalType::String),
        ]
    );
    assert!(super::unsupported(&resolve, function).is_none());
}
