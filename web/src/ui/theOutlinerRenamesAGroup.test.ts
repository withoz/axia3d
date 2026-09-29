/**
 * The Outliner renames a group.
 *
 * `ComponentPanel`'s own header has listed 이름 편집 among its features since
 * the file was written, and until 2026-09-29 nothing in it did that. The rest
 * of the chain was complete and unused:
 *
 * ```text
 *   Command::RenameGroup      axia-core
 *   rename_group              WASM export
 *   renameGroup               WasmBridge wrapper — zero callers in the app
 *   the panel                 the only missing hop
 * ```
 *
 * ⚠ It was invisible to every wiring guard, and correctly so: no `data-action`
 * string pointed at it, so link A had nothing to check, and the bridge wrapper
 * calls `rename_group`, which IS exported, so link D was satisfied. A wrapper
 * nobody calls breaks no link. It was found by walking the map backwards —
 * bridge methods with no caller — which nothing in the repo does.
 *
 * ⚠ The first backwards scan reported 47 such methods and was WRONG: it matched
 * `name(` but not `name?.()`, so live calls like `exportSnapshotSilent?.()`
 * read as dead. The same blind spot that had cost link D 81 names. Corrected,
 * it is 37, of which 32 are never mentioned at all — mostly `demo_boolean_*`.
 *
 * Row dblclick could not carry the rename: it already means "enter group edit".
 * So this uses the `data-action` button pattern the row already has for delete
 * and place.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ComponentPanel } from './ComponentPanel';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

const GROUP = {
  id: 3,
  name: '벽체',
  faceCount: 4,
  faceIds: [1, 2, 3, 4],
  parent: null,
  children: [],
  visible: true,
  locked: false,
  isComponent: false,
};

function harness() {
  const bridge = {
    getAllGroups: vi.fn().mockReturnValue([GROUP]),
    getGroupInfo: vi.fn().mockReturnValue(GROUP),
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
    selectGroup: vi.fn(),
    ungroupSelected: vi.fn(),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
  const container = document.createElement('div');
  document.body.appendChild(container);
  const panel = new ComponentPanel(container, bridge, selection, {});
  panel.show();
  return { bridge, container, panel };
}

function renameButton(container: HTMLElement): HTMLElement | null {
  return container.querySelector('[data-action="rename"]');
}

describe('the Outliner', () => {
  let prompt: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    document.body.innerHTML = '';
    prompt = vi.spyOn(window, 'prompt');
  });
  afterEach(() => prompt.mockRestore());

  it('offers a rename on the row', () => {
    const { container } = harness();
    // PREMISE: the row rendered at all — without it every assertion below is
    // vacuously about an empty tree.
    expect(container.querySelector('.cp-name')?.textContent, 'the group row').toBe('벽체');
    expect(renameButton(container), 'and a rename affordance on it').not.toBeNull();
  });

  it('asks with the current name and renames with what it is given', () => {
    const { bridge, container } = harness();
    prompt.mockReturnValue('기둥');
    renameButton(container)!.click();
    expect(prompt, 'it opens with the name already there').toHaveBeenCalledWith(
      expect.anything(),
      '벽체',
    );
    expect(bridge.renameGroup, 'and renames THAT group').toHaveBeenCalledWith(3, '기둥');
  });

  it('does nothing on cancel, on blank, or on the same name', () => {
    const { bridge, container } = harness();
    for (const answer of [null, '   ', '벽체']) {
      bridge.renameGroup.mockClear();
      prompt.mockReturnValue(answer as string);
      renameButton(container)!.click();
      expect(bridge.renameGroup, `answer ${JSON.stringify(answer)} must not rename`).not
        .toHaveBeenCalled();
    }
  });

  it('trims what it is given', () => {
    const { bridge, container } = harness();
    prompt.mockReturnValue('  기둥  ');
    renameButton(container)!.click();
    expect(bridge.renameGroup).toHaveBeenCalledWith(3, '기둥');
  });

  it('and the row click still selects rather than renaming', () => {
    const { bridge, container } = harness();
    prompt.mockReturnValue('기둥');
    (container.querySelector('.cp-name') as HTMLElement).click();
    expect(bridge.renameGroup, 'clicking the name is selection, not rename').not
      .toHaveBeenCalled();
  });
});
