use super::*;

#[test]
fn facade_preserves_function_indices_and_complete_public_gc_signatures() {
    let source = wat::parse_str(
        r#"(module
        (type $a (array (mut eqref)))
        (import "private" "read" (func $read (param (ref $a) i32) (result eqref)))
        (memory (export "memory") 1)
        (func (export "run") (result i32) i32.const 7)
        (export "read" (func $read)))"#,
    )
    .unwrap();
    let facade = boundary_module(&source, &[]).unwrap();
    let original = crate::CoreTypes::from_module(&source).unwrap();
    let projected = crate::CoreTypes::from_module(&facade).unwrap();
    assert_eq!(
        original.signature(1).unwrap(),
        projected.signature(1).unwrap()
    );
    assert_eq!(
        original.signature(2).unwrap(),
        projected.signature(2).unwrap()
    );
    assert!(
        !Parser::new(0)
            .parse_all(&facade)
            .any(|payload| matches!(payload, Ok(Payload::ImportSection(_))))
    );
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(&facade) {
        if let Payload::ExportSection(section) = payload.unwrap() {
            for export in section {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func {
                    exports.push((export.name.to_string(), export.index));
                }
            }
        }
    }
    assert!(exports.contains(&("read".into(), 0)));
    assert!(exports.contains(&("run".into(), 1)));
}

#[test]
fn an_exported_import_only_module_gets_a_valid_boundary_declaration() {
    let source = wat::parse_str(
        r#"(module
        (import "private" "run" (func (result i32)))
        (memory (export "memory") 1)
        (export "run" (func 0)))"#,
    )
    .unwrap();
    let projected = boundary_module(&source, &[]).unwrap();
    wasmparser::Validator::new()
        .validate_all(&projected)
        .unwrap();
}

#[test]
fn facade_retains_registered_scalar_libraries_and_relocates_raw_reference_functions() {
    let source = wat::parse_str(
        r#"(module
      (type $array (array (mut eqref)))
      (import "raw" "read" (func $read (param (ref $array) i32) (result eqref)))
      (import "scalar" "allocate" (func $allocate (param i32) (result i32)))
      (memory (export "memory") 1)
      (export "read" (func $read))
      (export "allocate" (func $allocate)))"#,
    )
    .unwrap();
    let facade = boundary_module(&source, &["scalar".into()]).unwrap();
    wasmparser::Validator::new().validate_all(&facade).unwrap();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(&facade) {
        match payload.unwrap() {
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    imports.push(import.unwrap().module.to_owned());
                }
            }
            Payload::ExportSection(section) => {
                for export in section {
                    let export = export.unwrap();
                    if export.kind == wasmparser::ExternalKind::Func {
                        exports.push((export.name.to_owned(), export.index));
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(imports, ["scalar"]);
    assert!(exports.contains(&("allocate".into(), 0)));
    assert!(exports.contains(&("read".into(), 1)));
    let original = crate::CoreTypes::from_module(&source).unwrap();
    let projected = crate::CoreTypes::from_module(&facade).unwrap();
    assert_eq!(
        original.signature(1).unwrap(),
        projected.signature(1).unwrap()
    );
    assert_eq!(
        original.signature(2).unwrap(),
        projected.signature(2).unwrap()
    );
}

#[test]
fn reserved_boundary_identity_and_storage_imports_are_rejected() {
    let mut source = Module::new();
    source.section(&CustomSection {
        name: Cow::Borrowed(MARKER),
        data: Cow::Borrowed(b"1"),
    });
    assert!(
        boundary_module(&source.finish(), &[])
            .unwrap_err()
            .contains("reserved")
    );
    let source = wat::parse_str(r#"(module (import "env" "memory" (memory 1)))"#).unwrap();
    assert!(
        boundary_module(&source, &[])
            .unwrap_err()
            .contains("application-owned storage")
    );
}

#[test]
fn index_mapping_tracks_application_identity_and_component_scopes() {
    let mut attach = Attach {
        modules: 4,
        instances: 4,
        providers: Default::default(),
        application_module: 2,
        depth: 0,
        module_seen: true,
        instance_seen: true,
        source_instances: 3,
        source_application: Some(1),
    };
    assert_eq!(attach.module_index(0), 2);
    assert_eq!(attach.module_index(1), 4);
    assert_eq!(attach.instance_index(0), 4);
    assert_eq!(attach.instance_index(1), 5);
    assert_eq!(attach.instance_index(2), 6);
    attach.push_depth();
    assert_eq!(attach.module_index(0), 0);
    assert_eq!(attach.instance_index(1), 1);
    assert_eq!(attach.outer_module_index(1, 0), 2);
    attach.pop_depth();
}

#[test]
fn malformed_boundary_module_and_instance_identities_are_rejected() {
    fn attach() -> Attach {
        Attach {
            modules: 2,
            instances: 2,
            providers: Default::default(),
            application_module: 1,
            depth: 0,
            module_seen: false,
            instance_seen: false,
            source_instances: 0,
            source_application: None,
        }
    }
    let missing = wat::parse_str("(component (core module))").unwrap();
    let error = attach()
        .parse_component(&mut Component::new(), Parser::new(0), &missing)
        .unwrap_err();
    assert!(error.to_string().contains("identity marker"));

    let mut module = Module::new();
    module.section(&CustomSection {
        name: Cow::Borrowed(MARKER),
        data: Cow::Borrowed(b"1"),
    });
    let bytes = module.finish();
    let mut component = wasm_encoder::ComponentBuilder::default();
    component.core_module_raw(None, &bytes);
    component.core_module_raw(None, &bytes);
    let repeated = component.finish();
    let error = attach()
        .parse_component(&mut Component::new(), Parser::new(0), &repeated)
        .unwrap_err();
    assert!(error.to_string().contains("identity is repeated"));

    let mut component = wasm_encoder::ComponentBuilder::default();
    let module = component.core_module_raw(None, &bytes);
    component.core_instantiate(None, module, []);
    component.core_instantiate(None, module, []);
    let repeated = component.finish();
    let error = attach()
        .parse_component(&mut Component::new(), Parser::new(0), &repeated)
        .unwrap_err();
    assert!(error.to_string().contains("instance is repeated"));
}

#[test]
fn facade_retains_host_imports_and_remaps_private_function_references() {
    let source = wat::parse_str(
        r#"(module
        (type $a (array (mut eqref)))
        (import "private" "read" (func $read (param (ref $a) i32) (result eqref)))
        (import "host" "write" (func $host (param i32)))
        (table 2 funcref)
        (elem (i32.const 0) func $read $host)
        (global (ref func) ref.func $read)
        (memory (export "memory") 1)
        (func (export "run") (result i32) i32.const 7)
        (export "read" (func $read))
        (export "host" (func $host)))"#,
    )
    .unwrap();
    let projected = boundary_module(&source, &["host".into()]).unwrap();
    let mut exports = Vec::new();
    let mut imports = Vec::new();
    let mut functions = Vec::new();
    for payload in Parser::new(0).parse_all(&projected) {
        match payload.unwrap() {
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    let import = import.unwrap();
                    imports.push((import.module.to_string(), import.name.to_string()));
                }
            }
            Payload::ExportSection(section) => {
                for export in section {
                    let export = export.unwrap();
                    if export.kind == wasmparser::ExternalKind::Func {
                        exports.push((export.name.to_string(), export.index));
                    }
                }
            }
            Payload::ElementSection(section) => {
                for element in section {
                    let wasmparser::ElementItems::Functions(items) = element.unwrap().items else {
                        panic!("function element expected")
                    };
                    functions.extend(items.into_iter().map(Result::unwrap));
                }
            }
            _ => {}
        }
    }
    assert_eq!(imports, [("host".into(), "write".into())]);
    assert_eq!(functions, [1, 0]);
    assert!(exports.contains(&("host".into(), 0)));
    assert!(exports.contains(&("read".into(), 1)));
    assert!(exports.contains(&("run".into(), 2)));
}
