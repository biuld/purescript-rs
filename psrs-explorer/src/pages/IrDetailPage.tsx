import { Link, Navigate, useParams } from 'react-router';
import { backendEnums } from '../content/backendEnums';
import { backendReview } from '../content/backendReview';
import { passes } from '../content/passes';
import { representations } from '../content/representations';
import type { Pass, Representation } from '../content/types';

export function IrDetailPage() {
  const { representationId } = useParams();
  const representation = representations.find((candidate) => candidate.id === representationId);
  if (representationId && !representation) return <Navigate replace to="/irs" />;

  const entering = representation ? passes.filter((pass) => pass.output === representation.id) : [];
  const leaving = representation ? passes.filter((pass) => pass.input === representation.id) : [];

  return (
    <main className="page detail-page ir-browser-page">
      <aside className="ir-sidebar">
        <div className="ir-sidebar-heading">
          <p className="eyebrow">Browse</p>
          <h2>Representations</h2>
          <p>Choose a form to inspect its contract and place in the pipeline.</p>
        </div>
        <nav aria-label="Representations">
          <ol className="ir-sidebar-list">
            <li className={!representation ? 'is-current' : undefined}>
              <Link to="/irs" aria-current={!representation ? 'page' : undefined}>
                <span className="ir-sidebar-number">00</span>
                <span className="ir-sidebar-copy"><strong>All representations</strong><small>Compiler map</small></span>
                <span className="ir-sidebar-status">index</span>
              </Link>
            </li>
            {representations.map((candidate, index) => {
              const current = candidate.id === representation?.id;
              return (
                <li key={candidate.id} className={current ? 'is-current' : undefined}>
                  <Link to={`/irs/${candidate.id}`} aria-current={current ? 'page' : undefined}>
                    <span className="ir-sidebar-number">{String(index + 1).padStart(2, '0')}</span>
                    <span className="ir-sidebar-copy">
                      <strong>{candidate.name}</strong><small>{candidate.family}</small>
                    </span>
                    <span className={`ir-sidebar-status ir-sidebar-status--${candidate.lifecycle}`}>
                      {candidate.lifecycle}
                    </span>
                  </Link>
                </li>
              );
            })}
          </ol>
        </nav>
      </aside>

      <div className="ir-detail-content">
        <nav className="detail-breadcrumb" aria-label="Breadcrumb">
          <Link to="/">Pipeline</Link><span aria-hidden="true">/</span><span>Representations</span>
        </nav>
        {representation ? (
          <RepresentationDetail representation={representation} entering={entering} leaving={leaving} />
        ) : <RepresentationIndex />}
      </div>
    </main>
  );
}

function RepresentationIndex() {
  const longLivedCount = representations.filter((item) => item.lifecycle === 'long-lived').length;
  const temporaryCount = representations.filter((item) => item.lifecycle === 'temporary').length;

  return (
    <>
      <header className="ir-index-hero">
        <p className="eyebrow">Compiler map · {representations.length} forms</p>
        <h1>Representations, end to end</h1>
        <p>Browse each form the program takes through the compiler, from source text to the final Wasm artifact.</p>
      </header>
      <div className="ir-index-stats" aria-label="Representation counts">
        <div><strong>{longLivedCount}</strong><span>long-lived IRs</span></div>
        <div><strong>{temporaryCount}</strong><span>temporary forms</span></div>
        <div><strong>{passes.length}</strong><span>pipeline passes</span></div>
      </div>
      <section className="ir-index-grid" aria-label="All representations">
        {representations.map((item, index) => (
          <Link className="ir-index-card" to={`/irs/${item.id}`} key={item.id}>
            <span className="ir-index-card-top">
              <span>{String(index + 1).padStart(2, '0')}</span>
              <span className={`ir-sidebar-status ir-sidebar-status--${item.lifecycle}`}>{item.lifecycle}</span>
            </span>
            <strong>{item.name}</strong>
            <span className="ir-index-family">{item.family}</span>
            <p>{item.purpose}</p>
            <span className="ir-index-open">Inspect contract <span aria-hidden="true">↗</span></span>
          </Link>
        ))}
      </section>
    </>
  );
}

function RepresentationDetail({
  representation,
  entering,
  leaving,
}: {
  representation: Representation;
  entering: Pass[];
  leaving: Pass[];
}) {
  return (
    <>
      <header className="detail-hero">
        <div className="detail-heading">
          <span className={`detail-index detail-index--ir detail-index--${representation.lifecycle}`}>
            {representation.id === 'artifact' ? 'WASM' : representation.id.toUpperCase()}
          </span>
          <div>
            <p className="eyebrow">{representation.family} · {representation.lifecycle}</p>
            <h1>{representation.name}</h1>
          </div>
        </div>
        <p className="detail-lede">{representation.purpose}</p>
      </header>

      <section className="ir-summary-grid" aria-label="Representation contract and role">
        <article className="ir-summary-card ir-summary-card--invariant">
          <p className="eyebrow">Invariant</p>
          <p className="ir-summary-text">{representation.invariant}</p>
        </article>
        <article className="ir-summary-card">
          <p className="eyebrow">Role in the pipeline</p>
          <p className="ir-summary-text">{representation.rationale}</p>
        </article>
      </section>

      {backendEnums[representation.id] && (
        <BackendDesign representation={representation} />
      )}

      <section className="ir-inventory-grid" aria-label="Representation contents">
        <article className="ir-inventory-card ir-inventory-card--contains">
          <header>
            <span className="ir-inventory-number">01</span>
            <div><p className="eyebrow">Inside this form</p><h2>It contains</h2></div>
          </header>
          <ul>{representation.contains.map((item) => <li key={item}><span aria-hidden="true">+</span>{item}</li>)}</ul>
        </article>
        <article className="ir-inventory-card ir-inventory-card--excludes">
          <header>
            <span className="ir-inventory-number">02</span>
            <div><p className="eyebrow">Outside this form</p><h2>It excludes</h2></div>
          </header>
          <ul>{representation.excludes.map((item) => <li key={item}><span aria-hidden="true">−</span>{item}</li>)}</ul>
        </article>
      </section>

      <section className="ir-connections" aria-labelledby="connections-heading">
        <header className="ir-section-heading">
          <div>
            <p className="eyebrow">Pipeline connections</p>
            <h2 id="connections-heading">Where this representation moves</h2>
          </div>
          <p>Follow the passes that create and consume this form.</p>
        </header>
        <div className="ir-connection-grid">
          <ConnectionCard direction="incoming" representation={representation} passes={entering} />
          <ConnectionCard direction="outgoing" representation={representation} passes={leaving} />
        </div>
      </section>
    </>
  );
}

function BackendDesign({ representation }: { representation: Representation }) {
  const enums = backendEnums[representation.id];
  const review = backendReview[representation.id];
  if (!enums || !review) return null;
  const variantCount = enums.reduce((count, item) => count + item.variants.length, 0);

  return (
    <section className="backend-design" aria-labelledby="backend-design-heading">
      <header className="ir-section-heading">
        <div>
          <p className="eyebrow">Backend IR review</p>
          <h2 id="backend-design-heading">Current variants and target design</h2>
        </div>
        <p>{enums.length} Rust enums · {variantCount} current variants</p>
      </header>
      <p className="backend-design-position">{review.position}</p>
      <div className="backend-design-review">
        <h3>Design assessment</h3>
        <ol>
          {review.findings.map((finding) => (
            <li key={finding.title}>
              <h4>{finding.title} <span className={`backend-review-status backend-review-status--${finding.status}`}>{finding.status}</span></h4>
              <p><strong>Current:</strong> {finding.current}</p>
              <p><strong>Target:</strong> {finding.target}</p>
              <p><strong>Why:</strong> {finding.reason}</p>
              <small>Evidence: {finding.evidence}</small>
            </li>
          ))}
        </ol>
      </div>
      <div className="backend-enum-heading">
        <h3>Current enum variants</h3>
        <p>Names below match the Rust source. Remaining design work is marked in the assessment above.</p>
      </div>
      {enums.map((item) => (
        <section className="backend-enum" key={item.name} aria-label={`${item.name} variants`}>
          <header>
            <div><h4><code>{item.name}</code></h4><p>{item.purpose}</p></div>
            <code className="backend-enum-source">{item.source}</code>
          </header>
          <ul className="backend-variant-list">
            {item.variants.map((variant) => (
              <li key={variant.name}><code>{variant.name}</code><span>{variant.purpose}</span></li>
            ))}
          </ul>
        </section>
      ))}
    </section>
  );
}

function ConnectionCard({
  direction,
  representation,
  passes: connectedPasses,
}: {
  direction: 'incoming' | 'outgoing';
  representation: Representation;
  passes: Pass[];
}) {
  const incoming = direction === 'incoming';
  const heading = incoming ? 'Produced by' : 'Consumed by';

  return (
    <article className="ir-connection-card">
      <header>
        <span className="connection-direction" aria-hidden="true">{incoming ? '←' : '→'}</span>
        <div><p className="eyebrow">{direction}</p><h3>{heading}</h3></div>
      </header>
      {connectedPasses.length ? (
        <ul className="ir-connection-list">
          {connectedPasses.map((pass) => {
            const adjacentId = incoming ? pass.input : pass.output;
            const adjacent = representations.find((candidate) => candidate.id === adjacentId);
            const inputName = incoming ? adjacent?.name : representation.name;
            const outputName = incoming ? representation.name : adjacent?.name;
            return (
              <li key={pass.id}>
                <Link to={`/passes/${pass.id}`}>
                  <span className="connection-pass-number">P{pass.ordinal}</span>
                  <span className="connection-pass-copy">
                    <strong>{pass.name}</strong><small>{inputName} → {outputName}</small>
                  </span>
                  <span className="connection-open" aria-hidden="true">↗</span>
                </Link>
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="connection-empty">
          {incoming
            ? 'This is the starting form; no pass produces it.'
            : 'This is the final artifact; no later pass consumes it.'}
        </p>
      )}
    </article>
  );
}
