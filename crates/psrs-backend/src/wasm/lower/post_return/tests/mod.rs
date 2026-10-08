use super::super::super::{
    Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Import, Memory,
    MemoryIndex, Module, Op, TypeIndex,
};
use super::{post_return_name, synthesize_owned_handle_post_return};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};
use wit_parser::{Resolve, WorldId, WorldItem};

fn span() -> TextRange {
    TextRange::new(0, 1)
}

mod buffer;
mod composition;
use buffer::{allocator_requirement, buffer_post_return_module, string_world};
use composition::componentize;

#[test]
fn post_return_drops_an_owned_export_handle() {
    let wit = r#"
            package fixture:handles@0.1.0;
            interface types {
                resource thing;
            }
            interface api {
                use types.{thing};
                take: func() -> thing;
            }
            world guest {
                import types;
                export api;
            }
        "#;
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str("handles.wit", wit)
        .expect("the handle fixture should resolve");
    let world = resolve.packages[package]
        .worlds
        .get("guest")
        .copied()
        .expect("guest world");
    let (key, item) = resolve.worlds[world].exports.iter().next().unwrap();
    let WorldItem::Interface { id, .. } = item else {
        panic!("interface export");
    };
    let export = format!(
        "{}#{}",
        resolve.name_world_key(key),
        resolve.interfaces[*id].functions["take"].name
    );
    assert_eq!(export, "fixture:handles/api@0.1.0#take");
    assert_eq!(
        post_return_name(&export),
        "cabi_post_fixture:handles/api@0.1.0#take"
    );

    let drop_type = TypeIndex(0);
    let take_type = TypeIndex(1);
    let post_type = TypeIndex(0);
    let (post_signature, post_function, post_export) = synthesize_owned_handle_post_return(
        &export,
        post_type,
        FunctionIndex(2),
        FunctionIndex(0),
        SymbolId::new(ModuleId(0), 1),
        span(),
    );
    assert_eq!(post_signature.parameters, vec![ValType::I32]);
    assert!(post_signature.results.is_empty());
    assert_eq!(post_export.name, post_return_name(&export));

    let module = Module {
        name: "Handles".into(),
        imports: vec![Import {
            module: "fixture:handles/types@0.1.0".into(),
            name: "[resource-drop]thing".into(),
            type_index: drop_type,
        }],
        types: vec![
            FuncType {
                parameters: vec![ValType::I32],
                results: Vec::new(),
            },
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
        ],
        type_defs: Vec::new(),
        functions: vec![
            Function {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: export.clone(),
                type_index: take_type,
                parameters: Vec::new(),
                locals: Vec::new(),
                body: vec![Op::Leaf(Instruction::I32Const(1))],
                span: span(),
            },
            post_function,
        ],
        memories: vec![Memory {
            id: crate::types::MemoryId(0),
            index: MemoryIndex(0),
            minimum: 1,
            maximum: None,
        }],
        data: Vec::new(),
        exports: vec![
            Export {
                name: export,
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(1)),
            },
            post_export,
            Export {
                name: "memory".into(),
                kind: ExportKind::Memory,
                index: ExportIndex::Memory(MemoryIndex(0)),
            },
        ],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };
    let interface = "fixture:handles/types@0.1.0".to_string();
    let requirement = psrs_linker::BindingRequirement {
        id: psrs_linker::RequirementId(0),
        origin: "owned handle post-return".into(),
        boundary: psrs_linker::Boundary::ResolvedWit {
            interface: interface.clone(),
            function: "[resource-drop]thing".into(),
        },
        expected: Some(psrs_linker::CoreSignature {
            parameters: vec![psrs_linker::CoreType::I32],
            result: None,
        }),
        provider: psrs_linker::Provider::HostInterface { interface },
    };
    let component = componentize(module, resolve, world, vec![requirement])
        .expect("componentizing post-return");
    crate::validator()
        .validate_all(&component)
        .expect("the component should validate");
    let text = wasmprinter::print_bytes(&component).expect("printing the component");
    assert!(
        text.contains("resource.drop"),
        "post-return should lower to canon resource.drop: {text}"
    );
    assert!(
        text.contains("cabi_post_fixture:handles/api@0.1.0#take") || text.contains("post-return"),
        "the export should have a post-return: {text}"
    );
}

fn require_wasmtime() -> bool {
    match std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
    {
        Ok(_) => true,
        Err(_) if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") => {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        Err(_) => {
            eprintln!("skipping post-return execution: wasmtime is not installed");
            false
        }
    }
}

#[test]
fn buffer_post_return_frees_the_returned_string() {
    assert_eq!(post_return_name("get-string"), "cabi_post_get-string");
    let module = buffer_post_return_module();
    crate::wasm::verify::verify_module(&module).expect("thin Wasm module verifies");
    let binary = crate::wasm::encode_module(&module).expect("Wasm encodes");
    crate::validator()
        .validate_all(&binary)
        .expect("post-return module validates");

    if !require_wasmtime() {
        return;
    }
    let (resolve, world) = string_world();
    let component = componentize(module, resolve, world, vec![allocator_requirement()])
        .expect("componentizing the reclaim driver");
    let path = std::env::temp_dir().join(format!(
        "psrs-buffer-post-return-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, component).expect("write post-return test module");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("check-reclaim()")
        .arg(&path)
        .output()
        .expect("run post-return test module");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "driver trapped: {output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "1",
        "post-return did not reclaim the returned buffer"
    );
}

#[test]
fn component_attaches_the_buffer_post_return() {
    let module = buffer_post_return_module();
    let (resolve, world) = string_world();
    let component = componentize(module, resolve, world, vec![allocator_requirement()])
        .expect("componentizing the string export");
    crate::validator()
        .validate_all(&component)
        .expect("the component should validate");
    let text = wasmprinter::print_bytes(&component).expect("printing the component");
    assert!(
        text.contains("post-return"),
        "the component should attach a post-return: {text}"
    );

    if !require_wasmtime() {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "psrs-buffer-post-return-component-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, &component).expect("write post-return component");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("get-string()")
        .arg(&path)
        .output()
        .expect("run post-return component");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the component export failed: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello"),
        "the component should lift the string: {output:?}"
    );
}
