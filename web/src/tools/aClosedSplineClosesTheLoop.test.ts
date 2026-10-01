/**
 * A closed spline closes the loop.
 *
 * `DrawSplineTool` collected clicks, set `degree = min(3, n-1)`, built a
 * clamped uniform knot vector and called `drawBSplineWithCurve` — always the
 * OPEN curve, however the points sat. Its sibling `DrawBezierTool` has routed a
 * last click landing on the first to `drawClosedBezierAsCurve` since ADR-089
 * A-ψ. The spline had no such branch, so `drawClosedBSplineAsCurve` — engine op
 * (A-Α), WASM export and bridge wrapper all present — had **zero callers**.
 *
 * ⚠ No wiring guard could have seen it, and correctly so: no `data-action`
 * string pointed at it, and the bridge wrapper calls a name that IS exported,
 * so link D was satisfied. A wrapper nobody calls breaks no link. It came out
 * of walking the map backwards, the pass that also found the chamfer menu item
 * and the Outliner's rename.
 *
 * ⚠ And the NURBS sibling is NOT this: `drawClosedNURBSAsCurve` is rational and
 * wants per-point weights, which no tool collects. `DrawNurbsTool` is a 2-click
 * rectangle that builds a tensor PATCH, not a curve. That one stays a deferral,
 * as CLAUDE.md says — only the B-spline half was a wiring gap.
 *
 * The engine side is measured in `a_closed_spline_closes.rs`, including the
 * floor this file's `n >= 4` guard comes from.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawSplineTool } from './DrawSplineTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function ctxWith(closedReturn: number | undefined = 1) {
  return {
    bridge: {
      drawBSplineWithCurve: vi.fn().mockReturnValue(0),
      drawClosedBSplineAsCurve: vi.fn().mockReturnValue(closedReturn),
      drawPolylineAsShape: vi.fn().mockReturnValue(0),
    },
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    snap: { setReferencePoint: vi.fn(), getSnappedPoint: vi.fn().mockReturnValue(null) },
    syncMesh: vi.fn(),
    setLastDrawnPlane: vi.fn(),
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), onFace: false,
    }),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn().mockReturnValue({ ray: { intersectPlane: vi.fn().mockReturnValue(null) } }),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
}

function commitWith(tool: DrawSplineTool, pts: THREE.Vector3[]) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const t = tool as any;
  t.plane = { normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0) };
  t.drawPlane3 = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  t.points = pts;
  t.commit();
}

const v = (x: number, y: number) => new THREE.Vector3(x, y, 0);
/**
 * Four corners of a square, then a click back onto the first — NEAR it, not
 * exactly on it (2e-4 mm, inside the 1e-3 gesture tolerance).
 *
 * ⚠ The first draft of this fixture put the last point EXACTLY on the first,
 * which made the exact-closure assertion vacuous: the overwrite was a no-op, so
 * deleting it left every test green. A real click never lands exactly, and the
 * engine checks at 1e-6, so the offset is the whole point of the fixture.
 */
const closedSquare = () => [v(0, 0), v(100, 0), v(100, 100), v(0, 100), v(2e-4, -1e-4)];

describe('a closed spline', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let ctx: any;
  let tool: DrawSplineTool;

  beforeEach(async () => {
    const { setDrawCurveMode } = await import('./DrawCurveSettings');
    setDrawCurveMode(true);
    ctx = ctxWith();
    tool = new DrawSplineTool(ctx);
  });

  it('goes to the closed op, not the open one', () => {
    commitWith(tool, closedSquare());
    expect(ctx.bridge.drawClosedBSplineAsCurve, 'the closed op').toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawBSplineWithCurve, 'and NOT the open one').not.toHaveBeenCalled();
  });

  it('hands the engine an exactly closed control polygon', () => {
    commitWith(tool, closedSquare());
    const [ctrl, knots, degree] = ctx.bridge.drawClosedBSplineAsCurve.mock.calls[0];
    const n = ctrl.length / 3;
    // PREMISE: five clicks in, five control points out — otherwise the
    // closure assertion below is about the wrong slot.
    expect(n, 'one control point per click').toBe(5);
    expect(degree, 'min(3, n-1)').toBe(3);
    expect(knots.length, 'engine contract: n + degree + 1').toBe(n + degree + 1);
    // The engine checks at 1e-6, tighter than the gesture tolerance, so the
    // last point must be written from the first rather than trusted.
    // PREMISE: the click did NOT land exactly, so this says something.
    const clicked = closedSquare()[4];
    expect(clicked.x === 0 && clicked.y === 0, 'the fixture must not pre-close it').toBe(false);
    expect([ctrl[(n - 1) * 3], ctrl[(n - 1) * 3 + 1], ctrl[(n - 1) * 3 + 2]])
      .toEqual([ctrl[0], ctrl[1], ctrl[2]]);
  });

  it('marks the plane sticky, as the Bezier does on a synthesised face', () => {
    commitWith(tool, closedSquare());
    expect(ctx.setLastDrawnPlane).toHaveBeenCalledTimes(1);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('is left open when the last click is nowhere near the first', () => {
    commitWith(tool, [v(0, 0), v(100, 0), v(100, 100), v(0, 100)]);
    expect(ctx.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawClosedBSplineAsCurve).not.toHaveBeenCalled();
  });

  it('is left open below the measured floor of four points', () => {
    // Three points with the last on the first: degree 2, which the engine
    // routes to its Bezier arm and refuses (a_closed_spline_closes.rs).
    commitWith(tool, [v(0, 0), v(100, 0), v(0, 0)]);
    expect(ctx.bridge.drawClosedBSplineAsCurve).not.toHaveBeenCalled();
    expect(ctx.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
  });

  it('is left open when curve mode is off', async () => {
    const { setDrawCurveMode } = await import('./DrawCurveSettings');
    setDrawCurveMode(false);
    commitWith(tool, closedSquare());
    expect(ctx.bridge.drawClosedBSplineAsCurve).not.toHaveBeenCalled();
    expect(ctx.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
    setDrawCurveMode(true);
  });

  it('falls through to the open curve when the kernel declines', () => {
    ctx = ctxWith(-1);
    tool = new DrawSplineTool(ctx);
    commitWith(tool, closedSquare());
    expect(ctx.bridge.drawClosedBSplineAsCurve).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawBSplineWithCurve, 'the user still gets their curve')
      .toHaveBeenCalledTimes(1);
    // ⚠ Not zero, and not a bug: this tool's OPEN path sets the sticky plane
    // too, deliberately — "open spline has no face; keep for subsequent draws
    // sharing the plane" (ADR-164, pre-existing). It diverges from the Bezier's
    // synthesised-face-only rule and says so. Exactly one call either way: the
    // closed branch returns before the open path on success, and sets nothing
    // when it falls through.
    expect(ctx.setLastDrawnPlane, 'the open path sets it, once').toHaveBeenCalledTimes(1);
  });
});
