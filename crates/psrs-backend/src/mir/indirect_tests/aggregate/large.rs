//! An aggregate result whose canonical return area exceeds the scratch region.

use super::fixtures::{reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ReprId, Representation, Signature, ValueDecl,
    ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// `result<big, s32>` -> `Either Big Int` where
/// `Big = { a :: Number, b :: Number, c :: Int }`, a 24-byte payload whose
/// return area does not fit the 16-byte scratch region.
pub(super) fn large_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-large.wit",
            "package wasi:io@0.2.12; interface streams { record big { a: f64, b: f64, c: s64 } get: func() -> result<big, s32>; }",
        )
        .expect("the large record WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Product {
                fields: vec![ValueShape::Number, ValueShape::Number, ValueShape::Integer],
            },
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: vec![reference(1)],
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![ValueShape::Integer],
                    },
                ],
            },
        ],
        signatures: Vec::new(),
        product_labels: [(
            ReprId(1),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        )]
        .into_iter()
        .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: reference(2),
        },
        ValueDecl {
            id: ValueId(1),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(2),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
        Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: Vec::new(),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::Constant(0),
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "LargeRecordAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(2),
            }),
        }],
        representations,
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(2),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(main_symbol),
        span: span(),
    };
    let bindings = ExternalBindings {
        imports: vec![ExternalBinding {
            symbol: external_symbol,
            interface: "wasi:io/streams".into(),
            function: "get".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

/// `result<_, big>` -> `Unit` where the error payload is a 24-byte record whose
/// return area exceeds the scratch region. The trap path reads only the
/// discriminant, but the area must still be allocated.
pub(super) fn large_unit_result_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-large-unit.wit",
            "package wasi:io@0.2.12; interface streams { record big { a: f64, b: f64, c: s64 } get: func() -> result<_, big>; }",
        )
        .expect("the large unit result WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Product {
                fields: vec![ValueShape::Number, ValueShape::Number, ValueShape::Integer],
            },
        ],
        signatures: Vec::new(),
        product_labels: [(
            ReprId(1),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        )]
        .into_iter()
        .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: "LargeUnitResultAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: ValueShape::Integer,
            }),
        }],
        representations,
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueShape::Integer,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::DirectCall {
                        function: external_symbol,
                        arguments: Vec::new(),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
            ],
            result: ValueId(1),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(main_symbol),
        span: span(),
    };
    let bindings = ExternalBindings {
        imports: vec![ExternalBinding {
            symbol: external_symbol,
            interface: "wasi:io/streams".into(),
            function: "get".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

#[cfg(test)]
mod tests {
    use super::super::lower_module_with_registry;
    use super::large_unit_result_fixture;
    use crate::TargetCapabilities;
    use crate::abi;

    #[test]
    fn large_unit_success_result_allocates_its_return_area() {
        let (module, bindings, resolve) = large_unit_result_fixture();
        let target = TargetCapabilities {
            wasi_cli: false,
            ..TargetCapabilities::default()
        };
        let registry = abi::WasiRegistry::from_resolve(resolve, target);
        let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
            .expect("P9 should lower a large unit-success result");
        let reallocates = mir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(
                    instruction,
                    crate::mir::Instruction::Call { function, .. }
                        if *function == crate::abi::REALLOC_SYMBOL
                )
            })
            .count();
        assert!(
            reallocates >= 1,
            "the large error payload must size the return area"
        );

        let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the unit ABI");
        let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
            .expect("P10 should lower the large unit-success result");
        let binary = crate::wasm::encode_module(&wasm).expect("the unit Wasm should encode");
        crate::validator_for(target)
            .validate_all(&binary)
            .expect("the large unit-success Wasm should validate");
    }
}
