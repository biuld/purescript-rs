use super::lower_module_with_bindings;
use crate::abi::{SourceSignature, SourceType};
use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn input(call: bool) -> (cc::Module, ExternalBindings) {
    let external = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let source_signature = SourceSignature {
        parameters: Vec::new(),
        result: SourceType::Int,
        span: span(),
    };
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
            signature: Some(source_signature),
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
    assert_eq!(wasi.imports().len(), 1);
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
    assert_eq!(wasi.imports().len(), 1);
    assert_eq!(wasi.imports()[0].name, crate::abi::names::GET_STDOUT);
}

#[test]
fn p9_rejects_a_binding_that_disagrees_with_cc() {
    let (module, mut bindings) = input(false);
    bindings.imports[0].signature.as_mut().unwrap().result = SourceType::Boolean;
    let errors =
        match lower_module_with_bindings(module, bindings, crate::TargetCapabilities::default()) {
            Ok(_) => panic!("P9 must not silently replace the CC external signature"),
            Err(errors) => errors,
        };
    assert!(
        errors
            .iter()
            .any(|error| { error.message.contains("disagrees with its CC signature") })
    );
}
