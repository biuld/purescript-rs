//! Capture producers and surviving slot reads stay anchored to logical CC.
use super::*;
use std::collections::HashSet;

pub(super) fn verify(
    source: &cc::Function,
    function: &Function,
    assignments: &[&cc::Assignment],
    instructions: &[&Instruction],
) -> Result<(), Vec<BackendError>> {
    let states = source
        .values
        .iter()
        .filter(|value| value.ty == cc::ValueShape::State)
        .map(|value| value.id)
        .collect::<HashSet<_>>();
    let slots = cc::state::capture_shapes(source)?;
    let mut expected = Vec::new();
    collect(assignments, &states, &slots, &mut expected);
    let mut actual = Vec::new();
    for instruction in instructions {
        let capture = match instruction {
            Instruction::ClosureNew {
                destination,
                function,
                captures,
                ..
            } => Some((*destination, Capture::New(*function, captures.clone()))),
            Instruction::ClosureGetCapture {
                destination,
                closure,
                index,
                ..
            } => Some((*destination, Capture::Get(*closure, *index))),
            _ => None,
        };
        if let Some(capture) = capture {
            actual.push(capture);
        }
    }
    if actual != expected {
        return Err(error(
            function,
            "MIR capture projection changes a producer, operand, logical slot or scope",
        ));
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Capture {
    New(psrs_hir::SymbolId, Vec<cc::ValueId>),
    Get(cc::ValueId, u32),
}

fn collect(
    assignments: &[&cc::Assignment],
    states: &HashSet<cc::ValueId>,
    slots: &[cc::ValueShape],
    expected: &mut Vec<(cc::ValueId, Capture)>,
) {
    for assignment in assignments {
        match &assignment.kind {
            cc::AssignmentKind::FunctionRef {
                function, captures, ..
            } => {
                expected.push((
                    assignment.destination,
                    Capture::New(
                        *function,
                        captures
                            .iter()
                            .filter(|value| !states.contains(value))
                            .copied()
                            .collect(),
                    ),
                ));
            }
            cc::AssignmentKind::ClosureGetCapture { closure, index }
                if !states.contains(&assignment.destination) =>
            {
                let physical = slots[..*index as usize]
                    .iter()
                    .filter(|shape| **shape != cc::ValueShape::State)
                    .count() as u32;
                expected.push((assignment.destination, Capture::Get(*closure, physical)));
            }
            _ => {}
        }
    }
}
