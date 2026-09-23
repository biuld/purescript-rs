import { Link } from 'react-router';
import { representationById } from '../content/representations';
import type { Pass } from '../content/types';

export function TransformationCanvas({ pass }: { pass: Pass }) {
  const input = representationById[pass.input];
  const output = representationById[pass.output];
  const kindLabel = pass.kind === 'boundary' ? 'Representation boundary' : pass.kind === 'same-representation' ? 'Same representation' : 'Temporary target form';
  return (
    <section className="transformation" aria-labelledby="transformation-heading">
      <header className="transformation-heading">
        <div>
          <p className="eyebrow">Transformation map</p>
          <h2 id="transformation-heading">Input to output</h2>
        </div>
        <span className={`pass-kind-label pass-kind-label--${pass.kind}`}>{kindLabel}</span>
      </header>
      <div className="transformation-flow">
        <Link className="representation-card" to={`/irs/${input.id}`} aria-label={`View the ${input.name} representation contract`}>
          <span className="representation-role">Input representation</span>
          <span className="representation-lifecycle">{input.lifecycle} · {input.family}</span>
          <strong>{input.name}</strong>
          <p>{input.invariant}</p>
          <span className="representation-open">View representation <span aria-hidden="true">↗</span></span>
        </Link>
        <div className="pass-arrow" aria-label={`${pass.name} transforms ${input.name} into ${output.name}`}>
          <span>P{pass.ordinal}</span><strong>{pass.name}</strong><span aria-hidden="true">→</span>
        </div>
        <Link className="representation-card" to={`/irs/${output.id}`} aria-label={`View the ${output.name} representation contract`}>
          <span className="representation-role">Output representation</span>
          <span className="representation-lifecycle">{output.lifecycle} · {output.family}</span>
          <strong>{output.name}</strong>
          <p>{output.invariant}</p>
          <span className="representation-open">View representation <span aria-hidden="true">↗</span></span>
        </Link>
      </div>
    </section>
  );
}
