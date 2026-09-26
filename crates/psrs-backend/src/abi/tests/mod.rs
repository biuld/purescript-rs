use super::*;
use wit_parser::Type as WitType;

mod capability_gates;
mod indirect;
mod integers;
mod records;
mod resources;

fn empty_core_module() -> psrs_core::Module {
    psrs_core::Module {
        id: psrs_hir::ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 0),
    }
}

#[test]
fn resolves_stdout_and_exit_imports() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let stdout = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve");
    assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
    assert!(stdout.parameters.is_empty());
    assert!(stdout.param_kinds.is_empty());
    assert_eq!(stdout.result, Some(ValueType::I32));

    let write = registry
        .import(names::STREAMS, names::WRITE_STDOUT)
        .expect("blocking-write-and-flush should resolve");
    assert_eq!(write.module, "wasi:io/streams@0.2.12");
    assert_eq!(
        write.param_kinds,
        vec![WasiParamKind::Handle, WasiParamKind::List]
    );
    assert_eq!(write.result_kind, WasiResultKind::Result);
    assert!(write.retptr);
    assert!(write.unsupported.is_none());

    let read = registry
        .import("wasi:io/streams", "[method]input-stream.read")
        .expect("input-stream.read should resolve");
    assert!(
        read.unsupported
            .as_deref()
            .is_some_and(|message| { message.contains("payload on success") })
    );
    let exit = registry
        .import(names::EXIT, names::EXIT_WITH_CODE)
        .expect("exit-with-code should resolve");
    assert_eq!(exit.module, "wasi:cli/exit@0.2.12");
    assert_eq!(exit.parameters, vec![ValueType::I32]);
    assert_eq!(exit.result, None);

    // Interning returns the same symbol for the same import.
    let stdout_again = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve again");
    assert_eq!(stdout.symbol, stdout_again.symbol);
}

#[test]
fn classifies_a_64_bit_parameter_by_its_wit_signedness() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let random = registry
        .import("wasi:random/random", "get-random-bytes")
        .expect("get-random-bytes should resolve");
    assert_eq!(
        random.param_kinds,
        vec![WasiParamKind::Scalar64 { signed: false }]
    );
}

#[test]
fn maps_wit_char_to_the_source_char_type() {
    use psrs_hir::{BuiltinType, Type as HirType, TypeKind as HirTypeKind};
    use psrs_span::TextRange;

    assert_eq!(
        param_kind(&Resolve::default(), &WitType::Char),
        WasiParamKind::Char
    );
    assert_eq!(
        result_kind(&Resolve::default(), &WitType::Char),
        WasiResultKind::Char
    );

    let span = TextRange::new(0, 1);
    let source = source_signature(
        &empty_core_module(),
        &HirType {
            kind: HirTypeKind::Constructor(BuiltinType::Char),
            span,
        },
    )
    .expect("Char should cross the source ABI boundary");
    assert_eq!(source.result, SourceType::Char);

    let import = WasiImport {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        module: "test:chars".into(),
        name: "roundtrip".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::Char],
        result: Some(ValueType::I32),
        result_kind: WasiResultKind::Char,
        unsupported: None,
        retptr: false,
    };
    let signature = SourceSignature {
        parameters: vec![SourceType::Char],
        result: SourceType::Char,
        span,
    };
    WasiRegistry::load()
        .expect("WASI WIT should load")
        .validate_signature(&import, &signature)
        .expect("a Char declaration should match WIT char");

    let incompatible = SourceSignature {
        parameters: vec![SourceType::Int],
        result: SourceType::Int,
        span,
    };
    assert!(
        WasiRegistry::load()
            .expect("WASI WIT should load")
            .validate_signature(&import, &incompatible)
            .is_err()
    );
}

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
    let source = source_signature(
        &core,
        &HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(enum_type.clone()),
                result: Box::new(enum_type),
            },
            span,
        },
    )
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
    };
    WasiRegistry::load()
        .expect("vendored WASI should load")
        .validate_signature(&import, &source)
        .expect("matching source constructor tags should pass ABI validation");
    assert_eq!(
        crate::cc::abstract_signature(&source, &core, &std::collections::HashMap::new())
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

#[test]
fn classifies_wit_f32_for_number_conversion() {
    use psrs_span::TextRange;

    assert_eq!(
        param_kind(&Resolve::default(), &WitType::F32),
        WasiParamKind::Float32
    );
    let import = WasiImport {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        module: "test:floats".into(),
        name: "roundtrip".into(),
        parameters: vec![ValueType::F32],
        param_kinds: vec![WasiParamKind::Float32],
        result: Some(ValueType::F32),
        result_kind: WasiResultKind::Scalar,
        unsupported: None,
        retptr: false,
    };
    let signature = SourceSignature {
        parameters: vec![SourceType::Number],
        result: SourceType::Number,
        span: TextRange::new(0, 1),
    };
    WasiRegistry::load()
        .expect("WASI WIT should load")
        .validate_signature(&import, &signature)
        .expect("source Number should adapt to and from WIT f32");
}

#[test]
fn classifies_only_source_compatible_wit_scalar_parameters() {
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::Bool),
        WasiParamKind::Boolean
    );
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::S32),
        WasiParamKind::Integer32
    );
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::F64),
        WasiParamKind::Float64
    );
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::U32),
        WasiParamKind::Integer32
    );
    assert_eq!(
        result_kind(&Resolve::default(), &WitType::Bool),
        WasiResultKind::Boolean
    );
    assert_eq!(
        result_kind(&Resolve::default(), &WitType::U32),
        WasiResultKind::Scalar
    );
}

#[test]
fn validates_wit_scalar_parameters_against_exact_source_types() {
    use psrs_span::TextRange;

    let registry = WasiRegistry::load().expect("WASI WIT should load");
    let span = TextRange::new(0, 1);
    let cases = [
        (
            WasiParamKind::Integer32,
            SourceType::Int,
            SourceType::Boolean,
        ),
        (WasiParamKind::Boolean, SourceType::Boolean, SourceType::Int),
        (WasiParamKind::Float64, SourceType::Number, SourceType::Int),
    ];
    for (index, (kind, accepted, rejected)) in cases.into_iter().enumerate() {
        let import = WasiImport {
            symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), index as u32),
            module: "test:scalar".into(),
            name: "accepts-one-value".into(),
            parameters: vec![match kind {
                WasiParamKind::Integer32 | WasiParamKind::Boolean => ValueType::I32,
                WasiParamKind::Float64 => ValueType::F64,
                _ => unreachable!("this test only covers scalar parameters"),
            }],
            param_kinds: vec![kind],
            result: None,
            result_kind: WasiResultKind::None,
            unsupported: None,
            retptr: false,
        };
        let signature = |parameter| SourceSignature {
            parameters: vec![parameter],
            result: SourceType::Unit,
            span,
        };
        registry
            .validate_signature(&import, &signature(accepted))
            .expect("the matching source scalar should be accepted");
        assert!(
            registry
                .validate_signature(&import, &signature(rejected))
                .is_err()
        );
    }
}

#[test]
fn validates_a_vendored_wit_boolean_result_against_boolean_source_type() {
    use psrs_span::TextRange;

    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let import = registry
        .import("wasi:io/poll", "[method]pollable.ready")
        .expect("pollable.ready should resolve");
    assert_eq!(import.result_kind, WasiResultKind::Boolean);
    let signature = SourceSignature {
        parameters: vec![SourceType::Int],
        result: SourceType::Boolean,
        span: TextRange::new(0, 1),
    };
    registry
        .validate_signature(&import, &signature)
        .expect("pollable.ready should accept its Boolean source signature");

    let incompatible = SourceSignature {
        result: SourceType::Int,
        ..signature
    };
    assert!(registry.validate_signature(&import, &incompatible).is_err());
}

#[test]
fn rejects_a_disabled_wasi_service_before_lowering() {
    let target = TargetCapabilities {
        wasi_random: false,
        ..TargetCapabilities::default()
    };
    let mut registry = WasiRegistry::load_with_capabilities(target)
        .expect("WASI WIT should load with a restricted target");
    let random = registry
        .import("wasi:random/random", "get-random-bytes")
        .expect("the WIT declaration should still resolve");
    assert!(
        random
            .unsupported
            .as_deref()
            .is_some_and(|message| message.contains("disabled"))
    );
}
