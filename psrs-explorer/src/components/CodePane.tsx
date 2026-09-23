type CodePaneProps = {
  label: string;
  value: string;
  emphasis?: 'source' | 'representation';
};

export function CodePane({ label, value, emphasis = 'representation' }: CodePaneProps) {
  const lines = value.trimEnd().split('\n');
  return (
    <section className={`code-pane code-pane--${emphasis}`} aria-label={label}>
      <header>{label}</header>
      <pre><code>{lines.map((line, index) => (
        <span className="code-line" key={`${index}-${line}`}>
          <span aria-hidden="true" className="line-number">{index + 1}</span>
          <span>{line || ' '}</span>
        </span>
      ))}</code></pre>
    </section>
  );
}
