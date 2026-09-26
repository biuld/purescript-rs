//! A synthesized `list<record>` fixture: no in-scope WASI interface exposes a
//! list of records, so this drives the ABI path directly, like the composite
//! indirect-parameter fixture.

use super::lower_module_with_registry;
use crate::TargetCapabilities;
use crate::abi;
use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;
use wit_parser::Resolve;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn reference(repr: u32) -> ValueShape {
    ValueShape::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Repr(cc::ReprId(repr)),
    })
}

fn fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "record-list.wit",
            "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: f64 } take: func(values: list<pair>); }",
        )
        .expect("the record list WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::Number],
            },
            cc::Representation::Array {
                element: reference(0),
            },
        ],
        signatures: Vec::new(),
        product_labels: [(cc::ReprId(0), vec!["x".to_string(), "y".to_string()])]
            .into_iter()
            .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueShape::Number,
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(0),
        },
        ValueDecl {
            id: ValueId(3),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(4),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
        Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::Constant(7),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::NumberConstant("1.5".into()),
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::ProductNew {
                destination: ValueId(2),
                representation: cc::ReprId(0),
                arguments: vec![ValueId(0), ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(3),
                representation: cc::ReprId(1),
                elements: vec![ValueId(2)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(3)],
            },
            span: span(),
        },
    ];

    let module = cc::Module {
        name: "RecordListAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(1)],
                result: ValueShape::Integer,
            }),
        }],
        representations,
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(4),
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
            function: "take".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

#[test]
fn record_list_parameters_lower_to_a_canonical_layout() {
    let (cc, bindings, resolve) = fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<record> parameter");

    let copy = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            crate::mir::Instruction::ListCopyRecord {
                direction,
                size,
                fields,
                ..
            } => Some((*direction, *size, fields.clone())),
            _ => None,
        })
        .expect("a ListCopyRecord should be emitted");
    assert_eq!(copy.0, crate::mir::ListDirection::Store);
    assert_eq!(copy.1, 16, "pair {{ x: s32, y: f64 }} is 16 bytes");
    let layout = copy
        .2
        .iter()
        .map(|field| (field.offset, field.index, field.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        layout,
        vec![
            (0, 0, crate::abi::layout::SlotKind::Word),
            (8, 1, crate::abi::layout::SlotKind::F64),
        ]
    );

    let mir =
        crate::mir::opt::optimize(mir, target).expect("P10 should preserve the record list copy");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the record list copy");
    let binary = crate::wasm::encode_module(&wasm).expect("the record list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the record list Wasm should validate");
}

fn result_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "record-list.wit",
            "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: f64 } get: func() -> list<pair>; }",
        )
        .expect("the record list WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::Number],
            },
            cc::Representation::Array {
                element: reference(0),
            },
        ],
        signatures: Vec::new(),
        product_labels: [(cc::ReprId(0), vec!["x".to_string(), "y".to_string()])]
            .into_iter()
            .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: "RecordListResultAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(1),
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
                    ty: reference(1),
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

#[test]
fn record_list_results_rebuild_the_array() {
    let (cc, bindings, resolve) = result_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<record> result");
    let direction = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            crate::mir::Instruction::ListCopyRecord { direction, .. } => Some(*direction),
            _ => None,
        });
    assert_eq!(direction, Some(crate::mir::ListDirection::Load));

    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the record list result");
    let binary =
        crate::wasm::encode_module(&wasm).expect("the record list result Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the record list result Wasm should validate");
}
