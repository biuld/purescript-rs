use super::*;
use crate::cc::{
    AggregateConvert, External, RecordOperation, Reference, Representation, Signature,
    ValueConversion, ValueDecl,
};
use crate::types::{CompositeType, HeapType, RefType, StorageType, ValueType};
use crate::{ExternalBindings, RuntimeBinding};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId};

fn reference(representation: crate::cc::ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: crate::cc::RefShape::Repr(representation),
    })
}

fn build_module() -> (CcModule, ExternalBindings, [SymbolId; 2]) {
    let mut representations = RepresentationTable::default();
    let input_array = representations.reserve();
    representations.set(
        input_array,
        Representation::Array {
            element: crate::cc::payload::erased_shape(),
        },
    );
    let storage = representations.ensure_record_storage();
    let build = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let length = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE + 1);
    let span = span();
    let module = CcModule {
        name: "CheckedRecordBuild".into(),
        externals: vec![
            External {
                projection: None,
                symbol: build,
                signature: Some(Signature {
                    parameters: vec![reference(input_array), reference(input_array)],
                    result: reference(storage.representation()),
                }),
            },
            External {
                projection: None,
                symbol: length,
                signature: Some(Signature {
                    parameters: vec![reference(storage.representation())],
                    result: ValueShape::Integer,
                }),
            },
        ],
        representations,
        functions: vec![CcFunction {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueShape::String,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: crate::cc::payload::erased_shape(),
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: reference(input_array),
                },
                ValueDecl {
                    id: ValueId(3),
                    ty: reference(input_array),
                },
                ValueDecl {
                    id: ValueId(4),
                    ty: reference(storage.representation()),
                },
                ValueDecl {
                    id: ValueId(5),
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::StringConstant("a".into()),
                    span,
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::AggregateConvert {
                        destination: ValueId(1),
                        value: ValueId(0),
                        conversion: AggregateConvert {
                            source: ValueShape::String,
                            destination: crate::cc::payload::erased_shape(),
                            plan: ValueConversion::EraseReference,
                        },
                    },
                    span,
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(2),
                        representation: input_array,
                        elements: vec![ValueId(1)],
                    },
                    span,
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(3),
                        representation: input_array,
                        elements: vec![ValueId(1)],
                    },
                    span,
                },
                Assignment {
                    destination: ValueId(4),
                    kind: AssignmentKind::RecordPrimitive {
                        provider: build,
                        record: None,
                        operation: RecordOperation::Build {
                            labels: ValueId(2),
                            values: ValueId(3),
                        },
                    },
                    span,
                },
                Assignment {
                    destination: ValueId(5),
                    kind: AssignmentKind::RecordPrimitive {
                        provider: length,
                        record: Some(ValueId(4)),
                        operation: RecordOperation::Length,
                    },
                    span,
                },
            ],
            result: ValueId(5),
            result_type: ValueShape::Integer,
            span,
        }],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span,
    };
    let bindings = ExternalBindings {
        imports: Vec::new(),
        runtime: [(build, "build"), (length, "length")]
            .into_iter()
            .map(|(symbol, function)| RuntimeBinding {
                symbol,
                source_module: ModuleId(0),
                module: psrs_runtime::RECORD_STORAGE_MODULE.into(),
                function: function.into(),
                type_id: None,
                span,
            })
            .collect(),
    };
    (module, bindings, [build, length])
}

fn lower_build() -> crate::mir::Module {
    let (module, bindings, _) = build_module();
    crate::cc::verify_module(&module).expect("checked provider-backed Build CC");
    crate::mir::lower_module_with_bindings(module, bindings, crate::TargetCapabilities::default())
        .expect("checked Build provider lowers through the record owner")
        .0
}

fn assert_p9_rejection(module: CcModule, bindings: ExternalBindings, pass: &str, message: &str) {
    crate::cc::verify_module(&module).expect("malformed provider cases retain valid CC shape");
    let errors = match crate::mir::lower_module_with_bindings(
        module,
        bindings,
        crate::TargetCapabilities::default(),
    ) {
        Err(errors) => errors,
        Ok(_) => panic!("malformed provider metadata must be rejected by P9"),
    };
    assert!(
        errors
            .iter()
            .any(|error| error.pass == pass && error.message == message),
        "expected {pass:?} rejection {message:?}, got {errors:#?}"
    );
}

#[test]
fn p9_record_provider_valid_base_passes_cc_and_p9() {
    let (module, bindings, _) = build_module();
    crate::cc::verify_module(&module).expect("valid provider-backed Build CC");
    crate::mir::lower_module_with_bindings(module, bindings, crate::TargetCapabilities::default())
        .expect("valid provider bindings pass P9");
}

#[test]
fn p9_record_provider_rejects_another_known_provider_symbol() {
    let (mut module, bindings, [_, length]) = build_module();
    let AssignmentKind::RecordPrimitive { provider, .. } =
        &mut module.functions[0].assignments[4].kind
    else {
        panic!("fixture Build assignment retains its provider witness");
    };
    *provider = length;

    assert_p9_rejection(
        module,
        bindings,
        "P9 runtime projection",
        "CC record operation disagrees with its checked provider export",
    );
}

#[test]
fn p9_record_provider_rejects_a_missing_runtime_binding() {
    let (module, mut bindings, [build, _]) = build_module();
    bindings.runtime.retain(|binding| binding.symbol != build);

    assert_p9_rejection(
        module,
        bindings,
        "P9 external binding validation",
        &format!("CC external {build:?} has no binding"),
    );
}

// P9's entry point delegates export signature checking to the P8 runtime ABI owner.
#[test]
fn p9_record_provider_rejects_an_export_for_a_different_operation() {
    let (module, mut bindings, [build, _]) = build_module();
    let binding = bindings
        .runtime
        .iter_mut()
        .find(|binding| binding.symbol == build)
        .expect("Build provider binding");
    binding.function = "length".into();

    assert_p9_rejection(
        module,
        bindings,
        "P8 runtime ABI",
        "record primitive CC signature has wrong arity",
    );
}

#[test]
fn p9_record_provider_rejects_direct_call_bypass() {
    let (mut module, bindings, [build, _]) = build_module();
    module.functions[0].assignments[4].kind = AssignmentKind::DirectCall {
        contract: None,
        function: build,
        arguments: vec![ValueId(2), ValueId(3)],
    };

    assert_p9_rejection(
        module,
        bindings,
        "P9 runtime projection",
        "record provider bypasses its checked CC operation",
    );
}

// P9's entry point delegates canonical CC signature checks to the P8 runtime ABI owner.
#[test]
fn p9_record_provider_rejects_a_noncanonical_external_signature() {
    let (mut module, bindings, [build, _]) = build_module();
    module
        .externals
        .iter_mut()
        .find(|external| external.symbol == build)
        .and_then(|external| external.signature.as_mut())
        .expect("Build external signature")
        .result = ValueShape::Integer;

    assert_p9_rejection(
        module,
        bindings,
        "P8 runtime ABI",
        "record operand has no canonical record shape",
    );
}

#[test]
fn direct_record_build_executes_through_the_shared_owner() {
    let mir = lower_build();
    run_gc(&mir, 1);
}

#[test]
fn direct_record_build_traps_when_either_input_array_contains_null() {
    for null_array in 0..2 {
        let mut mir = lower_build();
        let candidate = mir.types.iter().flat_map(|group| &group.0).find_map(|ty| {
            let CompositeType::Array(field) = &ty.composite else {
                return None;
            };
            match field.storage {
                StorageType::Ref(reference)
                    if reference.nullable && reference.heap == HeapType::Eq =>
                {
                    Some(*field)
                }
                _ => None,
            }
        });
        assert!(candidate.is_some(), "the Build inputs use erased arrays");
        let function = &mut mir.functions[0];
        let (block, instruction, span) = function
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(block, body)| {
                body.instructions
                    .iter()
                    .enumerate()
                    .filter_map(move |(instruction, value)| match value {
                        Instruction::ArrayNew { elements, span, .. } if elements.len() == 1 => {
                            Some((block, instruction, *span))
                        }
                        _ => None,
                    })
            })
            .nth(null_array)
            .expect("the fixture has label and payload arrays");
        let null = ValueId(
            function
                .values
                .iter()
                .map(|value| value.id.0)
                .max()
                .unwrap()
                + 1,
        );
        function.values.push(crate::types::ValueDecl {
            id: null,
            ty: ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }),
        });
        let body = &mut function.blocks[block];
        let Instruction::ArrayNew { elements, .. } = &mut body.instructions[instruction] else {
            unreachable!("selected array creation")
        };
        elements[0] = null;
        body.instructions.insert(
            instruction,
            Instruction::RefNull {
                destination: null,
                heap: HeapType::Eq,
                span,
            },
        );
        crate::mir::verify_module(&mir).expect("nullable null elements are valid erased arrays");
        if let Some(output) = run_gc_output(&mir) {
            assert!(!output.status.success(), "{output:?}");
            let expected = if null_array == 0 {
                "cast failure"
            } else {
                "unreachable"
            };
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(expected),
                "{output:?}"
            );
        }
    }
}
