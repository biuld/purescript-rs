import type { RepresentationId } from './types';

type Finding = {
  title: string;
  status: 'implemented' | 'remaining';
  current: string;
  target: string;
  reason: string;
  evidence: string;
};
type BackendReview = { position: string; findings: Finding[] };

export const backendReview: Partial<Record<RepresentationId, BackendReview>> = {
  cc: {
    position: 'CC IR is the target-neutral contract between Typed Core and the representation planners. Its variants describe semantic operations and logical value shapes; P9 owns GC objects, byte offsets, and target calls.',
    findings: [
      { title: 'One representation per sum type', status: 'implemented', current: 'Constructor lowering now interns one Variant requirement per sum type and uses VariantNew, VariantTag, and VariantGet. Nullary-only sums remain immediate integer tags.', target: 'Keep construction and matching on these semantic operations for every supported data type and target profile.', reason: 'The same CC module can choose GC subtype objects or linear tag-and-payload records in P9.', evidence: 'cc/layout/mod.rs; cc/case/aggregate.rs; DEC-08' },
      { title: 'Reserve casts for erased values', status: 'implemented', current: 'CC verifies that RepresentationTest and RepresentationCast cross an erased-reference boundary. Constructor matching compares VariantTag.', target: 'Maintain this rule as new patterns and representation adaptations are added.', reason: 'Constructor dispatch must not commit CC to GC reference tests.', evidence: 'cc/verify/adaptation.rs; cc/case/aggregate.rs; D-06 P8' },
      { title: 'Finish erased adaptation on MVP', status: 'remaining', current: 'Linear lowering copies compatible erased reference handles; erased RepresentationTest still reports a P9 diagnostic.', target: 'Define runtime type evidence for any dynamic erased test that must work on MVP, then verify and lower it explicitly.', reason: 'An unchecked pointer copy cannot answer a dynamic type-test question.', evidence: 'mir/lower_linear/mod.rs; D-08; D-10' },
    ],
  },
  mir: {
    position: 'MIR is a target-specific, language-independent SSA CFG. P9 selects concrete types, layouts, memory resources, and call conventions; MIR verification checks those choices before P10 encodes them.',
    findings: [
      { title: 'Check target legality at MIR', status: 'implemented', current: 'P9 and P10 call the target-aware MIR verifier with the selected TargetCapabilities. The capability check lives beside structural MIR verification.', target: 'Keep every new concrete instruction and type covered by this check.', reason: 'Unsupported target features fail where concrete MIR is produced.', evidence: 'mir/verify/capability.rs; mir/mod.rs; D-06 P9' },
      { title: 'Validate subtyping', status: 'implemented', current: 'MIR checks an earlier non-final supertype, struct and array fields, reference nullability, and function parameter/result variance.', target: 'Keep subtype validation aligned with the supported Wasm type model as that model grows.', reason: 'A valid type index alone does not establish safe GC field or function use.', evidence: 'mir/verify/subtype.rs; D-06 MIR verifier' },
      { title: 'Make linear accesses self-describing', status: 'remaining', current: 'LinearLoad and LinearStore now record memory identity and a verified alignment. P9 conservatively emits alignment 1; object provenance and bounds are still not recorded.', target: 'Carry a planned object/layout identity or equivalent evidence so MIR can check each access width and offset against its allocation.', reason: 'The verifier should establish memory safety from MIR without reconstructing P9 planner state.', evidence: 'mir/instruction.rs; mir/verify/instruction/memory.rs; D-10' },
      { title: 'Complete platform calls for MVP', status: 'remaining', current: 'The linear path lowers aggregates, arrays, variants, and table closures; canonical WIT calls still receive a P9 diagnostic.', target: 'Materialize supported canonical ABI adapters as ordinary MIR for the linear profile.', reason: 'The same semantic CC call should work under every advertised target profile.', evidence: 'mir/lower_linear/mod.rs; D-07; D-06 M7' },
    ],
  },
  wasm: {
    position: 'Structured Wasm remains a short-lived encoding form after verified MIR. It models control regions and delegates leaf instructions to wasm-encoder.',
    findings: [
      { title: 'Keep Op deliberately small', status: 'implemented', current: 'Op has Leaf and If; leaf instructions use wasm_encoder::Instruction.', target: 'Add another region variant only when a new MIR control-flow shape requires it.', reason: 'The target layer stays an encoder, not another optimization IR.', evidence: 'wasm/mod.rs; DEC-02; D-06 P10/P11' },
      { title: 'Make encoding mechanical', status: 'implemented', current: 'P10 checks MIR under the target profile, assigns typed final indices, structures control flow, and verifies the thin form.', target: 'Keep layout selection, concrete type creation, and ABI adaptation in P9 as target coverage grows.', reason: 'Encoding should preserve decisions already checked at the MIR boundary.', evidence: 'wasm/lower/mod.rs; wasm/verify.rs; D-06 P10/P11' },
    ],
  },
};
