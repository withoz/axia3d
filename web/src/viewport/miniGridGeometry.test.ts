import { describe, it, expect } from 'vitest';
import {
  perspectiveRadiusWorld,
  orthographicRadiusWorld,
  miniGridSegments,
} from './miniGridGeometry';
import {
  MINI_GRID_RADIUS_PX_DEFAULT,
  MINI_GRID_CELLS_DEFAULT,
  MINI_GRID_LINE_HW_PX_DEFAULT,
} from '../tools/MiniGridSettings';

/**
 * The cursor mini-grid's geometry.
 *
 * The point of the whole thing is that the patch keeps its APPARENT size, so
 * that is what gets asserted — not the world numbers, which are supposed to
 * change.
 */
describe('mini grid geometry', () => {
  const FOV = Math.PI / 4;
  const H = 975;

  it('grows in proportion to depth, which is what makes it zoom-invariant', () => {
    const near = perspectiveRadiusWorld(48, 4, FOV, H);
    const far = perspectiveRadiusWorld(48, 40, FOV, H);
    // Ten times the depth, ten times the world radius — so ten times smaller on
    // screen per world unit, which cancels. A fixed world radius was the bug.
    expect(far / near).toBeCloseTo(10, 6);
  });

  it('is proportional to the pixel radius it was asked for', () => {
    const a = perspectiveRadiusWorld(48, 10, FOV, H);
    const b = perspectiveRadiusWorld(96, 10, FOV, H);
    expect(b / a).toBeCloseTo(2, 6);
  });

  /**
   * ⚠ The branch Kayac does not have. Its comment says "still perspective
   * projection — true ortho is a separate polish"; our axis views run
   * `OrthographicCamera`, where magnification does not depend on depth.
   */
  it('does not depend on depth under an orthographic camera', () => {
    const frustumH = 200;
    const a = orthographicRadiusWorld(48, frustumH, H);
    const b = orthographicRadiusWorld(48, frustumH, H);
    expect(a).toBe(b);
    // and it does track the frustum, which is what zooming an ortho camera moves
    expect(orthographicRadiusWorld(48, frustumH * 3, H) / a).toBeCloseTo(3, 6);
  });

  it('keeps the cell density constant as the camera pulls back', () => {
    // Each cell is radius/cells wide, and radius is linear in depth, so a cell
    // covers the same number of pixels at any depth.
    const cells = MINI_GRID_CELLS_DEFAULT;
    const per = (depth: number) => perspectiveRadiusWorld(48, depth, FOV, H) / cells / depth;
    expect(per(4)).toBeCloseTo(per(400), 12);
  });

  describe('the disc', () => {
    const C = { x: 0, y: 0, z: 0 };
    const U = { x: 1, y: 0, z: 0 };
    const V = { x: 0, y: 1, z: 0 };

    it('clips every line to the circle rather than drawing a square', () => {
      const seg = miniGridSegments(C, U, V, 10, 4);
      expect(seg.length).toBeGreaterThan(0);
      // No point may stick out of the disc.
      for (let i = 0; i < seg.length; i += 3) {
        const r = Math.hypot(seg[i], seg[i + 1], seg[i + 2]);
        expect(r).toBeLessThanOrEqual(10 + 1e-9);
      }
    });

    it('has a line through the centre and none outside the rim', () => {
      const cells = 4;
      const r = 10;
      const seg = miniGridSegments(C, U, V, r, cells);
      // 2*cells+1 offsets, but the two at |off| == r have zero chord and are
      // dropped, so 2*cells-1 offsets survive, each giving 2 lines.
      expect(seg.length / 6).toBe((2 * cells - 1) * 2);
      // The centre cross reaches the full radius.
      const longest = Math.max(
        ...Array.from({ length: seg.length / 6 }, (_, k) => {
          const o = k * 6;
          return Math.hypot(seg[o + 3] - seg[o], seg[o + 4] - seg[o + 1], seg[o + 5] - seg[o + 2]);
        }),
      );
      expect(longest).toBeCloseTo(2 * r, 9);
    });

    it('lies in the plane it was given', () => {
      // A vertical plane: u along X, v along Z. Nothing may leave y = 7.
      const c = { x: 1, y: 7, z: 2 };
      const seg = miniGridSegments(c, { x: 1, y: 0, z: 0 }, { x: 0, y: 0, z: 1 }, 5, 3);
      for (let i = 1; i < seg.length; i += 3) expect(seg[i]).toBeCloseTo(7, 12);
    });

    it('returns nothing rather than NaN when there is no radius to draw', () => {
      expect(miniGridSegments(C, U, V, 0, 4)).toEqual([]);
      expect(miniGridSegments(C, U, V, Number.NaN, 4)).toEqual([]);
      expect(miniGridSegments(C, U, V, -1, 4)).toEqual([]);
    });

    it('draws more lines when asked for more cells', () => {
      const few = miniGridSegments(C, U, V, 10, 2).length;
      const many = miniGridSegments(C, U, V, 10, 16).length;
      expect(many).toBeGreaterThan(few);
    });
  });

  it('ships the values Kayac ships', () => {
    // The spec table, and Kayac's `MiniGridSettings::default()`.
    expect(MINI_GRID_RADIUS_PX_DEFAULT).toBe(48);
    expect(MINI_GRID_CELLS_DEFAULT).toBe(4);
    expect(MINI_GRID_LINE_HW_PX_DEFAULT).toBe(0.5);
  });
});
