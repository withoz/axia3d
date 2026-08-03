/**
 * Two ways for a pick to point at something other than what the user aimed at.
 *
 * 1. **Index convention.** `hit.index` from a LineSegments raycast is the
 *    segment's FIRST VERTEX; `edgeMap` is keyed by segment. Every consumer
 *    halves it — edgePick, EraseTool ×2, SelectTool, ToolManager — except
 *    SplitTool, which read it raw. That highlighted and split the edge at twice
 *    the right slot, and ran off the end of the map over the far half of the
 *    wireframe (where it also returned without clearing, leaving a stale
 *    highlight under a cursor that had moved on).
 *
 * 2. **Preference radius.** The hover exists to predict the click, so both have
 *    to answer "is the cursor near enough to an edge for the edge to win?" the
 *    same way. SelectTool's hover took `pickEdgeOrFace`'s 5px default while its
 *    click asked for 18 — so along a ~13px ribbon beside every edge, the face
 *    lit up and the click then took the edge.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '..', p), 'utf8');

const CONSUMERS = [
  'tools/edgePick.ts',
  'tools/EraseTool.ts',
  'tools/SelectTool.ts',
  'tools/SplitTool.ts',
  'tools/ToolManagerRefactored.ts',
];

describe('a raycast hit index is a vertex index, and edgeMap is keyed by segment', () => {
  it('every edgeMap lookup halves the hit index', () => {
    // Stated as a property over all consumers rather than pinned to SplitTool,
    // so the next tool to read `edgeMap` is covered before it ships.
    const offenders: string[] = [];
    for (const f of CONSUMERS) {
      const src = read(f);
      src.split('\n').forEach((line, i) => {
        // A segment index taken straight from `.index` with no division.
        if (/=\s*(picked\.)?hit\.index\s*;/.test(line) && !/\/\s*2/.test(line)) {
          offenders.push(`${f}:${i + 1} — ${line.trim()}`);
        }
      });
    }
    expect(
      offenders,
      `these read the raw hit index as a segment index: ${offenders.join('; ')}. ` +
        'edgeMap is keyed by segment; halve it (Math.floor(index / 2)).',
    ).toEqual([]);
  });

  it('the extractor sees the convention it is policing (a guard over nothing passes free)', () => {
    const halved = CONSUMERS.map((f) => [...read(f).matchAll(/hit\.index\s*\/\s*2/g)].length)
      .reduce((a, b) => a + b, 0);
    expect(halved, 'no consumer halves the index — did edgeMap change shape?')
      .toBeGreaterThanOrEqual(5);
  });
});

describe('SplitTool clears its highlight on every bail-out', () => {
  const SRC = read('tools/SplitTool.ts');
  const body = SRC.slice(SRC.indexOf('  onMouseMove('), SRC.indexOf('\n  onMouseDown('));

  it('every early return in onMouseMove is preceded by bailHover()', () => {
    expect(body.length, 'onMouseMove not found').toBeGreaterThan(200);
    const lines = body.split('\n');
    const unguarded: string[] = [];
    lines.forEach((line, i) => {
      const trimmed = line.trim();
      // Same-line form: `if (…) { this.bailHover(); return; }` is fine; a bare
      // `if (…) return;` is not.
      if (/^if \([^)]*\)\s*(\{[^}]*\})?\s*return;/.test(trimmed)) {
        if (!/bailHover\(\)/.test(trimmed)) unguarded.push(trimmed);
        return;
      }
      // Own-line form: `return;` is fine only if bailHover() came just before.
      if (/^return;$/.test(trimmed)) {
        const prev = lines.slice(0, i).reverse().find((l) => l.trim() !== '') ?? '';
        if (!/bailHover\(\)/.test(prev)) unguarded.push(`${prev.trim()} → ${trimmed}`);
      }
    });
    expect(
      unguarded,
      `these leave the previous edge lit and its marker drawn: ${unguarded.join('; ')}. ` +
        'Route them through bailHover().',
    ).toEqual([]);
  });

  it('and the bail-out actually drops both the line and the marker', () => {
    const bail = SRC.slice(SRC.indexOf('private bailHover'), SRC.indexOf('onMouseMove('));
    expect(bail).toContain('this.clearHover()');
    expect(bail).toContain('this.clearMarker()');
    expect(bail).toContain('this.hoverEdgeId = null');
    expect(bail).toContain('this.splitPoint = null');
  });
});

describe('hover and click agree on how near an edge has to be', () => {
  const SRC = read('tools/SelectTool.ts');

  it('both go through one constant', () => {
    // The click side and the hover side, named rather than numbered, so they
    // cannot drift apart again the way 18 and the 5px default did.
    expect(SRC, 'the shared radius is gone').toContain('const EDGE_PREFER_PX =');
    const uses = [...SRC.matchAll(/EDGE_PREFER_PX/g)].length;
    expect(uses, 'the constant is declared but not used by both sides')
      .toBeGreaterThanOrEqual(3);
  });

  it('the hover pick no longer takes the default radius', () => {
    // `pickEdgeOrFace(x, y)` with two arguments silently means 5px.
    const hover = SRC.slice(SRC.indexOf('computeHoverTarget'), SRC.indexOf('refineEdgeHoverWithAnalytic'));
    expect(hover.length, 'computeHoverTarget not found').toBeGreaterThan(100);
    expect(
      /pickEdgeOrFace\?\.\([^)]*\)/.exec(hover)?.[0],
      'the hover pick dropped its radius argument and is back on the 5px default',
    ).toContain('EDGE_PREFER_PX');
  });

  it('the click side uses it too', () => {
    const click = SRC.slice(SRC.indexOf('onMouseDown('), SRC.indexOf('onMouseMove('));
    expect(click).toContain('preferEdgeWithinPx: EDGE_PREFER_PX');
    expect(/preferEdgeWithinPx:\s*\d/.test(click), 'a bare number is back on the click side')
      .toBe(false);
  });
});
