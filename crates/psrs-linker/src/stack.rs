//! Static linear-stack bound analysis for a nonrecursive core artifact.
//!
//! Each function's prologue subtracts its frame size from the stack-pointer
//! global. The maximum bytes an artifact can use is the largest sum of frame
//! sizes along any call path. A call graph with a cycle is not a supported
//! artifact and is rejected rather than bounded.

use wasmparser::{Operator, Parser, Payload};

/// A measured upper bound on linear-stack bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackBound {
    pub bytes: u32,
}

/// Measures the static stack bound of a core module.
///
/// `stack_pointer_global` is the mutable global whose prologue adjustment
/// reserves a frame. Indirect calls and calls into imported functions make the
/// bound unknown, so they are errors rather than silent under-approximations.
pub fn measure_stack_bound(bytes: &[u8], stack_pointer_global: u32) -> Result<StackBound, String> {
    let mut frames = Vec::new();
    let mut callees = Vec::new();
    let mut imported_functions = 0_usize;

    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|error| error.to_string())? {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    if matches!(
                        import.map_err(|error| error.to_string())?.ty,
                        wasmparser::TypeRef::Func(_)
                    ) {
                        imported_functions += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let (frame, calls) = analyze_body(&body, stack_pointer_global)?;
                frames.push(frame);
                callees.push(calls);
            }
            _ => {}
        }
    }

    let count = frames.len();
    let mut memo = vec![None::<u32>; count];
    let mut visiting = vec![false; count];
    let mut bound = 0_u32;
    for index in 0..count {
        bound = bound.max(visit(
            index,
            &frames,
            &callees,
            &mut memo,
            &mut visiting,
            imported_functions,
        )?);
    }
    Ok(StackBound { bytes: bound })
}

fn analyze_body(
    body: &wasmparser::FunctionBody<'_>,
    stack_pointer_global: u32,
) -> Result<(u32, Vec<u32>), String> {
    let mut frame = 0_u32;
    let mut calls = Vec::new();
    let mut pending_stack_pointer = false;
    let mut pending_frame = None;
    let mut operators = body
        .get_operators_reader()
        .map_err(|error| error.to_string())?;
    while !operators.eof() {
        match operators.read().map_err(|error| error.to_string())? {
            Operator::GlobalGet { global_index } if global_index == stack_pointer_global => {
                pending_stack_pointer = true;
                pending_frame = None;
            }
            Operator::I32Const { value } if pending_stack_pointer => {
                pending_frame = Some(value.max(0) as u32);
            }
            Operator::I32Sub if pending_stack_pointer => {
                if let Some(value) = pending_frame.take() {
                    frame = frame.max(value);
                }
            }
            Operator::Call { function_index } => {
                calls.push(function_index);
                pending_stack_pointer = false;
                pending_frame = None;
            }
            Operator::ReturnCall { function_index } => {
                calls.push(function_index);
                pending_stack_pointer = false;
                pending_frame = None;
            }
            Operator::CallIndirect { .. } | Operator::ReturnCallIndirect { .. } => {
                return Err("an indirect call makes the stack bound unknown".into());
            }
            _ => {
                pending_stack_pointer = false;
                pending_frame = None;
            }
        }
    }
    Ok((frame, calls))
}

fn visit(
    index: usize,
    frames: &[u32],
    callees: &[Vec<u32>],
    memo: &mut [Option<u32>],
    visiting: &mut [bool],
    imported_functions: usize,
) -> Result<u32, String> {
    if let Some(value) = memo[index] {
        return Ok(value);
    }
    if visiting[index] {
        return Err("a recursive call graph is not a supported runtime library".into());
    }
    visiting[index] = true;
    let mut deepest = 0_u32;
    for callee in &callees[index] {
        let callee = *callee as usize;
        if callee < imported_functions {
            return Err("a call into an imported function makes the stack bound unknown".into());
        }
        let defined = callee - imported_functions;
        if defined < frames.len() {
            deepest = deepest.max(visit(
                defined,
                frames,
                callees,
                memo,
                visiting,
                imported_functions,
            )?);
        }
    }
    visiting[index] = false;
    let total = frames[index]
        .checked_add(deepest)
        .ok_or("stack bound overflows")?;
    memo[index] = Some(total);
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_artifact_has_a_measured_static_bound() {
        let bound = measure_stack_bound(psrs_runtime::NUMBER_FORMATTER.bytes, 0)
            .expect("the formatter is nonrecursive");
        assert_eq!(
            bound.bytes,
            psrs_runtime::NUMBER_FORMATTER.storage.stack_bound_bytes,
            "the declared stack bound must match the static analysis"
        );
    }

    #[test]
    fn a_recursive_graph_is_rejected() {
        // Two mutually recursive functions, each reserving a frame.
        let bytes = recursive_module();
        assert!(measure_stack_bound(&bytes, 0).is_err());
    }

    fn recursive_module() -> Vec<u8> {
        use wasm_encoder::{
            CodeSection, Function, FunctionSection, GlobalSection, GlobalType, Instruction, Module,
            TypeSection, ValType,
        };
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut functions = FunctionSection::new();
        functions.function(0);
        functions.function(0);
        module.section(&functions);
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(1024),
        );
        module.section(&globals);
        let mut code = CodeSection::new();
        let mut first = Function::new([]);
        first.instruction(&Instruction::GlobalGet(0));
        first.instruction(&Instruction::I32Const(32));
        first.instruction(&Instruction::I32Sub);
        first.instruction(&Instruction::GlobalSet(0));
        first.instruction(&Instruction::Call(1));
        first.instruction(&Instruction::End);
        code.function(&first);
        let mut second = Function::new([]);
        second.instruction(&Instruction::GlobalGet(0));
        second.instruction(&Instruction::I32Const(32));
        second.instruction(&Instruction::I32Sub);
        second.instruction(&Instruction::GlobalSet(0));
        second.instruction(&Instruction::Call(0));
        second.instruction(&Instruction::End);
        code.function(&second);
        module.section(&code);
        module.finish()
    }
}
