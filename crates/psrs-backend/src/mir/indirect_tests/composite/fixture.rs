use crate::abi::{SourceSignature, SourceType};
use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;
use wit_parser::Resolve;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn product_shape(representation: u32) -> ValueShape {
    ValueShape::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Repr(cc::ReprId(representation)),
    })
}

pub(super) fn composite_indirect_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let scalar_parameters = (0..14)
        .map(|index| format!("a{index}: s32"))
        .collect::<Vec<_>>()
        .join(", ");
    let wit = format!(
        "package wasi:io@0.2.12; interface streams {{ \
         flags access {{ write, read, audit, execute, debug }} \
         enum tone {{ red, green, blue }} \
         record details {{ text: list<u8>, state: tone }} \
         record payload {{ access: access, details: details, shade: tone }} \
         take-shapes: func({scalar_parameters}, payload: payload); }}"
    );
    let mut resolve = Resolve::default();
    resolve
        .push_str("indirect-composite.wit", &wit)
        .expect("the composite indirect WIT fixture should resolve");

    let cases = vec!["Red".into(), "Green".into(), "Blue".into()];
    let flag_source = SourceType::Record {
        fields: vec![
            ("audit".into(), Box::new(SourceType::Boolean)),
            ("debug".into(), Box::new(SourceType::Boolean)),
            ("execute".into(), Box::new(SourceType::Boolean)),
            ("read".into(), Box::new(SourceType::Boolean)),
            ("write".into(), Box::new(SourceType::Boolean)),
        ],
    };
    let details_source = SourceType::Record {
        fields: vec![
            (
                "state".into(),
                Box::new(SourceType::Enum {
                    cases: cases.clone(),
                }),
            ),
            ("text".into(), Box::new(SourceType::String)),
        ],
    };
    let payload_source = SourceType::Record {
        fields: vec![
            ("access".into(), Box::new(flag_source)),
            ("details".into(), Box::new(details_source)),
            (
                "shade".into(),
                Box::new(SourceType::Enum {
                    cases: cases.clone(),
                }),
            ),
        ],
    };

    let flags_shape = product_shape(0);
    let details_shape = product_shape(1);
    let payload_shape = product_shape(2);
    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Boolean; 5],
            },
            cc::Representation::Product {
                // Source record fields are sorted alphabetically. WIT declares
                // `text` before `state`, so lowering must project by field name.
                fields: vec![ValueShape::Integer, ValueShape::String],
            },
            cc::Representation::Product {
                fields: vec![flags_shape, details_shape, ValueShape::Integer],
            },
        ],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let mut values = Vec::new();
    let mut assignments = Vec::new();
    let mut call_arguments = Vec::new();
    let mut external_parameters = vec![ValueShape::Integer; 14];
    let mut source_parameters = vec![SourceType::Int; 14];
    for index in 0..14_u32 {
        let id = ValueId(index);
        values.push(ValueDecl {
            id,
            ty: ValueShape::Integer,
        });
        assignments.push(Assignment {
            destination: id,
            kind: AssignmentKind::Constant(1000 + index as i32),
            span: span(),
        });
        call_arguments.push(id);
    }

    let text = ValueId(14);
    values.push(ValueDecl {
        id: text,
        ty: ValueShape::String,
    });
    assignments.push(Assignment {
        destination: text,
        kind: AssignmentKind::StringConstant("fixture payload".into()),
        span: span(),
    });

    // Source record order is alphabetical; WIT flags order is
    // write/read/audit/execute/debug. These values therefore pack to 0b10110.
    let flag_bits = [1, 1, 0, 1, 0];
    let flag_values = flag_bits
        .into_iter()
        .enumerate()
        .map(|(index, bit)| {
            let id = ValueId(15 + index as u32);
            values.push(ValueDecl {
                id,
                ty: ValueShape::Boolean,
            });
            assignments.push(Assignment {
                destination: id,
                kind: AssignmentKind::Constant(bit),
                span: span(),
            });
            id
        })
        .collect::<Vec<_>>();

    let state = ValueId(20);
    values.push(ValueDecl {
        id: state,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: state,
        kind: AssignmentKind::Constant(2),
        span: span(),
    });

    let flags = ValueId(21);
    values.push(ValueDecl {
        id: flags,
        ty: flags_shape,
    });
    assignments.push(Assignment {
        destination: flags,
        kind: AssignmentKind::ProductNew {
            destination: flags,
            representation: cc::ReprId(0),
            arguments: flag_values,
        },
        span: span(),
    });

    let details = ValueId(22);
    values.push(ValueDecl {
        id: details,
        ty: details_shape,
    });
    assignments.push(Assignment {
        destination: details,
        kind: AssignmentKind::ProductNew {
            destination: details,
            representation: cc::ReprId(1),
            arguments: vec![state, text],
        },
        span: span(),
    });

    let shade = ValueId(23);
    values.push(ValueDecl {
        id: shade,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: shade,
        kind: AssignmentKind::Constant(1),
        span: span(),
    });

    let payload = ValueId(24);
    values.push(ValueDecl {
        id: payload,
        ty: payload_shape,
    });
    assignments.push(Assignment {
        destination: payload,
        kind: AssignmentKind::ProductNew {
            destination: payload,
            representation: cc::ReprId(2),
            arguments: vec![flags, details, shade],
        },
        span: span(),
    });
    call_arguments.push(payload);
    external_parameters.push(payload_shape);
    source_parameters.push(payload_source);

    let call_result = ValueId(25);
    values.push(ValueDecl {
        id: call_result,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: call_result,
        kind: AssignmentKind::DirectCall {
            function: external_symbol,
            arguments: call_arguments,
        },
        span: span(),
    });

    let module = cc::Module {
        name: "IndirectCompositeAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: external_parameters,
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
            result: call_result,
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
            function: "take-shapes".into(),
            signature: Some(SourceSignature {
                parameters: source_parameters,
                result: SourceType::Unit,
                span: span(),
            }),
            type_id: None,
        }],
    };
    (module, bindings, resolve)
}
