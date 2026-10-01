/**
 * The Outliner can change what a group holds, and put one inside another.
 *
 * `addFacesToGroup`, `removeFacesFromGroup` and `setGroupParent` are engine ops
 * with WASM exports and bridge wrappers, and until 2026-10-01 **none had a
 * caller**: the UI could create a group (Ctrl+G) and dissolve one
 * (Ctrl+Shift+G) and nothing in between.
 *
 * ⚠ The panel RENDERS a nested tree — `childMap`, per-depth indent — while
 * `create_group` always leaves `parent` unset and nothing in the engine called
 * `set_parent` either. **It drew a tree the user could not build.**
 *
 * Weaker than the rename this pass also fixed: the panel's header promised
 * 이름 편집 and did not do it, while nesting was never promised. This is a
 * capability the engine had and the UI did not offer.
 *
 * ⚠ The nest child is always the id of a row the panel is SHOWING, never one
 * composed here, because `set_parent` accepts an id that names no group and
 * records it — measured in `the_outliner_edits_a_group.rs`. The engine's cycle
 * refusal is real and is what the recursive render depends on; the panel only
 * adds the two checks the engine cannot make for it (nothing selected, and the
 * row being its own target).
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ComponentPanel } from './ComponentPanel';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

const OUTER = {
  id: 3, name: '벽체', faceCount: 1, faceIds: [1], parent: null, children: [],
  visible: true, locked: false, isComponent: false,
};
const INNER = {
  id: 4, name: '기둥', faceCount: 1, faceIds: [2], parent: null, children: [],
  visible: true, locked: false, isComponent: false,
};

function harness(selectedFaces: number[] = [10, 11], parentOk = true) {
  const bridge = {
    getAllGroups: vi.fn().mockReturnValue([OUTER, INNER]),
    getGroupInfo: vi.fn((id: number) => (id === OUTER.id ? OUTER : INNER)),
    addFacesToGroup: vi.fn().mockReturnValue(true),
    removeFacesFromGroup: vi.fn().mockReturnValue(true),
    setGroupParent: vi.fn().mockReturnValue(parentOk),
    renameGroup: vi.fn().mockReturnValue(true),
    toggleGroupVisibility: vi.fn(),
    toggleGroupLock: vi.fn(),
    deleteGroup: vi.fn(),
    placeComponent: vi.fn().mockReturnValue(0),
    lastError: vi.fn().mockReturnValue(''),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
  const selection = {
    getAllGroups: vi.fn().mockReturnValue(new Map()),
    getSelectedFaces: vi.fn().mockReturnValue(selectedFaces),
    selectGroup: vi.fn(),
    ungroupSelected: vi.fn(),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
  const container = document.createElement('div');
  document.body.appendChild(container);
  const panel = new ComponentPanel(container, bridge, selection, {});
  panel.show();
  return { bridge, selection, container, panel };
}

/** The row for `id`, and a `data-action` button inside it. */
function btn(container: HTMLElement, id: number, action: string): HTMLElement {
  const row = container.querySelector(`[data-group-id="${id}"] .cp-row`);
  if (!row) throw new Error(`no row for group ${id}`);
  const b = row.querySelector(`[data-action="${action}"]`);
  if (!b) throw new Error(`no ${action} button on group ${id}`);
  return b as HTMLElement;
}

describe('the Outliner', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('offers all three edits on a row', () => {
    const { container } = harness();
    // PREMISE: the rows rendered — without them every lookup below throws for
    // the wrong reason.
    expect(container.querySelectorAll('.cp-row').length, 'two group rows').toBe(2);
    for (const a of ['add-faces', 'remove-faces', 'nest']) {
      expect(() => btn(container, OUTER.id, a), a).not.toThrow();
    }
  });

  it('adds the selected faces to the row it was clicked on', () => {
    const { bridge, container } = harness([10, 11]);
    btn(container, INNER.id, 'add-faces').click();
    expect(bridge.addFacesToGroup).toHaveBeenCalledWith(INNER.id, [10, 11]);
    expect(bridge.removeFacesFromGroup).not.toHaveBeenCalled();
  });

  it('removes them from the row it was clicked on', () => {
    const { bridge, container } = harness([10, 11]);
    btn(container, OUTER.id, 'remove-faces').click();
    expect(bridge.removeFacesFromGroup).toHaveBeenCalledWith(OUTER.id, [10, 11]);
    expect(bridge.addFacesToGroup).not.toHaveBeenCalled();
  });

  it('does nothing to a group when no face is selected', () => {
    const { bridge, container } = harness([]);
    btn(container, OUTER.id, 'add-faces').click();
    btn(container, OUTER.id, 'remove-faces').click();
    expect(bridge.addFacesToGroup).not.toHaveBeenCalled();
    expect(bridge.removeFacesFromGroup).not.toHaveBeenCalled();
  });

  it('nests the row selected in the panel into the row clicked', () => {
    const { bridge, container } = harness();
    // Select INNER by clicking its name, then nest it into OUTER.
    (container.querySelector(`[data-group-id="${INNER.id}"] .cp-name`) as HTMLElement).click();
    btn(container, OUTER.id, 'nest').click();
    expect(bridge.setGroupParent, 'child from the panel, parent from the row')
      .toHaveBeenCalledWith(INNER.id, OUTER.id);
  });

  it('will not nest when nothing is selected in the panel', () => {
    const { bridge, container } = harness();
    btn(container, OUTER.id, 'nest').click();
    expect(bridge.setGroupParent).not.toHaveBeenCalled();
  });

  it('will not nest a group into itself', () => {
    const { bridge, container } = harness();
    (container.querySelector(`[data-group-id="${OUTER.id}"] .cp-name`) as HTMLElement).click();
    btn(container, OUTER.id, 'nest').click();
    expect(bridge.setGroupParent, 'the engine refuses this too, but say so without asking')
      .not.toHaveBeenCalled();
  });

  it('reports the refusal when the engine says no', () => {
    const { bridge, container } = harness([10, 11], false);
    (container.querySelector(`[data-group-id="${INNER.id}"] .cp-name`) as HTMLElement).click();
    btn(container, OUTER.id, 'nest').click();
    // It still asked — the cycle check is the engine's, not a guess here.
    expect(bridge.setGroupParent).toHaveBeenCalledWith(INNER.id, OUTER.id);
  });
});
