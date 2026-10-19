//! Opaque transport, nested cleanup and original tagged-payload propagation.
use super::structuring::{constant, enabled_target, function};
use super::*;
use crate::mir::{CatchClause, verify_module_with_capabilities};
use crate::types::{HeapType, RefType};

fn reference_module() -> crate::mir::Module {
    module(
        function(
            vec![
                block(
                    0,
                    vec![],
                    vec![constant(0, 37, at(0))],
                    Terminator::Try {
                        protected: BlockId(1),
                        continuation: None,
                        clauses: vec![CatchClause::Catch {
                            tag: TagId(0),
                            target: BlockId(4),
                        }],
                        span: at(1),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(2),
                        continuation: None,
                        clauses: vec![CatchClause::CatchAllRef { target: BlockId(3) }],
                        span: at(2),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![ValueId(0)],
                        handler: Some(BlockId(3)),
                        span: at(3),
                    },
                ),
                block(
                    3,
                    vec![2],
                    vec![],
                    Terminator::ThrowRef {
                        exception: ValueId(2),
                        span: at(4),
                    },
                ),
                block(
                    4,
                    vec![1],
                    vec![],
                    Terminator::Return {
                        value: ValueId(1),
                        span: at(5),
                    },
                ),
            ],
            &[
                (0, ValueType::I32),
                (1, ValueType::I32),
                (
                    2,
                    ValueType::Ref(RefType {
                        nullable: false,
                        heap: HeapType::Exn,
                    }),
                ),
            ],
            vec![],
            1,
        ),
        vec![tag(0, vec![ValueType::I32])],
    )
}

#[test]
fn propagates_original_tag_and_payload_through_nested_reference_catch() {
    let mir = reference_module();
    verify_module_with_capabilities(&mir, enabled_target()).expect("nested reference transport");
    let mir =
        crate::mir::opt::optimize(mir, enabled_target()).expect("optimization preserves transport");
    let Some(output) = structuring::run(&mir) else {
        return;
    };
    assert_eq!(output.status.code(), Some(37), "{:?}", output);
}

#[test]
fn forwarding_preserves_the_identity_of_a_mutable_reference_payload() {
    use crate::types::{
        CompositeType, DefinedType, DefinedTypeId, FieldType, RecGroup, StorageType,
    };
    let mut mir = reference_module();
    let array = DefinedTypeId(0);
    let reference = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(array),
    });
    mir.types.push(RecGroup(vec![DefinedType {
        final_type: true,
        supertype: None,
        composite: CompositeType::Array(FieldType {
            storage: StorageType::I32,
            mutable: true,
        }),
    }]));
    mir.tags[0].payload = vec![reference];
    let f = &mut mir.functions[0];
    f.values[0].ty = reference;
    f.values[1].ty = reference;
    f.values.extend(declarations(&[
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
    ]));
    f.blocks[0].instructions = vec![
        constant(3, 0, at(0)),
        constant(4, 42, at(0)),
        Instruction::ArrayNew {
            destination: ValueId(0),
            type_index: array,
            elements: vec![ValueId(3)],
            span: at(0),
        },
    ];
    f.blocks[4].instructions = vec![
        Instruction::ArraySet {
            type_index: array,
            value: ValueId(1),
            index: ValueId(3),
            new_value: ValueId(4),
            span: at(5),
        },
        Instruction::ArrayGet {
            destination: ValueId(5),
            type_index: array,
            value: ValueId(0),
            index: ValueId(3),
            span: at(5),
        },
    ];
    f.blocks[4].terminator = Some(Terminator::Return {
        value: ValueId(5),
        span: at(5),
    });
    f.result = ValueId(5);
    let mir = crate::mir::opt::optimize(mir, enabled_target())
        .expect("reference payload verifies and optimizes");
    let Some(output) = structuring::run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "the caught payload aliases its original array: {output:?}"
    );
}

#[test]
fn reference_throw_edges_include_outer_handlers_and_caller_propagation() {
    let mut mir = reference_module();
    let graph = crate::mir::unwind::UnwindGraph::new(&mir.functions[0], "test").unwrap();
    assert_eq!(graph.successors(BlockId(3)), &[BlockId(4)]);
    assert!(
        graph.escapes(BlockId(3)),
        "other unknown tags can escape the named outer catch"
    );
    let Some(Terminator::Try { clauses, .. }) = &mut mir.functions[0].blocks[0].terminator else {
        unreachable!()
    };
    *clauses = vec![CatchClause::CatchAllRef { target: BlockId(4) }];
    let graph = crate::mir::unwind::UnwindGraph::new(&mir.functions[0], "test").unwrap();
    assert_eq!(graph.successors(BlockId(3)), &[BlockId(4)]);
    assert!(!graph.escapes(BlockId(3)));
}

#[test]
fn rejects_nullable_or_aggregate_exception_operands() {
    for reference in [
        RefType {
            nullable: true,
            heap: HeapType::Exn,
        },
        RefType {
            nullable: false,
            heap: HeapType::Any,
        },
    ] {
        let mut mir = reference_module();
        mir.functions[0].values[2].ty = ValueType::Ref(reference);
        assert!(verify_module(&mir).is_err());
    }
}

#[test]
fn exception_types_and_reference_throws_require_the_capability() {
    let target = crate::TargetCapabilities {
        exceptions: false,
        ..enabled_target()
    };
    let errors = verify_module_with_capabilities(&reference_module(), target).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("exception-handling"))
    );
}

#[test]
fn rejects_shadowed_catches_and_wrong_reference_handler_arity() {
    let mut mir = reference_module();
    let Some(Terminator::Try { clauses, .. }) = &mut mir.functions[0].blocks[1].terminator else {
        unreachable!()
    };
    clauses.push(CatchClause::Catch {
        tag: TagId(0),
        target: BlockId(4),
    });
    assert!(verify_module(&mir).is_err());
    let mut mir = reference_module();
    mir.functions[0].blocks[3].parameters.clear();
    assert!(verify_module(&mir).is_err());
}

#[test]
fn an_exception_reference_type_alone_requires_the_capability() {
    let mut bare = reference_module();
    bare.tags.clear();
    bare.functions[0].parameters = vec![ValueId(2)];
    bare.functions[0].blocks = vec![block(
        0,
        vec![],
        vec![constant(0, 42, at(0))],
        Terminator::Return {
            value: ValueId(0),
            span: at(0),
        },
    )];
    bare.functions[0].result = ValueId(0);
    let target = crate::TargetCapabilities {
        exceptions: false,
        ..enabled_target()
    };
    let errors = verify_module_with_capabilities(&bare, target).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("exception-handling")),
        "the type alone requires exceptions: {errors:?}"
    );
}

#[test]
fn near_limit_prologue_throw_can_restore_and_clean_up_without_another_call() {
    use crate::types::{
        CompositeType, DefinedType, DefinedTypeId, FieldType, RecGroup, StorageType,
    };
    let array = DefinedTypeId(0);
    let reference = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(array),
    });
    let token = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Exn,
    });
    let mut main = function(
        vec![
            block(
                0,
                vec![],
                vec![
                    constant(0, 0, at(0)),
                    constant(1, 42, at(0)),
                    Instruction::ArrayNew {
                        destination: ValueId(2),
                        type_index: array,
                        elements: vec![ValueId(0)],
                        span: at(0),
                    },
                ],
                Terminator::Try {
                    protected: BlockId(1),
                    continuation: None,
                    clauses: vec![CatchClause::CatchAllRef { target: BlockId(2) }],
                    span: at(0),
                },
            ),
            block(
                1,
                vec![],
                vec![Instruction::Call {
                    destination: ValueId(4),
                    function: SymbolId::new(ModuleId(0), 1),
                    contract: None,
                    arguments: vec![],
                    span: at(1),
                }],
                Terminator::Return {
                    value: ValueId(4),
                    span: at(1),
                },
            ),
            block(
                2,
                vec![3],
                vec![
                    Instruction::ArraySet {
                        type_index: array,
                        value: ValueId(2),
                        index: ValueId(0),
                        new_value: ValueId(1),
                        span: at(2),
                    },
                    Instruction::ArrayGet {
                        destination: ValueId(5),
                        type_index: array,
                        value: ValueId(2),
                        index: ValueId(0),
                        span: at(2),
                    },
                ],
                Terminator::Return {
                    value: ValueId(5),
                    span: at(2),
                },
            ),
        ],
        &[
            (0, ValueType::I32),
            (1, ValueType::I32),
            (2, reference),
            (3, token),
            (4, ValueType::I32),
            (5, ValueType::I32),
        ],
        vec![],
        5,
    );
    // Deliberately model a nearly full native prefix without allocating a real
    // deep stack. A second instrumented call in cleanup would exhaust again.
    main.frame_weight = crate::mir::budget::STACK_BUDGET as u32 - 16;
    let mut leaf = function(
        vec![block(
            0,
            vec![],
            vec![constant(0, 0, at(3))],
            Terminator::Return {
                value: ValueId(0),
                span: at(3),
            },
        )],
        &[(0, ValueType::I32)],
        vec![],
        0,
    );
    leaf.id = FunctionId(1);
    leaf.symbol = SymbolId::new(ModuleId(0), 1);
    leaf.name = "overflowing_prologue".into();
    leaf.frame_weight = 32;
    let mut mir = module(main, vec![]);
    mir.functions.push(leaf);
    mir.types.push(RecGroup(vec![DefinedType {
        final_type: true,
        supertype: None,
        composite: CompositeType::Array(FieldType {
            storage: StorageType::I32,
            mutable: true,
        }),
    }]));
    verify_module(&mir).expect("near-limit cleanup fixture verifies");
    let Some(output) = structuring::run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "cleanup after the prologue throw must return normally: {output:?}"
    );
}

#[test]
fn rejects_a_missing_or_internal_normal_continuation() {
    for exit in [BlockId(99), BlockId(0), BlockId(1), BlockId(4)] {
        let mut mir = reference_module();
        let Some(Terminator::Try { continuation, .. }) = &mut mir.functions[0].blocks[0].terminator
        else {
            unreachable!()
        };
        *continuation = Some(exit);
        assert!(
            verify_module(&mir).is_err(),
            "invalid normal continuation {exit:?}"
        );
    }
}
