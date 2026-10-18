//! EH-12/EH-13/EH-14 local inspection: the recorded frame weight, the budget
//! prologue/epilogue emission, and the watermark save/restore, inspected on
//! the lowered thin Wasm IR and its WAT. The runtime fixtures (tail
//! recursion, the leak fixture, the coordination fixture) live in
//! `budget_execution`.

mod budget_execution;
mod coordination;
mod watermark;

use super::*;
use crate::TargetCapabilities;
use crate::mir::NumericOp;
use crate::mir::budget;
use crate::mir::verify_module_with_capabilities;
use psrs_hir::SymbolId;

pub(super) fn enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

/// The symbols the shared fixtures use: main (entry), the non-tail recursive
/// `rec`, and the immediate `thrower`.
pub(super) const MAIN: usize = 0;
pub(super) const REC: usize = 1;
pub(super) const THROWER: usize = 2;

pub(super) fn symbol(index: usize) -> SymbolId {
    SymbolId::new(ModuleId(0), index as u32)
}

pub(super) fn at(offset: u32) -> TextRange {
    TextRange::new(offset, offset + 1)
}

pub(super) fn constant(destination: u32, value: i32, span: TextRange) -> Instruction {
    Instruction::Constant {
        destination: ValueId(destination),
        value,
        span,
    }
}

pub(super) fn primitive(
    destination: u32,
    op: NumericOp,
    left: u32,
    right: u32,
    span: TextRange,
) -> Instruction {
    Instruction::Primitive {
        destination: ValueId(destination),
        op,
        left: ValueId(left),
        right: ValueId(right),
        span,
    }
}

pub(super) fn call(
    destination: u32,
    function: usize,
    arguments: Vec<u32>,
    span: TextRange,
) -> Instruction {
    Instruction::Call {
        contract: None,
        destination: ValueId(destination),
        function: symbol(function),
        arguments: arguments.into_iter().map(ValueId).collect(),
        span,
    }
}

pub(super) fn jump(target: u32, arguments: Vec<u32>, span: TextRange) -> Terminator {
    Terminator::Jump {
        target: BlockId(target),
        arguments: arguments.into_iter().map(ValueId).collect(),
        span,
    }
}

pub(super) fn block(
    id: u32,
    parameters: Vec<u32>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
) -> crate::mir::BasicBlock {
    crate::mir::BasicBlock {
        id: BlockId(id),
        parameters: parameters.into_iter().map(ValueId).collect(),
        instructions,
        terminator: Some(terminator),
    }
}

fn values(types: &[(u32, ValueType)]) -> Vec<ValueDecl> {
    types
        .iter()
        .map(|(id, ty)| ValueDecl {
            id: ValueId(*id),
            ty: *ty,
        })
        .collect()
}

/// Builds a multi-function module; `entry` is the entry function's index.
pub(super) fn module(functions: Vec<Function>, tags: Vec<Tag>, entry: usize) -> crate::mir::Module {
    crate::mir::Module {
        name: "BudgetFixture".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags,
        layout: None,
        imports: Vec::new(),
        entry: Some(functions[entry].symbol),
        functions,
        span: at(950),
    }
}

fn function(
    index: usize,
    name: &str,
    parameters: Vec<u32>,
    value_types: Vec<(u32, ValueType)>,
    result: u32,
    blocks: Vec<crate::mir::BasicBlock>,
) -> Function {
    Function {
        id: FunctionId(index as u32),
        symbol: symbol(index),
        name: name.into(),
        parameters: parameters.into_iter().map(ValueId).collect(),
        values: values(&value_types),
        entry: BlockId(0),
        blocks,
        result: ValueId(result),
        result_type: ValueType::I32,
        frame_weight: 0,
        span: at(900 + index as u32),
    }
}

/// `rec(n) = if n <= 0 then 0 else 1 + rec(n - 1)` — genuine non-tail
/// recursion: the `1 +` after the call forces a real frame per level, and P9
/// tail marking (not run on hand-built MIR) would not convert it either.
pub(super) fn rec_function() -> Function {
    let blocks = vec![
        block(
            0,
            vec![],
            vec![
                constant(1, 0, at(60)),
                primitive(2, NumericOp::I32LtS, 0, 1, at(60)),
            ],
            Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(1),
                else_block: BlockId(2),
                span: at(60),
            },
        ),
        block(
            1,
            vec![],
            vec![],
            Terminator::Return {
                value: ValueId(1),
                span: at(61),
            },
        ),
        block(
            2,
            vec![],
            vec![
                constant(3, 1, at(62)),
                primitive(4, NumericOp::I32Sub, 0, 3, at(62)),
                call(5, REC, vec![4], at(62)),
                primitive(6, NumericOp::I32Add, 5, 3, at(62)),
            ],
            Terminator::Return {
                value: ValueId(6),
                span: at(62),
            },
        ),
    ];
    let value_types = vec![
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::Boolean),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
        (6, ValueType::I32),
    ];
    let mut rec = function(REC, "rec", vec![0], value_types, 6, blocks);
    rec.frame_weight = budget::frame_weight(8);
    rec
}

/// An immediate payload-less throw of the stack-exhaustion tag.
pub(super) fn thrower_function() -> Function {
    let blocks = vec![block(
        0,
        vec![],
        vec![],
        Terminator::Throw {
            tag: TagId(0),
            arguments: Vec::new(),
            handler: None,
            span: at(70),
        },
    )];
    function(
        THROWER,
        "thrower",
        vec![],
        vec![(0, ValueType::I32)],
        0,
        blocks,
    )
}

/// The stack-exhaustion tag as a MIR tag: empty payload, `TagId(0)`.
pub(super) fn exhaustion_tag() -> Tag {
    Tag {
        name: "psrs:stack-exhaustion".into(),
        payload: Vec::new(),
        span: at(940),
    }
}

/// Verifies and lowers the module to thin Wasm with the given target.
pub(super) fn lower(mir: &crate::mir::Module, target: TargetCapabilities) -> crate::wasm::Module {
    verify_module_with_capabilities(mir, target).expect("the fixture verifies");
    let mut wasi = crate::abi::WasiRegistry::load_with_capabilities(target)
        .expect("the vendored WASI WIT should load");
    let context = crate::linking::default_context().expect("default target context");
    let link = crate::linking::plan_for_module(&context, mir, &mut wasi, target)
        .expect("checked target plan");
    crate::wasm::lower_module_with_plan(mir, &mut wasi, target, &link)
        .expect("the MIR should lower to Wasm")
}

pub(super) fn wat(wasm: &crate::wasm::Module) -> String {
    let binary = crate::wasm::encode_module(wasm).expect("encoding Wasm");
    wasmprinter::print_bytes(&binary).expect("the module should print")
}

// ------------------------------------------------------------------ EH-12

/// P9 records `frame_weight = FRAME_WEIGHT_FIXED + FRAME_WEIGHT_PER_LOCAL ×
/// (parameters + values)` on every lowered function.
#[test]
fn records_the_frame_weight_on_every_lowered_function() {
    let (cc, bindings) = coordination::module(0);
    let (mir, _) = crate::mir::lower_module_with_bindings(cc, bindings, enabled_target())
        .expect("the module lowers");
    for function in &mir.functions {
        let expected = budget::frame_weight(function.parameters.len() + function.values.len());
        assert_eq!(function.frame_weight, expected, "{}", function.name);
        assert!(function.frame_weight > 0, "{}", function.name);
    }
}

// ------------------------------------------------------------------ EH-13

/// A try-free function with a trapping block, an ordinary return, and a tail
/// call: the prologue sits before the first structured block, the decrement
/// runs on the return and tail paths, and the trapping block carries none.
#[test]
fn emits_the_prologue_and_the_return_and_tail_decrements() {
    // main(): n = -1, so the branch takes the trap arm; the tail-call path
    // still carries its decrement in the emitted body. The entry function
    // must be a zero-argument Int declaration.
    let main = function(
        MAIN,
        "main",
        vec![],
        vec![
            (0, ValueType::I32),
            (1, ValueType::I32),
            (2, ValueType::Boolean),
            (3, ValueType::I32),
        ],
        0,
        vec![
            block(
                0,
                vec![],
                vec![
                    constant(0, -1, at(40)),
                    constant(1, 0, at(40)),
                    primitive(2, NumericOp::I32LtS, 0, 1, at(40)),
                ],
                Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span: at(40),
                },
            ),
            block(1, vec![], vec![], Terminator::Trap { span: at(41) }),
            block(
                2,
                vec![],
                vec![primitive(3, NumericOp::I32Sub, 0, 1, at(42))],
                Terminator::ReturnCall {
                    contract: None,
                    function: symbol(REC),
                    arguments: vec![ValueId(3)],
                    span: at(42),
                },
            ),
        ],
    );
    let mir = module(
        vec![main, rec_function(), thrower_function()],
        vec![exhaustion_tag()],
        MAIN,
    );
    let wasm = lower(&mir, enabled_target());
    let wat = wat(&wasm);
    // The module's WAT holds every function; this test inspects the first
    // defined function's text (functions[0] is encoded first; the `(func`
    // occurrences at line starts mark the defined functions after the
    // import).
    let starts: Vec<usize> = wat
        .match_indices("\n  (func")
        .map(|(index, _)| index)
        .collect();
    assert!(starts.len() >= 2, "{wat}");
    let wat = &wat[starts[0]..starts[1]];

    // The prologue: exactly one overflow check, before the function's first
    // structured block.
    assert_eq!(wat.matches("i32.gt_s").count(), 1, "{wat}");
    let prologue = wat.find("i32.gt_s").expect("the prologue check");
    let if_open = wat[prologue..].find("if").expect("the overflow if") + prologue;
    let throw = wat[if_open..].find("throw").expect("the overflow throw") + if_open;
    let first_block = wat.find("block ;; label").expect("the first block");
    assert!(prologue < first_block, "{wat}");
    assert!(throw < first_block, "{wat}");

    // The decrements, counted on the thin IR so rec's own `i32.sub`
    // arithmetic cannot confuse the count: main's single tail-call site and
    // rec's two return paths each carry one. The trapping block carries
    // none — if it did, main's count would be two.
    assert_eq!(count_decrements(&wasm.functions[0].body), 1, "{wat}");
    assert_eq!(count_decrements(&wasm.functions[1].body), 2, "{wat}");
    // The tail decrement sits between the prologue and the tail call.
    let tail = wat.find("return_call").expect("the tail call");
    let between = &wat[first_block..tail];
    assert!(
        between.contains("i32.sub") && between.contains("global.set"),
        "{wat}"
    );
}

/// Counts `[global.get, i32.const, i32.sub, global.set]` sequences — the
/// budget decrement — anywhere in a structured body.
fn count_decrements(body: &[crate::wasm::Op]) -> usize {
    use wasm_encoder::Instruction;
    let count = body
        .windows(4)
        .filter(|w| {
            matches!(
                (&w[0], &w[1], &w[2], &w[3]),
                (
                    crate::wasm::Op::Leaf(Instruction::GlobalGet(_)),
                    crate::wasm::Op::Leaf(Instruction::I32Const(_)),
                    crate::wasm::Op::Leaf(Instruction::I32Sub),
                    crate::wasm::Op::Leaf(Instruction::GlobalSet(_))
                )
            )
        })
        .count();
    count
        + body
            .iter()
            .map(|op| match op {
                crate::wasm::Op::Block { body, .. } | crate::wasm::Op::Loop { body, .. } => {
                    count_decrements(body)
                }
                crate::wasm::Op::Try { body, .. } => count_decrements(body),
                crate::wasm::Op::If {
                    then_body,
                    else_body,
                    ..
                } => count_decrements(then_body) + count_decrements(else_body),
                _ => 0,
            })
            .sum::<usize>()
}

// -------------------------------------------------------- disabled profile

/// The disabled profile emits none of the budget: no depth global, no
/// stack-exhaustion tag, no prologue check, no decrements — the try-free
/// bodies are unchanged. (A module with a tag or a `Throw` is rejected at the
/// MIR capability gate under this profile, so the fixture carries none.)
#[test]
fn the_disabled_profile_emits_no_budget_code() {
    let main = function(
        MAIN,
        "main",
        vec![],
        vec![
            (0, ValueType::I32),
            (1, ValueType::I32),
            (2, ValueType::Boolean),
            (3, ValueType::I32),
        ],
        0,
        vec![
            block(
                0,
                vec![],
                vec![
                    constant(0, -1, at(40)),
                    constant(1, 0, at(40)),
                    primitive(2, NumericOp::I32LtS, 0, 1, at(40)),
                ],
                Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span: at(40),
                },
            ),
            block(1, vec![], vec![], Terminator::Trap { span: at(41) }),
            block(
                2,
                vec![],
                vec![primitive(3, NumericOp::I32Sub, 0, 1, at(42))],
                Terminator::ReturnCall {
                    contract: None,
                    function: symbol(REC),
                    arguments: vec![ValueId(3)],
                    span: at(42),
                },
            ),
        ],
    );
    let mir = module(vec![main, rec_function()], Vec::new(), MAIN);
    let wasm = lower(&mir, TargetCapabilities::default());
    assert!(wasm.globals.is_empty(), "no depth global");
    assert!(wasm.tags.is_empty(), "no stack-exhaustion tag");
    let wat = wat(&wasm);
    assert!(!wat.contains("throw"), "{wat}");
    assert!(!wat.contains("i32.gt_s"), "{wat}");
    // rec's only `i32.sub` is its own arithmetic, not the epilogue decrement;
    // no `global.set` occurs in any body.
    assert!(!wat.contains("global.set"), "{wat}");
}

/// The enabled profile installs the budget even on a module with no MIR tag
/// of its own: the compiler-defined tag and the depth global appear.
#[test]
fn the_enabled_profile_adds_the_depth_global_and_tag() {
    let main = function(
        MAIN,
        "main",
        vec![],
        vec![(0, ValueType::I32)],
        0,
        vec![block(
            0,
            vec![],
            vec![constant(0, 7, at(50))],
            Terminator::Return {
                value: ValueId(0),
                span: at(50),
            },
        )],
    );
    let mir = module(vec![main], Vec::new(), MAIN);
    let wasm = lower(&mir, enabled_target());
    assert_eq!(wasm.globals.len(), 1, "the depth global");
    assert_eq!(wasm.globals[0].index, crate::wasm::GlobalIndex(0));
    assert!(wasm.globals[0].mutable);
    assert_eq!(wasm.globals[0].ty, wasm_encoder::ValType::I32);
    assert_eq!(wasm.tags.len(), 1, "the stack-exhaustion tag");
}
