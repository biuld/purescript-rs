//! Tag-section encoding and profile-gate evidence (EH-02, EH-03).

use super::*;

/// EH-02: a tag-using module encodes with the tag section (id 13) in position
/// between the memory and global sections.
#[test]
fn encodes_the_tag_section_between_memory_and_global() {
    let mut module = tag_fixture(catches_payload_body(), vec![WasmValType::I32]);
    module.memories = vec![Memory {
        id: crate::types::MemoryId(0),
        index: super::super::MemoryIndex(0),
        minimum: 1,
        maximum: None,
    }];
    module.globals = vec![Global {
        index: super::super::GlobalIndex(0),
        mutable: true,
        ty: WasmValType::I32,
        init: GlobalInit::I32(0),
    }];
    verify(&module).expect("the memory and global skeleton should verify");
    let binary = encode_module(&module).expect("the tagged module should encode");

    let mut order = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&binary) {
        match payload.unwrap() {
            Payload::TypeSection(_) => order.push("type"),
            Payload::FunctionSection(_) => order.push("function"),
            Payload::MemorySection(_) => order.push("memory"),
            Payload::TagSection(_) => order.push("tag"),
            Payload::GlobalSection(_) => order.push("global"),
            Payload::ExportSection(_) => order.push("export"),
            Payload::CodeSectionEntry(_) => order.push("code"),
            _ => {}
        }
    }
    let memory = order
        .iter()
        .position(|section| *section == "memory")
        .unwrap();
    let tag = order.iter().position(|section| *section == "tag").unwrap();
    let global = order
        .iter()
        .position(|section| *section == "global")
        .unwrap();
    assert!(memory < tag && tag < global, "section order: {order:?}");

    crate::validator_for(exceptions_enabled_target())
        .validate_all(&binary)
        .expect("wasmparser should accept the tagged module under the enabled profile");
}

/// EH-02: a module without tags emits no tag section.
#[test]
fn encodes_no_tag_section_without_tags() {
    let module = module_with_function_body("NoTags", vec![Op::Leaf(Instruction::I32Const(0))]);
    assert!(module.tags.is_empty());
    let binary = encode_module(&module).expect("the plain module should encode");
    for payload in wasmparser::Parser::new(0).parse_all(&binary) {
        assert!(
            !matches!(payload.unwrap(), Payload::TagSection(_)),
            "a tag-less module must not carry a tag section"
        );
    }
}

/// EH-02: the encoded tagged module prints as WAT carrying the tag, the
/// try_table region, and the throw.
#[test]
fn prints_the_tagged_module_as_wat() {
    let module = tag_fixture(catches_payload_body(), vec![WasmValType::I32]);
    let binary = encode_module(&module).expect("the tagged module should encode");
    let wat = wasmprinter::print_bytes(&binary).expect("the tagged module should print");
    assert!(wat.contains("tag"), "{wat}");
    assert!(wat.contains("try_table"), "{wat}");
    assert!(wat.contains("throw"), "{wat}");
}

/// EH-03: the profile-derived validator rejects a tag-using module while
/// `exceptions` is disabled and accepts it once enabled; enabling `exceptions`
/// turns on exactly that one wasmparser feature.
#[test]
fn exceptions_gate_the_tag_using_module() {
    let module = tag_fixture(catches_payload_body(), vec![WasmValType::I32]);
    let binary = encode_module(&module).expect("the tagged module should encode");

    let disabled = TargetCapabilities {
        exceptions: false,
        ..TargetCapabilities::default()
    };
    let rejection = crate::validator_for(disabled)
        .validate_all(&binary)
        .err()
        .expect("a tag-using module must not validate without exceptions");
    assert!(
        rejection
            .to_string()
            .contains("exceptions proposal not enabled"),
        "the rejection should name the disabled proposal: {rejection}"
    );

    let enabled = exceptions_enabled_target();
    crate::validator_for(enabled)
        .validate_all(&binary)
        .expect("the same module should validate with exceptions enabled");
    assert_eq!(
        enabled.wasm_features() ^ disabled.wasm_features(),
        WasmFeatures::EXCEPTIONS,
        "enabling exceptions must touch no other feature"
    );
}
