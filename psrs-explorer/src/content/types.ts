export type RepresentationId =
  | 'source'
  | 'tokens'
  | 'cst'
  | 'ast'
  | 'hir'
  | 'thir'
  | 'core'
  | 'cc'
  | 'mir'
  | 'wasm'
  | 'artifact';

export type PassKind = 'boundary' | 'same-representation' | 'target-encoding';

export type Representation = {
  id: RepresentationId;
  name: string;
  family: string;
  lifecycle: 'long-lived' | 'temporary' | 'source' | 'artifact';
  purpose: string;
  invariant: string;
  contains: string[];
  excludes: string[];
  rationale: string;
};

export type Pass = {
  id: string;
  ordinal: number;
  name: string;
  input: RepresentationId;
  output: RepresentationId;
  kind: PassKind;
  purpose: string;
  guarantees: string[];
  exclusions: string[];
  nearestCommand?: string;
};

export type Snapshot = {
  stage: RepresentationId;
  title: string;
  provenance: 'generated' | 'curated';
  text: string;
  note: string;
};

export type Example = {
  id: string;
  name: string;
  source: string;
  snapshots: Snapshot[];
};
