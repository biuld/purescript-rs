//! Observational guards instrument the final Wasm, after compiler verification.
//! The deliberately trapping budget walk is not part of the control protocol.
use psrs_backend::cc::ControlHelperRole as R;
use psrs_backend::wasm::{
    Body, FuncType, Function, Global, GlobalIndex, GlobalInit, Op, TypeIndex,
};
use psrs_backend::{
    Stages,
    types::{CompositeType, HeapType as MirHeap, ValueType},
};
use wasm_encoder::{HeapType, Instruction as I, RefType, ValType};

pub(super) fn instrument(stages: &mut Stages, limit: i32) -> usize {
    let check = stages
        .cc
        .representations
        .control_helpers()
        .iter()
        .find(|h| h.role() == R::Check)
        .unwrap()
        .function();
    let helper = stages
        .mir
        .functions
        .iter()
        .find(|f| f.symbol == check)
        .unwrap();
    let parameter = helper
        .values
        .iter()
        .find(|v| v.id == helper.parameters[0])
        .unwrap()
        .ty;
    let ValueType::Ref(reference) = parameter else {
        panic!("extent reference")
    };
    let MirHeap::Index(base) = reference.heap else {
        panic!("extent type")
    };
    let cases = stages
        .wasm
        .type_defs
        .iter()
        .flat_map(|group| &group.0)
        .enumerate()
        .filter_map(|(index, ty)| {
            if ty.supertype != Some(base) {
                return None;
            }
            let CompositeType::Struct(fields) = &ty.composite else {
                panic!("extent struct")
            };
            Some((fields.len(), index as u32))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let root = cases[&2];
    let child = cases[&3];
    let module = &mut stages.wasm;
    let index = module.imports.len()
        + module.functions.len()
        + usize::from(module.entry.is_some())
        + usize::from(module.realloc.is_some())
        + module.helpers.len();
    let mut count = 0;
    for function in &mut module.functions {
        let start = function.parameters.len() + function.locals.len();
        // Nullable locals can hold the constructed non-null subtype. local.tee
        // retains a nullable static type, so restore the original subtype after
        // the observation before the production consumer reads it.
        function
            .locals
            .extend([reference_type(root, true), reference_type(child, true)]);
        count += insert(
            &mut function.body,
            root,
            child,
            start as u32,
            index as u32,
            limit,
        );
    }
    let type_index = TypeIndex(module.defined_type_count() + module.types.len() as u32);
    let parameters = vec![reference_type(base.0, false), ValType::I32];
    module.types.push(FuncType {
        parameters: parameters.clone(),
        results: vec![],
    });
    let span = module.span;
    let body = vec![Op::Loop {
        result: None,
        span,
        body: vec![
            leaf(I::LocalGet(1)),
            leaf(I::I32Const(0)),
            leaf(I::I32LtS),
            Op::If {
                then_body: vec![leaf(I::Unreachable)],
                else_body: vec![],
                result: None,
                span,
            },
            leaf(I::LocalGet(0)),
            leaf(I::StructGet {
                struct_type_index: base.0,
                field_index: 0,
            }),
            leaf(I::I32Eqz),
            Op::If {
                then_body: vec![leaf(I::Return)],
                else_body: vec![],
                result: None,
                span,
            },
            leaf(I::LocalGet(0)),
            leaf(I::RefCastNonNull(HeapType::Concrete(child))),
            leaf(I::StructGet {
                struct_type_index: child,
                field_index: 2,
            }),
            leaf(I::LocalSet(0)),
            leaf(I::LocalGet(1)),
            leaf(I::I32Const(1)),
            leaf(I::I32Sub),
            leaf(I::LocalSet(1)),
            leaf(I::Br(0)),
        ],
    }];
    module.helpers.push(Function {
        symbol: psrs_hir::SymbolId::new(check.module, u32::MAX - 1),
        name: "test_control_ancestry_probe".into(),
        type_index,
        parameters,
        locals: vec![],
        body,
        span,
    });
    count
}

fn reference_type(index: u32, nullable: bool) -> ValType {
    ValType::Ref(RefType {
        nullable,
        heap_type: HeapType::Concrete(index),
    })
}
fn leaf(instruction: I<'static>) -> Op {
    Op::Leaf(instruction)
}

fn counter(global: u32, budget: i32, span: psrs_span::TextRange) -> Body {
    vec![
        leaf(I::GlobalGet(global)),
        leaf(I::I32Const(1)),
        leaf(I::I32Add),
        leaf(I::GlobalSet(global)),
        leaf(I::GlobalGet(global)),
        leaf(I::I32Const(budget)),
        leaf(I::I32GtS),
        Op::If {
            then_body: vec![leaf(I::Unreachable)],
            else_body: vec![],
            result: None,
            span,
        },
    ]
}
fn insert(body: &mut Body, root: u32, child: u32, local: u32, checker: u32, limit: i32) -> usize {
    let mut count = 0;
    let mut result = Vec::new();
    for mut op in std::mem::take(body) {
        match &mut op {
            Op::If {
                then_body,
                else_body,
                ..
            } => {
                count += insert(then_body, root, child, local, checker, limit);
                count += insert(else_body, root, child, local, checker, limit);
            }
            Op::Block { body, .. } | Op::Loop { body, .. } => {
                count += insert(body, root, child, local, checker, limit)
            }
            _ => {}
        }
        let constructed = match &op {
            Op::Leaf(I::StructNew(index)) if *index == root || *index == child => Some(*index),
            _ => None,
        };
        result.push(op);
        if let Some(index) = constructed {
            count += 1;
            let slot = local + u32::from(index == child);
            result.extend([
                leaf(I::LocalTee(slot)),
                leaf(I::LocalGet(slot)),
                leaf(I::RefCastNonNull(HeapType::Concrete(index))),
                leaf(I::I32Const(limit)),
                leaf(I::Call(checker)),
                leaf(I::RefCastNonNull(HeapType::Concrete(index))),
            ]);
        }
    }
    *body = result;
    count
}

/// Count replay invocations by incrementing a private global at each
/// `CloneSequence`/`RebuildSequence` entry and trapping once it exceeds
/// `budget`. The smallest budget that does not trap is therefore the number of
/// replay invocations, measured independently of the extent ancestry bound.
pub(super) fn instrument_replay(stages: &mut Stages, budget: i32) -> usize {
    let symbol = |role| {
        stages
            .cc
            .representations
            .control_helpers()
            .iter()
            .find(|helper| helper.role() == role)
            .expect("control helper")
            .function()
    };
    let clone = symbol(R::CloneSequence);
    let rebuild = symbol(R::RebuildSequence);
    let module = &mut stages.wasm;
    let global = module.globals.len() as u32;
    module.globals.push(Global {
        index: GlobalIndex(global),
        mutable: true,
        ty: ValType::I32,
        init: GlobalInit::I32(0),
    });
    let mut count = 0;
    for function in &mut module.functions {
        if function.symbol != clone && function.symbol != rebuild {
            continue;
        }
        let span = function.span;
        let mut prefix = counter(global, budget, span);
        prefix.extend(std::mem::take(&mut function.body));
        function.body = prefix;
        count += 1;
    }
    count
}

/// Observe each loopified dispatch iteration, including self-tail backedges.
/// Optional `ordinal` selects one replay activation by its Clone entry; it
/// remains selected across Clone/Rebuild tail loops and their helper transfer.
pub(super) fn instrument_replay_steps(
    stages: &mut Stages,
    budget: i32,
    ordinal: Option<i32>,
) -> usize {
    let plan = stages
        .cc
        .representations
        .control_plan()
        .expect("planned control machine");
    let clone = plan.helper(R::CloneSequence).unwrap().function();
    let rebuild = plan.helper(R::RebuildSequence).unwrap().function();
    let module = &mut stages.wasm;
    let global = module.globals.len() as u32;
    for index in [global, global + 1] {
        module.globals.push(Global {
            index: GlobalIndex(index),
            mutable: true,
            ty: ValType::I32,
            init: GlobalInit::I32(0),
        });
    }
    let mut helpers = 0;
    for function in &mut module.functions {
        if function.symbol != clone && function.symbol != rebuild {
            continue;
        }
        let prefix = if let Some(ordinal) = ordinal {
            vec![
                leaf(I::GlobalGet(global + 1)),
                leaf(I::I32Const(ordinal)),
                leaf(I::I32Eq),
                Op::If {
                    then_body: counter(global, budget, function.span),
                    else_body: vec![],
                    result: None,
                    span: function.span,
                },
            ]
        } else {
            counter(global, budget, function.span)
        };
        let loops = instrument_loops(&mut function.body, &prefix);
        assert_eq!(
            loops, 1,
            "replay observation requires one loopified dispatch header per helper, got {loops} in {:?}",
            function.symbol
        );
        if function.symbol == clone {
            let mut entry = vec![
                leaf(I::GlobalGet(global + 1)),
                leaf(I::I32Const(1)),
                leaf(I::I32Add),
                leaf(I::GlobalSet(global + 1)),
            ];
            entry.extend(std::mem::take(&mut function.body));
            function.body = entry;
        }
        helpers += 1;
    }
    assert_eq!(
        helpers, 2,
        "both replay helpers must survive the verified pipeline"
    );
    helpers
}

fn instrument_loops(body: &mut Body, prefix: &Body) -> usize {
    let mut count = 0;
    for op in body {
        match op {
            Op::Loop { body, .. } => {
                count += 1 + instrument_loops(body, prefix);
                let mut observed = prefix.clone();
                observed.extend(std::mem::take(body));
                *body = observed;
            }
            Op::Block { body, .. } => count += instrument_loops(body, prefix),
            Op::If {
                then_body,
                else_body,
                ..
            } => {
                count += instrument_loops(then_body, prefix) + instrument_loops(else_body, prefix);
            }
            _ => {}
        }
    }
    count
}

/// Replace the program core module in the already assembled component. The
/// observation adds only private locals/functions; imports/exports stay fixed.
pub(super) fn component(stages: &Stages, core: &[u8]) -> Vec<u8> {
    use wasm_encoder::Encode;
    let source = &stages.artifact.wasm;
    assert_eq!(&source[..8], b"\0asm\x0d\0\x01\0");
    let mut result = source[..8].to_vec();
    let mut offset = 8;
    let mut replaced = 0;
    while offset < source.len() {
        let start = offset;
        let id = source[offset];
        offset += 1;
        let mut length = 0usize;
        let mut shift = 0;
        loop {
            let byte = source[offset];
            offset += 1;
            length |= usize::from(byte & 127) << shift;
            if byte & 128 == 0 {
                break;
            }
            shift += 7;
        }
        let end = offset + length;
        if id == 1 {
            replaced += 1;
            result.push(id);
            (core.len() as u32).encode(&mut result);
            result.extend_from_slice(core);
        } else {
            result.extend_from_slice(&source[start..end]);
        }
        offset = end;
    }
    assert_eq!(
        replaced, 1,
        "cost fixture must have one program core module"
    );
    result
}
