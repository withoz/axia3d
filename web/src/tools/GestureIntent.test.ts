/**
 * Two gestures that did the opposite of what the user meant.
 *
 * **Right-click while drawing committed the shape.** The context-menu handler
 * says "right click cancels the operation" and implemented that by forwarding a
 * synthetic right-button mousedown, hoping the tool would recognise it. Only
 * DrawLine ever read `e.button`; Rect, Circle, Arc, Polygon, Ellipse, Bezier and
 * Freehand took the event as the closing click and committed. Right-clicking to
 * back out of a rectangle drew one, with the context menu opening on top of it.
 *
 * **Double-click dropped the user's modifiers.** One double-click runs
 * SelectTool.onMouseDown twice AND the canvas dblclick handler, in that order.
 * SelectTool got it right (`selectFaceWithEdges(fid, shift, ctrl, alt)`); the
 * dblclick handler called the same method with no arguments, and since it fires
 * last its all-false defaults won — a shift+double-click meant to add a face
 * wiped the accumulated selection. The two also picked differently: face-only
 * versus edge-preferred-within-18px, so one gesture had two targets.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const TOOLS_DIR = resolve(__dirname);
const read = (p: string) => readFileSync(resolve(__dirname, p), 'utf8');
const TM = read('ToolManagerRefactored.ts');

/** Every drawing tool — discovered, so a new one is covered on arrival. */
const drawTools = () =>
  readdirSync(TOOLS_DIR).filter((f) => /^Draw\w+Tool\.ts$/.test(f));

describe('right-click while drawing cancels, it does not commit', () => {
  it('the handler cancels unless the tool claims the gesture', () => {
    const h = TM.slice(TM.indexOf("addEventListener('contextmenu'"), TM.indexOf("// ===== MOUSE DOWN ====="));
    expect(h.length, 'contextmenu handler not found').toBeGreaterThan(100);
    expect(h, 'the cancel path is gone — a right-click will commit again')
      .toContain('this.cancelCurrentTool()');
    expect(h, 'the forward is no longer gated on the tool opting in')
      .toContain('handlesRightClick');
  });

  it('exactly the tools that read e.button opt in', () => {
    // The flag and the behaviour have to agree: opting in means "I handle it",
    // so a tool that never inspects the button must NOT be opted in, or it goes
    // back to committing.
    const readsButton = new Set<string>();
    const optedIn = new Set<string>();
    for (const f of drawTools()) {
      const src = read(f);
      if (/e\.button === 2/.test(src)) readsButton.add(f);
      if (/handlesRightClick\s*=\s*true/.test(src)) optedIn.add(f);
    }
    expect(readsButton.size, 'no draw tool reads e.button — did the convention change?')
      .toBeGreaterThan(0);
    expect([...optedIn].sort(), 'a tool claims right-clicks without reading the button')
      .toEqual([...readsButton].sort());
  });

  it('and DrawLine is the one that does, because it ends a polyline', () => {
    expect(read('DrawLineTool.ts')).toMatch(/handlesRightClick\s*=\s*true/);
    expect(read('DrawRectTool.ts'), 'Rect claims right-clicks but never reads the button')
      .not.toMatch(/handlesRightClick\s*=\s*true/);
  });
});

describe('a double-click resolves to one selection, with the modifiers intact', () => {
  const h = TM.slice(TM.indexOf("addEventListener('dblclick'"), TM.indexOf("// ===== CONTEXT MENU"));

  it('the dblclick handler no longer selects faces itself', () => {
    expect(h.length, 'dblclick handler not found').toBeGreaterThan(100);
    expect(
      /selectFaceWithEdges\s*\(/.test(h),
      'the dblclick handler is selecting faces again — SelectTool already did it, ' +
        'with the modifiers, and this one fires last',
    ).toBe(false);
  });

  it('it still enters group edit, which SelectTool cannot do', () => {
    // Removing the duplicate must not have removed the reason the handler exists.
    expect(h).toContain('enterEditMode');
    expect(read('SelectTool.ts'), 'SelectTool gained a group-edit path — this handler may be redundant now')
      .not.toContain('enterEditMode');
  });

  it('SelectTool passes the modifiers through on the second press', () => {
    const st = read('SelectTool.ts');
    const dbl = st.slice(st.indexOf('clickCount === 2'), st.indexOf('clickCount === 2') + 400);
    expect(dbl).toMatch(/selectFaceWithEdges\([^)]*e\.shiftKey[^)]*e\.ctrlKey/);
  });

  it('and the defaults that made the loss silent are still all-false', () => {
    // If these ever became `true`, a no-argument call would stop clearing and
    // this whole class of bug would change shape rather than disappear.
    const sm = read('SelectionManager.ts');
    const sig = /selectFaceWithEdges\(faceId: number,([^)]*)\)/.exec(sm)?.[1] ?? '';
    expect(sig, 'selectFaceWithEdges signature not found').not.toBe('');
    expect(sig.replace(/\s/g, '')).toContain('shiftKey:boolean=false');
  });
});

describe('Alt reaches the tools that mean something by it', () => {
  it('the canvas no longer drops every alt-click', () => {
    const h = TM.slice(TM.indexOf("addEventListener('mousedown'"), TM.indexOf("// ===== MOUSE MOVE ====="));
    expect(h.length, 'mousedown handler not found').toBeGreaterThan(100);
    expect(
      /if \(e\.button !== 0 \|\| e\.altKey\) return;/.test(h),
      'the blanket alt-click drop is back — Alt-removes-from-selection and ' +
        'alt-drag-select both start in this handler and would be dead again',
    ).toBe(false);
    expect(h, 'alt must still be gated on the tool opting in').toContain('handlesAltClick');
  });

  it('exactly the tools that read e.altKey on a press opt in', () => {
    const readsAlt = new Set<string>();
    const optedIn = new Set<string>();
    // ITool.ts is the interface, not a tool — it names the flag by definition.
    for (const f of readdirSync(TOOLS_DIR).filter((n) => /^\w+Tool\.ts$/.test(n) && n !== 'ITool.ts')) {
      const src = read(f);
      // onMouseDown body only — a keyboard guard like `!e.altKey` in onKeyDown
      // is not a claim on the click.
      const md = src.slice(src.indexOf('onMouseDown('), src.indexOf('onKeyDown(') >= 0 ? src.indexOf('onKeyDown(') : undefined);
      if (md.includes('onMouseDown(') && /e\.altKey/.test(md)) readsAlt.add(f);
      if (/handlesAltClick\s*=\s*true/.test(src)) optedIn.add(f);
    }
    expect(readsAlt.size, 'no tool reads e.altKey on a press — did the convention change?')
      .toBeGreaterThan(0);
    expect([...optedIn].sort(), 'a tool claims alt-clicks without reading the modifier, or vice versa')
      .toEqual([...readsAlt].sort());
  });

  it('and Alt still means "remove from the selection" downstream', () => {
    const sm = read('SelectionManager.ts');
    const body = sm.slice(sm.indexOf('handleClick(faceId'), sm.indexOf('handleClick(faceId') + 900);
    expect(body).toMatch(/if \(altKey\)/);
    expect(body).toMatch(/this\.selected\.delete\(faceId\)/);
  });
});
