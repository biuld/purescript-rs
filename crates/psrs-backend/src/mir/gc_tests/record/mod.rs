use super::*;
use crate::cc::{
    AggregateConvert, BoxKind, RecordOperation, RecoveryEvidence, RefShape, ReprId,
    ValueConversion, ValueDecl,
};

mod build;
mod malformed;

fn reference(id: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    })
}

#[derive(Default)]
struct Body {
    values: Vec<ValueDecl>,
    assignments: Vec<Assignment>,
}

impl Body {
    fn add(&mut self, ty: ValueShape, kind: impl FnOnce(ValueId) -> AssignmentKind) -> ValueId {
        let id = ValueId(self.values.len() as u32);
        self.values.push(ValueDecl { id, ty });
        self.assignments.push(Assignment {
            destination: id,
            kind: kind(id),
            span: span(),
        });
        id
    }
}

fn roundtrip(wrong_label: bool, short_payload: bool, update_label: Option<&str>) -> CcModule {
    let mut table = RepresentationTable::default();
    let boxed = table.reserve();
    table.set(
        boxed,
        Representation::Box {
            value: ValueShape::Integer,
        },
    );
    let array = table.reserve();
    table.set(
        array,
        Representation::Array {
            element: ValueShape::Integer,
        },
    );
    let source = table.reserve();
    let target = table.reserve();
    let fields = vec![ValueShape::String, reference(array), ValueShape::Integer];
    table.set(
        source,
        Representation::Product {
            fields: fields.clone(),
        },
    );
    table.set(target, Representation::Product { fields });
    let labels = vec!["alpha".into(), "list".into(), "value".into()];
    let mut input_labels = labels.clone();
    if wrong_label {
        input_labels[2] = "wrong".into();
    }
    table.set_product_labels(source, input_labels.clone());
    table.set_product_labels(target, labels.clone());
    let storage = table.ensure_record_storage();
    let erase_int = ValueConversion::Sequence(vec![
        ValueConversion::BoxScalar {
            kind: BoxKind::Integer,
            representation: boxed,
        },
        ValueConversion::EraseReference,
    ]);
    let recover_int = ValueConversion::UnboxScalar {
        kind: BoxKind::Integer,
        representation: boxed,
        destination: ValueShape::Integer,
    };
    let erase_fields = vec![
        ValueConversion::EraseReference,
        ValueConversion::Sequence(vec![
            ValueConversion::ArrayMap {
                source: array,
                target: storage.values(),
                element: Box::new(erase_int.clone()),
            },
            ValueConversion::EraseReference,
        ]),
        erase_int,
    ];
    let recover_fields = vec![
        ValueConversion::RecoverReference {
            destination: ValueShape::String,
            evidence: RecoveryEvidence::TypeInstantiation,
        },
        ValueConversion::Sequence(vec![
            ValueConversion::RecoverReference {
                destination: reference(storage.values()),
                evidence: RecoveryEvidence::TypeInstantiation,
            },
            ValueConversion::ArrayMap {
                source: storage.values(),
                target: array,
                element: Box::new(recover_int.clone()),
            },
        ]),
        recover_int,
    ];
    let mut body = Body::default();
    // A non-null String field precedes an ArrayMap introducing control labels.
    let text = body.add(ValueShape::String, |_: ValueId| {
        AssignmentKind::StringConstant("λ😀".into())
    });
    let n = body.add(ValueShape::Integer, |_| AssignmentKind::Constant(42));
    let other = body.add(ValueShape::Integer, |_| AssignmentKind::Constant(37));
    let input_array = body.add(reference(array), |id| AssignmentKind::ArrayNew {
        destination: id,
        representation: array,
        elements: vec![other, n],
    });
    let input = body.add(reference(source), |id| AssignmentKind::ProductNew {
        destination: id,
        representation: source,
        arguments: vec![text, input_array, n],
    });
    let mut packed = body.add(reference(storage.representation()), |id| {
        AssignmentKind::AggregateConvert {
            destination: id,
            value: input,
            conversion: AggregateConvert {
                source: reference(source),
                destination: reference(storage.representation()),
                plan: ValueConversion::RecordPack {
                    source,
                    target: storage.representation(),
                    labels: input_labels,
                    fields: erase_fields,
                },
            },
        }
    });
    if short_payload {
        let keys = body.add(reference(storage.labels()), |id| {
            AssignmentKind::ProductGet {
                destination: id,
                representation: storage.representation(),
                field: 0,
                value: packed,
            }
        });
        let empty = body.add(reference(storage.values()), |id| AssignmentKind::ArrayNew {
            destination: id,
            representation: storage.values(),
            elements: Vec::new(),
        });
        packed = body.add(reference(storage.representation()), |id| {
            AssignmentKind::ProductNew {
                destination: id,
                representation: storage.representation(),
                arguments: vec![keys, empty],
            }
        });
    }
    if let Some(update_label) = update_label {
        // The replacement payload erases through the same owner policy as the
        // pack step before entering the private labelled slot.
        let replacement = body.add(ValueShape::Integer, |_| AssignmentKind::Constant(77));
        let replacement_erased = crate::cc::payload::erased_shape();
        let erase_replacement = ValueConversion::Sequence(vec![
            ValueConversion::BoxScalar {
                kind: BoxKind::Integer,
                representation: boxed,
            },
            ValueConversion::EraseReference,
        ]);
        let erased_value = body.add(replacement_erased, |id| AssignmentKind::AggregateConvert {
            destination: id,
            value: replacement,
            conversion: AggregateConvert {
                source: ValueShape::Integer,
                destination: replacement_erased,
                plan: erase_replacement,
            },
        });
        let label = body.add(ValueShape::String, |_| {
            AssignmentKind::StringConstant(update_label.into())
        });
        packed = body.add(reference(storage.representation()), |id| {
            AssignmentKind::Record {
                destination: id,
                record: packed,
                operation: RecordOperation::Update {
                    replacements: vec![(label, erased_value)],
                },
            }
        });
    }
    let output = body.add(reference(target), |id| AssignmentKind::AggregateConvert {
        destination: id,
        value: packed,
        conversion: AggregateConvert {
            source: reference(storage.representation()),
            destination: reference(target),
            plan: ValueConversion::RecordUnpack {
                source: storage.representation(),
                target,
                labels,
                fields: recover_fields,
            },
        },
    });
    let output_text = body.add(ValueShape::String, |id| AssignmentKind::ProductGet {
        destination: id,
        representation: target,
        field: 0,
        value: output,
    });
    let output_array = body.add(reference(array), |id| AssignmentKind::ProductGet {
        destination: id,
        representation: target,
        field: 1,
        value: output,
    });
    let output_number = body.add(ValueShape::Integer, |id| AssignmentKind::ProductGet {
        destination: id,
        representation: target,
        field: 2,
        value: output,
    });
    let equal = body.add(ValueShape::Boolean, |_| AssignmentKind::Primitive {
        op: crate::cc::BinaryOp::StringEq,
        left: text,
        right: output_text,
    });
    let text_score = body.add(ValueShape::Integer, |_| AssignmentKind::Unary {
        op: crate::cc::UnaryOp::BooleanToInt,
        value: equal,
    });
    let index = body.add(ValueShape::Integer, |_| AssignmentKind::Constant(1));
    let array_number = body.add(ValueShape::Integer, |id| AssignmentKind::ArrayGet {
        destination: id,
        representation: array,
        value: output_array,
        index,
    });
    let sum = body.add(ValueShape::Integer, |_| AssignmentKind::Primitive {
        op: crate::cc::BinaryOp::IntAdd,
        left: output_number,
        right: array_number,
    });
    let result = body.add(ValueShape::Integer, |_| AssignmentKind::Primitive {
        op: crate::cc::BinaryOp::IntAdd,
        left: sum,
        right: text_score,
    });
    let symbol = SymbolId::new(ModuleId(0), 0);
    CcModule {
        name: "RecordStoreRoundtrip".into(),
        externals: Vec::new(),
        representations: table,
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: body.values,
            assignments: body.assignments,
            result,
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    }
}

#[test]
fn record_store_roundtrip_preserves_strings_and_nested_array_maps() {
    let cc = roundtrip(false, false, None);
    crate::cc::verify_module(&cc).expect("checked pack/unpack CC");
    let (mir, _) = super::super::lower_module(cc).expect("record conversions lower to MIR");
    super::super::verify_module(&mir).expect("record MIR physical contracts");
    run_gc(&mir, 85);
}

#[test]
fn record_update_replaces_one_label_in_fresh_storage_and_keeps_the_rest() {
    let cc = roundtrip(false, false, Some("value"));
    crate::cc::verify_module(&cc).expect("checked record update CC");
    let (mir, _) = super::super::lower_module(cc).expect("record update lowers to MIR");
    super::super::verify_module(&mir).expect("record update MIR physical contracts");
    // The text (score 1) and the list element (42) survive the update; the
    // replaced `value` field reads as 77: 1 + 42 + 77.
    run_gc(&mir, 120);
}

#[test]
fn record_update_traps_on_a_label_missing_from_the_runtime_store() {
    let cc = roundtrip(false, false, Some("absent"));
    crate::cc::verify_module(&cc).expect("checked record update CC");
    let (mir, _) = super::super::lower_module(cc).expect("checked record update MIR");
    if let Some(output) = run_gc_output(&mir) {
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("wasm trap: wasm `unreachable` instruction executed"),
            "{output:?}"
        );
    }
}

fn update_replacements(module: &mut CcModule) -> &mut (ValueId, ValueId) {
    module.functions[0]
        .assignments
        .iter_mut()
        .find_map(|assignment| match &mut assignment.kind {
            AssignmentKind::Record {
                operation: RecordOperation::Update { replacements },
                ..
            } => replacements.first_mut(),
            _ => None,
        })
        .expect("record update fixture")
}

#[test]
fn forged_record_update_operands_fail_at_the_public_cc_boundary() {
    let baseline = roundtrip(false, false, Some("value"));
    crate::cc::verify_module(&baseline).unwrap();
    let mutations: &[fn(&mut (ValueId, ValueId))] = &[
        |pair| std::mem::swap(&mut pair.0, &mut pair.1),
        |pair| pair.1 = pair.0,
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut module = baseline.clone();
        mutate(update_replacements(&mut module));
        assert!(
            crate::cc::verify_module(&module).is_err(),
            "forged record update {index} accepted"
        );
    }
}

#[test]
fn record_store_recovery_rejects_wrong_labels_and_payload_lengths() {
    for (wrong_label, short_payload) in [(true, false), (false, true)] {
        let cc = roundtrip(wrong_label, short_payload, None);
        crate::cc::verify_module(&cc).expect("physical inputs remain valid CC");
        let (mir, _) = super::super::lower_module(cc).expect("checked record recovery MIR");
        if let Some(output) = run_gc_output(&mir) {
            assert!(!output.status.success(), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("wasm trap: wasm `unreachable` instruction executed"),
                "{output:?}"
            );
        }
    }
}

fn record_plan(module: &mut CcModule, packing: bool) -> &mut ValueConversion {
    module.functions[0]
        .assignments
        .iter_mut()
        .find_map(|assignment| {
            if let AssignmentKind::AggregateConvert { conversion, .. } = &mut assignment.kind
                && matches!(
                    (&conversion.plan, packing),
                    (ValueConversion::RecordPack { .. }, true)
                        | (ValueConversion::RecordUnpack { .. }, false)
                )
            {
                Some(&mut conversion.plan)
            } else {
                None
            }
        })
        .expect("record conversion fixture")
}

#[test]
fn forged_record_conversion_plans_fail_at_the_public_cc_boundary() {
    let baseline = roundtrip(false, false, Some("value"));
    crate::cc::verify_module(&baseline).unwrap();
    let mutations: &[fn(&mut CcModule)] = &[
        |module| {
            if let ValueConversion::RecordPack { labels, .. } = record_plan(module, true) {
                labels.swap(0, 1);
            }
        },
        |module| {
            if let ValueConversion::RecordPack { fields, .. } = record_plan(module, true) {
                fields.pop();
            }
        },
        |module| {
            if let ValueConversion::RecordPack { fields, .. } = record_plan(module, true) {
                fields[0] = ValueConversion::Identity;
            }
        },
        |module| {
            if let ValueConversion::RecordUnpack { fields, .. } = record_plan(module, false) {
                fields[2] = ValueConversion::Identity;
            }
        },
        |module| {
            if let ValueConversion::RecordUnpack { source, target, .. } = record_plan(module, false)
            {
                std::mem::swap(source, target);
            }
        },
        |module| {
            module.representations = RepresentationTable::from_parts(
                module.representations.representations.clone(),
                module.representations.signatures.clone(),
                module.representations.product_labels.clone(),
            );
        },
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut module = baseline.clone();
        mutate(&mut module);
        assert!(
            crate::cc::verify_module(&module).is_err(),
            "forged plan {index} accepted"
        );
    }
}
