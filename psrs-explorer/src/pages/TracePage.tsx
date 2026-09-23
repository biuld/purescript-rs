import { Link, Navigate, useParams } from 'react-router';
import { useMemo, useState } from 'react';
import { CodePane } from '../components/CodePane';
import { examples, exampleById } from '../content/examples';
import { representationById } from '../content/representations';
import type { RepresentationId } from '../content/types';

export function TracePage() {
  const { exampleId } = useParams();
  const example = exampleId ? exampleById[exampleId] : undefined;
  const [stage, setStage] = useState<RepresentationId>('cst');
  const snapshot = useMemo(
    () => example?.snapshots.find((candidate) => candidate.stage === stage) ?? example?.snapshots[0],
    [example, stage],
  );
  if (!example || !snapshot) return <Navigate replace to="/trace/basic" />;

  return (
    <main className="page detail-page trace-page">
      <aside className="trace-sidebar">
        <div className="trace-sidebar-heading">
          <p className="eyebrow">Browse</p>
          <h2>Programs</h2>
          <p>Choose a sample program to follow through the compiler.</p>
        </div>
        <nav aria-label="Trace examples">
          <ol className="trace-sample-list">
            {examples.map((candidate, index) => {
              const current = candidate.id === example.id;
              return (
                <li key={candidate.id} className={current ? 'is-current' : undefined}>
                  <Link to={`/trace/${candidate.id}`} aria-current={current ? 'page' : undefined}>
                    <span className="trace-sample-number">{String(index + 1).padStart(2, '0')}</span>
                    <span className="trace-sample-copy">
                      <strong>{candidate.name}</strong><small>{candidate.id}</small>
                    </span>
                    <span className="trace-sample-open" aria-hidden="true">↗</span>
                  </Link>
                </li>
              );
            })}
          </ol>
        </nav>
      </aside>

      <div className="trace-content">
        <header className="page-intro">
          <p className="eyebrow">Program trace · {example.id}</p>
          <h1>{example.name}</h1>
          <p>Compare the original source with an available compiler representation.</p>
        </header>
        <div className="trace-controls">
          <fieldset>
            <legend>Available snapshot</legend>
            <div className="stage-rail">
              {example.snapshots.map((candidate) => (
                <button
                  key={candidate.stage}
                  type="button"
                  aria-pressed={snapshot.stage === candidate.stage}
                  onClick={() => setStage(candidate.stage)}
                >
                  {representationById[candidate.stage].name}
                </button>
              ))}
            </div>
          </fieldset>
        </div>
        <section className="trace-grid" aria-label="Source and selected representation">
          <CodePane label="PureScript source" value={example.source} emphasis="source" />
          <div>
            <div className="provenance">{snapshot.provenance} teaching view</div>
            <CodePane label={snapshot.title} value={snapshot.text} />
            <p className="pane-note">{snapshot.note}</p>
          </div>
        </section>
        <section className="mapping-inspector" aria-labelledby="mapping-heading">
          <p className="eyebrow">Mapping inspector</p>
          <h2 id="mapping-heading">No false one-to-one correspondence</h2>
          <p>The initial trace introduces the selected representation and its provenance. Source-range and node links will appear with generated snapshots; lowerings that combine, duplicate, or synthesize values will declare that relation instead of highlighting a misleading match.</p>
        </section>
      </div>
    </main>
  );
}
