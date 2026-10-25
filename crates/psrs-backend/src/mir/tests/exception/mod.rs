//! MIR exception-channel tests (EH-05..EH-11): tag table, `Try`/`Throw`
//! terminators, unwind-edge verification, protected-region tail-call rules,
//! capability inference, the P9 discharge of the `psrs:runtime-exception`
//! provider, the structurer's try unit, and the dispatcher rejection
//! boundary. Fixtures are hand-built direct MIR (or direct CC for the
//! discharge) executed under wasmtime where the row requires it.

mod budget;
mod capability;
mod catch_discharge;
mod discharge;
mod dispatch_boundary;
mod execution;
mod protected_recursion;
mod references;
mod regions;
mod source_raise;
mod structuring;
mod tail;
mod throw_destination;
mod unwind;

use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, Tag, TagId, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

pub(super) fn at(offset: u32) -> TextRange {
    TextRange::new(offset, offset + 1)
}

/// The explicitly exceptions-enabled target the EH-09..EH-11 fixtures lower
/// with; the stable default profile keeps `exceptions` disabled.
pub(super) fn enabled_target() -> crate::TargetCapabilities {
    crate::TargetCapabilities {
        exceptions: true,
        ..crate::TargetCapabilities::default()
    }
}

pub(super) fn tag(id: u32, payload: Vec<ValueType>) -> Tag {
    Tag {
        name: format!("psrs:test-{id}"),
        payload,
        span: at(800 + id),
    }
}

fn declarations(types: &[(u32, ValueType)]) -> Vec<ValueDecl> {
    types
        .iter()
        .map(|(id, ty)| ValueDecl {
            id: ValueId(*id),
            ty: *ty,
        })
        .collect()
}

pub(super) fn block(
    id: u32,
    parameters: Vec<u32>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        parameters: parameters.into_iter().map(ValueId).collect(),
        instructions,
        terminator: Some(terminator),
    }
}

/// A fixture whose single `i32` function parameter is the result, so every
/// block can return it.
pub(super) fn simple_function(
    blocks: Vec<BasicBlock>,
    extra_values: Vec<(u32, ValueType)>,
) -> Function {
    let mut values = vec![(0u32, ValueType::I32)];
    values.extend(extra_values);
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "try_fixture".into(),
        parameters: vec![ValueId(0)],
        values: declarations(&values),
        entry: BlockId(0),
        blocks,
        result: ValueId(0),
        result_type: ValueType::I32,
        frame_weight: 0,
        span: at(900),
    }
}

pub(super) fn module(function: Function, tags: Vec<Tag>) -> Module {
    let entry = function.symbol;
    Module {
        name: "ExceptionTest".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags,
        layout: None,
        imports: Vec::new(),
        functions: vec![function],
        entry: Some(entry),
        span: at(950),
    }
}

pub(super) fn tag_only_module(tags: Vec<Tag>) -> Module {
    Module {
        name: "ExceptionTest".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags,
        layout: None,
        imports: Vec::new(),
        functions: Vec::new(),
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: at(950),
    }
}

pub(super) fn terminator(block: &BasicBlock) -> &Terminator {
    block
        .terminator
        .as_ref()
        .expect("every fixture block terminates")
}

use crate::mir::verify_module;

// ---------------------------------------------------------------- EH-05

#[test]
fn try_and_throw_terminators_expose_the_documented_fields() {
    let module = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        continuation: None,
                        clauses: vec![crate::mir::CatchClause::Catch {
                            tag: TagId(0),
                            target: BlockId(2),
                        }],
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![],
                        handler: Some(BlockId(2)),
                        span: at(11),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(12),
                    },
                ),
            ],
            Vec::new(),
        ),
        vec![tag(0, Vec::new())],
    );
    verify_module(&module).expect("a well-formed try/throw module verifies");

    let function = &module.functions[0];
    let Terminator::Try {
        protected,
        clauses,
        span,
        ..
    } = terminator(&function.blocks[0])
    else {
        panic!("the entry terminator is a Try");
    };
    assert_eq!(*protected, BlockId(1));
    assert_eq!(
        *clauses,
        vec![crate::mir::CatchClause::Catch {
            tag: TagId(0),
            target: BlockId(2)
        }]
    );
    assert_eq!(*span, at(10));

    let Terminator::Throw {
        tag,
        arguments,
        handler,
        span,
    } = terminator(&function.blocks[1])
    else {
        panic!("the protected terminator is a Throw");
    };
    assert_eq!(*tag, TagId(0));
    assert_eq!(*arguments, Vec::<ValueId>::new());
    assert_eq!(*handler, Some(BlockId(2)));
    assert_eq!(*span, at(11));

    assert_eq!(module.tags.len(), 1);
    assert_eq!(module.tags[0].name, "psrs:test-0");
    assert_eq!(module.tags[0].payload, Vec::<ValueType>::new());
    assert_eq!(module.tags[0].span, at(800));
}

#[test]
fn rejects_a_throw_with_an_unknown_tag() {
    let module = module(
        simple_function(
            vec![block(
                0,
                vec![],
                vec![],
                Terminator::Throw {
                    tag: TagId(7),
                    arguments: vec![],
                    handler: None,
                    span: at(10),
                },
            )],
            Vec::new(),
        ),
        Vec::new(),
    );
    let errors = verify_module(&module).expect_err("an unknown throw tag is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unknown tag") && error.span == at(10)),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_throw_payload_that_differs_from_its_tag() {
    // Arity: the tag carries no payload, the throw passes one argument.
    let arity = module(
        simple_function(
            vec![block(
                0,
                vec![],
                vec![],
                Terminator::Throw {
                    tag: TagId(0),
                    arguments: vec![ValueId(0)],
                    handler: None,
                    span: at(10),
                },
            )],
            Vec::new(),
        ),
        vec![tag(0, Vec::new())],
    );
    let errors = verify_module(&arity).expect_err("a payload arity mismatch is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("payload arity") && error.span == at(10)),
        "{errors:?}"
    );

    // Type: the tag carries an i64 payload, the throw passes an i32 value.
    let mismatched = module(
        simple_function(
            vec![block(
                0,
                vec![],
                vec![],
                Terminator::Throw {
                    tag: TagId(0),
                    arguments: vec![ValueId(0)],
                    handler: None,
                    span: at(11),
                },
            )],
            Vec::new(),
        ),
        vec![tag(0, vec![ValueType::I64])],
    );
    let errors = verify_module(&mismatched).expect_err("a payload type mismatch is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("payload has the wrong type")
                && error.span == at(11)),
        "{errors:?}"
    );

    // A payload agreement is accepted.
    let agreeing = module(
        simple_function(
            vec![block(
                0,
                vec![],
                vec![],
                Terminator::Throw {
                    tag: TagId(0),
                    arguments: vec![ValueId(0)],
                    handler: None,
                    span: at(12),
                },
            )],
            Vec::new(),
        ),
        vec![tag(0, vec![ValueType::I32])],
    );
    verify_module(&agreeing).expect("an agreeing payload verifies");
}

#[test]
fn rejects_a_multi_value_tag_payload() {
    let module = tag_only_module(vec![tag(0, vec![ValueType::I32, ValueType::I32])]);
    let errors = verify_module(&module).expect_err("a multi-value payload is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("more than one value") && error.span == at(800)),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_try_naming_a_missing_block() {
    let missing_entry = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(9),
                        continuation: None,
                        clauses: vec![crate::mir::CatchClause::CatchAll { target: BlockId(1) }],
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(11),
                    },
                ),
            ],
            Vec::new(),
        ),
        Vec::new(),
    );
    let errors = verify_module(&missing_entry).expect_err("a missing protected entry is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("protected entry block does not exist")
                && error.span == at(10)
        }),
        "{errors:?}"
    );

    let missing_handler = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        continuation: None,
                        clauses: vec![crate::mir::CatchClause::CatchAll { target: BlockId(9) }],
                        span: at(12),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(13),
                    },
                ),
            ],
            Vec::new(),
        ),
        Vec::new(),
    );
    let errors = verify_module(&missing_handler).expect_err("a missing handler is rejected");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("handler block does not exist") && error.span == at(12)
        }),
        "{errors:?}"
    );
}
