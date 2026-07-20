/**
 * ADR-050 P-6 — XiaInspector badge label regression.
 *
 * Source-inspection level test (no Three.js / DOM rendering required).
 * Verifies that the XiaInspector module + index.html template carry the
 * correct ADR-049 §4 Q3 form/property labels:
 *   - "형태 (Shape)" for the no-material (form layer) state
 *   - "XIA (특성)" for the with-material (property layer) state
 * and that the deprecated "Appearance" / "XIA (물체)" / "XIA(물체)"
 * labels are no longer present.
 *
 * Drift guard — if anyone re-introduces the old labels, this test
 * fails fast.
 */
import { describe, it, expect } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';

describe('ADR-050 P-6 — XiaInspector badge labels', () => {
  it('XiaInspector.ts uses the new form/property label strings', () => {
    const file = path.resolve(__dirname, 'XiaInspector.ts');
    const src = fs.readFileSync(file, 'utf-8');

    // P-6 new labels present.
    expect(src).toContain("'형태 (Shape)'");
    expect(src).toContain("'XIA (특성)'");

    // Old labels removed.
    expect(src).not.toContain("'Appearance'");
    expect(src).not.toContain("'XIA (물체)'");
    expect(src).not.toContain("'XIA(물체)'");
  });

  it('index.html template carries P-6 labels (no remnant Appearance)', () => {
    // The Inspector is initialized with the form-state default.
    const file = path.resolve(__dirname, '..', '..', 'index.html');
    const src = fs.readFileSync(file, 'utf-8');

    // Initial badge text in xi-phys-badge must match form-state.
    expect(src).toContain('형태 (Shape)');
    // Hint message references property layer ("XIA (특성)").
    expect(src).toContain('XIA (특성)');
    // Old labels removed from template.
    expect(src).not.toContain('Appearance');
    expect(src).not.toContain('XIA(물체)');
  });
});

describe('I opens the Inspector — and only a bare I', () => {
  // Source-level, matching this file's approach: initXiaInspector needs a
  // bridge and a live viewport, so the behaviour itself is verified in the
  // browser. This guards the shape of the fix against re-introduction.
  const src = () =>
    fs.readFileSync(path.resolve(__dirname, 'XiaInspector.ts'), 'utf-8');

  it('the I listener reads modifiers before toggling', () => {
    // It read none, so Alt+I opened the Inspector on top of the intersection
    // snap it also toggles (KeyboardShortcuts A5: every Alt+<letter> is a snap
    // filter). One keystroke, two unrelated things.
    expect(src()).toMatch(/const bare = !e\.ctrlKey && !e\.altKey && !e\.metaKey && !e\.shiftKey/);
    expect(src()).toMatch(/bare && \(e\.key === 'i' \|\| e\.key === 'I'\)/);
  });

  it('KeyboardShortcuts no longer maps I to the Pie tool', () => {
    const ks = fs.readFileSync(path.resolve(__dirname, 'KeyboardShortcuts.ts'), 'utf-8');
    // Pie claimed I as "a free key" while this file already owned it.
    expect(ks).not.toMatch(/'i':\s*'pie'/);
    expect(ks).not.toMatch(/'I':\s*'pie'/);
  });
});

describe('ADR-203 δ — the element-type picker', () => {
  const src = () =>
    fs.readFileSync(path.resolve(__dirname, 'XiaInspector.ts'), 'utf-8');
  const html = () =>
    fs.readFileSync(path.resolve(__dirname, '..', '..', 'index.html'), 'utf-8');

  it('the picker exists in the Inspector template', () => {
    // Without it a slab exports as a wall and the user cannot say otherwise.
    expect(html()).toContain('id="xi-element-kind"');
    expect(html()).toContain('부재 종류');
  });

  it('the option list comes from the engine, not a hard-coded copy', () => {
    // Two lists would drift: the engine would accept a kind the picker never
    // offers, or offer one it rejects.
    expect(src()).toMatch(/bridge\.ifcElementKinds\(\)/);
  });

  it('it resolves the owner from the live selection, not a stale cache', () => {
    // The handler first read the Inspector's own currentFaceIds, which is
    // updated on redraw — changing the picker right after selecting did
    // nothing at all. Verified in the browser after the fix.
    expect(src()).toMatch(/toolManager\.selection\.getSelectedFaces\(\)/);
  });

  it('it classifies Shapes as well as Xias', () => {
    // Drawn members are Form citizens (LOCKED #26); handling only Xias would
    // leave most of what a user draws unclassifiable.
    expect(src()).toMatch(/bridge\.getShapeForFace/);
    expect(src()).toMatch(/bridge\.setShapeElementKind/);
    expect(src()).toMatch(/bridge\.setXiaElementKind/);
  });

  it('a rejected kind reverts the picker instead of leaving it lying', () => {
    // The engine refuses unknown kinds; the UI must not keep showing one.
    expect(src()).toMatch(/if \(!ok\) \{[\s\S]{0,200}refreshElementKind/);
  });
});
