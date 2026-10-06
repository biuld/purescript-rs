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
    // Exception and suspension proposals require a separate frame-unwind
    // proof. They cannot enter this normal-return-only analysis.
    use wasmparser::WasmFeatures as F;
    let features = F::MVP
        | F::MUTABLE_GLOBAL
        | F::SIGN_EXTENSION
        | F::SATURATING_FLOAT_TO_INT
        | F::MULTI_VALUE
        | F::BULK_MEMORY
        | F::REFERENCE_TYPES
        | F::FUNCTION_REFERENCES
        | F::GC
        | F::TAIL_CALL;
    wasmparser::Validator::new_with_features(features)
        .validate_all(bytes)
        .map_err(|error| error.to_string())?;
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
    let mut calls = Vec::new();
    let mut reader = body
        .get_operators_reader()
        .map_err(|error| error.to_string())?;
    let mut operators = Vec::new();
    while !reader.eof() {
        operators.push(reader.read().map_err(|error| error.to_string())?);
    }
    let (frame, frame_local, prologue_end) = match operators.as_slice() {
        [
            Operator::GlobalGet { global_index },
            Operator::I32Const { value },
            Operator::I32Sub,
            Operator::LocalTee { local_index },
            Operator::GlobalSet {
                global_index: target,
            },
            ..,
        ] if *global_index == stack_pointer_global
            && *target == stack_pointer_global
            && *value > 0 =>
        {
            (*value as u32, Some(*local_index), 5)
        }
        [
            Operator::GlobalGet { global_index },
            Operator::I32Const { value },
            Operator::I32Sub,
            Operator::GlobalSet {
                global_index: target,
            },
            ..,
        ] if *global_index == stack_pointer_global
            && *target == stack_pointer_global
            && *value > 0 =>
        {
            (*value as u32, None, 4)
        }
        _ => (0, None, 0),
    };
    let mut depth = 0_u32;
    let mut restored = false;
    let mut index = prologue_end;
    while index < operators.len() {
        match &operators[index] {
            Operator::GlobalGet { global_index } if *global_index == stack_pointer_global => {
                if frame_local.is_none()
                    && frame > 0
                    && depth == 0
                    && !restored
                    && matches!(operators.get(index + 1), Some(Operator::I32Const { value }) if *value as u32 == frame)
                    && matches!(operators.get(index + 2), Some(Operator::I32Add))
                    && matches!(operators.get(index + 3), Some(Operator::GlobalSet { global_index }) if *global_index == stack_pointer_global)
                {
                    restored = true;
                    index += 4;
                    continue;
                }
                return Err("unrecognized stack-pointer access makes the bound unknown".into());
            }
            Operator::LocalGet { local_index } if Some(*local_index) == frame_local => {
                if depth == 0
                    && !restored
                    && matches!(operators.get(index + 1), Some(Operator::I32Const { value }) if *value as u32 == frame)
                    && matches!(operators.get(index + 2), Some(Operator::I32Add))
                    && matches!(operators.get(index + 3), Some(Operator::GlobalSet { global_index }) if *global_index == stack_pointer_global)
                {
                    restored = true;
                    index += 4;
                    continue;
                }
            }
            Operator::GlobalSet { global_index } if *global_index == stack_pointer_global => {
                return Err("unrecognized stack-pointer write makes the bound unknown".into());
            }
            Operator::LocalSet { local_index } | Operator::LocalTee { local_index }
                if Some(*local_index) == frame_local =>
            {
                return Err("the saved stack frame is overwritten".into());
            }
            Operator::Call { function_index } => {
                calls.push(*function_index);
            }
            Operator::ReturnCall { function_index } => {
                if frame > 0 && !restored {
                    return Err("return bypasses stack restoration".into());
                }
                calls.push(*function_index);
            }
            Operator::CallIndirect { .. }
            | Operator::ReturnCallIndirect { .. }
            | Operator::CallRef { .. }
            | Operator::ReturnCallRef { .. } => {
                return Err("an indirect call makes the stack bound unknown".into());
            }
            Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
                if restored {
                    return Err("control flow after stack restoration is unsupported".into());
                }
                depth += 1;
            }
            Operator::End if depth > 0 => depth -= 1,
            Operator::Return if frame > 0 && !restored => {
                return Err("return bypasses stack restoration".into());
            }
            Operator::Br { relative_depth }
            | Operator::BrIf { relative_depth }
            | Operator::BrOnNull { relative_depth }
            | Operator::BrOnNonNull { relative_depth }
            | Operator::BrOnCast { relative_depth, .. }
            | Operator::BrOnCastFail { relative_depth, .. }
                if frame > 0 && !restored && *relative_depth >= depth =>
            {
                return Err("branch bypasses stack restoration".into());
            }
            Operator::BrTable { targets }
                if frame > 0
                    && !restored
                    && (targets.default() >= depth
                        || targets
                            .targets()
                            .any(|target| target.is_ok_and(|target| target >= depth))) =>
            {
                return Err("branch bypasses stack restoration".into());
            }
            _ => {}
        }
        index += 1;
    }
    if frame > 0
        && !restored
        && !matches!(
            operators.as_slice(),
            [.., Operator::Unreachable, Operator::End]
        )
    {
        return Err("a returning function does not restore its stack frame".into());
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
        } else {
            return Err("callee is outside the analyzed module".into());
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
        assert!(
            measure_stack_bound(&bytes, 0)
                .unwrap_err()
                .contains("recursive call graph")
        );
    }

    #[test]
    fn unrecognized_stack_writes_and_unbalanced_frames_are_rejected() {
        use wasm_encoder::Instruction as I;
        let prologue = [
            I::GlobalGet(0),
            I::I32Const(32),
            I::I32Sub,
            I::LocalTee(0),
            I::GlobalSet(0),
        ];
        for suffix in [
            vec![I::I32Const(1), I::GlobalSet(0)],
            vec![I::Return],
            vec![I::Br(0)],
            vec![I::I32Const(9), I::LocalSet(0)],
            vec![I::GlobalGet(0), I::I32Const(32), I::I32Sub, I::GlobalSet(0)],
            vec![],
        ] {
            let mut ops = prologue.to_vec();
            ops.extend(suffix);
            assert!(measure_stack_bound(&single_function(&ops), 0).is_err());
        }
        assert!(
            measure_stack_bound(&single_function(&[I::I32Const(5), I::GlobalSet(0)]), 0).is_err()
        );
    }

    #[test]
    fn a_reference_branch_cannot_bypass_an_otherwise_valid_restoration() {
        use wasm_encoder::Instruction as I;
        let ops = [
            I::GlobalGet(0),
            I::I32Const(32),
            I::I32Sub,
            I::LocalTee(0),
            I::GlobalSet(0),
            I::RefNull(wasm_encoder::HeapType::FUNC),
            I::BrOnNull(0),
            I::Drop,
            I::LocalGet(0),
            I::I32Const(32),
            I::I32Add,
            I::GlobalSet(0),
        ];
        let bytes = single_function(&ops);
        wasmparser::Validator::new()
            .validate_all(&bytes)
            .expect("valid module");
        assert!(
            measure_stack_bound(&bytes, 0)
                .unwrap_err()
                .contains("branch bypasses")
        );
    }

    fn single_function(ops: &[wasm_encoder::Instruction<'_>]) -> Vec<u8> {
        use wasm_encoder::*;
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(1024),
        );
        module.section(&globals);
        let mut function = Function::new([(1, ValType::I32)]);
        for op in ops {
            function.instruction(op);
        }
        function.instruction(&Instruction::End);
        let mut code = CodeSection::new();
        code.function(&function);
        module.section(&code);
        module.finish()
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
        first.instruction(&Instruction::GlobalGet(0));
        first.instruction(&Instruction::I32Const(32));
        first.instruction(&Instruction::I32Add);
        first.instruction(&Instruction::GlobalSet(0));
        first.instruction(&Instruction::End);
        code.function(&first);
        let mut second = Function::new([]);
        second.instruction(&Instruction::GlobalGet(0));
        second.instruction(&Instruction::I32Const(32));
        second.instruction(&Instruction::I32Sub);
        second.instruction(&Instruction::GlobalSet(0));
        second.instruction(&Instruction::Call(0));
        second.instruction(&Instruction::GlobalGet(0));
        second.instruction(&Instruction::I32Const(32));
        second.instruction(&Instruction::I32Add);
        second.instruction(&Instruction::GlobalSet(0));
        second.instruction(&Instruction::End);
        code.function(&second);
        module.section(&code);
        module.finish()
    }
}
