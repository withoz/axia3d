/**
 * The app reads materials back from the engine (ADR-313 D3).
 *
 * The viewport colours a face from this library's table of assignments, and
 * until ADR-313 nothing refilled that table from the engine. Measured in a real
 * browser: a brick box saved, reopened and still 벽돌 in the engine came back
 * grey with the Inspector reading "없음"; an IFC-imported brick member never
 * showed its colour at all. The method this replaces only re-read faces the
 * app already knew, so a face it had never seen stayed unknown forever.
 *
 * These tests drive `syncFromEngine` against a fake bridge that answers the
 * way the engine does. Whether the app actually CALLS it at the right moments
 * is held by the E2E `the-app-shows-the-materials-the-engine-holds`.
 *
 * ⚠ Mutation-checked: iterate only the known assignments (the old method's
 * shape) and "a face the app never saw" fails; drop the mirroring and "a
 * material the app has no entry for" fails.
 */
import { describe, it, expect, vi } from 'vitest';
import { MaterialLibrary } from './MaterialLibrary';

function engine(faces: Array<[number, number]> | null, list: object[] = []) {
  return {
    getFaceMaterials: vi.fn(() => (faces ? new Map(faces) : null)),
    getAllMaterials: vi.fn(() => JSON.stringify(list)),
  };
}

describe('syncFromEngine', () => {
  it('learns a face the app never saw', () => {
    const lib = new MaterialLibrary();
    // 5 is 벽돌 in both tables since ADR-313.
    lib.setBridge(engine([[42, 5]]));
    lib.syncFromEngine();
    expect(lib.getMaterialForFace(42)?.id).toBe('brick');
  });

  it('forgets a face the engine no longer gives a material', () => {
    const lib = new MaterialLibrary();
    lib.setBridge(engine([]));
    lib.assignToFaces([7], 'concrete'); // with the fake, this only sets the table
    expect(lib.getMaterialForFace(7)?.id).toBe('concrete');
    lib.syncFromEngine();
    expect(lib.getMaterialForFace(7)).toBeUndefined();
  });

  it('mirrors a material the app has no entry for, from the engine’s numbers', () => {
    const lib = new MaterialLibrary();
    lib.setBridge(
      engine(
        [[9, 150]],
        [{
          id: 150, name: '적벽돌', nameEn: 'Red brick', density: 1800, color: '#aa3322',
          thermalConductivity: 0.7, roughness: 0.8, metalness: 0, opacity: 1,
          friction: 0.6, restitution: 0.1, specificGravity: 1.8,
        }],
      ),
    );
    lib.syncFromEngine();
    const m = lib.getMaterialForFace(9);
    expect(m?.name).toBe('적벽돌');
    expect(m?.rustId).toBe(150);
    expect(m?.visual.color).toBe(0xaa3322);
    expect(m?.physical.density).toBe(1800);
    expect(m?.physical.thermalConductivity).toBe(0.7);
    // The engine's fire rating is a different model and an import claims none.
    expect(m?.physical.fireRating).toBeUndefined();
  });

  it('keeps what it has when the engine cannot answer — null is not "none"', () => {
    const lib = new MaterialLibrary();
    lib.setBridge(engine(null));
    lib.assignToFaces([3], 'wood');
    lib.syncFromEngine();
    expect(lib.getMaterialForFace(3)?.id).toBe('wood');
  });

  it('tells its listeners only when something changed', () => {
    const lib = new MaterialLibrary();
    lib.setBridge(engine([[1, 1]]));
    const heard = vi.fn();
    lib.onChange(heard);
    lib.syncFromEngine();
    expect(heard).toHaveBeenCalledTimes(1);
    lib.syncFromEngine();
    expect(heard).toHaveBeenCalledTimes(1);
  });
});
