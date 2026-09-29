/**
 * A custom material is one the engine holds (ADR-313 D4).
 *
 * Two places made materials only the app's table knew:
 *
 * ```text
 *   Quick Colour     invented rustId ≥ 10001, "the engine stores ids as opaque
 *                    numbers" — it does not; AssignMaterial refuses an id its
 *                    library does not hold
 *   texture dialog   sent rustId 0, "addCustom assigns one" — it never did;
 *                    0 is FORM_MATERIAL ("no material"), which the engine
 *                    refuses since ADR-313 — the face keeps what it had
 * ```
 *
 * So neither material was ever held by the engine: not recorded on a face,
 * saved or exported, and the first read-back from the engine took the colour
 * away — measured (ADR-313 §2.4): a red Quick Colour face turned grey on an
 * unrelated draw and undo.
 *
 * `addEngineMaterial` asks the engine to create the material first and uses
 * the id it returns. The E2E `a-quick-colour-is-kept` holds the real engine.
 *
 * ⚠ Mutation-checked: make `addEngineMaterial` fall back to `addCustom` with
 * an invented id when the engine does not answer, and "adds nothing" fails;
 * point either call site back at `addCustom` and its call-site check fails.
 */
import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { MaterialLibrary } from './MaterialLibrary';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');
const code = (p: string) =>
  read(p).replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');

const spec = {
  id: 'quick-1',
  name: '색상 #ff0000',
  nameEn: 'Color #ff0000',
  category: 'custom' as const,
  physical: { density: 1000, friction: 0.5, restitution: 0.5, specificGravity: 1, thermalConductivity: 0.5 },
  visual: { color: 0xff0000, roughness: 0.5, metalness: 0, opacity: 1 },
};

describe('addEngineMaterial', () => {
  it('uses the id the engine gives it', () => {
    const lib = new MaterialLibrary();
    const addProjectMaterial = vi.fn(() => 101);
    lib.setBridge({ addProjectMaterial });
    const m = lib.addEngineMaterial(spec);
    expect(addProjectMaterial).toHaveBeenCalledWith('색상 #ff0000', 'Color #ff0000', 0xff0000);
    expect(m?.rustId).toBe(101);
    expect(lib.get('quick-1')?.rustId).toBe(101);
  });

  it('adds nothing when the engine cannot create it', () => {
    const lib = new MaterialLibrary();
    lib.setBridge({ addProjectMaterial: vi.fn(() => null) });
    expect(lib.addEngineMaterial(spec)).toBeNull();
    expect(lib.get('quick-1')).toBeUndefined();

    const noBridge = new MaterialLibrary();
    expect(noBridge.addEngineMaterial(spec)).toBeNull();
    expect(noBridge.get('quick-1')).toBeUndefined();
  });
});

describe('the two call sites that make custom materials', () => {
  it('Quick Colour asks the engine, and invents no id', () => {
    const tm = code('src/tools/ToolManagerRefactored.ts');
    // PREMISE: the action is still there, or every absence below is vacuous.
    expect(tm, 'the Quick Colour action must still exist').toContain("=== 'assign-quick-color'");
    const branch = tm.slice(tm.indexOf("=== 'assign-quick-color'"), tm.indexOf("=== 'chamfer-edge'"));
    expect(branch.length, 'the branch must be found between its neighbours').toBeGreaterThan(200);
    expect(branch).toContain('lib.addEngineMaterial(');
    expect(branch, 'no custom material the engine does not hold').not.toContain('lib.addCustom(');
    expect(branch, 'no invented id').not.toMatch(/10001/);
  });

  it('the texture dialog asks the engine, and sends no 0', () => {
    const dialog = code('src/ui/TextureUploadDialog.ts');
    expect(dialog).toContain('lib.addEngineMaterial(');
    expect(dialog).not.toContain('lib.addCustom(');
    expect(dialog, '0 is FORM_MATERIAL — "no material"').not.toMatch(/rustId:\s*0\b/);
  });
});
