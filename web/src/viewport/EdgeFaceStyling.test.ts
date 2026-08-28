/**
 * `LineSegments2` extends `THREE.Mesh`, and `Line2` extends `LineSegments2`.
 * Every mesh edge, centerline and warning overlay in `meshGroup` is therefore
 * an `instanceof THREE.Mesh`, and `LineMaterial` leaves `side` at its default
 * `FrontSide` — so the face-styling loops, which tested `instanceof Mesh` and
 * then branched on `side === FrontSide`, were repainting the edges.
 *
 * Measured in the running app before the fix: picking a face colour took the
 * edges from #0a0a14 to #ff0000, and the edge colour picker could not put them
 * back because `setEdgeStyle` only ever set `.visible` on a LineSegments2. The
 * width slider had the same shape — the material pool that exists to apply it
 * was iterated by a loop with no body.
 *
 * The overlays make this sharper than a cosmetic bug: the non-manifold
 * highlight is a LineSegments2 in the same group, so changing the face colour
 * silently recoloured a correctness signal.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as THREE from 'three';
import { LineSegments2 } from 'three/examples/jsm/lines/LineSegments2.js';
import { Line2 } from 'three/examples/jsm/lines/Line2.js';
import { LineMaterial } from 'three/examples/jsm/lines/LineMaterial.js';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const SRC = readFileSync(resolve(__dirname, 'Viewport.ts'), 'utf8');

describe('why the face loops needed a guard', () => {
  it('an edge line IS a Mesh — the premise of the bug', () => {
    // Checked by walking the prototype chain rather than with `instanceof
    // THREE.Mesh`: vitest aliases bare `three` to a mock, so the real
    // LineSegments2 extends a different Mesh object than the one this file
    // imports and the instanceof would be false for a reason that has nothing
    // to do with the claim. The class relationship is what matters, and it is
    // the same relationship the browser has.
    const chain = (c: unknown) => {
      const out: string[] = [];
      for (let p = Object.getPrototypeOf(c); p && p !== Function.prototype; p = Object.getPrototypeOf(p)) {
        out.push((p as { name: string }).name);
      }
      return out;
    };
    expect(chain(LineSegments2), 'LineSegments2 no longer extends Mesh').toContain('Mesh');
    expect(chain(Line2), 'Line2 no longer routes through LineSegments2').toContain('LineSegments2');
    expect(chain(Line2), 'Line2 no longer extends Mesh').toContain('Mesh');
  });

  it('and its material presents as FrontSide, which is the branch that repainted it', () => {
    const mat = new LineMaterial({ color: 0x0a0a14 });
    expect(mat.side).toBe(THREE.FrontSide);
  });
});

describe('face styling does not reach edge lines', () => {
  // Exercising the real Viewport needs a WebGL context, so this reads the two
  // loops instead. Both are one line each and the mistake is exactly the shape
  // of that line, so a source check is honest here — and the behavioural proof
  // is the browser measurement recorded in the header.
  const bodyOf = (name: string) => {
    const i = SRC.indexOf(`  ${name}(`);
    expect(i, `${name} not found in Viewport.ts`).toBeGreaterThan(-1);
    return SRC.slice(i, SRC.indexOf('\n  }', i));
  };

  it('setFaceColors filters with _isFaceMesh, not a bare instanceof Mesh', () => {
    const body = bodyOf('setFaceColors');
    expect(body, 'setFaceColors is back to catching edge lines as faces')
      .toContain('_isFaceMesh(child)');
    expect(/if \(child instanceof THREE\.Mesh\)/.test(body), 'bare instanceof Mesh is the bug')
      .toBe(false);
  });

  it('setFaceOpacity does the same', () => {
    const body = bodyOf('setFaceOpacity');
    expect(body, 'setFaceOpacity would fade the edges with the faces')
      .toContain('_isFaceMesh(child)');
    expect(/if \(child instanceof THREE\.Mesh\)/.test(body)).toBe(false);
  });

  it('_isFaceMesh excludes LineSegments2, which is what covers Line2 too', () => {
    expect(SRC, 'the predicate the two loops depend on is gone')
      .toContain('private static _isFaceMesh(');
    expect(SRC, 'excluding LineSegments2 is what also excludes Line2')
      .toContain('!(o instanceof LineSegments2)');
  });
});

describe('edge styling restyles by role, so overlays keep their meaning', () => {
  const setEdgeStyle = SRC.slice(SRC.indexOf('  setEdgeStyle('), SRC.indexOf('\n  }', SRC.indexOf('  setEdgeStyle(')));

  it('the pooled loop has a body — it used to be `if (mat.dashed) continue;` and nothing else', () => {
    expect(setEdgeStyle, 'colour is not applied to live materials').toContain('mat.color.setHex(this._edgeColor)');
    expect(setEdgeStyle, 'width is not applied to live materials').toContain('mat.linewidth =');
  });

  it('it dispatches on the role tag, not on `dashed`', () => {
    // `dashed` cannot separate these: the free-edge overlay is dashed like a
    // centerline, and the non-manifold overlay is not dashed at all — so a
    // dashed-based loop would recolour the warning highlight.
    expect(setEdgeStyle).toContain("=== 'mesh-edge'");
    expect(setEdgeStyle).toContain("=== 'centerline'");
    expect(/if \(mat\.dashed\)/.test(setEdgeStyle), 'dashed is not a role').toBe(false);
  });

  it('every pooled material is tagged, and the overlays are tagged as overlays', () => {
    // An untagged material silently falls through the role switch and stops
    // responding to the picker — the failure this whole test exists to prevent,
    // one level down.
    const roles = [...SRC.matchAll(/axiaEdgeRole = '([a-z-]+)'/g)].map((m) => m[1]);
    expect(roles.sort(), 'a pooled LineMaterial lost its role tag').toEqual(
      ['centerline', 'mesh-edge', 'overlay', 'overlay'],
    );
    // Four tags but six pushes, because `_makeMeshEdgeMaterial` is called from
    // three places and tags inside the factory. The count is pinned so that a
    // seventh push has to come here and say which role it is — an untagged
    // material stops responding to the picker without failing anything else.
    const pushes = [...SRC.matchAll(/_meshEdgeMaterials\.push\(/g)].length;
    expect(pushes, 'a new material joined the pool — give it an axiaEdgeRole and update this count')
      .toBe(6);
    // The axis lines also build a LineMaterial (Viewport.ts:389) but live in
    // `scene`, not `meshGroup`, and use their own resize array — so they are
    // deliberately outside this scheme.
    //
    // The cursor mini-grid (`setMiniGridCursor`) is the same exception for the
    // same reason: it hangs off `scene`, the viewport owns and disposes that one
    // object itself, and its width comes from `MiniGridSettings` — not from the
    // edge-style picker. Tagging it would hand a cursor decoration to a picker
    // that must not restyle it. Six constructions, four tagged.
    const built = [...SRC.matchAll(/new LineMaterial\(/g)].length;
    expect(built, 'a LineMaterial appeared or vanished — check whether it reaches the pool').toBe(6);
  });
});

describe('the material pool still gets its resize sweep', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('resize updates resolution for every pooled material regardless of role', () => {
    // The roles must not have narrowed the one thing the pool was built for.
    const resize = SRC.slice(SRC.indexOf('_resizeObserver = new ResizeObserver'), SRC.indexOf('this._resizeObserver.observe'));
    expect(resize).toContain('for (const mat of this._meshEdgeMaterials)');
    expect(resize).toContain('mat.resolution.set(w, h)');
    expect(/axiaEdgeRole/.test(resize), 'resize must not skip overlays').toBe(false);
  });
});
