import { describe, it, expect, beforeEach, vi } from 'vitest';
import { initKeyboardShortcuts, KeyboardShortcutsDeps } from './KeyboardShortcuts';

// Mock VCB
vi.mock('./VCB', () => ({ vcbTools: new Set(['line', 'rect', 'circle', 'pushpull']) }));

function createDOM(): void {
  document.body.innerHTML = `
    <div id="toolbar">
      <button class="tool-btn" data-tool="select">Select</button>
      <button class="tool-btn" data-tool="line">Line</button>
      <button class="tool-btn" data-tool="rect">Rect</button>
      <button class="tool-btn" data-tool="undo">Undo</button>
      <button class="tool-btn" data-tool="redo">Redo</button>
    </div>
    <div id="tool-label">Select</div>
    <div id="view-mode-bar">
      <button class="view-btn" data-view="3d">3D</button>
      <button class="view-btn" data-view="top">Top</button>
      <button class="view-btn" data-view="front">Front</button>
    </div>
    <div id="home-btn">Home</div>
  `;
}

function mockDeps(): KeyboardShortcutsDeps {
  return {
    toolManager: {
      setTool: vi.fn(),
      executeAction: vi.fn(),
      cancelCurrentTool: vi.fn(),
      isToolBusy: vi.fn().mockReturnValue(false),
      currentTool: 'select',
      // ADR-270 — plane reset (Home / F5 / 🏠) targets
      isPlaneLocked: vi.fn().mockReturnValue(false),
      hasPinnedPlane: vi.fn().mockReturnValue(false),
      resetDrawingPlane: vi.fn(),
      unlockPlane: vi.fn(),
      snap: { toggle: vi.fn(), enabled: true },
      setAxisLock: vi.fn(),
      selection: {
        isInGroupEditMode: vi.fn().mockReturnValue(false),
        exitGroupEdit: vi.fn(),
        clearSelection: vi.fn(),
        // ADR-074 §E.5-4 — Boolean Group shortcut targets
        getSelectedFaces: vi.fn().mockReturnValue([10, 20, 30]),
        setGroupTag: vi.fn(),
        clearGroupTags: vi.fn(),
      },
    } as any,
    viewport: {
      setViewMode: vi.fn(),
      resetCamera: vi.fn(),
      viewMode: '3d',
    } as any,
    toolbar: document.getElementById('toolbar')!,
    viewModeBar: document.getElementById('view-mode-bar'),
    saveProject: vi.fn(),
    openProject: vi.fn(),
  };
}

function fireKey(key: string, opts: Partial<KeyboardEventInit> = {}): void {
  window.dispatchEvent(new KeyboardEvent('keydown', {
    key,
    bubbles: true,
    ...opts,
  }));
}

describe('KeyboardShortcuts', () => {
  let deps: ReturnType<typeof mockDeps>;

  beforeEach(() => {
    createDOM();
    deps = mockDeps();
    initKeyboardShortcuts(deps);
  });

  describe('tool shortcuts', () => {
    it('P switches to select (ADR-246 P↔V swap)', () => {
      fireKey('p');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('select');
    });

    it('L switches to line', () => {
      fireKey('l');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('line');
    });

    it('R switches to rect', () => {
      fireKey('r');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('rect');
    });

    it('C switches to circle', () => {
      fireKey('c');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('circle');
    });

    it('I does NOT pick the Pie tool — I is the XIA Inspector', () => {
      // Pie took I as "a free key", but XiaInspector had bound it long before,
      // so I picked the Pie tool AND opened the Inspector. The catalog and the
      // menu badge both said Pie; the user's call (2026-07-16) is Inspector.
      // Pie keeps its menu entry and the palette.
      fireKey('i');
      expect(deps.toolManager.setTool).not.toHaveBeenCalledWith('pie');
    });

    it('V switches to pushpull/Extrude-Cut (ADR-246 P↔V swap)', () => {
      fireKey('v');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('pushpull');
    });

    it('M switches to move', () => {
      fireKey('m');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('move');
    });

    it('Q switches to rotate', () => {
      fireKey('q');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('rotate');
    });

    it('S switches to scale', () => {
      fireKey('s');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('scale');
    });

    it('O switches to offset', () => {
      fireKey('o');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('offset');
    });

    it('E switches to erase', () => {
      fireKey('e');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('erase');
    });

    it('tool switch blocked when tool is busy', () => {
      (deps.toolManager.isToolBusy as any).mockReturnValue(true);
      fireKey('r');
      expect(deps.toolManager.setTool).not.toHaveBeenCalled();
    });

    it('updates tool label on switch', () => {
      fireKey('l');
      const label = document.getElementById('tool-label')!;
      expect(label.textContent).toBe('Line');
    });
  });

  describe('Shift combos', () => {
    it('Shift+L switches to polyline', () => {
      fireKey('L', { shiftKey: true });
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('polyline');
    });

    it('Shift+F switches to freehand', () => {
      fireKey('F', { shiftKey: true });
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('freehand');
    });

    it('Shift+F does NOT also jump to Front view', () => {
      // The test above passed all along, because it only looked at half of what
      // happened: the view listener is on `window` too and guarded only
      // `!ctrlKey && !altKey`, so Shift+F picked freehand AND moved the camera
      // to Front. One keystroke, two unrelated things, and the status bar named
      // the view while the tool was what changed.
      fireKey('F', { shiftKey: true });
      expect(deps.viewport.setViewMode).not.toHaveBeenCalled();
    });

    it('Shift+K is still Back view — the help sheet promises it', () => {
      // The fix must not swing the other way: Shift+K is not in shiftMap, so it
      // never reaches the tool branch and stays a view key ('F / Shift+K').
      fireKey('K', { shiftKey: true });
      expect(deps.viewport.setViewMode).toHaveBeenCalledWith('back');
    });
  });

  describe('Ctrl combos', () => {
    it('Ctrl+S calls saveProject', () => {
      fireKey('s', { ctrlKey: true });
      expect(deps.saveProject).toHaveBeenCalled();
    });

    it('Ctrl+O calls openProject', () => {
      fireKey('o', { ctrlKey: true });
      expect(deps.openProject).toHaveBeenCalled();
    });

    it('Ctrl+G calls group action', () => {
      fireKey('g', { ctrlKey: true });
      expect(deps.toolManager.executeAction).toHaveBeenCalledWith('group');
    });

    it('Ctrl+Shift+G calls ungroup action', () => {
      fireKey('G', { ctrlKey: true, shiftKey: true });
      expect(deps.toolManager.executeAction).toHaveBeenCalledWith('ungroup');
    });

    it('Ctrl+A calls select-all', () => {
      fireKey('a', { ctrlKey: true });
      expect(deps.toolManager.executeAction).toHaveBeenCalledWith('select-all');
    });
  });

  describe('special keys', () => {
    it('Delete calls delete action', () => {
      fireKey('Delete');
      expect(deps.toolManager.executeAction).toHaveBeenCalledWith('delete');
    });

    it('F3 toggles snap', () => {
      fireKey('F3');
      expect(deps.toolManager.snap.toggle).toHaveBeenCalled();
    });

    it('Spacebar cancels busy tool and switches to select', () => {
      (deps.toolManager.isToolBusy as any).mockReturnValue(true);
      (deps.toolManager as any)._currentTool = 'line';
      Object.defineProperty(deps.toolManager, 'currentTool', {
        get() { return this._currentTool; },
        configurable: true,
      });
      fireKey(' ');
      expect(deps.toolManager.cancelCurrentTool).toHaveBeenCalled();
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('select');
    });

    it('Spacebar switches idle tool to select (SketchUp style)', () => {
      (deps.toolManager.isToolBusy as any).mockReturnValue(false);
      (deps.toolManager as any)._currentTool = 'line';
      Object.defineProperty(deps.toolManager, 'currentTool', {
        get() { return this._currentTool; },
        configurable: true,
      });
      fireKey(' ');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('select');
    });

    it('Spacebar is a no-op when already in select tool', () => {
      (deps.toolManager.isToolBusy as any).mockReturnValue(false);
      (deps.toolManager as any)._currentTool = 'select';
      Object.defineProperty(deps.toolManager, 'currentTool', {
        get() { return this._currentTool; },
        configurable: true,
      });
      fireKey(' ');
      expect(deps.toolManager.setTool).not.toHaveBeenCalled();
    });

    // (2026-07-03 — 'h' 카메라 홈 단축키 제거됨: F5 / 🏠 와 중복. 카메라 홈은
    //  F5 + 🏠, 평면 초기화는 Home.)
    it('F5 resets camera AND the drawing plane', () => {
      fireKey('F5');
      expect(deps.viewport.resetCamera).toHaveBeenCalled();
      expect(deps.toolManager.resetDrawingPlane).toHaveBeenCalled();
    });

    // ADR-270 §F amendment 2 — Home 키 = 평면 초기화 (Ctrl+Shift+P 는 Command
    // Palette 로 이전). 평면이 pin 된 상태에서만 reset 호출.
    it('Home key resets the drawing plane when a plane is pinned', () => {
      (deps.toolManager.hasPinnedPlane as ReturnType<typeof vi.fn>).mockReturnValue(true);
      fireKey('Home');
      expect(deps.toolManager.resetDrawingPlane).toHaveBeenCalled();
    });
  });

  describe('axis lock', () => {
    it('ArrowRight locks X axis', () => {
      fireKey('ArrowRight');
      expect(deps.toolManager.setAxisLock).toHaveBeenCalledWith('x');
    });

    it('ArrowUp locks Y axis', () => {
      fireKey('ArrowUp');
      expect(deps.toolManager.setAxisLock).toHaveBeenCalledWith('y');
    });

    it('ArrowLeft locks Z axis', () => {
      fireKey('ArrowLeft');
      expect(deps.toolManager.setAxisLock).toHaveBeenCalledWith('z');
    });

    it('ArrowDown clears axis lock', () => {
      fireKey('ArrowDown');
      expect(deps.toolManager.setAxisLock).toHaveBeenCalledWith(null);
    });

    it.each(['input', 'textarea', 'select'])(
      'an arrow key typed in a <%s> does not touch the axis lock',
      (tag) => {
        // This listener guards with isTypingInInput. Its twin in ToolManager
        // had no guard at all and called preventDefault(), so ArrowLeft in the
        // VCB locked Z and swallowed the caret — verified in the live app
        // before the fix.
        const field = document.createElement(tag);
        document.body.appendChild(field);
        field.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
        expect(deps.toolManager.setAxisLock).not.toHaveBeenCalled();
        field.remove();
      },
    );

    it('a view key typed in a <textarea> does not move the camera', () => {
      // The view listener guarded `instanceof HTMLInputElement` while the main
      // listener in the same file used the full check — the two disagreed about
      // what "typing" means, and t/b/f/k are bare letters.
      const ta = document.createElement('textarea');
      document.body.appendChild(ta);
      ta.dispatchEvent(new KeyboardEvent('keydown', { key: 't', bubbles: true }));
      expect(deps.viewport.setViewMode).not.toHaveBeenCalled();
      ta.remove();
    });
  });

  describe('Escape key', () => {
    it('exits group edit mode if active', () => {
      (deps.toolManager.selection.isInGroupEditMode as any).mockReturnValue(true);
      fireKey('Escape');
      expect(deps.toolManager.selection.exitGroupEdit).toHaveBeenCalled();
    });

    it('switches to select tool in 3D mode', () => {
      fireKey('Escape');
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('select');
    });
  });

  describe('view shortcuts', () => {
    it('T switches to top view', () => {
      fireKey('t');
      expect(deps.viewport.setViewMode).toHaveBeenCalledWith('top');
    });

    it('F switches to front view', () => {
      fireKey('f');
      expect(deps.viewport.setViewMode).toHaveBeenCalledWith('front');
    });

    it('B switches to bottom view', () => {
      fireKey('b');
      expect(deps.viewport.setViewMode).toHaveBeenCalledWith('bottom');
    });
  });

  describe('input field bypass', () => {
    it('ignores shortcuts when input is focused', () => {
      const input = document.createElement('input');
      document.body.appendChild(input);
      input.focus();
      // Simulate keydown with target being the input
      const event = new KeyboardEvent('keydown', { key: 'l', bubbles: true });
      Object.defineProperty(event, 'target', { value: input });
      window.dispatchEvent(event);
      expect(deps.toolManager.setTool).not.toHaveBeenCalled();
    });
  });

  describe('home button', () => {
    it('clicking home button resets camera', () => {
      document.getElementById('home-btn')!.click();
      expect(deps.viewport.resetCamera).toHaveBeenCalled();
    });

    // ADR-270 §F amendment 3 — 🏠 도 드로잉 평면을 기본(z=0)으로 복귀.
    it('clicking home button also resets the drawing plane', () => {
      document.getElementById('home-btn')!.click();
      expect(deps.toolManager.resetDrawingPlane).toHaveBeenCalled();
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-074 §E.5-4 — Boolean Group A/B 단축키 (Alt+A / Alt+B / Alt+0).
  // ContextMenu (U-2) 의 단축 진입점. 우클릭 우회로 파워유저 효율 향상.
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-074 §E.5-4 Boolean Group shortcuts', () => {
    it('Alt+A calls selection.setGroupTag with selected faces and "A"', () => {
      // selection.getSelectedFaces returns [10, 20, 30] (mockDeps default).
      fireKey('a', { altKey: true });
      expect((deps.toolManager.selection as any).setGroupTag)
        .toHaveBeenCalledWith([10, 20, 30], 'A');
    });

    it('Alt+B calls selection.setGroupTag with selected faces and "B"', () => {
      fireKey('b', { altKey: true });
      expect((deps.toolManager.selection as any).setGroupTag)
        .toHaveBeenCalledWith([10, 20, 30], 'B');
    });

    it('Alt+0 calls selection.clearGroupTags', () => {
      fireKey('0', { altKey: true });
      expect((deps.toolManager.selection as any).clearGroupTags)
        .toHaveBeenCalled();
    });

    it('Alt+A is no-op when selection is empty', () => {
      (deps.toolManager.selection.getSelectedFaces as any)
        .mockReturnValue([]);
      fireKey('a', { altKey: true });
      expect((deps.toolManager.selection as any).setGroupTag)
        .not.toHaveBeenCalled();
    });

    it('plain "a" (no Alt) does not trigger setGroupTag', () => {
      // Ensures Alt+A is not swallowed by a plain-key handler.
      // Conflict guard: Ctrl+A is select-all (separate); plain `a` has
      // no group binding.
      fireKey('a');
      expect((deps.toolManager.selection as any).setGroupTag)
        .not.toHaveBeenCalled();
    });
  });
});
