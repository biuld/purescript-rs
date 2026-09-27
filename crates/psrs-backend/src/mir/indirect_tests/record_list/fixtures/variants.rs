//! Synthesized `list<option<T>>`, `list<variant { ... }>`, and nested-list
//! fixtures. No in-scope WASI interface exposes these, so the ABI path is
//! driven directly like the record-list fixtures.

use super::{reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, RefShape, Reference, Representation, Signature,
    ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// The abstract aggregate reference a variant is built into before it is cast
/// to its concrete supertype.
fn aggregate() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

/// The concrete reference of a representation handle.
fn concrete(repr: u32) -> Reference {
    Reference {
        nullable: false,
        heap: RefShape::Repr(cc::ReprId(repr)),
    }
}

/// The representation of `Data.Maybe.Maybe String`: a two-case variant whose
/// `Just` field holds a GC string.
fn maybe_string() -> Representation {
    Representation::Variant {
        cases: vec![
            VariantCase {
                tag: 0,
                fields: Vec::new(),
            },
            VariantCase {
                tag: 1,
                fields: vec![ValueShape::String],
            },
        ],
    }
}

/// Builds the shared parameter module: a source array of the element calls an
/// import that takes the matching canonical list. `element_value_ty` is the
/// declared type of the element before it is cast to `reference(element)`.
fn parameter_fixture(
    wit: &str,
    representations: Vec<Representation>,
    element: u32,
    element_value_ty: ValueShape,
    build: impl FnOnce(ValueId) -> Vec<Assignment>,
) -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str("aggregate-list.wit", wit)
        .expect("the aggregate list WIT fixture should resolve");
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::String,
        },
        ValueDecl {
            id: ValueId(1),
            ty: element_value_ty,
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(element),
        },
        ValueDecl {
            id: ValueId(3),
            ty: reference(element + 1),
        },
        ValueDecl {
            id: ValueId(4),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(5),
            ty: ValueShape::Integer,
        },
    ];
    let mut assignments = vec![Assignment {
        destination: ValueId(0),
        kind: AssignmentKind::StringConstant("hi".into()),
        span: span(),
    }];
    assignments.extend(build(ValueId(1)));
    assignments.push(Assignment {
        destination: ValueId(2),
        kind: AssignmentKind::RepresentationCast {
            destination: ValueId(2),
            value: ValueId(1),
            reference: concrete(element),
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: ValueId(3),
        kind: AssignmentKind::ArrayNew {
            destination: ValueId(3),
            representation: cc::ReprId(element + 1),
            elements: vec![ValueId(2)],
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: ValueId(4),
        kind: AssignmentKind::DirectCall {
            function: external_symbol,
            arguments: vec![ValueId(3)],
        },
        span: span(),
    });
    let module = cc::Module {
        name: "AggregateListAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(element + 1)],
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable {
            representations,
            signatures: Vec::new(),
            product_labels: Default::default(),
        },
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

/// `list<option<string>>` -> `Array (Maybe String)`.
pub(crate) fn option_string_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    parameter_fixture(
        "package wasi:io@0.2.12; interface streams { take: func(values: list<option<string>>); }",
        vec![
            maybe_string(),
            Representation::Array {
                element: reference(0),
            },
        ],
        0,
        aggregate(),
        |element| {
            vec![Assignment {
                destination: element,
                kind: AssignmentKind::VariantNew {
                    destination: element,
                    representation: cc::ReprId(0),
                    case: 1,
                    fields: vec![ValueId(0)],
                },
                span: span(),
            }]
        },
    )
}

/// `list<variant { none, some(string) }>` -> the same source shape, declared as
/// a WIT variant rather than `option`.
pub(crate) fn variant_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    parameter_fixture(
        "package wasi:io@0.2.12; interface streams { variant item { none, some(string) } take: func(values: list<item>); }",
        vec![
            maybe_string(),
            Representation::Array {
                element: reference(0),
            },
        ],
        0,
        aggregate(),
        |element| {
            vec![Assignment {
                destination: element,
                kind: AssignmentKind::VariantNew {
                    destination: element,
                    representation: cc::ReprId(0),
                    case: 1,
                    fields: vec![ValueId(0)],
                },
                span: span(),
            }]
        },
    )
}

/// `list<list<s32>>` -> `Array (Array Int)`.
pub(crate) fn nested_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    parameter_fixture(
        "package wasi:io@0.2.12; interface streams { take: func(values: list<list<s32>>); }",
        vec![
            Representation::Array {
                element: ValueShape::Integer,
            },
            Representation::Array {
                element: reference(0),
            },
        ],
        0,
        reference(0),
        |element| {
            vec![
                Assignment {
                    destination: ValueId(5),
                    kind: AssignmentKind::Constant(7),
                    span: span(),
                },
                Assignment {
                    destination: element,
                    kind: AssignmentKind::ArrayNew {
                        destination: element,
                        representation: cc::ReprId(0),
                        elements: vec![ValueId(5)],
                    },
                    span: span(),
                },
            ]
        },
    )
}

/// A `list<option<string>>` result: the import returns the array.
pub(crate) fn option_string_list_result_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-list-result.wit",
            "package wasi:io@0.2.12; interface streams { get: func() -> list<option<string>>; }",
        )
        .expect("the aggregate list result WIT fixture should resolve");
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: "AggregateListResultAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(1),
            }),
        }],
        representations: cc::RepresentationTable {
            representations: vec![
                maybe_string(),
                Representation::Array {
                    element: reference(0),
                },
            ],
            signatures: Vec::new(),
            product_labels: Default::default(),
        },
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

/// `list<option<list<s32>>>` -> `Array (Maybe (Array Int))`, exercising a
/// nested-list payload inside a variant element.
pub(crate) fn option_list_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-list-nested.wit",
            "package wasi:io@0.2.12; interface streams { take: func(values: list<option<list<s32>>>); }",
        )
        .expect("the nested aggregate list WIT fixture should resolve");
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(1),
            ty: reference(0),
        },
        ValueDecl {
            id: ValueId(2),
            ty: aggregate(),
        },
        ValueDecl {
            id: ValueId(3),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(4),
            ty: reference(2),
        },
        ValueDecl {
            id: ValueId(5),
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
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(1),
                representation: cc::ReprId(0),
                elements: vec![ValueId(0)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::VariantNew {
                destination: ValueId(2),
                representation: cc::ReprId(1),
                case: 1,
                fields: vec![ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::RepresentationCast {
                destination: ValueId(3),
                value: ValueId(2),
                reference: concrete(1),
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(4),
                representation: cc::ReprId(2),
                elements: vec![ValueId(3)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(5),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(4)],
            },
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "AggregateListNestedAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(2)],
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable {
            representations: vec![
                Representation::Array {
                    element: ValueShape::Integer,
                },
                Representation::Variant {
                    cases: vec![
                        VariantCase {
                            tag: 0,
                            fields: Vec::new(),
                        },
                        VariantCase {
                            tag: 1,
                            fields: vec![reference(0)],
                        },
                    ],
                },
                Representation::Array {
                    element: reference(1),
                },
            ],
            signatures: Vec::new(),
            product_labels: Default::default(),
        },
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(5),
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
