//! The stack-exhaustion budget's weight model (EH-12): the per-function frame
//! weight `W_f` and the profile-fixed constants the emitted budget check
//! compares against. These are the design's provisional calibration targets
//! (`docs/design/backend/fp/exception-handling-and-stack-exhaustion.md`,
//! "The stack-exhaustion budget"); the coordination fixture of EH-15 is the
//! protocol that validates them under the pinned engine, and this file's
//! documented values record the measured result of that protocol.

/// The fixed part of every frame's estimated native stack weight, in weight
/// units.
pub const FRAME_WEIGHT_FIXED: i32 = 32;

/// The per-local part of every frame's estimated native stack weight, in
/// weight units. `locals(f)` is the lowered function's parameter and local
/// value count; P10's list-copy and dispatcher locals are not known at P9 and
/// stay inside the headroom the budget reserves against the engine limit.
pub const FRAME_WEIGHT_PER_LOCAL: i32 = 16;

/// The budget the emitted prologue check compares the depth global against,
/// in the same weight units. The counter `depth` accumulates the current
/// call stack's weights, so a frame whose prologue pushes `depth` past this
/// value throws the `psrs:stack-exhaustion` tag instead of consuming the
/// native stack the engine's `max_wasm_stack` limit would trap on.
///
/// Calibration contract (EH-15): on the pinned wasmtime baseline with the
/// engine stack limit pinned for budget-dependent evidence, deep non-tail
/// recursion must produce the compiler's stack-exhaustion exception and never
/// the engine's `call stack exhausted` trap, and the corpus's deepest
/// *returning* recursion (TCOMutRec's depth-100 calls) must still return. The
/// value below is the measured outcome of that protocol for wasmtime 48.0.1
/// (7bac2c277 2026-08-24) with `-W max-wasm-stack=524288`; see the checklist
/// record for the measurement.
pub const STACK_BUDGET: i32 = 262_144;

/// The estimated native stack weight of one frame of a lowered function with
/// `locals` parameters and local values:
/// `W_f = FRAME_WEIGHT_FIXED + FRAME_WEIGHT_PER_LOCAL × locals`.
///
/// Recorded on each MIR function as `Function::frame_weight` at P9; the
/// structuring emission (P10) reads it back for the prologue increment and
/// the epilogue/tail decrements.
pub fn frame_weight(locals: usize) -> u32 {
    (FRAME_WEIGHT_FIXED + FRAME_WEIGHT_PER_LOCAL * (locals as i32)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_weight_adds_a_fixed_weight_per_local() {
        // The design's formula: fixed 32, 16 per local.
        assert_eq!(frame_weight(0), FRAME_WEIGHT_FIXED as u32);
        assert_eq!(frame_weight(3), 32 + 16 * 3);
        assert_eq!(
            frame_weight(12),
            (FRAME_WEIGHT_FIXED + FRAME_WEIGHT_PER_LOCAL * 12) as u32
        );
        // The constants are the design's provisional targets.
        assert_eq!(FRAME_WEIGHT_FIXED, 32);
        assert_eq!(FRAME_WEIGHT_PER_LOCAL, 16);
        // The budget leaves the depth-100 returning recursion of the corpus
        // (TCOMutRec) inside the budget: its frames weigh well below
        // `STACK_BUDGET` at depth 100.
        assert!(frame_weight(16) as i64 * 100 < STACK_BUDGET as i64);
    }
}
