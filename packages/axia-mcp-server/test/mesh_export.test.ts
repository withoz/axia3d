/**
 * The three interchange formats, checked as FILES rather than as calls.
 *
 * ⚠ `export_obj`, `export_stl` and `export_step` were declared in the tier
 * surface with no implementation behind them. Tier 1 is enabled by default, so
 * an agent saw all three in `tools/list` and got
 * "declared but not implemented" on every call — advertised and absent, which
 * is the one shape ADR-042 P27.4 exists to prevent.
 *
 * A test that only checks the handler returns something would have passed while
 * the bytes were nonsense, so these decode the base64 and read the file: OBJ
 * indices are 1-based and in range, STL's facet count matches its byte length,
 * STEP's every `#n` reference resolves to an entity the file defines.
 */
import { describe, it, expect } from 'vitest';
import { toObj, toStlBinary, toStep, statsOf, type Triangles } from '../src/capabilities/meshExport.js';

/** A unit tetrahedron: four vertices, four faces, no degenerate triangle. */
function tetra(): Triangles {
  return {
    positions: new Float32Array([0, 0, 0, 10, 0, 0, 0, 10, 0, 0, 0, 10]),
    normals: new Float32Array([0, 0, -1, 0, -1, 0, -1, 0, 0, 1, 1, 1]),
    indices: new Uint32Array([0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3]),
  };
}

/** Two triangles where the second is degenerate — the case the writers must survive. */
function withDegenerate(): Triangles {
  return {
    positions: new Float32Array([0, 0, 0, 10, 0, 0, 0, 10, 0, 20, 0, 0, 40, 0, 0]),
    normals: new Float32Array(15),
    // second triangle is three collinear points on the x axis
    indices: new Uint32Array([0, 1, 2, 1, 3, 4]),
  };
}

describe('statsOf', () => {
  it('counts what the buffers hold', () => {
    expect(statsOf(tetra())).toEqual({ vertices: 4, triangles: 4 });
  });
});

describe('OBJ', () => {
  it('writes every vertex and a face per triangle, 1-based and in range', () => {
    const t = tetra();
    const text = toObj(t, 'part');
    const v = text.split('\n').filter((l) => l.startsWith('v '));
    const vn = text.split('\n').filter((l) => l.startsWith('vn '));
    const fs = text.split('\n').filter((l) => l.startsWith('f '));
    expect(v).toHaveLength(4);
    expect(vn).toHaveLength(4);
    expect(fs).toHaveLength(4);
    expect(text).toContain('o part');

    for (const line of fs) {
      for (const tok of line.slice(2).trim().split(/\s+/)) {
        const idx = Number(tok.split('//')[0]);
        // ⚠ 1-based. An off-by-one here loads with every face shifted by a
        // vertex, which looks like a mesh and is not one.
        expect(idx).toBeGreaterThanOrEqual(1);
        expect(idx).toBeLessThanOrEqual(4);
      }
    }
  });

  it('omits vn and uses plain f when the engine gave no normals', () => {
    const t = tetra();
    t.normals = new Float32Array(0);
    const text = toObj(t);
    expect(text).not.toContain('vn ');
    expect(text).toMatch(/^f \d+ \d+ \d+$/m);
  });

  it('places the first vertex where the buffer says', () => {
    const text = toObj(tetra());
    expect(text).toContain('v 0 0 0');
    expect(text).toContain('v 10 0 0');
  });
});

describe('binary STL', () => {
  it('is exactly 84 + 50n bytes and says so in its own count', () => {
    const bytes = toStlBinary(tetra());
    expect(bytes.byteLength).toBe(84 + 4 * 50);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    expect(view.getUint32(80, true)).toBe(4);
  });

  it('does not begin with "solid", which would make a reader parse it as ASCII', () => {
    const bytes = toStlBinary(tetra());
    const head = Buffer.from(bytes.slice(0, 5)).toString('ascii');
    expect(head.toLowerCase()).not.toBe('solid');
  });

  it('carries the caller name in the header', () => {
    const bytes = toStlBinary(tetra(), 'bracket');
    expect(Buffer.from(bytes.slice(0, 80)).toString('ascii')).toContain('bracket');
  });

  it('writes a unit facet normal, and zero for a degenerate triangle', () => {
    const bytes = toStlBinary(withDegenerate());
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const n0 = [view.getFloat32(84, true), view.getFloat32(88, true), view.getFloat32(92, true)];
    expect(Math.hypot(...n0)).toBeCloseTo(1, 5);
    const n1 = [view.getFloat32(134, true), view.getFloat32(138, true), view.getFloat32(142, true)];
    // ⚠ Kept, not dropped: STL is a triangle soup with no topology to break,
    // and 0,0,0 is what a reader takes as "work it out yourself".
    expect(Math.hypot(...n1)).toBe(0);
  });

  it('writes the three corners of each facet', () => {
    const bytes = toStlBinary(tetra());
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    // ⚠ Spell the layout out. The first version of this test guessed offsets
    // and failed against a correct writer.
    //   84  normal (3 x f32 = 12 bytes)
    //   96  corner 1      108  corner 2      120  corner 3      132  attr u16
    // Facet 0 is indices [0,2,1] = (0,0,0), (0,10,0), (10,0,0).
    const at = (o: number) => view.getFloat32(o, true);
    expect([at(96), at(100), at(104)]).toEqual([0, 0, 0]);
    expect([at(108), at(112), at(116)]).toEqual([0, 10, 0]);
    expect([at(120), at(124), at(128)]).toEqual([10, 0, 0]);
  });
});

describe('STEP', () => {
  const text = toStep(tetra(), 'part');

  it('is a well-formed ISO 10303-21 part 21 file', () => {
    expect(text.startsWith('ISO-10303-21;')).toBe(true);
    expect(text.trimEnd().endsWith('END-ISO-10303-21;')).toBe(true);
    expect(text).toContain('HEADER;');
    expect(text).toContain('DATA;');
    expect(text.match(/ENDSEC;/g)).toHaveLength(2);
  });

  it('declares the schema its entities actually belong to', () => {
    // ⚠ AUTOMOTIVE_DESIGN is AP214. The first draft called it AP203 in the
    // comments while writing this line — a file trusted for the wrong reason.
    expect(text).toContain('AUTOMOTIVE_DESIGN');
    expect(text).not.toContain('CONFIG_CONTROL_DESIGN');
  });

  it('resolves every reference it makes', () => {
    const defined = new Set([...text.matchAll(/^#(\d+)=/gm)].map((m) => m[1]));
    const referenced = [...text.matchAll(/#(\d+)/g)].map((m) => m[1]);
    const dangling = [...new Set(referenced)].filter((r) => !defined.has(r));
    // ⚠ The single most useful thing to assert about a Part 21 file: a
    // dangling #n is how a writer that "looks right" fails in every reader.
    expect(dangling, `dangling references: ${dangling.join(', ')}`).toEqual([]);
  });

  it('defines each entity id exactly once', () => {
    const ids = [...text.matchAll(/^#(\d+)=/gm)].map((m) => m[1]);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('builds a closed shell of one face per triangle, inside a solid', () => {
    expect(text.match(/ADVANCED_FACE/g)).toHaveLength(4);
    expect(text.match(/CLOSED_SHELL/g)).toHaveLength(1);
    expect(text).toContain('MANIFOLD_SOLID_BREP');
    expect(text).toContain('ADVANCED_BREP_SHAPE_REPRESENTATION');
    expect(text).toContain('SHAPE_DEFINITION_REPRESENTATION');
  });

  it('says the scene is in millimetres', () => {
    expect(text).toContain('SI_UNIT(.MILLI.,.METRE.)');
  });

  it('drops a degenerate triangle rather than emitting a zero DIRECTION', () => {
    const d = toStep(withDegenerate());
    // ⚠ A zero-length DIRECTION is out of range in the schema and rejected by
    // readers, so the face goes rather than the file.
    expect(d.match(/ADVANCED_FACE/g)).toHaveLength(1);
    expect(d).not.toMatch(/DIRECTION\('',\(0,0,0\)\)/);
    const defined = new Set([...d.matchAll(/^#(\d+)=/gm)].map((m) => m[1]));
    const dangling = [...new Set([...d.matchAll(/#(\d+)/g)].map((m) => m[1]))]
      .filter((r) => !defined.has(r));
    expect(dangling).toEqual([]);
  });

  it('shares a vertex between the faces that meet at it', () => {
    // Four vertices for four faces that all touch: 4 VERTEX_POINT, not 12.
    expect(text.match(/VERTEX_POINT/g)).toHaveLength(4);
  });
});

describe('an empty scene', () => {
  const empty: Triangles = {
    positions: new Float32Array(0),
    normals: new Float32Array(0),
    indices: new Uint32Array(0),
  };

  it('still produces a readable file in each format', () => {
    expect(toObj(empty)).toContain('o axia');
    expect(toStlBinary(empty).byteLength).toBe(84);
    const s = toStep(empty);
    expect(s.startsWith('ISO-10303-21;')).toBe(true);
    const defined = new Set([...s.matchAll(/^#(\d+)=/gm)].map((m) => m[1]));
    const dangling = [...new Set([...s.matchAll(/#(\d+)/g)].map((m) => m[1]))]
      .filter((r) => !defined.has(r));
    expect(dangling).toEqual([]);
  });
});
