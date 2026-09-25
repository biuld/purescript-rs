import { useEffect, useRef } from 'react';
import { Link } from 'react-router';
import { passes } from '../content/passes';
import { representationById } from '../content/representations';

type PipelineMapProps = { activePassId?: string };

export function PipelineMap({ activePassId }: PipelineMapProps) {
  const activeStepRef = useRef<HTMLLIElement>(null);

  useEffect(() => {
    activeStepRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
  }, [activePassId]);

  return (
    <ol className="pipeline-map" aria-label="Compiler pipeline">
      {passes.map((pass) => {
        const active = pass.id === activePassId;
        const input = representationById[pass.input];
        const output = representationById[pass.output];
        return (
          <li key={pass.id} ref={active ? activeStepRef : undefined} className={`pipeline-step ${active ? 'is-active' : ''}`}>
            <Link to={`/passes/${pass.id}`} aria-current={active ? 'step' : undefined}>
              <span className="step-number">P{pass.ordinal}</span>
              <span className="step-name">{pass.name}</span>
              <span className="step-io">{input.name} → {output.name}</span>
              <span className="step-kind">{pass.kind === 'boundary' ? 'IR boundary' : pass.kind === 'same-representation' ? 'same IR' : 'target form'}</span>
            </Link>
          </li>
        );
      })}
    </ol>
  );
}
