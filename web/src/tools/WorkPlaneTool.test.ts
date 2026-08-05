/**
 * THREE CLICKS GIVE ANY PLANE — including an oblique one.
 *
 * Measured 2026-08-05: the engine has always been free of direction. A rect and
 * a circle drawn on the normal (0.577, 0.577, 0.577) come out with zero
 * invariant violations, and `enterSketch` has always accepted any (origin,
 * normal, up, right) — both `getDrawPlane` and `get3DPoint` consult the sketch
 * before a face hit, so a plane set that way governs every tool.
 *
 * What did not exist was a way for the user to NAME one. The entries were XZ,
 * XY, YZ, a selected face and auto: three cardinal planes plus whatever face
 * happens to be there. An oblique plane in open space could not be reached at
 * all. Three points express any plane, so that is what this asks for.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as THREE from 'three';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { WorkPlaneTool } from './WorkPlaneTool';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function makeCtx(): any {
  return {
    enterSketch: vi.fn(),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    syncMesh: vi.fn(),
  };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function clickAt(tool: WorkPlaneTool, ctx: any, p: THREE.Vector3) {
  ctx.get3DPoint.mockReturnValue(p);
  tool.onMouseDown({ clientX: 0, clientY: 0 } as MouseEvent, p);
}

describe('WorkPlaneTool', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let ctx: any;
  let tool: WorkPlaneTool;

  beforeEach(() => {
    ctx = makeCtx();
    tool = new WorkPlaneTool(ctx);
    tool.onActivate();
  });

  it('three points on a slope give that slope, not a cardinal plane', () => {
    // A plane whose normal is (1,1,1)/√3 — the case that had no entry at all.
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(0, 100, 0));
    expect(ctx.enterSketch).not.toHaveBeenCalled(); // not until the third
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 100));

    expect(ctx.enterSketch).toHaveBeenCalledTimes(1);
    const plane = ctx.enterSketch.mock.calls[0][0];
    const n = plane.normal;
    const s = 1 / Math.sqrt(3);
    // Either winding is the same plane; compare the direction, not the sign.
    expect(Math.abs(Math.abs(n.x) - s)).toBeLessThan(1e-9);
    expect(Math.abs(Math.abs(n.y) - s)).toBeLessThan(1e-9);
    expect(Math.abs(Math.abs(n.z) - s)).toBeLessThan(1e-9);
    // The origin is the first point the user gave.
    expect([plane.origin.x, plane.origin.y, plane.origin.z]).toEqual([100, 0, 0]);
  });

  it('the frame is orthonormal, so the plane is usable as a sketch', () => {
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 50));
    clickAt(tool, ctx, new THREE.Vector3(0, 100, 0));
    const { normal, up, right } = ctx.enterSketch.mock.calls[0][0];
    for (const v of [normal, up, right]) {
      expect(Math.abs(v.length() - 1)).toBeLessThan(1e-9);
    }
    expect(Math.abs(normal.dot(up))).toBeLessThan(1e-9);
    expect(Math.abs(normal.dot(right))).toBeLessThan(1e-9);
    expect(Math.abs(up.dot(right))).toBeLessThan(1e-9);
  });

  it('a vertical plane stands upright — world up laid onto it', () => {
    // The XZ wall: normal ±Y, so `up` must come out ±Z rather than an
    // arbitrary in-plane direction.
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 100));
    const { up } = ctx.enterSketch.mock.calls[0][0];
    expect(Math.abs(Math.abs(up.z) - 1)).toBeLessThan(1e-9);
  });

  it('a horizontal plane falls back to the first edge, since world up vanishes', () => {
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 50));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 50));
    clickAt(tool, ctx, new THREE.Vector3(0, 100, 50));
    const { up, normal } = ctx.enterSketch.mock.calls[0][0];
    expect(Math.abs(Math.abs(normal.z) - 1)).toBeLessThan(1e-9);
    expect(Math.abs(up.length() - 1)).toBeLessThan(1e-9);
    expect(Math.abs(up.dot(normal))).toBeLessThan(1e-9);
  });

  it('three points on a line are refused, and the third is asked for again', () => {
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(200, 0, 0)); // collinear
    expect(ctx.enterSketch).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true); // still holding the first two

    clickAt(tool, ctx, new THREE.Vector3(0, 50, 0)); // off the line
    expect(ctx.enterSketch).toHaveBeenCalledTimes(1);
  });

  it('the same point twice does not count', () => {
    clickAt(tool, ctx, new THREE.Vector3(10, 20, 30));
    clickAt(tool, ctx, new THREE.Vector3(10, 20, 30));
    clickAt(tool, ctx, new THREE.Vector3(50, 0, 0));
    expect(ctx.enterSketch).not.toHaveBeenCalled(); // only two distinct so far
  });

  it('Esc drops the points and Esc with none is not swallowed', () => {
    expect(tool.onKeyDown({ key: 'Escape' } as KeyboardEvent)).toBe(false);
    clickAt(tool, ctx, new THREE.Vector3(1, 2, 3));
    expect(tool.isBusy()).toBe(true);
    expect(tool.onKeyDown({ key: 'Escape' } as KeyboardEvent)).toBe(true);
    expect(tool.isBusy()).toBe(false);
  });

  it('a context that cannot start a sketch says so instead of throwing', () => {
    ctx.enterSketch = undefined;
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    expect(() => clickAt(tool, ctx, new THREE.Vector3(0, 100, 0))).not.toThrow();
    expect(tool.isBusy()).toBe(false);
  });
});

describe('the work plane can actually be reached', () => {
  const read = (p: string) => readFileSync(join(__dirname, '..', '..', p), 'utf8');

  it('registered, dispatched, in the menu and in the catalog', () => {
    // A tool that works but appears nowhere is one only its author can use —
    // this repo has shipped that twice (ADR-299, ADR-308).
    expect(read('src/tools/ToolManagerRefactored.ts'))
      .toContain("this.tools.set('workplane'");
    expect(read('src/ui/MenuBar.ts'))
      .toContain("case 'tool-workplane': setActiveTool('workplane');");
    const html = read('index.html');
    expect(html).toContain('data-action="tool-workplane"');
    expect(read('../packages/axia-action-catalog/src/catalog.ts'))
      .toContain("id: 'tool-workplane'");
  });
});
