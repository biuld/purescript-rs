use super::*;

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
    let kind = param_kind(&resolve, &function.params[0].ty);
    assert_eq!(kind, WasiParamKind::Flags { names });
    assert_eq!(
        resolve
            .wasm_signature(wit_parser::abi::AbiVariant::GuestImport, function)
            .params,
        vec![
            wit_parser::abi::WasmType::I32,
            wit_parser::abi::WasmType::I32
        ]
    );
    assert_eq!(flattened_parameter_count(&kind), 2);
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
    let parameter_kind = param_kind(&resolve, &function.params[0].ty);
    let result_kind = function
        .result
        .as_ref()
        .map(|ty| result_kind(&resolve, ty))
        .expect("the fixture returns a color");
    let cases = vec!["Red".to_string(), "GreenBlue".to_string()];
    assert_eq!(
        parameter_kind,
        WasiParamKind::Enum {
            cases: cases.clone()
        }
    );
    assert_eq!(
        result_kind,
        WasiResultKind::Enum {
            cases: cases.clone()
        }
    );
    let flags_function = &resolve.interfaces[interface].functions["use-flags"];
    let flags_kind = param_kind(&resolve, &flags_function.params[0].ty);
    assert_eq!(
        flags_kind,
        WasiParamKind::Flags {
            names: vec!["read".into(), "write".into()]
        }
    );
    let flags_source = SourceType::Record {
        fields: vec![
            ("read".into(), Box::new(SourceType::Boolean)),
            ("write".into(), Box::new(SourceType::Boolean)),
        ],
    };
    let flags_import = WasiImport {
        symbol: psrs_hir::SymbolId::new(ModuleId(0), 1),
        module: "test:enums".into(),
        name: "use-flags".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![flags_kind],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    let flags_signature = SourceSignature {
        parameters: vec![flags_source],
        result: SourceType::Unit,
        span: TextRange::new(0, 1),
    };
    WasiRegistry::load()
        .expect("vendored WASI should load")
        .validate_signature(&flags_import, &flags_signature)
        .expect("Boolean record fields should map to named WIT flags");
    let incompatible_flags = SourceSignature {
        parameters: vec![SourceType::Record {
            fields: vec![
                ("read".into(), Box::new(SourceType::Boolean)),
                ("write".into(), Box::new(SourceType::Int)),
            ],
        }],
        ..flags_signature
    };
    assert!(
        WasiRegistry::load()
            .expect("vendored WASI should load")
            .validate_signature(&flags_import, &incompatible_flags)
            .is_err()
    );

    let span = TextRange::new(0, 1);
    let type_id = HirTypeId::new(ModuleId(0), 0);
    let mut core = empty_core_module();
    core.constructors = cases
        .iter()
        .enumerate()
        .map(|(tag, name)| ConstructorInfo {
            symbol: psrs_hir::SymbolId::new(ModuleId(0), tag as u32),
            name: name.clone(),
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
    let source = source_signature(&core, &function)
        .expect("the nullary source enum should have an ABI representation");
    let import = WasiImport {
        symbol: psrs_hir::SymbolId::new(ModuleId(0), 0),
        module: "test:enums".into(),
        name: "convert".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![parameter_kind],
        result: Some(ValueType::I32),
        result_kind,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    WasiRegistry::load()
        .expect("vendored WASI should load")
        .validate_signature(&import, &source)
        .expect("matching source constructor tags should pass ABI validation");
    assert_eq!(
        crate::cc::abstract_signature(
            Some(function_id),
            &core,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .expect("a source enum should lower to an abstract scalar signature")
        .parameters,
        vec![crate::cc::ValueShape::Integer]
    );

    let reversed = SourceSignature {
        parameters: vec![SourceType::Enum {
            cases: cases.into_iter().rev().collect(),
        }],
        ..source.clone()
    };
    assert!(
        WasiRegistry::load()
            .expect("vendored WASI should load")
            .validate_signature(&import, &reversed)
            .is_err()
    );
    let reversed_result = SourceSignature {
        result: SourceType::Enum {
            cases: vec!["GreenBlue".into(), "Red".into()],
        },
        ..source
    };
    assert!(
        WasiRegistry::load()
            .expect("vendored WASI should load")
            .validate_signature(&import, &reversed_result)
            .is_err()
    );
}
