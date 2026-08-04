/**
 * A `tool-*` menu item must activate that tool, if the tool exists.
 *
 * Three of them did not. 꼭짓점 챔퍼 ran the EDGE chamfer, so asking for a vertex
 * chamfer answered "챔퍼할 엣지 1개를 먼저 선택하세요" and, given an edge, did the
 * same thing as the row underneath it — leaving ChamferTool, the only route to
 * the valence-3 vertex cut, reachable from the command palette alone. 미러 도구
 * and 필렛 도구 had the same shape: labels promising a repeatable tool, bodies
 * firing a one-shot action.
 *
 * ADR-220 L-220-3 had already ruled on this, dropping the stale
 * `legacy: ['tool-mirror'/'tool-fillet'/'tool-chamfer']` aliases because these
 * are current separate tools. That correction landed in the catalog and not in
 * MenuBar, so this guard exists to keep both halves together.
 *
 * The existing guards cannot do it: ActionWiring measures reachability by id
 * (`!domActions.has('tool-' + id)`), so `tool-chamfer` counted as a surface for
 * `chamfer` no matter what its case body did, and the MenuBar test only asserts
 * a `case` exists.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');
const MENUBAR = read('src/ui/MenuBar.ts');
const TOOLMGR = read('src/tools/ToolManagerRefactored.ts');
const HTML = read('index.html');

const registeredTools = () =>
  new Set([...TOOLMGR.matchAll(/tools\.set\('([a-z0-9-]+)'/g)].map((m) => m[1]));

const toolCases = () =>
  [...MENUBAR.matchAll(/case 'tool-([a-z0-9-]+)':([^\n]*)/g)].map((m) => ({
    tool: m[1],
    body: m[2].trim(),
  }));

/**
 * `tool-group` is the one case that legitimately runs an action while a tool of
 * that name exists. Its label is 그룹 (Group) with a Ctrl+G hint, Ctrl+G is bound
 * to the `group` action, and GroupTool's own header says the menu route is meant
 * to create the group ("면 선택 후 G키 또는 메뉴 → 그룹 생성") while the tool
 * handles click and double-click. The label and the behaviour agree, so the id
 * is merely misnamed. Anything else appearing here needs the same defence.
 */
const ACTION_BY_DESIGN = new Set(['group']);

describe('tool-* menu items activate their tool', () => {
  it('the extractors find both sides (a guard over nothing passes for free)', () => {
    expect(registeredTools().size, 'no registered tools found').toBeGreaterThan(20);
    expect(toolCases().length, 'no tool-* cases found in MenuBar').toBeGreaterThan(20);
  });

  it('every tool-* case whose tool exists calls setActiveTool for it', () => {
    const registered = registeredTools();
    const wrong = toolCases()
      .filter(({ tool }) => registered.has(tool) && !ACTION_BY_DESIGN.has(tool))
      .filter(({ tool, body }) => !body.includes(`setActiveTool('${tool}')`))
      .map(({ tool, body }) => `tool-${tool} → ${body}`);
    expect(
      wrong,
      `these menu items name a tool and run something else: ${wrong.join('; ')}. ` +
        'If the item really is meant to be a one-shot, say so in ACTION_BY_DESIGN ' +
        'with the label that justifies it.',
    ).toEqual([]);
  });

  it('the three that regressed are pinned by name', () => {
    // The rule above is general, but these three are the ones with history and
    // an ADR behind them, so they get named. Their labels all say 도구/Vertex.
    for (const t of ['mirror', 'fillet', 'chamfer']) {
      expect(MENUBAR, `tool-${t} stopped activating its tool`)
        .toContain(`case 'tool-${t}': setActiveTool('${t}'); break;`);
    }
  });

  it('the one-shot rows they used to run are still in the menu', () => {
    // Rerouting must not have cost anyone the immediate operations — ADR-046
    // P31 #4 is additive-only, and these are the rows the old bodies fired.
    for (const a of ['mirror-x', 'mirror-y', 'mirror-z', 'fillet-edge', 'chamfer-edge']) {
      expect(HTML, `the ${a} row disappeared`).toContain(`data-action="${a}"`);
      expect(MENUBAR, `${a} lost its dispatch`).toContain(`'${a}'`);
    }
  });

  it('ACTION_BY_DESIGN only shelters cases that really are exceptions', () => {
    // An entry here silences the main check, so it must correspond to a real
    // case that really does run an action — otherwise the exemption list can
    // rot into a place where fixes go to be forgotten.
    const registered = registeredTools();
    for (const tool of ACTION_BY_DESIGN) {
      const c = toolCases().find((x) => x.tool === tool);
      expect(c, `ACTION_BY_DESIGN lists ${tool}, which has no tool-* case`).toBeTruthy();
      expect(registered.has(tool), `ACTION_BY_DESIGN lists ${tool}, which is not a tool`).toBe(true);
      expect(
        c!.body.includes('executeAction'),
        `ACTION_BY_DESIGN lists ${tool}, but its case no longer runs an action — drop the exemption`,
      ).toBe(true);
    }
  });
});

/**
 * A command's label has to name what it does. The palette called three entries
 * "축 스냅" / "그리드 스냅" / "엣지 스냅"; they dispatch to the status bar's
 * F4/F6/F7 buttons, which toggle the viewport's grid, edges and axes. Measured
 * live: each flipped its display flag true→false while `snap.enabled` stayed
 * true. Real axis/grid snap modes DO exist in SnapManager, so the labels named
 * something the engine has and these commands do not touch — the worst kind of
 * wrong name.
 *
 * (SettingsPanel's own "그리드 스냅" checkbox is honest — it sets
 * `units.gridSnap` — so the phrase itself is fine, just not for these three.)
 */
describe('a command labelled X does X', () => {
  const CMDS = read('src/commands/AxiaCommands.ts');
  const STATUS = read('src/ui/StatusBar.ts');

  it('the display toggles are not called snap', () => {
    for (const id of ['axis', 'grid', 'edge']) {
      const line = CMDS.split('\n').find((l) => l.includes(`action('${id}',`));
      expect(line, `palette entry '${id}' not found`).toBeTruthy();
      expect(
        /스냅/.test(line!),
        `'${id}' is labelled a snap but dispatches to the ${id} DISPLAY toggle`,
      ).toBe(false);
    }
  });

  it('…because that is what the handler behind them does', () => {
    // If a future change makes these actually toggle snapping, this fails and
    // the labels should go back to saying "스냅".
    for (const [id, call] of [
      ['axis', 'setAxisVisible'],
      ['grid', 'setGridVisible'],
      ['edge', 'setEdgeStyle'],
    ]) {
      const at = STATUS.indexOf(`case '${id}': {`);
      expect(at, `StatusBar case '${id}' not found`).toBeGreaterThan(-1);
      const arm = STATUS.slice(at, at + 400);
      expect(arm).toContain(call);
      expect(arm, `'${id}' now touches snap — relabel it`).not.toContain('snap.toggle');
    }
  });

  it('and the one that really is a snap toggle still says so', () => {
    const at = STATUS.indexOf("case 'osnap': {");
    expect(at).toBeGreaterThan(-1);
    expect(STATUS.slice(at, at + 300)).toContain('snap.toggle');
    expect(CMDS.split('\n').find((l) => l.includes("action('osnap',"))).toMatch(/스냅/);
  });
});
