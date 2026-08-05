/**
 * ALT DRAWS FLAT ON A CURVED FACE.
 *
 * Drawing on a sphere, a cylinder, a cone or a torus follows the surface, and
 * has since ADR-284 — the first click captures the curved host and the shape is
 * projected onto it. There was no way to ask for the other thing. A flat plate
 * resting tangent to a sphere, a rectangle standing off a cylinder: measured
 * 2026-08-05, all six curved-aware tools read zero modifiers, so the surface
 * always won.
 *
 * Alt is the escape. It works by declining to CAPTURE the host: every commit
 * path already handles a host that was never captured, because that is the
 * ordinary non-curved case, so the tangent-plane draw comes back on its own
 * rather than through a second code path that could drift from the first.
 *
 * ⚠ Alt reads opposite here and in DrawLineTool — there the default is the
 * plane and Alt follows the solid; here the default is the surface and Alt
 * leaves it. Both are the same rule: **Alt means "not the default"**. That is a
 * decision, written down so it is not read as an accident.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as THREE from 'three';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const TOOLS = join(__dirname);
const CURVED_TOOLS = [
  'DrawRectTool.ts',
  'DrawPolygonTool.ts',
  'DrawCircleTool.ts',
  'DrawEllipseTool.ts',
  'DrawFreehandTool.ts',
  'DrawBezierTool.ts',
];

const read = (f: string) => readFileSync(join(TOOLS, f), 'utf8');

describe('Alt draws flat on a curved face', () => {
  it('every tool that draws on a surface offers the escape', () => {
    for (const f of CURVED_TOOLS) {
      const src = read(f);
      expect(src, `${f} does not route to a curved surface any more — is this list stale?`)
        .toMatch(/drawPolylineOnCurved|drawCircleOn(Sphere|Cylinder|Cone|Torus)/);
      expect(src, `${f} has no Alt escape — a flat draw on this surface is impossible`)
        .toMatch(/handlesAltClick\s*=\s*true/);
    }
  });

  it('the escape is on the CAPTURE, so the flat path is the ordinary one', () => {
    // Gating the capture (not the commit) is what keeps the two paths from
    // drifting: with no host captured, a curved face commits exactly the way a
    // planar one does.
    for (const f of CURVED_TOOLS) {
      const src = read(f);
      const onDown = src.slice(src.indexOf('onMouseDown('));
      const guard = /(if \(ck && !\w+\.altKey)|(surfaceKind === \d && !\w+\.altKey)/;
      expect(guard.test(onDown), `${f}: alt does not gate the curved-host capture`).toBe(true);
    }
  });

  it('no OTHER tool quietly claims alt-clicks by copying the flag', () => {
    // The blanket gate in ToolManager drops alt-clicks for tools that do not opt
    // in, so an unused flag would silently change what a modifier does there.
    const optedIn = readdirSync(TOOLS)
      .filter((n) => /^\w+Tool\.ts$/.test(n) && n !== 'ITool.ts')
      .filter((n) => /handlesAltClick\s*=\s*true/.test(read(n)));
    const expected = new Set([...CURVED_TOOLS, 'DrawLineTool.ts', 'SelectTool.ts', 'GroupTool.ts']);
    for (const f of optedIn) {
      expect(expected.has(f), `${f} claims alt-clicks — is that intended?`).toBe(true);
    }
  });
});

describe('what Alt actually does to a rectangle on a sphere', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let ctx: any;

  beforeEach(() => {
    ctx = {
      bridge: {
        drawPolylineOnCurved: vi.fn().mockReturnValue('{"cap":2,"annulus":0}'),
        drawRectAsShape: vi.fn().mockReturnValue(1),
        faceCount: vi.fn().mockReturnValue(2),
        lastError: vi.fn().mockReturnValue(''),
      },
      viewport: {
        pick: vi.fn().mockReturnValue({ faceIndex: 0, point: { x: 100, y: 0, z: 0 } }),
        scene: { add: vi.fn(), remove: vi.fn() },
      },
      getFaceId: vi.fn().mockReturnValue(7),
      getDrawPlane: vi.fn().mockReturnValue({
        surfaceKind: 3, // Sphere
        normal: new THREE.Vector3(1, 0, 0),
        up: new THREE.Vector3(0, 0, 1),
        right: new THREE.Vector3(0, 1, 0),
        origin: new THREE.Vector3(100, 0, 0),
        onFace: true,
      }),
      get3DPoint: vi.fn().mockReturnValue(new THREE.Vector3(100, 0, 0)),
      snap: { setReferencePoint: vi.fn(), clearReferencePoint: vi.fn() },
      lockPlane: vi.fn(),
      syncMesh: vi.fn(),
      setVCB: vi.fn(),
    };
  });

  async function firstClick(alt: boolean) {
    const { DrawRectTool } = await import('./DrawRectTool');
    const tool = new DrawRectTool(ctx);
    const e = { clientX: 10, clientY: 10, altKey: alt, button: 0 } as MouseEvent;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    tool.onMouseDown(e, new THREE.Vector3(100, 0, 0) as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return tool as any;
  }

  it('without Alt the sphere is captured — the rect will follow it', async () => {
    const tool = await firstClick(false);
    expect(tool.curvedKind).toBe('sphere');
    expect(tool.curvedHostFace).toBe(7);
  });

  it('with Alt nothing is captured — the rect stays on the tangent plane', async () => {
    const tool = await firstClick(true);
    expect(tool.curvedKind).toBeNull();
    expect(tool.curvedHostFace).toBe(-1);
  });
});
