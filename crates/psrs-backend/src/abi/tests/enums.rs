use super::canonical::{CanonicalType, flatten as canonical_flatten, resolve as canonical_resolve};
use super::test_support::import;
use super::*;
use psrs_core::Type as CoreType;

#[test]
fn flags_spanning_multiple_words_match_the_canonical_parameter_count() {
    fn alphabetic(mut index: usize) -> String {
        let mut suffix = String::new();
        loop {
            suffix.insert(0, char::from(b'a' + (index % 26) as u8));
            if index < 26 {
                return suffix;
            }
            index = index / 26 - 1;
        }
    }

    let names = (0..33)
        .map(|index| format!("flag-{}", alphabetic(index)))
        .collect::<Vec<_>>();
    let source = format!(
        "package test:feature-flags@0.1.0; interface access {{ flags many {{ {} }} take: func(value: many); }}",
        names.join(", ")
    );
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str("many-flags.wit", &source)
        .expect("the multiword flags WIT fixture should resolve");
    let interface = resolve.packages[package].interfaces["access"];
    let function = &resolve.interfaces[interface].functions["take"];
    let ty = canonical_resolve(&resolve, &function.params[0].ty).expect("flags should resolve");
    assert_eq!(ty, CanonicalType::Flags(names));
    assert_eq!(
        resolve
            .wasm_signature(wit_parser::abi::AbiVariant::GuestImport, function)
            .params,
        vec![
            wit_parser::abi::WasmType::I32,
            wit_parser::abi::WasmType::I32
        ]
    );
    assert_eq!(canonical_flatten(&ty).len(), 2);
}

#[test]
fn maps_nullary_source_constructors_to_matching_wit_enum_cases() {
    use psrs_core::ConstructorInfo;
    use psrs_hir::{ModuleId, Type as HirType, TypeId as HirTypeId, TypeKind as HirTypeKind};
    use psrs_span::TextRange;

    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "enum.wit",
            "package test:enums@0.1.0; interface colors { enum color { red, green-blue } convert: func(value: color) -> color; flags access { read, write } use-flags: func(value: access); }",
        )
        .expect("the WIT enum fixture should resolve");
    let interface = resolve.packages[package].interfaces["colors"];
    let function = &resolve.interfaces[interface].functions["convert"];
    let parameter_type =
        canonical_resolve(&resolve, &function.params[0].ty).expect("the parameter should resolve");
    let result_type = function
        .result
        .as_ref()
        .and_then(|ty| canonical_resolve(&resolve, ty))
        .expect("the fixture returns a color");
    let cases = vec!["red".to_string(), "green-blue".to_string()];
    assert_eq!(parameter_type, CanonicalType::Enum(cases.clone()));
    assert_eq!(result_type, CanonicalType::Enum(cases.clone()));
    let flags_function = &resolve.interfaces[interface].functions["use-flags"];
    let flags_type = canonical_resolve(&resolve, &flags_function.params[0].ty)
        .expect("the flags parameter should resolve");
    assert_eq!(
        flags_type,
        CanonicalType::Flags(vec!["read".into(), "write".into()])
    );
    let flags_import = import(
        psrs_hir::SymbolId::new(ModuleId(0), 1),
        "test:enums",
        "use-flags",
        vec![flags_type],
        None,
    );
    let (flags_module, flags_record) =
        record_module(&[("read", CoreType::Boolean), ("write", CoreType::Boolean)]);
    let mut flags_module = flags_module;
    let flags_unit = unit_type(&mut flags_module);
    validate_against(&flags_import, flags_module, &[flags_record], flags_unit)
        .expect("Boolean record fields should map to named WIT flags");
    let (bad_module, bad_record) =
        record_module(&[("read", CoreType::Boolean), ("write", CoreType::I32)]);
    let mut bad_module = bad_module;
    let bad_unit = unit_type(&mut bad_module);
    assert!(
        validate_against(&flags_import, bad_module, &[bad_record], bad_unit).is_err(),
        "a non-Boolean flags field must be rejected"
    );

    let span = TextRange::new(0, 1);
    let type_id = HirTypeId::new(ModuleId(0), 0);
    let mut core = empty_core_module();
    core.constructors = ["Red", "GreenBlue"]
        .into_iter()
        .enumerate()
        .map(|(tag, name)| ConstructorInfo {
            symbol: psrs_hir::SymbolId::new(ModuleId(0), tag as u32),
            name: name.into(),
            type_id,
            tag: tag as u32,
            field_count: 0,
            field_types: Vec::new(),
        })
        .collect();
    let enum_type = HirType {
        kind: HirTypeKind::Named(type_id),
        span,
    };
    let function = HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(enum_type.clone()),
            result: Box::new(enum_type),
        },
        span,
    };
    let function_id = crate::abi::intern_source_type(&mut core, &function)
        .expect("the enum function type should intern");
    let import = import(
        psrs_hir::SymbolId::new(ModuleId(0), 0),
        "test:enums",
        "convert",
        vec![parameter_type],
        Some(result_type),
    );
    crate::abi::link::validate_import_signature(&import, &core, function_id)
        .expect("matching source constructor tags should pass ABI validation");
    assert_eq!(
        crate::cc::abstract_signature(
            Some(function_id),
            &core,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .expect("a source enum should lower to an abstract scalar signature")
        .parameters,
        vec![crate::cc::ValueShape::Integer]
    );

    // The same enum with reversed constructor tags must not match the WIT
    // enum case order.
    let mut reversed_core = empty_core_module();
    reversed_core.constructors = ["Red", "GreenBlue"]
        .into_iter()
        .rev()
        .enumerate()
        .map(|(tag, name)| ConstructorInfo {
            symbol: psrs_hir::SymbolId::new(ModuleId(0), tag as u32),
            name: name.into(),
            type_id,
            tag: tag as u32,
            field_count: 0,
            field_types: Vec::new(),
        })
        .collect();
    let reversed_id = crate::abi::intern_source_type(&mut reversed_core, &function)
        .expect("the reversed enum function type should intern");
    assert!(
        crate::abi::link::validate_import_signature(&import, &reversed_core, reversed_id).is_err()
    );
}

#[test]
fn validates_a_list_of_nullary_enums() {
    use crate::types::ValueType;
    use psrs_core::{ConstructorInfo, TypeConstructor};
    use psrs_hir::{ModuleId, TypeId as HirTypeId};

    let type_id = HirTypeId::new(ModuleId(0), 0);
    let cases = ["Red", "GreenBlue"];
    let mut module = empty_core_module();
    module.constructors = cases
        .iter()
        .enumerate()
        .map(|(tag, name)| ConstructorInfo {
            symbol: psrs_hir::SymbolId::new(ModuleId(0), tag as u32),
            name: (*name).into(),
            type_id,
            tag: tag as u32,
            field_count: 0,
            field_types: Vec::new(),
        })
        .collect();
    let enum_id = intern_all(
        &mut module,
        vec![CoreType::Constructor(TypeConstructor::User(type_id))],
    )
    .pop()
    .expect("one enum type");
    let array_ctor = intern_all(
        &mut module,
        vec![CoreType::Constructor(TypeConstructor::Array)],
    )
    .pop()
    .expect("one array constructor");
    let array = intern_all(
        &mut module,
        vec![CoreType::Application(array_ctor, enum_id)],
    )
    .pop()
    .expect("one array type");
    let unit = unit_type(&mut module);
    let import = import(
        psrs_hir::SymbolId::new(ModuleId(0), 0),
        "test:enums",
        "take-list",
        vec![CanonicalType::List(Box::new(CanonicalType::Enum(vec![
            "red".into(),
            "green-blue".into(),
        ])))],
        None,
    );
    let _ = ValueType::I32;
    validate_against(&import, module, &[array], unit)
        .expect("list<enum> should validate against the source enum");
}
