//! Verify encoded application contracts before executing a checked link plan.
use crate::{CheckedLinkPlan, CoreSignature, CoreType, LinkErrors, LinkStage};
use wasmparser::{CompositeInnerType, Operator, Parser, Payload, TypeRef, ValType};

pub(crate) fn verify(plan: &CheckedLinkPlan, bytes: &[u8]) -> Result<(), LinkErrors> {
    check(plan, bytes).map_err(|message| LinkErrors::plain(LinkStage::Compose, message))
}

fn check(plan: &CheckedLinkPlan, bytes: &[u8]) -> Result<(), String> {
    wasmparser::Validator::new()
        .validate_all(bytes)
        .map_err(|error| error.to_string())?;
    let mut types = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut memory = None;
    let mut data = Vec::new();
    let mut function_types = Vec::new();
    let mut function_bodies = Vec::new();
    let mut function_exports = Vec::new();
    let mut imported_functions = 0_u32;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|error| error.to_string())? {
            Payload::TypeSection(reader) => {
                for group in reader {
                    for ty in group.map_err(|error| error.to_string())?.into_types() {
                        types.push(match ty.composite_type.inner {
                            CompositeInnerType::Func(ty) => Some(ty),
                            _ => None,
                        });
                    }
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|error| error.to_string())?;
                    let TypeRef::Func(index) = import.ty else {
                        return Err("application imports an unplanned non-function item".into());
                    };
                    imported_functions += 1;
                    let binding = plan
                        .bindings()
                        .iter()
                        .find(|binding| {
                            binding.module == import.module && binding.field == import.name
                        })
                        .ok_or_else(|| {
                            format!(
                                "application has an unplanned import `{}.{}`",
                                import.module, import.name
                            )
                        })?;
                    let ty = types
                        .get(index as usize)
                        .and_then(Option::as_ref)
                        .ok_or("application import has no function signature")?;
                    let signature = CoreSignature {
                        parameters: ty
                            .params()
                            .iter()
                            .copied()
                            .map(scalar)
                            .collect::<Result<_, _>>()?,
                        result: match ty.results() {
                            [] => None,
                            [ty] => Some(scalar(*ty)?),
                            _ => {
                                return Err(
                                    "application import has unsupported multiple results".into()
                                );
                            }
                        },
                    };
                    if signature != binding.signature {
                        return Err(
                            "application import signature disagrees with the checked plan".into(),
                        );
                    }
                    if !seen.insert((binding.module.clone(), binding.field.clone())) {
                        return Err("application duplicates a planned import".into());
                    }
                }
            }
            Payload::MemorySection(reader) => {
                for ty in reader {
                    let ty = ty.map_err(|error| error.to_string())?;
                    if memory.replace(ty).is_some() || ty.memory64 || ty.shared {
                        return Err("application memory differs from the checked plan".into());
                    }
                }
            }
            Payload::DataSection(reader) => {
                for segment in reader {
                    let segment = segment.map_err(|error| error.to_string())?;
                    if let wasmparser::DataKind::Active {
                        memory_index,
                        offset_expr,
                    } = segment.kind
                    {
                        let mut ops = offset_expr.get_operators_reader();
                        let Operator::I32Const { value } =
                            ops.read().map_err(|error| error.to_string())?
                        else {
                            return Err("application data offset is not constant".into());
                        };
                        if memory_index != 0
                            || !matches!(
                                ops.read().map_err(|error| error.to_string())?,
                                Operator::End
                            )
                        {
                            return Err("application data has an unplanned initialization".into());
                        }
                        data.push((value as u32, segment.data));
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for ty in reader {
                    function_types.push(ty.map_err(|error| error.to_string())?);
                }
            }
            Payload::CodeSectionEntry(body) => function_bodies.push(body),
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|error| error.to_string())?;
                    if export.kind == wasmparser::ExternalKind::Func {
                        function_exports.push((export.name.to_string(), export.index));
                    }
                }
            }
            Payload::StartSection { .. } => {
                return Err("application start precedes planned shim resolution".into());
            }
            _ => {}
        }
    }
    if let Some(boundary) = plan.memory().heap_getter.as_ref() {
        let (_, index) = function_exports
            .iter()
            .find(|(name, _)| name == &boundary.field)
            .ok_or("application does not export the heap getter")?;
        let defined = index
            .checked_sub(imported_functions)
            .ok_or("application heap getter must be defined locally")?
            as usize;
        let ty = function_types
            .get(defined)
            .and_then(|index| types.get(*index as usize))
            .and_then(Option::as_ref)
            .ok_or("application heap getter has no signature")?;
        if !ty.params().is_empty() || ty.results() != [ValType::I32] {
            return Err("application heap getter must have signature () -> i32".into());
        }
        let body = function_bodies
            .get(defined)
            .ok_or("application heap getter has no body")?;
        if body
            .get_locals_reader()
            .map_err(|error| error.to_string())?
            .get_count()
            != 0
        {
            return Err("application heap getter must have no locals".into());
        }
        let mut ops = body
            .get_operators_reader()
            .map_err(|error| error.to_string())?;
        if !matches!(ops.read().map_err(|error| error.to_string())?, Operator::I32Const { value }
            if value as u32 == plan.memory().heap_start)
            || !matches!(
                ops.read().map_err(|error| error.to_string())?,
                Operator::End
            )
            || !ops.eof()
        {
            return Err("application heap getter must return only the checked constant".into());
        }
    }

    for binding in plan.bindings() {
        if !seen.contains(&(binding.module.clone(), binding.field.clone())) {
            return Err(format!(
                "application omits planned import `{}.{}`",
                binding.module, binding.field
            ));
        }
    }
    if memory.is_some_and(|memory| memory.initial < plan.memory().minimum_pages)
        || (!plan.artifacts().is_empty() && memory.is_none())
    {
        return Err("application memory does not cover the checked reservations".into());
    }
    for (start, bytes) in data {
        let end = start
            .checked_add(u32::try_from(bytes.len()).map_err(|_| "data length overflows")?)
            .ok_or("application data range overflows")?;
        let scratch = plan.memory().canonical_scratch;
        let allocator = plan.memory().allocator_state;
        if start >= allocator.0 && end <= allocator.1 {
            if start != allocator.0
                || bytes.len() != 8
                || bytes[..4] != [0; 4]
                || bytes[4..] != plan.memory().heap_start.to_le_bytes()
            {
                return Err("application allocator state disagrees with the checked plan".into());
            }
        } else if start < scratch.0 || end > scratch.1 {
            return Err("application active data overlaps an unowned region".into());
        }
    }
    Ok(())
}

fn scalar(ty: ValType) -> Result<CoreType, String> {
    match ty {
        ValType::I32 => Ok(CoreType::I32),
        ValType::I64 => Ok(CoreType::I64),
        ValType::F32 => Ok(CoreType::F32),
        ValType::F64 => Ok(CoreType::F64),
        _ => Err("a target import has an unchecked reference representation".into()),
    }
}
