import { useState, type FocusEvent } from 'react';
import { Link } from 'react-router';
import { passes } from '../content/passes';
import { representationById } from '../content/representations';
import type { RepresentationId } from '../content/types';

const rowHeight = 64;
const leftWidth = 410;
const passWidth = 440;
const rightWidth = 410;

type Cell = 'input' | 'pass' | 'output';
type HighlightedCell = { row: number; cell: Cell };
type PipelineRegion = 'source' | 'frontend' | 'backend';

function representationRegion(id: RepresentationId): PipelineRegion {
  if (id === 'source') return 'source';
  if (id === 'cc' || id === 'mir' || id === 'wasm' || id === 'artifact') return 'backend';
  return 'frontend';
}

function inputPath() {
  return `M 0 0 H ${leftWidth} V 20 A 12 12 0 0 1 ${leftWidth} 44 V ${rowHeight} H 0 Z`;
}

function passPath() {
  return `M 0 20 A 12 12 0 0 1 0 44 V ${rowHeight} H ${passWidth} V 44 A 12 12 0 0 0 ${passWidth} 20 V 0 H 0 Z`;
}

function outputPath() {
  return `M 0 20 A 12 12 0 0 1 0 44 V ${rowHeight} H ${rightWidth} V 0 H 0 Z`;
}

function TextLines({ lines, x, y, className }: { lines: readonly (string | undefined)[]; x: number; y: number; className: string }) {
  return <text className={className} x={x} y={y}>{lines.map((line, index) => line && <tspan key={line} x={x} dy={index === 0 ? 0 : 18}>{line}</tspan>)}</text>;
}

export function ArchitectureMap() {
  const [hoveredCell, setHoveredCell] = useState<HighlightedCell | null>(null);
  const [focusedCell, setFocusedCell] = useState<HighlightedCell | null>(null);
  const width = leftWidth + passWidth + rightWidth;
  const highlightedCell = focusedCell ?? hoveredCell;

  function cellHandlers(row: number, cell: Cell) {
    const target = { row, cell };
    const isTarget = (current: HighlightedCell | null) =>
      current?.row === row && current.cell === cell;

    return {
      onPointerEnter: () => setHoveredCell(target),
      onPointerLeave: () => setHoveredCell((current) => isTarget(current) ? null : current),
      onFocus: (event: FocusEvent<HTMLAnchorElement>) => {
        if (event.currentTarget.matches(':focus-visible')) setFocusedCell(target);
      },
      onBlur: () => setFocusedCell((current) => isTarget(current) ? null : current),
    };
  }

  return <section className="architecture-map" aria-label="Compiler representation and pass map">
    <svg className="puzzle-map pipeline-puzzle-map" viewBox={`0 0 ${width} ${passes.length * rowHeight}`} role="img" aria-label="Each row connects an input representation, a compiler pass, and its output representation">
      {passes.map((pass, index) => {
        const y = index * rowHeight;
        const regionLabel = pass.ordinal === 0 ? 'Frontend' : pass.ordinal === 8 ? 'Backend' : null;
        const input = representationById[pass.input];
        const output = representationById[pass.output];
        return <g key={pass.id} transform={`translate(0 ${y})`}>
          <Link {...cellHandlers(index, 'input')} className={`puzzle-link pipeline-input pipeline-region-${representationRegion(input.id)} puzzle-${input.lifecycle}`} to={`/irs/${input.id}`} aria-label={input.name}>
            <path className="puzzle-shape" d={inputPath()} />
            <text className="puzzle-eyebrow" x="22" y="17">{input.family}</text>
            <TextLines className="puzzle-label" x={22} y={43} lines={[input.name]} />
          </Link>
          <Link {...cellHandlers(index, 'pass')} className={`puzzle-link pipeline-pass pipeline-region-${representationRegion(pass.output)} puzzle-${pass.kind}`} to={`/passes/${pass.id}`} aria-label={pass.name}>
            <g transform={`translate(${leftWidth} 0)`}>
              <path className="puzzle-shape" d={passPath()} />
              <text className="puzzle-eyebrow" x="22" y="17">{regionLabel ? `${regionLabel} · P${pass.ordinal}` : `P${pass.ordinal}`}</text>
              <TextLines className="puzzle-label puzzle-pass-label" x={22} y={43} lines={[pass.name]} />
            </g>
          </Link>
          <Link {...cellHandlers(index, 'output')} className={`puzzle-link pipeline-output pipeline-region-${representationRegion(output.id)} puzzle-${output.lifecycle}`} to={`/irs/${output.id}`} aria-label={output.name}>
            <g transform={`translate(${leftWidth + passWidth} 0)`}>
              <path className="puzzle-shape" d={outputPath()} />
              <text className="puzzle-eyebrow" x="22" y="17">{output.family}</text>
              <TextLines className="puzzle-label" x={22} y={43} lines={[output.name]} />
            </g>
          </Link>
        </g>;
      })}
      {highlightedCell && <path
        className={`puzzle-highlight${focusedCell ? ' puzzle-highlight-focus' : ''}`}
        d={highlightedCell.cell === 'input' ? inputPath() : highlightedCell.cell === 'pass' ? passPath() : outputPath()}
        transform={`translate(${highlightedCell.cell === 'input' ? 0 : highlightedCell.cell === 'pass' ? leftWidth : leftWidth + passWidth} ${highlightedCell.row * rowHeight})`}
        aria-hidden="true"
      />}
    </svg>
  </section>;
}
