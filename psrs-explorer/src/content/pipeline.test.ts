import { describe, expect, it } from 'vitest';
import { passes } from './passes';
import { representationById } from './representations';

describe('pipeline content', () => {
  it('covers P0 through P11 in order', () => {
    expect(passes.map((pass) => pass.ordinal)).toEqual([...Array(12).keys()]);
  });

  it('uses known representations at every pass boundary', () => {
    for (const pass of passes) {
      expect(representationById[pass.input]).toBeDefined();
      expect(representationById[pass.output]).toBeDefined();
    }
  });

  it('marks temporary target forms as non-long-lived', () => {
    expect(representationById.tokens.lifecycle).toBe('temporary');
    expect(representationById.wasm.lifecycle).toBe('temporary');
  });
});
