use super::lower_module_with_bindings;
use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn input(call: bool) -> (cc::Module, ExternalBindings) {
    let external = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let assignment = if call {
        Assignment {
            destination: crate::types::ValueId(0),
            kind: AssignmentKind::DirectCall {
                function: external,
                arguments: Vec::new(),
            },
            span: span(),
        }
    } else {
        Assignment {
            destination: crate::types::ValueId(0),
            kind: AssignmentKind::Constant(0),
            span: span(),
        }
    };
    let module = cc::Module {
        name: "Bindings".into(),
        externals: vec![External {
            result_guest: None,
            symbol: external,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable::default(),
        functions: vec![cc::Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![ValueDecl {
                id: crate::types::ValueId(0),
                ty: ValueShape::Integer,
            }],
            assignments: vec![assignment],
            result: crate::types::ValueId(0),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: span(),
    };
    let bindings = ExternalBindings {
        imports: vec![ExternalBinding {
            symbol: external,
            interface: crate::abi::names::STDOUT.into(),
            function: crate::abi::names::GET_STDOUT.into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings)
}

#[test]
fn p9_does_not_emit_unreachable_external_bindings() {
    let (module, bindings) = input(false);
    let (mir, wasi) = lower_module_with_bindings(
        module.clone(),
        bindings,
        crate::TargetCapabilities::default(),
    )
    .expect("an unused external should still be validated");
    assert!(mir.imports.is_empty());
    assert!(
        wasi.imports()
            .iter()
            .any(|import| import.name == crate::abi::names::GET_STDOUT)
    );
    assert!(
        wasi.imports()
            .iter()
            .any(|import| import.name == "[resource-drop]output-stream")
    );
    let dump = format!("{module:?}");
    assert!(!dump.contains(crate::abi::names::STDOUT));
}

#[test]
fn p9_emits_a_referenced_external_binding() {
    let (module, bindings) = input(true);
    let (mir, wasi) =
        lower_module_with_bindings(module, bindings, crate::TargetCapabilities::default())
            .expect("the referenced WIT binding should lower");
    assert_eq!(mir.imports.len(), 1);
    assert!(
        wasi.imports()
            .iter()
            .any(|import| import.name == crate::abi::names::GET_STDOUT)
    );
    assert!(
        mir.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .all(|instruction| !matches!(instruction, crate::mir::Instruction::CallVoid { .. })),
        "returning the owned handle does not drop it in this function"
    );
}

#[test]
fn p9_exposes_an_owned_handle_without_dropping_it() {
    let external = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let module = cc::Module {
        name: "Drop".into(),
        externals: vec![External {
            result_guest: None,
            symbol: external,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable::default(),
        functions: vec![cc::Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: crate::types::ValueId(0),
                    ty: ValueShape::Integer,
                },
                ValueDecl {
                    id: crate::types::ValueId(1),
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![
                Assignment {
                    destination: crate::types::ValueId(0),
                    kind: AssignmentKind::DirectCall {
                        function: external,
                        arguments: Vec::new(),
                    },
                    span: span(),
                },
                Assignment {
                    destination: crate::types::ValueId(1),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
            ],
            result: crate::types::ValueId(1),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: span(),
    };
    let bindings = ExternalBindings {
        imports: vec![ExternalBinding {
            symbol: external,
            interface: crate::abi::names::STDOUT.into(),
            function: crate::abi::names::GET_STDOUT.into(),
            type_id: None,
            span: span(),
        }],
    };
    let (mir, _wasi) =
        lower_module_with_bindings(module, bindings, crate::TargetCapabilities::default())
            .expect("an owned stdout handle should lower");
    assert!(
        mir.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .all(|instruction| !matches!(instruction, crate::mir::Instruction::CallVoid { .. })),
        "the compiler must not drop a handle on its own (DEC-14)"
    );
}
