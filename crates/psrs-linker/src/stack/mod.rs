//! Static linear-stack bound analysis for a nonrecursive core artifact.
//!
//! Each function's prologue subtracts its frame size from the stack-pointer
//! global. The maximum bytes an artifact can use is the largest sum of frame
//! sizes along a path reachable from an exported function or start function.
//! Reachable cycles and indirect calls are rejected rather than bounded.

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
/// Exported tables are also rejected: callers could otherwise enter a private
/// function through them. Modules with no entry points are analyzed in full.
pub fn measure_stack_bound(bytes: &[u8], stack_pointer_global: u32) -> Result<StackBound, String> {
    measure_stack_bound_with_imports(bytes, stack_pointer_global, &[])
}

/// Imported function indices proven by composition to use no linear stack.
/// All other imported calls retain the conservative rejection behavior.
pub(crate) fn measure_stack_bound_with_imports(
    bytes: &[u8],
    stack_pointer_global: u32,
    stackless_imports: &[u32],
) -> Result<StackBound, String> {
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
    let mut bodies = Vec::new();
    let mut roots = Vec::new();
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
                bodies.push(analyze_body(&body, stack_pointer_global));
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|error| error.to_string())?;
                    match export.kind {
                        wasmparser::ExternalKind::Func => roots.push(export.index),
                        wasmparser::ExternalKind::Table => {
                            return Err(
                                "an exported table makes runtime entry points unknown".into()
                            );
                        }
                        _ => {}
                    }
                }
            }
            Payload::StartSection { func, .. } => roots.push(func),
            _ => {}
        }
    }

    let count = bodies.len();
    // A private function can only execute through an entry point's call graph.
    // Unsupported indirect calls are still rejected on every reachable path.
    // Definition-only test modules retain conservative whole-module analysis.
    if roots.is_empty() {
        roots.extend((0..count).map(|index| index as u32 + imported_functions as u32));
    }
    let mut memo = vec![None::<u32>; count];
    let mut visiting = vec![false; count];
    let mut bound = 0_u32;
    for root in roots {
        let index = (root as usize)
            .checked_sub(imported_functions)
            .filter(|index| *index < count)
            .ok_or("an imported or unknown entry point makes the stack bound unknown")?;
        bound = bound.max(visit(
            index,
            &bodies,
            &mut memo,
            &mut visiting,
            imported_functions,
            stackless_imports,
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
                let restores = !restored
                    && matches!(operators.get(index + 1), Some(Operator::I32Const { value }) if *value as u32 == frame)
                    && matches!(operators.get(index + 2), Some(Operator::I32Add))
                    && matches!(operators.get(index + 3), Some(Operator::GlobalSet { global_index }) if *global_index == stack_pointer_global);
                // LLVM leaves the epilogue inside the function's wrapper block,
                // optionally reloads the return value, and returns immediately.
                // That return leaves the function, so the restore covers it
                // without blessing any later path.
                let covers_return = match operators.get(index + 4) {
                    Some(Operator::Return) => Some(5),
                    Some(Operator::LocalGet { .. })
                        if matches!(operators.get(index + 5), Some(Operator::Return)) =>
                    {
                        Some(6)
                    }
                    _ => None,
                };
                if restores && (depth == 0 || covers_return.is_some()) {
                    if depth == 0 {
                        restored = true;
                        index += 4;
                    } else {
                        index += covers_return.expect("the return is covered");
                    }
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
    bodies: &[Result<(u32, Vec<u32>), String>],
    memo: &mut [Option<u32>],
    visiting: &mut [bool],
    imported_functions: usize,
    stackless_imports: &[u32],
) -> Result<u32, String> {
    if let Some(value) = memo[index] {
        return Ok(value);
    }
    if visiting[index] {
        return Err("a recursive call graph is not a supported runtime library".into());
    }
    visiting[index] = true;
    let (frame, callees) = bodies[index].as_ref().map_err(Clone::clone)?;
    let mut deepest = 0_u32;
    for callee in callees {
        let callee = *callee as usize;
        if callee < imported_functions {
            if stackless_imports.contains(&(callee as u32)) {
                continue;
            }
            return Err("a call into an imported function makes the stack bound unknown".into());
        }
        let defined = callee - imported_functions;
        if defined < bodies.len() {
            deepest = deepest.max(visit(
                defined,
                bodies,
                memo,
                visiting,
                imported_functions,
                stackless_imports,
            )?);
        } else {
            return Err("callee is outside the analyzed module".into());
        }
    }
    visiting[index] = false;
    let total = frame.checked_add(deepest).ok_or("stack bound overflows")?;
    memo[index] = Some(total);
    Ok(total)
}

#[cfg(test)]
mod tests;
