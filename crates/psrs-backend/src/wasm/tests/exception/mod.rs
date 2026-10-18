//! Exception-channel thin-IR evidence (EH-01..EH-03).
//!
//! The MIR lowering and structurer do not produce try regions yet (EH-05+),
//! so these tests construct the thin Wasm IR directly and check the verifier,
//! the tag section, and the profile gate. Execution fixtures live in the
//! driver's test suite (`psrs-driver/src/tests/exception_handling.rs`).

use super::*;
use crate::TargetCapabilities;
use wasmparser::{Payload, WasmFeatures};

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn function(name: &str, type_index: u32, body: Body) -> Function {
    Function {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: name.into(),
        type_index: super::super::TypeIndex(type_index),
        parameters: Vec::new(),
        locals: Vec::new(),
        body,
        span: span(),
    }
}

fn tag(type_index: u32) -> TagDef {
    TagDef {
        type_index: super::super::TypeIndex(type_index),
        span: span(),
    }
}

fn verify(module: &Module) -> Result<(), Vec<crate::BackendError>> {
    super::super::verify::verify_module(module)
}

fn exceptions_enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

/// A module-local tag with the given payload parameters, plus a `run`
/// function returning one `i32`, exported as `run`. Type 0 is the tag's
/// function type; type 1 is the exported function's.
fn tag_fixture(body: Body, tag_parameters: Vec<WasmValType>) -> Module {
    super::super::Module {
        name: "TagFixture".into(),
        imports: Vec::new(),
        types: vec![
            FuncType {
                parameters: tag_parameters,
                results: Vec::new(),
            },
            FuncType {
                parameters: Vec::new(),
                results: vec![WasmValType::I32],
            },
        ],
        type_defs: Vec::new(),
        functions: vec![function("run", 1, body)],
        memories: Vec::new(),
        tags: vec![tag(0)],
        globals: Vec::new(),
        data: Vec::new(),
        exports: vec![Export {
            name: "run".into(),
            kind: ExportKind::Function,
            index: super::super::ExportIndex::Function(super::super::FunctionIndex(0)),
        }],
        entry: None,
        realloc: None,
        helpers: Vec::new(),
        span: span(),
    }
}

/// Like [`tag_fixture`] but the exported function returns nothing, so the
/// active labels' result types differ from the payload tests.
fn void_tag_fixture(body: Body, tag_parameters: Vec<WasmValType>) -> Module {
    let mut module = tag_fixture(body, tag_parameters);
    module.types[1].results = Vec::new();
    module
}

/// `block (result i32) [ try_table (catch $t 0) [i32.const 7; throw $t] end;
/// unreachable ] end`: the catch clause's depth-0 label is the enclosing
/// result-bearing block, so the thrown payload becomes the function result.
fn catches_payload_body() -> Body {
    vec![Op::Block {
        body: vec![
            Op::Try {
                clauses: vec![CatchClause::Catch {
                    tag: TagIndex(0),
                    label: 0,
                }],
                body: vec![
                    Op::Leaf(Instruction::I32Const(7)),
                    Op::Leaf(Instruction::Throw(0)),
                ],
                span: span(),
            },
            Op::Leaf(Instruction::Unreachable),
        ],
        result: Some(WasmValType::I32),
        span: span(),
    }]
}

/// `block (result i32) [ block [] [ try_table (catch_all 0) [throw $t] end;
/// unreachable ] end; i32.const 42 ] end`.
fn catches_all_body() -> Body {
    vec![Op::Block {
        body: vec![
            Op::Block {
                body: vec![
                    Op::Try {
                        clauses: vec![CatchClause::CatchAll { label: 0 }],
                        body: vec![Op::Leaf(Instruction::Throw(0))],
                        span: span(),
                    },
                    Op::Leaf(Instruction::Unreachable),
                ],
                result: None,
                span: span(),
            },
            Op::Leaf(Instruction::I32Const(42)),
        ],
        result: Some(WasmValType::I32),
        span: span(),
    }]
}

/// EH-01: the verifier accepts a try region whose catch clauses resolve, and
/// the node's fields round-trip for inspection.
#[test]
fn accepts_try_regions_with_resolved_catch_clauses() {
    let payload_module = tag_fixture(catches_payload_body(), vec![WasmValType::I32]);
    verify(&payload_module).expect("a resolved catch clause should verify");

    let catch_all_module = tag_fixture(catches_all_body(), vec![WasmValType::I32]);
    // The catch_all target block takes no arguments even though the function
    // results are non-empty.
    verify(&catch_all_module).expect("a resolved catch_all clause should verify");

    let function = &payload_module.functions[0];
    match &function.body[0] {
        Op::Block { body, result, .. } => {
            assert_eq!(*result, Some(WasmValType::I32));
            match &body[0] {
                Op::Try { clauses, body, .. } => {
                    assert_eq!(
                        clauses,
                        &vec![CatchClause::Catch {
                            tag: TagIndex(0),
                            label: 0
                        }]
                    );
                    assert_eq!(body.len(), 2);
                    assert!(matches!(body[1], Op::Leaf(Instruction::Throw(0))));
                }
                other => panic!("expected a try region, got {other:?}"),
            }
        }
        other => panic!("expected a block, got {other:?}"),
    }
}

/// EH-01: a raw `throw` leaf with an out-of-range tag index is rejected.
#[test]
fn rejects_an_out_of_range_throw_tag_index() {
    let module = tag_fixture(
        vec![Op::Leaf(Instruction::Throw(1))],
        vec![WasmValType::I32],
    );
    let errors = verify(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("throw tag index is out of range")),
        "{errors:?}"
    );
}

/// EH-01: a catch clause naming an out-of-range tag index is rejected.
#[test]
fn rejects_an_out_of_range_catch_clause_tag_index() {
    let module = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::Catch {
                tag: TagIndex(1),
                label: 0,
            }],
            body: Vec::new(),
            span: span(),
        }],
        vec![WasmValType::I32],
    );
    let errors = verify(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("catch clause tag index is out of range")),
        "{errors:?}"
    );
}

/// EH-01: a catch clause whose label depth does not resolve against the
/// active labels at the try_table's emission point is rejected.
#[test]
fn rejects_an_unresolved_catch_clause_label_depth() {
    let payload_module = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::Catch {
                tag: TagIndex(0),
                label: 2,
            }],
            body: Vec::new(),
            span: span(),
        }],
        vec![WasmValType::I32],
    );
    let errors = verify(&payload_module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("catch clause label does not target an enclosing label")),
        "{errors:?}"
    );

    let catch_all_module = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::CatchAll { label: 1 }],
            body: Vec::new(),
            span: span(),
        }],
        vec![WasmValType::I32],
    );
    let errors = verify(&catch_all_module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("catch_all clause label does not target an enclosing label")),
        "{errors:?}"
    );
}

/// EH-01: a catch clause's label result type must agree with the payload it
/// delivers; `catch_all` requires an empty label.
#[test]
fn rejects_a_catch_clause_label_result_that_mismatches_the_payload() {
    // The function label is the only active label here and it delivers one
    // i32, so a `catch` of a payload-less tag does not fit it.
    let no_payload_tag = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::Catch {
                tag: TagIndex(0),
                label: 0,
            }],
            body: Vec::new(),
            span: span(),
        }],
        Vec::new(),
    );
    let errors = verify(&no_payload_tag).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("catch clause label result does not match the tag payload")),
        "{errors:?}"
    );

    // A payload-bearing tag caught to an empty label: an empty-payload tag
    // fits an empty label, but the mismatching payload types are rejected.
    let mismatched_payload = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::Catch {
                tag: TagIndex(0),
                label: 0,
            }],
            body: Vec::new(),
            span: span(),
        }],
        vec![WasmValType::I64],
    );
    let errors = verify(&mismatched_payload).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("catch clause label result does not match the tag payload")),
        "{errors:?}"
    );

    let catch_all_to_value_label = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::CatchAll { label: 0 }],
            body: Vec::new(),
            span: span(),
        }],
        vec![WasmValType::I32],
    );
    let errors = verify(&catch_all_to_value_label).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("catch_all clause label must deliver no values")),
        "{errors:?}"
    );

    // The positive forms: an empty label accepts a payload-less catch, and an
    // i32 label accepts a matching payload.
    let empty_tag_to_empty_label = void_tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::Catch {
                tag: TagIndex(0),
                label: 0,
            }],
            body: Vec::new(),
            span: span(),
        }],
        Vec::new(),
    );
    verify(&empty_tag_to_empty_label)
        .expect("a payload-less catch clause should target an empty label");
    let payload_tag_to_value_label = tag_fixture(
        vec![Op::Try {
            clauses: vec![CatchClause::Catch {
                tag: TagIndex(0),
                label: 0,
            }],
            body: Vec::new(),
            span: span(),
        }],
        vec![WasmValType::I32],
    );
    verify(&payload_tag_to_value_label)
        .expect("an i32 payload should target an i32-delivering label");
}

/// EH-01: branch depths inside the protected body count the try_table's own
/// internal label, like a raw block label.
#[test]
fn counts_the_try_region_internal_label_in_branch_depths() {
    // The function and block labels deliver nothing, so a `catch_all` clause
    // may target the function label (depth 1) and a `br 1` inside the
    // protected body targets the enclosing block.
    let module = void_tag_fixture(
        vec![Op::Block {
            body: vec![
                Op::Try {
                    clauses: vec![CatchClause::CatchAll { label: 1 }],
                    body: vec![Op::Leaf(Instruction::Br(1))],
                    span: span(),
                },
                Op::Leaf(Instruction::Unreachable),
            ],
            result: None,
            span: span(),
        }],
        Vec::new(),
    );
    verify(&module).expect("depth 1 inside the try region is the enclosing block");

    let too_deep = void_tag_fixture(
        vec![Op::Block {
            body: vec![Op::Try {
                clauses: vec![CatchClause::CatchAll { label: 0 }],
                body: vec![Op::Leaf(Instruction::Br(3))],
                span: span(),
            }],
            result: None,
            span: span(),
        }],
        Vec::new(),
    );
    let errors = verify(&too_deep).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("does not target an enclosing label")),
        "{errors:?}"
    );
}

/// EH-01: a tag's function type carries the payload as its parameters and
/// produces no results; anything else is rejected.
#[test]
fn rejects_malformed_tag_definitions() {
    let multi_payload = super::super::Module {
        types: vec![FuncType {
            parameters: vec![WasmValType::I32, WasmValType::I64],
            results: Vec::new(),
        }],
        tags: vec![tag(0)],
        ..tag_fixture(Vec::new(), Vec::new())
    };
    let errors = verify(&multi_payload).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("tag payload has more than one value")),
        "{errors:?}"
    );

    let result_producing = super::super::Module {
        types: vec![FuncType {
            parameters: Vec::new(),
            results: vec![WasmValType::I32],
        }],
        tags: vec![tag(0)],
        ..tag_fixture(Vec::new(), Vec::new())
    };
    let errors = verify(&result_producing).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("tag function type must produce no results")),
        "{errors:?}"
    );

    let out_of_range = super::super::Module {
        tags: vec![tag(3)],
        ..tag_fixture(Vec::new(), Vec::new())
    };
    let errors = verify(&out_of_range).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("tag type index is out of range")),
        "{errors:?}"
    );
}

mod encoding;
