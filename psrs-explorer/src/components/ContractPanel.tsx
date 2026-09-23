import { representationById } from '../content/representations';
import type { Pass } from '../content/types';

export function ContractPanel({ pass }: { pass: Pass }) {
  const input = representationById[pass.input];
  const output = representationById[pass.output];
  return (
    <aside className="contract-panel" aria-label={`${pass.name} contract`}>
      <header className="contract-panel-heading">
        <p className="eyebrow">Pass contract</p>
        <h2>What changes from {input.name} to {output.name}</h2>
      </header>
      <section>
        <h3><span className="contract-marker contract-marker--guarantee" aria-hidden="true">✓</span> Guaranteed after this pass</h3>
        <ul>{pass.guarantees.map((guarantee) => <li key={guarantee}>{guarantee}</li>)}</ul>
      </section>
      <section>
        <h3><span className="contract-marker contract-marker--excluded" aria-hidden="true">−</span> Still excluded</h3>
        <ul>{pass.exclusions.map((exclusion) => <li key={exclusion}>{exclusion}</li>)}</ul>
      </section>
      {pass.nearestCommand && <section className="command"><h3>Nearest live command</h3><code>{pass.nearestCommand}</code></section>}
    </aside>
  );
}
