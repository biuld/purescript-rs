//! Exception-channel execution evidence (EH-04).
//!
//! The MIR lowering and structurer do not produce try regions yet (EH-05+),
//! so each fixture constructs the thin Wasm IR directly
//! (`psrs_backend::wasm`), validates the encoded binary under an explicitly
//! enabled profile, and executes it with the wasmtime CLI. A fixture whose
//! catch path is taken wrongly would trap (`unreachable`) and exit non-zero.

use psrs_backend::TargetCapabilities;
use psrs_backend::wasm::{
    CatchClause, Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Module, Op,
    TagDef, TagIndex, TypeIndex,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

fn required_wasmtime() -> bool {
    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1")
}

fn wasmtime_available() -> Option<()> {
    let version = std::process::Command::new("wasmtime")
        .arg("--version")
        .output();
    let usable = matches!(&version, Ok(output) if output.status.success());
    if !usable {
        if required_wasmtime() {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not usable: {version:?}");
        }
        return None;
    }
    Some(())
}

/// Encodes, validates under the enabled profile, and invokes the module's
/// `run` export with the wasmtime CLI.
fn invoke_run(name: &str, module: &Module) -> Result<Option<std::process::Output>, String> {
    wasmtime_available().ok_or_else(|| format!("wasmtime is unavailable for `{name}`"))?;
    let binary =
        psrs_backend::wasm::encode_module(module).expect("the exception fixture should encode");
    let target = enabled_target();
    wasmparser::Validator::new_with_features(target.wasm_features())
        .validate_all(&binary)
        .map_err(|error| {
            format!("`{name}` failed validation under the enabled profile: {error}")
        })?;
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("psrs-exception-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, &binary).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Ok(Some(output))
}

fn enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn symbol() -> SymbolId {
    SymbolId::new(ModuleId(0), 0)
}

/// A module whose type 0 is the tag's function type, whose remaining types
/// follow, and which exports its last function as `run`.
fn module(tag_payload: &[ValType], other_types: &[FuncType], functions: Vec<Function>) -> Module {
    let mut types = vec![FuncType {
        parameters: tag_payload.to_vec(),
        results: Vec::new(),
    }];
    types.extend_from_slice(other_types);
    let run_index = functions.len() - 1;
    Module {
        name: "ExceptionFixtures".into(),
        imports: Vec::new(),
        types,
        type_defs: Vec::new(),
        functions,
        memories: Vec::new(),
        tags: vec![TagDef {
            type_index: TypeIndex(0),
            span: span(),
        }],
        globals: Vec::new(),
        data: Vec::new(),
        exports: vec![Export {
            name: "run".into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(FunctionIndex(run_index as u32)),
        }],
        entry: None,
        realloc: None,
        helpers: Vec::new(),
        span: span(),
    }
}

fn function(name: &str, type_index: u32, body: Vec<Op>) -> Function {
    Function {
        symbol: symbol(),
        name: name.into(),
        type_index: TypeIndex(type_index),
        parameters: Vec::new(),
        locals: Vec::new(),
        body,
        span: span(),
    }
}

/// `block (result i32) [ try_table (catch $t 0) [...] end; unreachable ] end`:
/// the catch clause's depth-0 label is the enclosing result-bearing block, so
/// the caught payload becomes the function result.
fn catch_block(catch_clause: CatchClause, protected: Vec<Op>) -> Vec<Op> {
    vec![Op::Block {
        body: vec![
            Op::Try {
                clauses: vec![catch_clause],
                body: protected,
                span: span(),
            },
            Op::Leaf(Instruction::Unreachable),
        ],
        result: Some(ValType::I32),
        span: span(),
    }]
}

/// A payload-bearing throw, caught by a named `catch` clause: the result is
/// the thrown payload, not the throw site's unreachable path.
#[test]
fn named_catch_delivers_the_tag_payload() {
    let module = module(
        &[ValType::I32],
        &[FuncType {
            parameters: Vec::new(),
            results: vec![ValType::I32],
        }],
        vec![function(
            "run",
            1,
            catch_block(
                CatchClause::Catch {
                    tag: TagIndex(0),
                    label: 0,
                },
                vec![
                    Op::Leaf(Instruction::I32Const(7)),
                    Op::Leaf(Instruction::Throw(0)),
                ],
            ),
        )],
    );
    match invoke_run("named_catch", &module) {
        Ok(Some(output)) => {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "7",
                "the catch handler must receive the thrown payload"
            );
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

/// A payload-less throw, caught by `catch_all`: the empty unwind block ends,
/// then the handler's sentinel value becomes the function result.
#[test]
fn catch_all_observes_a_payload_less_throw() {
    let module = module(
        &[],
        &[
            FuncType {
                parameters: Vec::new(),
                results: Vec::new(),
            },
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
        ],
        vec![function(
            "run",
            2,
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
                result: Some(ValType::I32),
                span: span(),
            }],
        )],
    );
    match invoke_run("catch_all", &module) {
        Ok(Some(output)) => {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "42",
                "the catch_all handler must run instead of the unreachable path"
            );
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

/// A throw in a callee two frames below the handler unwinds through both
/// frames to the handler's catch clause.
#[test]
fn throw_unwinds_through_nested_call_frames() {
    // Function indices: leaf 0, middle 1, run 2. Types: tag 0, void 1,
    // i32 result 2.
    let module = module(
        &[ValType::I32],
        &[
            FuncType {
                parameters: Vec::new(),
                results: Vec::new(),
            },
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
        ],
        vec![
            function(
                "leaf",
                1,
                vec![
                    Op::Leaf(Instruction::I32Const(5)),
                    Op::Leaf(Instruction::Throw(0)),
                ],
            ),
            function("middle", 1, vec![Op::Leaf(Instruction::Call(0))]),
            function(
                "run",
                2,
                catch_block(
                    CatchClause::Catch {
                        tag: TagIndex(0),
                        label: 0,
                    },
                    vec![Op::Leaf(Instruction::Call(1))],
                ),
            ),
        ],
    );
    match invoke_run("nested_unwind", &module) {
        Ok(Some(output)) => {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "5",
                "the payload must cross two call frames to the handler"
            );
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

/// A trap inside a protected region is not an exception: no catch clause
/// observes it, so `run` traps and exits non-zero.
#[test]
fn traps_are_not_exceptions() {
    let module = module(
        &[],
        &[FuncType {
            parameters: Vec::new(),
            results: Vec::new(),
        }],
        vec![function(
            "run",
            1,
            vec![Op::Block {
                body: vec![Op::Try {
                    clauses: vec![CatchClause::CatchAll { label: 0 }],
                    body: vec![Op::Leaf(Instruction::Unreachable)],
                    span: span(),
                }],
                result: None,
                span: span(),
            }],
        )],
    );
    match invoke_run("trap_not_caught", &module) {
        Ok(Some(output)) => {
            assert!(
                !output.status.success(),
                "the trap must not be converted into a catch: {output:?}"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .to_lowercase()
                    .contains("unreachable"),
                "wasmtime should report the unreachable trap: {output:?}"
            );
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}
