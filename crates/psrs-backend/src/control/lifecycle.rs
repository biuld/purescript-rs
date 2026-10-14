//! Extent helper bodies discharge lifecycle semantics independently of callees.
use super::events::Event;
use super::owned::OwnedOperands;
use crate::cc::{ControlHelperRole as R, ReprId, RepresentationTable};
use crate::types::ValueId;

pub(crate) struct Schema {
    pub boundary: ReprId,
    pub status: ReprId,
    pub check: psrs_hir::SymbolId,
}
impl Schema {
    pub fn checked(table: &RepresentationTable) -> Result<Self, &'static str> {
        let replay = table
            .control_replay_schema()
            .ok_or("lifecycle has no checked extent schema")?;
        let check = table
            .control_helper(R::Check)
            .ok_or("lifecycle has no Check contract")?
            .function();
        Ok(Self {
            boundary: replay.boundary,
            status: replay.status,
            check,
        })
    }
}

pub(crate) fn verify(
    schema: &Schema,
    view: &impl OwnedOperands,
    role: R,
    input: ValueId,
    result: Option<ValueId>,
    next: Option<&[ValueId]>,
    events: &[Event],
) -> Result<i32, &'static str> {
    let input = view.origin(input);
    let selected = events
        .iter()
        .filter_map(|event| match event {
            Event::Select { value, tag } if view.tag(*value) == Some((schema.boundary, input)) => {
                Some(*tag)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [tag @ 0..=1] = selected.as_slice() else {
        return Err("lifecycle must select its current extent case");
    };
    let is_cell =
        |cell| view.projection(cell, schema.boundary, Some(*tag as u32)) == Some((input, 0));
    let is_parent = |value| view.projection(value, schema.boundary, Some(1)) == Some((input, 1));
    let mut read = None;
    let mut guarded = false;
    let mut written = false;
    let mut advanced = false;
    for event in events {
        match event {
            Event::Refine { .. } => {}
            Event::Select { value, tag: actual }
                if view.tag(*value) == Some((schema.boundary, input)) && actual == tag => {}
            Event::Read {
                repr,
                cell,
                index,
                result,
            } if role == R::Check
                && *repr == schema.status
                && is_cell(*cell)
                && view.integer(*index) == Some(0)
                && read.is_none() =>
            {
                read = Some(view.origin(*result));
            }
            Event::Select { value, tag: 0 }
                if role == R::Check && read == Some(view.origin(*value)) && !guarded =>
            {
                guarded = true;
            }
            Event::Store {
                repr,
                cell,
                index,
                value,
            } if role == R::Close
                && *repr == schema.status
                && is_cell(*cell)
                && view.integer(*index) == Some(0)
                && view.integer(*value) == Some(1)
                && !written =>
            {
                written = true;
            }
            Event::Direct {
                function,
                arguments,
            } if role == R::Check
                && *tag == 1
                && guarded
                && *function == schema.check
                && arguments.len() == 1
                && is_parent(arguments[0])
                && !advanced =>
            {
                advanced = true;
            }
            Event::Advance { arguments }
                if role == R::Check
                    && *tag == 1
                    && guarded
                    && arguments.len() == 1
                    && is_parent(arguments[0])
                    && !advanced =>
            {
                advanced = true;
            }
            _ => return Err("lifecycle body contains an unaccounted operation"),
        }
    }
    if role == R::Close {
        if !written || next.is_some() || result.map(|value| view.origin(value)) != Some(input) {
            return Err("Close must write its own closed status and return its extent");
        }
    } else if !guarded {
        return Err("Check must guard its own live status before returning or checking ancestry");
    } else if *tag == 0 {
        if next.is_some() || result.map(|value| view.origin(value)) != Some(input) {
            return Err("root Check must return its checked extent");
        }
    } else if !advanced || !next.is_some_and(|args| args.len() == 1 && is_parent(args[0])) {
        return Err("child Check must continue with its own parent");
    }
    Ok(*tag)
}
