/**
 * THE THREE-POINT WORK PLANE, AND WHY IT NEEDED A SKETCH.
 *
 * ⚠ This file exists because I built a duplicate. Planning "an arbitrary work
 * plane", I grepped for `workPlane|WorkPlane|constructionPlane|customPlane`,
 * found nothing, and wrote that the entry did not exist. It did:
 * `DrawPlaneTool` (ADR-224, 2026-06-08) is a 3-point work plane, wired to the
 * menu, the toolbar, the palette and the catalog under `tool-plane`. The tool
 * listing I had already printed showed `DrawPlaneTool.ts` and I read past it.
 * The name I searched for was mine, not the repo's.
 *
 * What measurement DID find, once the right tool was under it, is a real gap.
 * The tool sets the ADR-166 plane LOCK, and a lock governs `getDrawPlane` —
 * rect, circle, polygon — but NOT `get3DPoint`, which is where a line's ends
 * come from. Measured 2026-08-05 in the running app: after three oblique
 * clicks `getDrawPlane` reported normal (0.5774, 0.5774, 0.5774) and
 * `get3DPoint` did not honour it at all. Rectangles landed on the plane and
 * lines did not.
 *
 * So the tool now also enters a sketch, which BOTH resolvers consult and which
 * does not auto-unlock on a face hit — that is what "I said to draw HERE" means.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
// STATIC, and it used to be four `await import(...)` calls inside the tests.
//
// `ToolManagerRefactored` is a 229 kB module that pulls in Three.js, so the
// FIRST of those four paid for transpiling it — inside a test's 5 s budget.
// Measured 2026-08-21 in a full 185-file run, where the machine is busy:
//
//     × offset …  5011 ms   Test timed out in 5000ms
//     ✓ tilt   …   664 ms   same import, now cached
//
// Nothing was wrong with the code under test; the first test was being charged
// for a module load. Hoisting it pays that at file collection, which is not on
// any test's clock, and the other three stop re-asking for it.
// `ToolManagerRefactored.test.ts` has always imported it this way, and the
// module has no top-level side effects, so there was never a reason for the
// dynamic form here.
import * as THREE from 'three';
import { ToolManager } from './ToolManagerRefactored';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { DrawPlaneTool } from './DrawPlaneTool';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function makeCtx(): any {
  return {
    enterSketch: vi.fn(),
    lockPlane: vi.fn(),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    snap: { setReferencePoint: vi.fn(), clearReferencePoint: vi.fn() },
    syncMesh: vi.fn(),
  };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function clickAt(tool: DrawPlaneTool, ctx: any, p: THREE.Vector3) {
  ctx.get3DPoint.mockReturnValue(p);
  tool.onMouseDown({ clientX: 0, clientY: 0 } as MouseEvent, p);
}

describe('the three-point work plane', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let ctx: any;
  let tool: DrawPlaneTool;

  beforeEach(() => {
    ctx = makeCtx();
    tool = new DrawPlaneTool(ctx);
    tool.onActivate();
  });

  it('three points on a slope give that slope, not a cardinal plane', () => {
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(0, 100, 0));
    expect(ctx.lockPlane).not.toHaveBeenCalled(); // not until the third
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 100));

    const lock = ctx.lockPlane.mock.calls[0][0];
    const s = 1 / Math.sqrt(3);
    // Either winding is the same plane; compare direction, not sign.
    for (const c of ['x', 'y', 'z'] as const) {
      expect(Math.abs(Math.abs(lock.normal[c]) - s)).toBeLessThan(1e-9);
    }
    expect([lock.origin.x, lock.origin.y, lock.origin.z]).toEqual([100, 0, 0]);
  });

  it('and a SKETCH too, or a line would still land on the ground', () => {
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(0, 100, 0));
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 100));
    expect(ctx.enterSketch).toHaveBeenCalledTimes(1);
    const sketch = ctx.enterSketch.mock.calls[0][0];
    const lock = ctx.lockPlane.mock.calls[0][0];
    // The two must describe the SAME plane, or the tools would disagree.
    expect(Math.abs(Math.abs(sketch.normal.dot(lock.normal)) - 1)).toBeLessThan(1e-9);
    expect([sketch.origin.x, sketch.origin.y, sketch.origin.z])
      .toEqual([lock.origin.x, lock.origin.y, lock.origin.z]);
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
    // The XZ wall: normal ±Y, so `up` must be ±Z rather than whichever edge
    // happened to be picked first, which is what it used to take.
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

  it('three points on a line set nothing', () => {
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(200, 0, 0));
    expect(ctx.lockPlane).not.toHaveBeenCalled();
    expect(ctx.enterSketch).not.toHaveBeenCalled();
  });

  it('a context without enterSketch still sets the lock', () => {
    // Backward compatible: the tool predates the sketch call, and an older host
    // or a test double must not lose the plane it always had.
    ctx.enterSketch = undefined;
    clickAt(tool, ctx, new THREE.Vector3(0, 0, 0));
    clickAt(tool, ctx, new THREE.Vector3(100, 0, 0));
    expect(() => clickAt(tool, ctx, new THREE.Vector3(0, 100, 0))).not.toThrow();
    expect(ctx.lockPlane).toHaveBeenCalledTimes(1);
  });
});

describe('the work plane and its moves can be reached', () => {
  const read = (p: string) => readFileSync(join(__dirname, '..', '..', p), 'utf8');

  it('the 3-point plane is registered, dispatched, in the menu and the catalog', () => {
    expect(read('src/tools/ToolManagerRefactored.ts')).toContain("this.tools.set('plane'");
    expect(read('src/ui/MenuBar.ts')).toContain("case 'tool-plane'");
    expect(read('index.html')).toContain('data-action="tool-plane"');
    expect(read('src/commands/AxiaCommands.ts')).toContain("'tool-plane'");
    expect(read('../packages/axia-action-catalog/src/catalog.ts')).toContain("id: 'tool-plane'");
  });

  it('and there is only ONE of it', () => {
    // The duplicate this file is named after. A second 3-point plane tool put
    // two near-identical entries in the palette, which is how a user finds out
    // the codebase disagrees with itself.
    expect(read('../packages/axia-action-catalog/src/catalog.ts')).not.toContain('tool-workplane');
    expect(read('src/commands/AxiaCommands.ts')).not.toContain('tool-workplane');
    expect(read('index.html')).not.toContain('tool-workplane');
    expect(read('src/tools/ToolManagerRefactored.ts')).not.toContain('WorkPlaneTool');
  });

  it('moving the plane once it is set is reachable too', () => {
    const tm = read('src/tools/ToolManagerRefactored.ts');
    const html = read('index.html');
    const menu = read('src/ui/MenuBar.ts');
    const cat = read('../packages/axia-action-catalog/src/catalog.ts');
    const cmds = read('src/commands/AxiaCommands.ts');
    for (const id of ['sketch-offset', 'sketch-tilt']) {
      expect(tm, `${id} is not dispatched`).toContain(`'${id}'`);
      expect(html, `${id} is in no menu`).toContain(`data-action="${id}"`);
      expect(menu, `${id} does not reach the tool manager`).toContain(`case '${id}':`);
      expect(cat, `${id} is not in the catalog`).toContain(`id: '${id}'`);
      expect(cmds, `${id} cannot be searched for`).toContain(`'${id}'`);
    }
  });
});

describe('moving a plane that is already set', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function managerWithSketch(normal: THREE.Vector3, up: THREE.Vector3, origin: THREE.Vector3): any {
    // The method reads and writes `this._sketch` and calls one viewport hook;
    // a stand-in with those is enough to measure what it does to the plane.
    const setVisual = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const mgr: any = {
      _sketch: {
        label: 'x', origin: origin.clone(), normal: normal.clone(), up: up.clone(),
        right: new THREE.Vector3().crossVectors(up, normal).normalize(),
      },
      viewport: { setSketchPlaneVisual: setVisual },
    };
    return { mgr, setVisual };
  }

  it('offset moves the origin along the normal and leaves the direction alone', () => {
    const { mgr } = managerWithSketch(
      new THREE.Vector3(0, 0, 1), new THREE.Vector3(0, 1, 0), new THREE.Vector3(0, 0, 0),
    );
    vi.spyOn(window, 'prompt').mockReturnValue('250');
    ToolManager.prototype.adjustSketchPlane.call(mgr, 'offset');
    expect([mgr._sketch.origin.x, mgr._sketch.origin.y, mgr._sketch.origin.z]).toEqual([0, 0, 250]);
    expect(mgr._sketch.normal.z).toBeCloseTo(1, 9);
  });

  it('tilt turns the plane about an axis IN it, so the origin stays put', () => {
    const origin = new THREE.Vector3(10, 20, 30);
    const { mgr } = managerWithSketch(
      new THREE.Vector3(0, 0, 1), new THREE.Vector3(0, 1, 0), origin,
    );
    vi.spyOn(window, 'prompt').mockReturnValue('90');
    ToolManager.prototype.adjustSketchPlane.call(mgr, 'tilt');
    // Turned a quarter, so the normal has left +Z entirely.
    expect(Math.abs(mgr._sketch.normal.z)).toBeLessThan(1e-9);
    // The origin is a point ON the plane and the tilt pivots about it.
    expect([mgr._sketch.origin.x, mgr._sketch.origin.y, mgr._sketch.origin.z])
      .toEqual([10, 20, 30]);
    const { normal, up, right } = mgr._sketch;
    expect(Math.abs(normal.dot(up))).toBeLessThan(1e-9);
    expect(Math.abs(normal.dot(right))).toBeLessThan(1e-9);
    expect(Math.abs(up.dot(right))).toBeLessThan(1e-9);
  });

  it('cancelling the prompt changes nothing', () => {
    const { mgr, setVisual } = managerWithSketch(
      new THREE.Vector3(0, 0, 1), new THREE.Vector3(0, 1, 0), new THREE.Vector3(0, 0, 0),
    );
    vi.spyOn(window, 'prompt').mockReturnValue(null);
    ToolManager.prototype.adjustSketchPlane.call(mgr, 'offset');
    expect(mgr._sketch.origin.z).toBe(0);
    expect(setVisual).not.toHaveBeenCalled();
  });

  it('a non-number is refused rather than making the plane NaN', () => {
    const { mgr } = managerWithSketch(
      new THREE.Vector3(0, 0, 1), new THREE.Vector3(0, 1, 0), new THREE.Vector3(0, 0, 0),
    );
    vi.spyOn(window, 'prompt').mockReturnValue('kkk');
    ToolManager.prototype.adjustSketchPlane.call(mgr, 'offset');
    expect(Number.isFinite(mgr._sketch.origin.z)).toBe(true);
    expect(mgr._sketch.origin.z).toBe(0);
  });
});
