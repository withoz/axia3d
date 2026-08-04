/**
 * 면을 따라 그리기 — real mouse, real WASM.
 *
 * 사용자 (2026-08-03):
 * > "입체면을 따라 그리는 기능 확장"
 *
 * A line drawn from a point on one face of a box to a point on another used to
 * be a straight chord through the interior: it touched neither face and divided
 * nothing. Along the surface it bends where it crosses the edge between them.
 *
 * The signature of that bend is a vertex sitting ON the shared edge, strictly
 * between the box's two corners — a chord produces no such point, and neither
 * does a line confined to one plane. That is what this spec looks for, through
 * the whole path: canvas mousedown → ToolManager → DrawLineTool → bridge →
 * WASM → the engine's surface route.
 */
import { test, expect } from '@playwright/test';

interface AxiaWindow {
  __axia?: { get<T>(key: string): T };
}

async function setup(page: import('@playwright/test').Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(
    () => !!(window as unknown as AxiaWindow).__axia,
    undefined,
    { timeout: 10_000 },
  );
  await page.waitForFunction(
    () => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia?.get<any>('bridge');
      return !!bridge?.isReady?.();
    },
    undefined,
    { timeout: 10_000 },
  );
}

/** Where a world point lands on screen, so the clicks hit the faces we mean. */
async function screenOf(
  page: import('@playwright/test').Page,
  world: [number, number, number],
): Promise<{ x: number; y: number }> {
  return page.evaluate((w) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const vp = (window as any).__axia.get('viewport');
    const cam = vp.activeCamera;
    cam.updateMatrixWorld();
    const rect = vp.renderer.domElement.getBoundingClientRect();
    // `cam.position` is a THREE.Vector3, so cloning it gives us one without
    // needing THREE on the page.
    const p = cam.position.clone().set(w[0], w[1], w[2]).project(cam);
    return {
      x: rect.left + (p.x * 0.5 + 0.5) * rect.width,
      y: rect.top + (-p.y * 0.5 + 0.5) * rect.height,
    };
  }, world);
}

/** Endpoints of every wireframe segment, as the user sees them. */
async function wirePoints(
  page: import('@playwright/test').Page,
): Promise<[number, number, number][]> {
  return page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge');
    const lines = bridge.getEdgeLines?.();
    const out: [number, number, number][] = [];
    if (!lines) return out;
    for (let i = 0; i + 2 < lines.length; i += 3) {
      out.push([lines[i], lines[i + 1], lines[i + 2]]);
    }
    return out;
  });
}

test.describe('면을 따라 그리기 (drawing along a solid)', () => {
  test('a line clicked across two faces bends on the edge — with Alt held', async ({ page }) => {
    test.setTimeout(45_000);
    await setup(page);

    // A box spanning x 0..200, y 0..200, z 0..100 — top face and east wall.
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const axia = (window as any).__axia;
      const bridge = axia.get('bridge');
      bridge.create_box(100, 100, 50, 200, 100, 200);
      axia.get('toolManager').syncMesh();
      axia.get('viewport').setViewMode?.('3d');
    });
    await page.waitForTimeout(400);

    const before = await wirePoints(page);
    const onSharedEdgeInterior = (pts: [number, number, number][]) =>
      pts.filter(
        ([x, y, z]) =>
          Math.abs(x - 200) < 0.5 && Math.abs(z - 100) < 0.5 && y > 1 && y < 199,
      );
    expect(
      onSharedEdgeInterior(before).length,
      'the plain box must have no vertex partway along the top-east edge',
    ).toBe(0);

    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });

    // One click on the top face, one on the east wall — the second with Alt
    // held, which is how following the solid is asked for (사용자 2026-08-04:
    // `수식어가 반대로 누르고 그리면 면따라서, 기본은 연장으로`).
    const a = await screenOf(page, [100, 100, 100]);
    const b = await screenOf(page, [200, 100, 50]);
    await page.mouse.click(a.x, a.y);
    await page.waitForTimeout(120);
    await page.keyboard.down('Alt');
    await page.mouse.click(b.x, b.y);
    await page.keyboard.up('Alt');
    await page.waitForTimeout(400);

    const after = await wirePoints(page);
    const bends = onSharedEdgeInterior(after);
    expect(
      bends.length,
      `the drawn line must turn the corner on the shared edge; wire points ` +
        `went ${before.length} → ${after.length}`,
    ).toBeGreaterThanOrEqual(1);

    // And nothing was left inside the solid, which is what the old chord did.
    const inside = after.filter(
      ([x, y, z]) => x > 1 && x < 199 && y > 1 && y < 199 && z > 1 && z < 99,
    );
    expect(inside, 'no drawn point may sit inside the box').toEqual([]);

    const health = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const inv = bridge.verifyInvariants?.();
      return { valid: inv?.valid, violations: inv?.violations?.length ?? 0 };
    });
    expect(health.valid, `invariants: ${health.violations} violations`).toBe(true);
  });

  /// The other half of the policy, and the half a user meets first.
  ///
  /// 사용자 (2026-08-04): `기본은 연장으로`. Without the modifier this used to
  /// bend by itself the moment the cursor landed on another face — the cursor
  /// deciding, not the user. Clicked the same way, the line must now stay on
  /// the plane it started on.
  test('the same two clicks WITHOUT Alt stay on the plane they started on', async ({ page }) => {
    test.setTimeout(45_000);
    await setup(page);

    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const axia = (window as any).__axia;
      axia.get('bridge').create_box(100, 100, 50, 200, 100, 200);
      axia.get('toolManager').syncMesh();
      axia.get('viewport').setViewMode?.('3d');
    });
    await page.waitForTimeout(400);

    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });

    const a = await screenOf(page, [100, 100, 100]);
    const b = await screenOf(page, [200, 100, 50]);
    await page.mouse.click(a.x, a.y);
    await page.waitForTimeout(120);
    await page.mouse.click(b.x, b.y); // no Alt
    await page.waitForTimeout(400);

    const after = await wirePoints(page);
    const onSharedEdgeInterior = after.filter(
      ([x, y, z]) =>
        Math.abs(x - 200) < 0.5 && Math.abs(z - 100) < 0.5 && y > 1 && y < 199,
    );
    expect(
      onSharedEdgeInterior,
      'without the modifier the line must not turn the corner',
    ).toEqual([]);

    const health = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const inv = bridge.verifyInvariants?.();
      return { valid: inv?.valid, violations: inv?.violations?.length ?? 0 };
    });
    expect(health.valid, `invariants: ${health.violations} violations`).toBe(true);
  });

  test('a line to the far side is refused and leaves the solid alone', async ({ page }) => {
    test.setTimeout(45_000);
    await setup(page);

    const state = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_box(100, 100, 50, 200, 100, 200);
      const before = bridge.faceCount();
      // Top face to bottom face: they do not touch, so there is no two-face
      // route. The engine must say so rather than drawing a chord.
      const id = bridge.drawLineAlongSurface(100, 100, 100, 100, 100, 0);
      const inv = bridge.verifyInvariants?.();
      return {
        before,
        after: bridge.faceCount(),
        id,
        reason: bridge.lastError?.() ?? '',
        valid: inv?.valid,
      };
    });

    expect(state.id).toBe(-1);
    expect(state.after, 'a refusal must not change the solid').toBe(state.before);
    expect(state.reason).toContain('맞닿지 않은');
    expect(state.valid).toBe(true);
  });
});
