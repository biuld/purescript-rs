import { Link, Navigate, useParams } from 'react-router';
import { CodePane } from '../components/CodePane';
import { ContractPanel } from '../components/ContractPanel';
import { TransformationCanvas } from '../components/TransformationCanvas';
import { exampleById } from '../content/examples';
import { passById, passes } from '../content/passes';
import { representationById } from '../content/representations';

export function PassDetailPage() {
  const { passId } = useParams();
  const pass = passId ? passById[passId] : undefined;
  if (passId && !pass) return <Navigate replace to="/passes" />;

  return (
    <main className="page detail-page pass-detail-page">
      <aside className="pass-sidebar">
        <div className="pass-sidebar-heading">
          <p className="eyebrow">Browse</p>
          <h2>Compiler passes</h2>
          <p>Follow each transformation from its input contract to its output.</p>
        </div>
        <nav aria-label="Compiler passes">
          <ol className="pass-list">
            <li className={!pass ? 'is-current' : undefined}>
              <Link to="/passes" aria-current={!pass ? 'page' : undefined}>
                <span className="pass-list-number">↗</span>
                <span className="pass-list-copy"><strong>All passes</strong><small>P0–P11 overview</small></span>
              </Link>
            </li>
            {passes.map((candidate) => {
              const current = candidate.id === pass?.id;
              return (
                <li key={candidate.id} className={current ? 'is-current' : undefined}>
                  <Link to={`/passes/${candidate.id}`} aria-current={current ? 'page' : undefined}>
                    <span className="pass-list-number">P{candidate.ordinal}</span>
                    <span className="pass-list-copy">
                      <strong>{candidate.name}</strong>
                      <small>{representationById[candidate.input].name} → {representationById[candidate.output].name}</small>
                    </span>
                  </Link>
                </li>
              );
            })}
          </ol>
        </nav>
      </aside>

      <div className="pass-content">
        <nav className="detail-breadcrumb" aria-label="Breadcrumb">
          <Link to="/">Pipeline</Link><span aria-hidden="true">/</span><span>Passes</span>
          {pass && <><span aria-hidden="true">/</span><span>P{pass.ordinal}</span></>}
        </nav>
        {pass ? <PassDetail pass={pass} /> : <PassIndex />}
      </div>
    </main>
  );
}

function PassIndex() {
  return (
    <>
      <header className="pass-index-hero">
        <p className="eyebrow">Pipeline map · {passes.length} passes</p>
        <h1>Compiler passes</h1>
        <p>Each pass has a defined input and output contract. Select one to inspect the transformation, its guarantees, and the nearest observable example.</p>
      </header>
      <section className="pass-index-grid" aria-label="All compiler passes">
        {passes.map((pass) => (
          <Link className="pass-index-card" to={`/passes/${pass.id}`} key={pass.id}>
            <span className="pass-index-meta">
              <span>P{pass.ordinal}</span>
              <span className={`pass-kind-label pass-kind-label--${pass.kind}`}>{pass.kind}</span>
            </span>
            <strong>{pass.name}</strong>
            <p>{pass.purpose}</p>
            <span className="pass-index-flow">
              <span>{representationById[pass.input].name}</span>
              <span aria-hidden="true">→</span>
              <span>{representationById[pass.output].name}</span>
            </span>
          </Link>
        ))}
      </section>
    </>
  );
}

function PassDetail({ pass }: { pass: (typeof passes)[number] }) {
  const example = exampleById.basic;
  const output = example.snapshots.find((snapshot) => snapshot.stage === pass.output);
  const previous = passes[pass.ordinal - 1];
  const next = passes[pass.ordinal + 1];

  return (
    <>
      <header className="detail-hero">
        <div className="detail-heading">
          <span className="detail-index">P{pass.ordinal}</span>
          <div>
            <p className="eyebrow">Compiler pass · {pass.kind}</p>
            <h1>{pass.name}</h1>
          </div>
        </div>
        <p className="detail-lede">{pass.purpose}</p>
      </header>

      <div className="pass-layout">
        <TransformationCanvas pass={pass} />
        <ContractPanel pass={pass} />
      </div>

      <section className="example-section" aria-labelledby="example-heading">
        <div className="section-heading">
          <div><p className="eyebrow">Example · arithmetic function</p><h2 id="example-heading">Nearest observable form</h2></div>
          <Link to="/trace/basic">Open synchronized trace</Link>
        </div>
        <div className="example-grid">
          <CodePane label="PureScript source" value={example.source} emphasis="source" />
          {output ? (
            <div>
              <div className="provenance">{output.provenance} teaching view</div>
              <CodePane label={output.title} value={output.text} />
              <p className="pane-note">{output.note}</p>
            </div>
          ) : (
            <div className="unavailable">
              <strong>No isolated dump at this boundary</strong>
              <p>This pass has a contract and curated transformation, but the current CLI does not expose a standalone snapshot here.</p>
            </div>
          )}
        </div>
      </section>

      <nav className="adjacent-passes" aria-label="Adjacent passes">
        {previous ? <Link to={`/passes/${previous.id}`}><span>Previous pass</span>← P{previous.ordinal} · {previous.name}</Link> : <span />}
        {next ? <Link to={`/passes/${next.id}`}><span>Next pass</span>P{next.ordinal} · {next.name} →</Link> : <span />}
      </nav>
    </>
  );
}
