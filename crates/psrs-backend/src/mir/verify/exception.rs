//! MIR verification of the exception channel: unwind edges, handler blocks,
//! protected regions, and tail calls inside them. The protected-region
//! partition itself is `crate::mir::try_region`; this module checks the
//! terminator-level rules on top of it.

use super::util::{mir_error, value_type};
use crate::mir::tag::Tag;
use crate::mir::try_region::{self, TryRegions};
use crate::mir::{BasicBlock, BlockId, Function, Terminator, ValueType};
use crate::types::ValueId;
use psrs_span::TextRange;
use std::collections::HashMap;

/// Module-level tag checks: the single-payload rule and payload type range.
pub(super) fn verify_tags(
    module: &crate::mir::Module,
    defined_types: super::DefinedTypeId,
    errors: &mut Vec<crate::BackendError>,
) {
    for tag in &module.tags {
        if tag.payload.len() > 1 {
            errors.extend(mir_error(
                tag.span,
                "MIR tag payload has more than one value; multi-value payloads need the multi-value block work",
            ));
        }
        for payload in &tag.payload {
            super::verify_value_type(payload, defined_types, tag.span, errors);
        }
    }
}

/// Per-function checks of every `Try` and `Throw` terminator, plus the
/// protected-region rule that no block inside a region ends in a forced tail
/// call. The operand existence and dominance checks for `Throw` arguments run
/// in `super::function` like every other terminator's.
pub(super) fn verify_function(
    function: &Function,
    blocks: &HashMap<BlockId, &BasicBlock>,
    definitions: &HashMap<ValueId, ValueType>,
    tags: &[Tag],
) -> Result<(), Vec<crate::BackendError>> {
    let regions = try_region::plan(function, "P9 MIR verification")?;
    let mut errors = Vec::new();
    let handlers = regions.handler_blocks();

    // Clause tags resolve, and each clause's handler receives its caught
    // payload as block parameters typed exactly as the tag's payload (none
    // for `catch_all`).
    for region in &regions.regions {
        for (tag_id, _) in &region.catches {
            if tags.get(tag_id.0 as usize).is_none() {
                errors.extend(mir_error(
                    region.span,
                    "MIR catch clause names an unknown tag",
                ));
            }
        }
        for (tag_id, handler) in &region.catches {
            let payload = tags
                .get(tag_id.0 as usize)
                .map(|tag| tag.payload.as_slice())
                .unwrap_or(&[]);
            check_handler_parameters(function, region.span, *handler, payload, &mut errors);
        }
        if let Some(handler) = region.catch_all {
            check_handler_parameters(function, region.span, handler, &[], &mut errors);
        }
    }
    // A handler belongs to exactly one `Try`.
    for (_, span) in &regions.conflicts {
        errors.extend(mir_error(
            *span,
            "MIR handler block is claimed by more than one try region",
        ));
    }
    // A handler has no normal predecessors: every predecessor edge is an
    // unwind edge of a `Throw` naming it or a catch clause of its `Try`.
    if handlers.contains(&function.entry) {
        errors.extend(mir_error(
            function.span,
            "MIR handler block is the function entry",
        ));
    }
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        for target in crate::mir::cfg::successors(terminator) {
            if handlers.contains(&target) {
                errors.extend(mir_error(
                    terminator_span(terminator),
                    "MIR handler block has a normal predecessor",
                ));
            }
        }
    }
    // Control enters the protected entry as a parameterless `Jump` would.
    for region in &regions.regions {
        if blocks
            .get(&region.entry)
            .is_some_and(|block| !block.parameters.is_empty())
        {
            errors.extend(mir_error(
                region.span,
                "MIR try region's protected entry has block parameters",
            ));
        }
    }
    verify_throws(function, &regions, tags, definitions, &mut errors);
    verify_no_forced_tail_calls(function, &regions, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_handler_parameters(
    function: &Function,
    span: TextRange,
    handler: BlockId,
    payload: &[ValueType],
    errors: &mut Vec<crate::BackendError>,
) {
    let Some(block) = function.blocks.iter().find(|block| block.id == handler) else {
        return;
    };
    if block.parameters.len() != payload.len() {
        errors.extend(mir_error(
            span,
            "MIR catch handler parameters differ from the caught payload",
        ));
        return;
    }
    for (parameter, expected) in block.parameters.iter().zip(payload) {
        match value_type(function, *parameter) {
            Some(actual) if actual == *expected => {}
            Some(_) => errors.extend(mir_error(
                span,
                "MIR catch handler parameter has the wrong type",
            )),
            None => errors.extend(mir_error(
                span,
                "MIR catch handler parameter has no value type",
            )),
        }
    }
}

fn verify_throws(
    function: &Function,
    regions: &TryRegions,
    tags: &[Tag],
    definitions: &HashMap<ValueId, ValueType>,
    errors: &mut Vec<crate::BackendError>,
) {
    for block in &function.blocks {
        let Some(Terminator::Throw {
            tag,
            arguments,
            handler,
            span,
        }) = &block.terminator
        else {
            continue;
        };
        let Some(tag_def) = tags.get(tag.0 as usize) else {
            errors.extend(mir_error(*span, "MIR throw names an unknown tag"));
            continue;
        };
        if arguments.len() != tag_def.payload.len() {
            errors.extend(mir_error(
                *span,
                "MIR throw payload arity differs from the tag",
            ));
        } else if let Some(expected) = tag_def.payload.first() {
            match definitions.get(&arguments[0]) {
                Some(actual) if actual == expected => {}
                Some(_) => errors.extend(mir_error(*span, "MIR throw payload has the wrong type")),
                None => errors.extend(mir_error(*span, "MIR throw payload has no value type")),
            }
        }
        // The handler resolves to the innermost enclosing region's matching
        // handler: the first region, walking outward, that catches the tag (a
        // tag clause or `catch_all`). For a throw inside a handler block the
        // owning region's `try_table` is already disarmed, so the enclosing
        // chain starts at the regions containing that try block. Regions
        // nest, so the containing regions form a chain.
        let base = regions.owners.get(&block.id).copied().unwrap_or(block.id);
        let is_handler_block = regions.owners.contains_key(&block.id);
        let mut enclosing = regions
            .regions
            .iter()
            .filter(|region| region.protected.contains(&base))
            .collect::<Vec<_>>();
        enclosing.sort_by_key(|region| region.protected.len());
        let landing = enclosing
            .iter()
            .find(|region| region.matching_handler(*tag).is_some())
            .and_then(|region| region.matching_handler(*tag));
        match handler {
            None => {
                if landing.is_some() {
                    errors.extend(mir_error(
                        *span,
                        "MIR throw must name the matching handler of the innermost enclosing try region",
                    ));
                } else if !is_handler_block
                    && regions
                        .regions
                        .iter()
                        .any(|region| region.protected.contains(&block.id))
                {
                    errors.extend(mir_error(
                        *span,
                        "MIR throw without a handler lies inside a protected try region",
                    ));
                }
            }
            Some(resolved) => match landing {
                None => errors.extend(mir_error(
                    *span,
                    "MIR throw tag is not caught by any enclosing try region",
                )),
                Some(landing) => {
                    if landing != *resolved {
                        errors.extend(mir_error(
                            *span,
                            "MIR throw handler does not resolve to the innermost enclosing region's matching handler",
                        ));
                    }
                }
            },
        }
    }
}

fn verify_no_forced_tail_calls(
    function: &Function,
    regions: &TryRegions,
    errors: &mut Vec<crate::BackendError>,
) {
    let protected = regions.protected_blocks();
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        if protected.contains(&block.id)
            && matches!(
                terminator,
                Terminator::ReturnCall { .. } | Terminator::ReturnCallRef { .. }
            )
        {
            errors.extend(mir_error(
                terminator_span(terminator),
                "MIR protected try-region block ends in a forced tail call",
            ));
        }
    }
}

fn terminator_span(terminator: &Terminator) -> TextRange {
    match terminator {
        Terminator::Trap { span }
        | Terminator::Return { span, .. }
        | Terminator::Jump { span, .. }
        | Terminator::Branch { span, .. }
        | Terminator::Switch { span, .. }
        | Terminator::ReturnCall { span, .. }
        | Terminator::ReturnCallRef { span, .. }
        | Terminator::Throw { span, .. }
        | Terminator::Try { span, .. } => *span,
    }
}
