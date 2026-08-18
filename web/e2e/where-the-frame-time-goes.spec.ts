/**
 * Where a frame's time actually goes — GPU submission, or CPU work.
 *
 * Written to answer one question with numbers instead of received wisdom: would
 * moving the renderer to WebGPU / wgpu improve anything here? That trade pays
 * where a frame is spent SUBMITTING work — many draw calls, many state changes
 * — and not where it is spent building the geometry to submit.
 *
 * Measured 2026-08-18, Chromium, production build:
 *
 * ```text
 *   프리미티브   면   엔진(ms)  동기화(ms)  드로우콜   삼각형   프레임(ms)
 *          8     24        10          4        17     7,072        0.1
 *         32    104        16          5        17    26,048        0.0
 *         96    320        97          8        17    75,956        0.0
 * ```
 *
 * Draw calls do not move with the scene — geometry is merged per type and per
 * material, which is what the drawcall audits (ADR-122 / #125 / #126 / #127)
 * already found. Triangles go up 11× and the frame does not notice. What DOES
 * scale is the engine: 10 ms to 97 ms building the same scene.
 *
 * So the submission side costs about 0.1 ms against a 16 ms budget, and the
 * work that costs is on the CPU — which is also where every measured win of the
 * last months came from (ADR-111 deferred a 145 ms BVH rebuild, ADR-112 removed
 * a 584 ms edges fallback, ADR-135 stopped far-field triangle explosion).
 *
 * ⚠ Two readings had to be thrown away before these: with the camera left where
 * it starts, everything was outside the frustum and `info.render` counted 52
 * triangles at every scale; and calling the bridge directly builds the mesh in
 * the ENGINE without syncing the viewport, so 320 faces still drew 52 triangles.
 * Both are the same mistake — the question had not reached the thing being
 * asked.
 */
import { test, expect } from '@playwright/test';

test.describe('where the frame time goes', () => {
  test('draw calls against CPU work, on a scene with curves and solids', async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge') && w.get('viewport');
      },
      { timeout: 30000 },
    );

    const r = await page.evaluate(async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const c = (window as any).__axia;
      const b = c.get('bridge');
      const vp = c.get('viewport');

      const t0 = performance.now();
      // A scene with what this engine actually makes: primitives whose faces are
      // analytic (so the tessellation is the CPU's work), a couple of solids,
      // and drawn shapes that divide them.
      for (let i = 0; i < 6; i++) {
        b.create_sphere?.(i * 300 - 750, 0, 0, 120);
        b.create_cylinder?.(i * 300 - 750, 400, 0, 90, 220);
        b.create_box?.(i * 300 - 750, -400, 100, 200, 200, 200);
      }
      b.drawRectAsShape?.(0, 0, 0, 0, 0, 1, 1, 0, 0, 900, 700);
      b.drawCircleAsCurve?.(0, 0, 0, 0, 0, 1, 500);
      const engineMs = performance.now() - t0;
      const tSync = performance.now();
      c.get('syncMesh')();
      const syncMs = performance.now() - tSync;
      const buildMs = performance.now() - t0;

      // Let the deferred work (BVH, smooth normals) actually run.
      await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      await new Promise((res) => setTimeout(res, 400));

      // Time a settled frame: no edits, just render.
      const times: number[] = [];
      for (let i = 0; i < 30; i++) {
        const s = performance.now();
        vp.renderer.render(vp.scene, vp.camera);
        times.push(performance.now() - s);
      }
      times.sort((a, b2) => a - b2);

      const info = vp.renderer.info;
      const stats = b.getStats?.() ?? {};
      const tel = c.tryGet?.('telemetry');

      return {
        faces: stats.faces ?? null,
        buildMs: Math.round(buildMs),
        engineMs: Math.round(engineMs),
        syncMs: Math.round(syncMs),
        render: {
          calls: info.render.calls,
          triangles: info.render.triangles,
          lines: info.render.lines,
          programs: info.programs?.length ?? null,
          geometries: info.memory.geometries,
          textures: info.memory.textures,
        },
        frameMs: {
          median: +times[Math.floor(times.length / 2)].toFixed(3),
          p90: +times[Math.floor(times.length * 0.9)].toFixed(3),
          max: +times[times.length - 1].toFixed(3),
        },
        telemetry: tel?.snapshot?.() ?? null,
      };
    });

    console.log(JSON.stringify(r, null, 2));

    // The scene has to actually exist, or the numbers mean nothing.
    expect(r.faces).toBeGreaterThan(20);

    // The shape of the answer: this renderer submits a SMALL number of draw
    // calls, because geometry is merged per material and per type. If that stops
    // being true, the drawcall audits (ADR-122 / #125 / #126 / #127) are stale
    // and the GPU-submission question is worth reopening.
    expect(r.render.calls).toBeLessThan(60);
  });

  test('the same reading at three scales — does the balance shift?', async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge') && w.get('viewport');
      },
      { timeout: 30000 },
    );

    const rows = await page.evaluate(async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const c = (window as any).__axia;
      const b = c.get('bridge');
      const vp = c.get('viewport');
      const out: Array<Record<string, number>> = [];
      let made = 0;

      for (const target of [8, 32, 96]) {
        const t0 = performance.now();
        for (; made < target; made++) {
          const x = (made % 16) * 320 - 2500;
          const y = Math.floor(made / 16) * 320 - 900;
          if (made % 3 === 0) b.create_sphere?.(x, y, 0, 120);
          else if (made % 3 === 1) b.create_cylinder?.(x, y, 0, 90, 220);
          else b.create_box?.(x, y, 100, 200, 200, 200);
        }
        const engineMs = performance.now() - t0;
        const tSync = performance.now();
        c.get('syncMesh')();
        const syncMs = performance.now() - tSync;
        const buildMs = performance.now() - t0;

        // Everything has to be IN FRAME or the GPU never sees it: the first
        // reading of this had triangles stuck at 52 across all three scales,
        // because the primitives were spread outside the default view and
        // `info.render` counts only what was drawn.
        vp.setCameraState?.({ radius: 6000, targetX: -1300, targetY: -300, targetZ: 0 });
        await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
        await new Promise((res) => setTimeout(res, 500));

        const times: number[] = [];
        for (let i = 0; i < 40; i++) {
          const s = performance.now();
          vp.renderer.render(vp.scene, vp.camera);
          times.push(performance.now() - s);
        }
        times.sort((a, b2) => a - b2);
        const info = vp.renderer.info;
        out.push({
          primitives: target,
          faces: b.getStats?.().faces ?? 0,
          buildMs: Math.round(buildMs),
          engineMs: Math.round(engineMs),
          syncMs: Math.round(syncMs),
          calls: info.render.calls,
          triangles: info.render.triangles,
          geometries: info.memory.geometries,
          frameMedianMs: +times[Math.floor(times.length / 2)].toFixed(3),
          frameMaxMs: +times[times.length - 1].toFixed(3),
        });
      }
      return out;
    });

    console.log();
    console.log('  scale sweep — GPU submission vs CPU work');
    for (const r of rows) {
      console.log('    ' + JSON.stringify(r));
    }
    console.log();

    // Draw calls must not grow with the scene — that is what merged-per-type
    // geometry buys, and it is the whole reason GPU submission is not the cost
    // here. If this starts scaling with primitive count, the audits are stale.
    const first = rows[0], last = rows[rows.length - 1];
    expect(last.calls).toBeLessThan(first.calls * 3);
    // And the frame must stay far under the 16 ms a 60 fps budget allows, at
    // every scale, or the GPU side really is worth revisiting.
    for (const r of rows) {
      expect(r.frameMedianMs).toBeLessThan(16);
    }
  });
});
