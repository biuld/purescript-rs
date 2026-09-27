//! Synthesized multi-word flags list and record fixtures. A `flags` type with
//! more than 32 members packs into several canonical `i32` words, and a
//! multi-word flags value inside a list element or record must copy every word.

use super::{reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ReprId, Representation, Signature, ValueDecl,
    ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

const FLAG_COUNT: usize = 33;

fn flag_names() -> Vec<String> {
    (0..FLAG_COUNT)
        .map(|index| format!("flag{index:02}"))
        .collect()
}

fn flags_wit(names: &[String]) -> String {
    format!(
        "package wasi:io@0.2.12; interface streams {{ flags access {{ {} }} ",
        names.join(", ")
    )
}

fn flags_representation() -> Representation {
    Representation::Product {
        fields: vec![ValueShape::Boolean; FLAG_COUNT],
    }
}

fn flag_values() -> (Vec<ValueDecl>, Vec<Assignment>) {
    let mut values = Vec::new();
    let mut assignments = Vec::new();
    for index in 0..FLAG_COUNT {
        values.push(ValueDecl {
            id: ValueId(index as u32),
            ty: ValueShape::Boolean,
        });
        assignments.push(Assignment {
            destination: ValueId(index as u32),
            kind: AssignmentKind::Constant((index % 2) as i32),
            span: span(),
        });
    }
    (values, assignments)
}

/// `list<access>` with 33 flags -> `Array Access`.
pub(crate) fn wide_flags_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let names = flag_names();
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "wide-flags-list.wit",
            &format!("{}take: func(values: list<access>); }}", flags_wit(&names)),
        )
        .expect("the wide flags list WIT fixture should resolve");
    let (mut values, mut assignments) = flag_values();
    let product = ValueId(FLAG_COUNT as u32);
    let array = ValueId(FLAG_COUNT as u32 + 1);
    let result = ValueId(FLAG_COUNT as u32 + 2);
    values.push(ValueDecl {
        id: product,
        ty: reference(0),
    });
    values.push(ValueDecl {
        id: array,
        ty: reference(1),
    });
    values.push(ValueDecl {
        id: result,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: product,
        kind: AssignmentKind::ProductNew {
            destination: product,
            representation: ReprId(0),
            arguments: (0..FLAG_COUNT as u32).map(ValueId).collect(),
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: array,
        kind: AssignmentKind::ArrayNew {
            destination: array,
            representation: ReprId(1),
            elements: vec![product],
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::DirectCall {
            function: SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE),
            arguments: vec![array],
        },
        span: span(),
    });
    finish(
        resolve,
        "WideFlagsListAbi",
        values,
        assignments,
        result,
        vec![
            flags_representation(),
            Representation::Array {
                element: reference(0),
            },
        ],
        vec![(ReprId(0), names.clone())],
        "take",
        Some(reference(1)),
    )
}

/// `list<message>` where `message = { access: access, n: s32 }`.
pub(crate) fn wide_flags_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let names = flag_names();
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "wide-flags-record.wit",
            &format!(
                "{}record message {{ access: access, n: s32 }} take: func(values: list<message>); }}",
                flags_wit(&names)
            ),
        )
        .expect("the wide flags record WIT fixture should resolve");
    let (mut values, mut assignments) = flag_values();
    let flags = ValueId(FLAG_COUNT as u32);
    let number = ValueId(FLAG_COUNT as u32 + 1);
    let product = ValueId(FLAG_COUNT as u32 + 2);
    let array = ValueId(FLAG_COUNT as u32 + 3);
    let result = ValueId(FLAG_COUNT as u32 + 4);
    values.push(ValueDecl {
        id: flags,
        ty: reference(0),
    });
    values.push(ValueDecl {
        id: number,
        ty: ValueShape::Integer,
    });
    values.push(ValueDecl {
        id: product,
        ty: reference(1),
    });
    values.push(ValueDecl {
        id: array,
        ty: reference(2),
    });
    values.push(ValueDecl {
        id: result,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: flags,
        kind: AssignmentKind::ProductNew {
            destination: flags,
            representation: ReprId(0),
            arguments: (0..FLAG_COUNT as u32).map(ValueId).collect(),
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: number,
        kind: AssignmentKind::Constant(7),
        span: span(),
    });
    assignments.push(Assignment {
        destination: product,
        kind: AssignmentKind::ProductNew {
            destination: product,
            representation: ReprId(1),
            arguments: vec![flags, number],
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: array,
        kind: AssignmentKind::ArrayNew {
            destination: array,
            representation: ReprId(2),
            elements: vec![product],
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::DirectCall {
            function: SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE),
            arguments: vec![array],
        },
        span: span(),
    });
    finish(
        resolve,
        "WideFlagsRecordAbi",
        values,
        assignments,
        result,
        vec![
            flags_representation(),
            Representation::Product {
                fields: vec![reference(0), ValueShape::Integer],
            },
            Representation::Array {
                element: reference(1),
            },
        ],
        vec![
            (ReprId(0), names.clone()),
            (ReprId(1), vec!["access".into(), "n".into()]),
        ],
        "take",
        Some(reference(2)),
    )
}

/// `list<option<access>>` where `access` has 33 flags.
pub(crate) fn wide_flags_variant_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let names = flag_names();
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "wide-flags-variant.wit",
            &format!(
                "{}take: func(values: list<option<access>>); }}",
                flags_wit(&names)
            ),
        )
        .expect("the wide flags variant WIT fixture should resolve");
    let (mut values, mut assignments) = flag_values();
    let flags = ValueId(FLAG_COUNT as u32);
    let variant = ValueId(FLAG_COUNT as u32 + 1);
    let array = ValueId(FLAG_COUNT as u32 + 2);
    let result = ValueId(FLAG_COUNT as u32 + 3);
    values.push(ValueDecl {
        id: flags,
        ty: reference(0),
    });
    values.push(ValueDecl {
        id: variant,
        ty: ValueShape::Reference(cc::Reference {
            nullable: false,
            heap: cc::RefShape::Aggregate,
        }),
    });
    values.push(ValueDecl {
        id: array,
        ty: reference(2),
    });
    values.push(ValueDecl {
        id: result,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: flags,
        kind: AssignmentKind::ProductNew {
            destination: flags,
            representation: ReprId(0),
            arguments: (0..FLAG_COUNT as u32).map(ValueId).collect(),
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: variant,
        kind: AssignmentKind::VariantNew {
            destination: variant,
            representation: ReprId(1),
            case: 1,
            fields: vec![flags],
        },
        span: span(),
    });
    // Cast the abstract variant to the concrete supertype before the array.
    let cast = ValueId(FLAG_COUNT as u32 + 4);
    values.push(ValueDecl {
        id: cast,
        ty: reference(1),
    });
    assignments.push(Assignment {
        destination: cast,
        kind: AssignmentKind::RepresentationCast {
            destination: cast,
            value: variant,
            reference: cc::Reference {
                nullable: false,
                heap: cc::RefShape::Repr(ReprId(1)),
            },
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: array,
        kind: AssignmentKind::ArrayNew {
            destination: array,
            representation: ReprId(2),
            elements: vec![cast],
        },
        span: span(),
    });
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::DirectCall {
            function: SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE),
            arguments: vec![array],
        },
        span: span(),
    });
    finish(
        resolve,
        "WideFlagsVariantAbi",
        values,
        assignments,
        result,
        vec![
            flags_representation(),
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
        vec![(ReprId(0), names.clone())],
        "take",
        Some(reference(2)),
    )
}

#[allow(clippy::too_many_arguments)]
fn finish(
    resolve: Resolve,
    name: &str,
    values: Vec<ValueDecl>,
    assignments: Vec<Assignment>,
    result: ValueId,
    representations: Vec<Representation>,
    labels: Vec<(ReprId, Vec<String>)>,
    function: &str,
    parameter: Option<ValueShape>,
) -> (cc::Module, ExternalBindings, Resolve) {
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: name.into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: parameter.into_iter().collect(),
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable {
            representations,
            signatures: Vec::new(),
            product_labels: labels.into_iter().collect(),
        },
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result,
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
            function: function.into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}
