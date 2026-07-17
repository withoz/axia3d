/**
 * AXiA 3D — Main Entry Point
 *
 * Initializes WASM engine, Three.js viewport, and tool manager.
 * Phase 1 Refactor: Uses ServiceContainer for dependency injection instead of window.__axia_* globals.
 */

import { Viewport } from './viewport/Viewport';
import { SectionPlane } from './viewport/SectionPlane';
import { ScenesManager } from './ui/ScenesManager';
import { ToolManager } from './tools/ToolManagerRefactored';
import { WasmBridge } from './bridge/WasmBridge';
import { UnitSystem } from './units/UnitSystem';
import { SettingsPanel } from './units/SettingsPanel';
import { translateDom } from './i18n/translateDom';
// FileImporter is now lazy-loaded via MenuBar (dynamic import on first use)
import { ComponentPanel } from './ui/ComponentPanel';
import { ConstraintPanel } from './ui/ConstraintPanel';
import { NurbsPatchPanel } from './ui/NurbsPatchPanel';
import { toolDisplayName } from './ui/toolDisplayNames';
import { HistoryPanel } from './ui/HistoryPanel';
import { CapabilityExplorerPanel } from './ui/CapabilityExplorerPanel';
import { InvariantVerifierPanel } from './ui/InvariantVerifierPanel';
import { AuditLogViewerPanel } from './ui/AuditLogViewerPanel';
import { getAuditLog } from './core/AuditLog';
import { AnalyticHoverOverlay } from './core/AnalyticHoverOverlay';
import { ConstraintVisual } from './ui/ConstraintVisual';
import { DimensionManager } from './ui/DimensionManager';
import { FileManager } from './file/FileManager';
import { MaterialLibrary } from './materials/MaterialLibrary';
import { DraggablePanelManager } from './ui/DraggablePanelManager';
import { CommandInput } from './ui/CommandInput';
import { ServiceContainer } from './core/ServiceContainer';
import { initCommandRegistry } from './ui/CommandRegistry';
import { initOsnapPanel } from './ui/OsnapPanel';
import { initStylePanel } from './ui/StylePanel';
import { initProjectSerializer } from './ui/ProjectSerializer';
import { initMenuBar } from './ui/MenuBar';
import { initVCB } from './ui/VCB';
import { initKeyboardShortcuts } from './ui/KeyboardShortcuts';
import { StatusBar } from './ui/StatusBar';
import { initContextMenu } from './ui/ContextMenu';
import { loadInitialScene } from './ui/InitialScene';
import { initXiaInspector } from './ui/XiaInspector';
import { debugLog } from './utils/debug';
import { isTypingInInput } from './utils/isTypingInInput';
import { Toast } from './ui/Toast';
import { getConsolePanel } from './ui/ConsolePanel';
import { makeFloatingDraggable } from './ui/makeFloatingDraggable';
import './ui/DraggablePanels.css';
import { t } from './i18n';
import { takeStashedScene } from './i18n/localeSwitchScene';

// Install in-UI console panel as early as possible so any errors during
// app boot are captured and visible to the user without DevTools.
// (ADR-045 D5 — first cut of Debug Panel surface.)
getConsolePanel().install();

// Make the console chip draggable (grab the pill, reposition, persist).
// The pill's own click (expand/collapse) still works — only a real drag
// suppresses the trailing click. See makeFloatingDraggable.
{
  const consoleRoot = document.getElementById('axia-console-panel');
  const consolePill = consoleRoot?.querySelector('button') as HTMLElement | null;
  if (consoleRoot && consolePill) {
    makeFloatingDraggable(consoleRoot, {
      handle: consolePill,
      storageKey: 'axia:console-pos',
    });
  }
}

/**
 * Detect whether the WASM binary on the server is newer than the one the
 * previous page load cached. If so, show a non-intrusive Toast so the
 * developer (or the user after a deploy) knows a hard refresh will pull
 * in the latest engine. Implementation uses a HEAD request so we don't
 * download the full binary just to check its Last-Modified.
 */
async function checkWasmFreshness(): Promise<void> {
  try {
    const res = await fetch('/src/wasm/axia_wasm_bg.wasm', {
      method: 'HEAD',
      cache: 'no-store',
    });
    if (!res.ok) return;
    const lastMod = res.headers.get('last-modified');
    if (!lastMod) return;
    const storageKey = 'axia:wasm-mtime';
    const stored = localStorage.getItem(storageKey);
    if (stored && stored !== lastMod) {
      debugLog(`[WASM] Binary updated: ${stored} → ${lastMod}`);
      Toast.info(
        t('AXiA 엔진이 업데이트됐습니다. 최신 기능이 적용됩니다.'),
        4000,
      );
    }
    localStorage.setItem(storageKey, lastMod);
  } catch (e) {
    debugLog('[WASM] freshness check skipped:', e);
  }
}

/**
 * Route an action id through the MenuBar: many ids (panels, imports, view
 * modes) are implemented as `#menubar [data-action]` items, not as branches of
 * `ToolManager.dispatchAction`, so the only way to run them is to click the
 * (possibly hidden) menu item. Scoped to `#menubar` so bare ids that live only
 * on the toolbar / context menu (group / ungroup / make-component) fall through
 * to executeAction. Returns false when no menubar item exists.
 *
 * ADR-069 — module scope so BOTH surfaces use it. It used to be a local inside
 * the Command Palette's registration, which is why the Capability Explorer sent
 * every id straight to executeAction: measured, 72 of the catalog's 136
 * `action()` ids have no executeAction branch, so from the Explorer they did
 * nothing at all — and were recorded as successes.
 */
/**
 * Context-menu ids the palette may fire.
 *
 * Not the whole context menu. Adding `#context-menu` to the query below would
 * re-route 13 ids that currently reach executeAction (merge-faces,
 * constrain-*, flip-faces …) through ContextMenu's switch instead — a routing
 * change nobody asked for. These three are here because measuring them showed
 * they read `selection.getSelectedFaces()`, not the right-click position, and
 * a selection survives opening the palette.
 *
 * snap-override stays out: it is a `ctx-submenu-trigger` whose own handler is
 * `case 'snap-override': return; // hover로 처리, 클릭 무시`. Firing it from
 * the palette would be a silent no-op, which is worse than not offering it.
 */
const CONTEXT_SELECTION_ACTIONS = new Set(['group-edit', 'group-hide', 'group-lock']);

const dispatchMenuAction = (id: string): boolean => {
  // #statusbar too: osnap / grid / edge / axis / help / rename are F-key
  // buttons down there, handled by StatusBar.ts — they exist in neither
  // #menubar nor executeAction, so from the palette they fell through both
  // hops and produced "unknown command". Measured in the wiring audit.
  const item =
    document.querySelector<HTMLElement>(
      `#menubar [data-action="${id}"], #statusbar [data-action="${id}"]`,
    ) ??
    (CONTEXT_SELECTION_ACTIONS.has(id)
      ? document.querySelector<HTMLElement>(`#context-menu [data-action="${id}"]`)
      : null);
  if (!item) return false;
  item.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  return true;
};

async function main() {
  debugLog('AXiA 3D starting...');

  // 0a. Translate index.html's static chrome (ADR-294 D8). FIRST, before any
  //     panel is constructed, so its scope is exactly the static markup —
  //     panels build their own DOM from TS and re-render, so a boot-time sweep
  //     would only paint over them until their first repaint. They get wrapped
  //     with t() in their own batch instead. A no-op in Korean.
  const domI18n = translateDom(document.body);
  if (domI18n.texts || domI18n.attrs || domI18n.untranslated.length) {
    debugLog(
      `[i18n] chrome: ${domI18n.texts} texts + ${domI18n.attrs} attrs translated` +
      (domI18n.untranslated.length ? `, ${domI18n.untranslated.length} untranslated` : ''),
    );
  }

  // 0. Initialize the Toast singleton FIRST — every Toast.info/warning/error/
  //    success is `Toast.getInstance()?.show(...)`, so without this init the
  //    instance stays null and ALL toasts silently no-op (the container is
  //    never even created). Direct testing (2026-07-06) found the app never
  //    called Toast.init, so user-facing feedback — including the ADR-275
  //    Boolean no-op warning and every error/success toast — never rendered.
  Toast.init(document.body);

  // 0b. WASM freshness check (non-blocking, just logs + Toast if newer).
  //    Runs alongside engine init so no wall-clock impact.
  checkWasmFreshness();

  // 1. Initialize WASM engine
  const bridge = new WasmBridge();
  await bridge.init();

  // Phase 2 — auto-intersect-on-draw 설정 WASM 에 반영 (초기 + 변경 시)
  const { getAutoIntersect, onAutoIntersectChange } = await import('./tools/AutoIntersectSettings');
  if (bridge.isReady()) bridge.setAutoIntersectOnDraw(getAutoIntersect());
  onAutoIntersectChange((v) => {
    if (bridge.isReady()) bridge.setAutoIntersectOnDraw(v);
  });

  // ADR-139 B-β-2 — auto-face-synthesis-on-draw 설정 WASM 에 반영
  // (LOCKED #12 ADR-025 P11 Step 4.99 자동 cycle face synthesis 토글).
  // Default `false` (메타-원칙 #16 자동화 antipattern 폐기).
  const { getAutoFaceSynthesis, onAutoFaceSynthesisChange } =
    await import('./tools/AutoFaceSynthesisSettings');
  if (bridge.isReady()) bridge.setAutoFaceSynthesisOnDraw(getAutoFaceSynthesis());
  onAutoFaceSynthesisChange((v) => {
    if (bridge.isReady()) bridge.setAutoFaceSynthesisOnDraw(v);
  });

  // ADR-186 (A) — face-rederive-on-draw 설정 WASM 에 반영 (derived-face 모델).
  // production default ON: UI 마우스 draw 도 analytic rederive (containment
  // annulus + smooth 곡선 hole) 경로 사용. engine default 는 OFF (회귀 보존).
  const { getFaceRederive, onFaceRederiveChange } = await import('./tools/FaceRederiveSettings');
  if (bridge.isReady()) bridge.setFaceRederiveOnDraw(getFaceRederive());
  onFaceRederiveChange((v) => {
    if (bridge.isReady()) bridge.setFaceRederiveOnDraw(v);
  });

  // ADR-186 A3/B6-2b — freeform-overlap-on-draw 설정 WASM 에 반영.
  // 겹치는 freeform self-loop (Bezier/BSpline/NURBS) → smooth lens 자동
  // split (circle/rect overlap 답습). face_rederive 의 하위 branch (둘 다
  // ON 이어야 효과). production default ON, engine default OFF (회귀 보존).
  const { getFreeformOverlap, onFreeformOverlapChange } =
    await import('./tools/FreeformOverlapSettings');
  if (bridge.isReady()) bridge.setFreeformOverlapOnDraw(getFreeformOverlap());
  onFreeformOverlapChange((v) => {
    if (bridge.isReady()) bridge.setFreeformOverlapOnDraw(v);
  });

  // ADR-094 B-θ post-retrospective default ON — Cylinder Path B 활성
  // (산업 CAD parity, 3 face / 2 edge / 2 vert, ~95% 메모리 절감).
  // ADR-049 P-5e-α 답습 — engine default OFF + production default ON
  // via localStorage. explicit OFF preference 보존.
  const { getCylinderPathBMode, onCylinderPathBModeChange } =
    await import('./tools/CylinderPathBSettings');
  if (bridge.isReady()) bridge.setCylinderPathBDefault(getCylinderPathBMode());
  onCylinderPathBModeChange((on) => {
    if (bridge.isReady()) bridge.setCylinderPathBDefault(on);
  });

  // ADR-104 β-1-ζ default ON — Sphere Path B 활성 (산업 CAD parity,
  // 2 hemisphere face / 1 equator edge / 1 vert canonical, 99%+ 메모리
  // 절감 vs 289-face polygonal mesh). Cylinder Path B 패턴 1:1 mirror.
  const { getSpherePathBMode, onSpherePathBModeChange } =
    await import('./tools/SpherePathBSettings');
  if (bridge.isReady()) bridge.setSpherePathBDefault(getSpherePathBMode());
  onSpherePathBModeChange((on) => {
    if (bridge.isReady()) bridge.setSpherePathBDefault(on);
  });

  // ADR-104 β-2-ζ default ON — Cone Path B 활성 (산업 CAD parity,
  // 2 face / 1 edge / 1 vert canonical, ~92% 메모리 절감 vs 25-face
  // polygonal cone). Sphere Path B 패턴 1:1 mirror (ADR-113 답습).
  const { getConePathBMode, onConePathBModeChange } =
    await import('./tools/ConePathBSettings');
  if (bridge.isReady()) bridge.setConePathBDefault(getConePathBMode());
  onConePathBModeChange((on) => {
    if (bridge.isReady()) bridge.setConePathBDefault(on);
  });

  // ADR-104 β-3-ζ default ON — Torus Path B 활성 (산업 CAD parity,
  // 1 face / 1 edge / 1 vert canonical, ~99.7% 메모리 절감 vs 289-face
  // hypothetical Path A baseline). ADR-104 Path B family closure
  // (cylinder + sphere + cone + torus). Sphere/cone 패턴 1:1 mirror.
  const { getTorusPathBMode, onTorusPathBModeChange } =
    await import('./tools/TorusPathBSettings');
  if (bridge.isReady()) bridge.setTorusPathBDefault(getTorusPathBMode());
  onTorusPathBModeChange((on) => {
    if (bridge.isReady()) bridge.setTorusPathBDefault(on);
  });

  // Note: WASM is optional for basic Three.js rendering (e.g., Sphere tool)
  // Continue even if WASM fails to initialize
  if (!bridge.isReady()) {
    console.warn('⚠ WASM engine not ready - continuing with basic Three.js mode');
  } else {
    debugLog('WASM engine ready');
  }

  // ADR-118 γ-7 (γ-4 component) — STEP/IGES engine pre-warm 활성 (사용자
  // 결재 2026-05-17). Background OCCT init 으로 사용자 Import 클릭 시
  // wait 시간 perceived 0s. Default ON, localStorage `'false'` 명시 opt-
  // out. requestIdleCallback (5s timeout fallback setTimeout 2s) 으로
  // initial render 영향 0.
  //
  // ADR-082 Drift #5 (180s+ wait) 의 perceived 본질 해소 — pre-warm
  // 완료 후 사용자 Import 클릭 시 즉시 응답. ADR-085 Toast progress 도
  // background init 도중 자동 표시 (사용자 인지 가능).
  //
  // Cross-link: ADR-118 (architectural spec), ADR-082 Drift #5,
  // ADR-085 (perception 보존), LOCKED #43 priority #3.
  import('./import/StepIgesPrewarm').then(({ prewarmStepIgesEngine }) => {
    prewarmStepIgesEngine();
  }).catch((e) => {
    debugLog('[main] StepIgesPrewarm import failed (graceful):', e);
  });

  // 2. Initialize viewport (always required)
  const viewportEl = document.getElementById('viewport');
  if (!viewportEl) throw new Error('Missing #viewport element');
  const viewport = new Viewport(viewportEl);

  // 3. Initialize unit system & settings
  const units = new UnitSystem();
  // bridge: switching the language reloads (ADR-294 D7) and the scene lives in
  // memory only, so the panel asks before discarding a drawing.
  const settingsPanel = new SettingsPanel(units, { bridge });

  // Settings button
  const settingsBtn = document.getElementById('settings-btn');
  if (settingsBtn) {
    settingsBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      settingsPanel.toggle();
    });
  }

  // 3b. Initialize file manager (FileImporter is lazy-loaded on first import)
  const fileManager = new FileManager(bridge);

  // 3d. Initialize material library and link to file manager
  const materialLibrary = new MaterialLibrary();
  fileManager.setMaterialLibrary(materialLibrary);

  // 3e. Initialize draggable panel manager
  const panelManager = new DraggablePanelManager();
  panelManager.registerAllPanels([
    'xia-inspector',
    'style-panel',
    'osnap-panel',
  ]);

  // 4. Initialize service container (Phase 1: Dependency Injection)
  const container = new ServiceContainer();

  // Register core services
  container.register('bridge', bridge);
  container.register('viewport', viewport);
  container.register('units', units);
  container.register('panelManager', panelManager);
  container.register('fileManager', fileManager);
  container.register('materialLibrary', materialLibrary);

  // 파일명 상태바 업데이트 함수
  const updateFileStatus = (fileName: string) => {
    const statFileEl = document.getElementById('stat-file');
    if (statFileEl) {
      statFileEl.textContent = fileName;
    }
  };

  // FileManager 파일명 변경 콜백 등록
  fileManager.onFileChange(() => updateFileStatus(fileManager.getCurrentFileName()));

  // 초기 파일명 표시
  updateFileStatus(fileManager.getCurrentFileName());

  // 단위 변경 시 그리드 간격 업데이트
  const updateGridForUnit = () => {
    // 내부 단위는 항상 mm, 그리드는 단위에 맞게 조정
    // mm: 1mm / 5mm, cm: 10mm / 50mm, m: 1000mm / 5000mm
    // in: 25.4mm / 127mm, ft: 304.8mm / 1524mm
    // 건축 스케일: 내부 단위 mm
    const gridMap: Record<string, [number, number]> = {
      mm: [1000, 5000],      // 1m / 5m 간격
      cm: [1000, 5000],      // 1m / 5m 간격
      m:  [1000, 5000],      // 1m / 5m 간격
      in: [25.4 * 12, 25.4 * 60],  // 1ft / 5ft 간격
      ft: [304.8, 304.8 * 5],      // 1ft / 5ft 간격
    };
    const [small, big] = gridMap[units.unit] || [1, 5];
    viewport.updateGridSpacing(small, big);
  };
  units.onChange(updateGridForUnit);
  updateGridForUnit();

  // Initialize tool manager (connects bridge ↔ viewport ↔ units)
  const toolManager = new ToolManager(viewport, bridge, units);
  container.register('toolManager', toolManager);
  // Expose SelectionManager via ServiceContainer — string-literal key 'selection'
  // is mangling-safe (vs production-minified field access via tm.selection).
  // External callers (E2E tests / UI panels) can reliably reach the singleton.
  container.register('selection', toolManager.selection);
  // String-literal 'syncMesh' function registration — bypass minified
  // method-name access in production build. External callers can reliably
  // trigger viewport update via `container.get('syncMesh')()`.
  container.register('syncMesh', () => toolManager.syncMesh());

  // Initialize command input (CAD-style commands)
  const commandInput = new CommandInput();
  container.register('commandInput', commandInput);

  // ADR-082 C-ε — OCCT loader (bundled function, Vite static analysis 활용).
  //   `loadOcct()` 호출 시 Vite 가 build-time 에 분석한 opencascade-deps
  //   lazy chunk 가 fetch + execute 됨. Playwright E2E 도 본 entry 를
  //   통해 chunk 접근 (browser context 의 bare specifier resolve 우회).
  container.register('loadOcct', () => import('opencascade.js'));

  // ADR-083 T-δ — StepIgesImporter loader for E2E testing.
  //   Vite 가 StepIgesImporter chunk 를 hash-named 로 빌드하므로 Playwright
  //   page.evaluate 에서 direct path import 불가. Container entry 를 통해
  //   StepIgesImporter module 접근 (loadOcct 패턴 답습).
  container.register(
    'loadStepIgesImporter',
    () => import('./import/StepIgesImporter'),
  );

  // ADR-098 S-ε — Asset Library Panel registration.
  //   Lazy-imports panel + Settings flag. Host (e.g. menu / status bar)
  //   can call window.__axia.get('assetLibraryPanel')() to toggle.
  //   Panel itself renders all 3 tiers; the Settings flag governs
  //   whether the User tier section is interactive (future host-side
  //   filtering — MVP shows all sections).
  const assetLibraryPanel = async () => {
    const [{ AssetLibraryPanel }, { getAssetLibraryUserTierMode }] =
      await Promise.all([
        import('./ui/AssetLibraryPanel'),
        import('./tools/AssetLibraryUserTierSettings'),
      ]);
    if (!bridge.isReady()) return null;
    // Single-instance: cache on container key after first creation.
    const existing = container.tryGet<{ panel: unknown; userTierEnabled: boolean }>(
      '__assetLibraryPanelInstance' as never,
    );
    if (existing) return existing;
    // Mount inside the standard right-side panel container if present,
    // otherwise body fallback (test surface).
    const host = document.getElementById('right-panel-container') ?? document.body;
    // ADR-099 L-ζ — Wire layered channel callbacks to bridge.
    //   Panel stays bridge-agnostic; main.ts injects via callbacks
    //   (ADR-091 §E L4 UI orchestration 분리 패턴).
    const panel = new AssetLibraryPanel(host, bridge, {
      hasLayeredMaterial: (id) => bridge.hasLayeredMaterial(id),
      onLayeredChannelUpload: (id, channel, info) =>
        bridge.setLayeredChannel(id, channel, info),
    });
    const instance = {
      panel,
      get userTierEnabled() { return getAssetLibraryUserTierMode(); },
    };
    return instance;
  };
  container.register('assetLibraryPanel', assetLibraryPanel);

  // ADR-100 R-ε — Material recovery service (Phase 5-C SSOT entry).
  //   Lazy-imports orchestrator + Settings flag. ADR-097 T-ε 답습 —
  //   1:1 mirror pattern (different localStorage key + orchestrator
  //   module). Call from material-removal sites or window.__axia
  //   for E2E.
  const materialRecovery = async () => {
    const [{ attemptMaterialRecoveryWithDialog }, { getAutoMaterialRecoveryMode }] =
      await Promise.all([
        import('./citizenship/MaterialRemovalRecoveryOrchestrator'),
        import('./tools/AutoMaterialRecoverySettings'),
      ]);
    if (!getAutoMaterialRecoveryMode()) return { skipped: true } as const;
    if (!bridge.isReady()) return { skipped: true } as const;
    return attemptMaterialRecoveryWithDialog(bridge);
  };
  container.register('materialRecovery', materialRecovery);

  // ADR-097 T-ε — Topology recovery service (Phase 4 SSOT entry).
  //   Lazy-imports orchestrator + Settings flag; checks flag inside
  //   the closure so listeners stay reactive to live setSetting updates.
  //   Call from any op-completion site (or from window.__axia for E2E).
  const topologyRecovery = async () => {
    const [{ attemptRecoveryWithDialog }, { getAutoTopologyRecoveryMode }] =
      await Promise.all([
        import('./citizenship/TopologyRecoveryOrchestrator'),
        import('./tools/AutoTopologyRecoverySettings'),
      ]);
    if (!getAutoTopologyRecoveryMode()) return { skipped: true } as const;
    if (!bridge.isReady()) return { skipped: true } as const;
    return attemptRecoveryWithDialog(bridge);
  };
  container.register('topologyRecovery', topologyRecovery);

  // Export single container to window (replaces all window.__axia_* globals)
  (window as any).__axia = container;

  // ADR-012 telemetry — install BEFORE any draw/sync work happens.
  //   Lookups are guarded by `?.` so cost is ~0 when window props missing
  //   (tests, headless), and bound minimal closures otherwise.
  void import('./core/telemetry').then(({ installTelemetryGlobal }) => {
    installTelemetryGlobal();
  });
  // ADR-013 §1·§2 memory budget — installs window.__AXIA_MEMORY getter.
  // Other modules (SnapManager, BVH, History) can register samplers via
  // memoryBudget.registerSampler(area, () => byteCount) at any point.
  void import('./core/memory').then(async ({ installMemoryGlobal, memoryBudget }) => {
    installMemoryGlobal();
    // ADR-013 §3 — eviction policy. Register handlers per area.
    const { evictionPolicy, installEvictionGlobal } = await import('./core/eviction');
    installEvictionGlobal();
    // Telemetry buffer evict — clears violation/frame history.
    evictionPolicy.register('telemetry', 4, () => {
      const t = (window as any).__AXIA_TELEMETRY_RESET as (() => void) | undefined;
      if (!t) return 0;
      // Estimate bytes freed: ~50 bytes/violation × cap 1000 = 50KB max.
      t();
      return 50_000;
    });
    // History evict — drop oldest entries from OperationLog.
    evictionPolicy.register('history', 3, () => {
      const log = (container.tryGet?.('operationLog') as { clear?: () => void; getAll?: () => unknown[] } | undefined);
      if (!log?.clear) return 0;
      const before = (log.getAll?.() ?? []).length;
      log.clear();
      return before * 200;  // ~200 bytes/entry
    });
    // Three.js geometry size sampler.
    memoryBudget.registerSampler('geometry', () => {
      const vp = container.tryGet?.('viewport') as { meshGroup?: { traverse?: (cb: (o: any) => void) => void } } | undefined;
      let bytes = 0;
      vp?.meshGroup?.traverse?.((obj: any) => {
        const geo = obj.geometry;
        if (!geo || !geo.attributes) return;
        for (const attr of Object.values(geo.attributes) as Array<{ array?: { byteLength?: number } }>) {
          bytes += attr.array?.byteLength ?? 0;
        }
        if (geo.index?.array?.byteLength) bytes += geo.index.array.byteLength;
      });
      return bytes;
    });
    // History (OperationLog) size sampler.
    memoryBudget.registerSampler('history', () => {
      try {
        const log = (container.tryGet?.('operationLog') as { getAll?: () => unknown[] } | undefined);
        const arr = log?.getAll?.() ?? [];
        // Approximate: 200 bytes/entry (id, kind, name, params, ts, inputs, outputs).
        return arr.length * 200;
      } catch { return 0; }
    });
  });
  debugLog('[Main] ServiceContainer initialized with services:', container.keys());

  // Register commands (line, help, backtick toggle)
  initCommandRegistry({ commandInput, bridge, toolManager });

  // ═══ 4-0. 초기 씬 로드 — see ui/InitialScene.ts ═══
  loadInitialScene({ bridge, fileManager, toolManager, updateFileStatus });

  // ADR-294 D7 — put back the drawing the language switch parked.
  //
  // D7 reloads because the catalogs are init-once; the scene is memory-only,
  // so the reload used to take the drawing with it. SettingsPanel stashes a
  // snapshot on the way out and this restores it, after loadInitialScene so
  // the empty-scene sync does not overwrite it.
  //
  // The undo stack does not come back — a snapshot is the scene, not the
  // transaction history. Say so, rather than let it be found with Ctrl+Z.
  const stashedScene = takeStashedScene();
  if (stashedScene && bridge.importSnapshotSilent(stashedScene)) {
    toolManager.syncMesh();
    Toast.info(t('언어를 바꿨습니다 — 작업은 그대로입니다 (실행취소 기록은 초기화)'), 4000);
  }

  // ═══ Selection status bar update ═══
  // Phase H 이후 status bar는 coords + F-keys에 집중.
  // "Selected: N" 정보는 XIA Inspector에서 이미 확인 가능하므로 status bar에
  // 반영하지 않음 (legacy stat-sel-wrap 은 제거됨 — 빈 onChange stub 도 삭제).

  // ═══ ADR-232/237 — NURBS control-net overlay + inline edit panel ═══
  // A single selected NURBS-class face (BezierPatch=6 / BSplineSurface=7 /
  // NURBSSurface=8) → show its control net (CP markers + net lines, ADR-232)
  // AND the inline CP editor panel (ADR-237); any other selection clears both.
  const nurbsPatchPanel = new NurbsPatchPanel(viewportEl, bridge, {
    syncMesh: () => toolManager.syncMesh(),
    selectFaces: (ids) => toolManager.selection.selectFaces(ids),
    updateOverlay: (params) => viewport.updateNurbsControlNet(params),
  });
  toolManager.selection.onChange((faces) => {
    if (faces.length === 1) {
      const kind = bridge.faceSurfaceKind(faces[0]);
      if (kind === 6 || kind === 7 || kind === 8) {
        viewport.updateNurbsControlNet(bridge.getNurbsSurfaceParams(faces[0]));
        nurbsPatchPanel.showFor(faces[0]); // ADR-237
        return;
      }
    }
    viewport.updateNurbsControlNet(null);
    nurbsPatchPanel.hide(); // ADR-237
  });

  // ═══ ADR-164 β-3 — Viewport.setViewMode → ToolManager reset hook ═══
  // L-164-2 — view mode change is a clear signal of user intent shift
  // away from the previous drawing context. Calls
  // `toolManager.notifyViewModeChange()` which resets `_lastDrawnPlane`
  // (β-1 API). Sticky plane re-acquires on next Draw tool face commit
  // (β-2).
  viewport.onViewModeChange(() => {
    toolManager.notifyViewModeChange();
  });

  // 5a. OSNAP toggle — legacy 체크박스 + 새 StatusBar F3 버튼 동기화.
  // (숨김 legacy #stat-osnap indicator 는 제거됨 — F3 버튼이 단독 표시.)
  const osnapToggle = document.getElementById('osnap-toggle');

  const updateOsnapUI = () => {
    const on = toolManager.snap.enabled;
    statusBar.setToggle('sb-fkey-osnap', on);
  };

  if (osnapToggle) {
    osnapToggle.addEventListener('click', () => {
      toolManager.snap.toggle();
      updateOsnapUI();
    });
  }

  // ═══ 새 상태바: 좌표 추적 + F1~F7 아이콘 바 + 커맨드바 우측 유틸 ═══
  const statusBar = new StatusBar({
    viewport,
    units,
    snap: toolManager.snap,
    openSettings: () => settingsPanel.toggle(),
  });
  statusBar.syncFromViewport();

  // ═══ OSNAP 설정 패널 (제도 설정값) — MenuBar/ContextMenu보다 먼저 초기화 ═══
  const osnapAPI = initOsnapPanel({
    snap: toolManager.snap,
    snapVisual: toolManager.snapVisual,
    updateOsnapUI,
  });
  const { openOsnapPanel } = osnapAPI;

  // ═══ Project Save/Load (.xia) — MenuBar/KeyboardShortcuts보다 먼저 초기화 ═══
  const { saveProject, openProject } = initProjectSerializer({ bridge, viewport, toolManager, units });

  // ═══ Command Catalog — single source of truth for command metadata.
  //   NOT a new dispatcher — each entry's `execute` callback delegates
  //   into the existing ToolManager / MenuBar paths. Adding a new
  //   command still happens in ToolManagerRefactored.executeAction or
  //   MenuBar; the catalog just gathers the metadata so toolbar / menu
  //   / keyboard / palette can all consult one list.
  void import('./commands/AxiaCommands').then(({ registerAxiaCommands }) => {
    registerAxiaCommands({ toolManager, dispatchMenuAction });
  });
  // Command Palette — Ctrl+K / Ctrl+Shift+P opens a searchable list of every
  //   registered command (single visible surface for the catalog).
  void import('./ui/CommandPalette').then(({ bindCommandPaletteHotkey }) => {
    bindCommandPaletteHotkey();
  });

  // ═══ 4a. CAD Menu Bar — see ui/MenuBar.ts ═══
  initMenuBar({ viewport, bridge, toolManager, scene: viewport.scene, fileManager, saveProject, openProject, openOsnapPanel });

  // 4b. Wire toolbar buttons
  const toolbar = document.getElementById('toolbar');
  if (!toolbar) throw new Error('Missing #toolbar element');

  // 툴바 data-action 디스패치 헬퍼 — 대부분 executeAction 으로 가지만
  // bool-union/subtract/intersect는 BooleanHandler로 라우팅 필요 (메뉴와 동일).
  // 이 분기를 한 곳에 모아 버튼/드롭다운 양쪽에서 공통 사용.
  const dispatchToolbarAction = (action: string) => {
    // tool-explode is a SketchUp-parity alias for ungroup (MenuBar aliases it
    // the same way at its switch). Without this the toolbar group-dropdown's
    // "분해 (Explode)" item is a silent no-op (executeAction has no such case).
    if (action === 'tool-explode') action = 'ungroup';
    if (action === 'bool-union' || action === 'bool-subtract' || action === 'bool-intersect') {
      const op = action.replace('bool-', '') as 'union' | 'subtract' | 'intersect';
      void import('./ui/BooleanHandler').then(({ startBooleanOp }) => {
        startBooleanOp({ bridge, toolManager }, op);
      });
      return;
    }
    toolManager.executeAction(action);
  };

  // 툴바 밖 클릭 시 열린 dropdown 모두 닫기
  document.addEventListener('click', (e) => {
    if (!(e.target as HTMLElement).closest('.tool-dropdown')) {
      toolbar.querySelectorAll('.tool-dropdown.open').forEach(d => d.classList.remove('open'));
    }
  });

  // ═══ Dropdown 트리거 + 선택 핸들러 ═══
  toolbar.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

    // 드롭다운 trigger (▼ 버튼)
    const trigger = target.closest('.tool-dropdown-trigger') as HTMLElement;
    if (trigger) {
      e.stopPropagation();
      const dropdown = trigger.closest('.tool-dropdown') as HTMLElement;
      if (dropdown) {
        // 다른 열린 드롭다운 닫기
        toolbar.querySelectorAll('.tool-dropdown.open').forEach(d => {
          if (d !== dropdown) d.classList.remove('open');
        });
        dropdown.classList.toggle('open');
      }
      return;
    }

    // 드롭다운 패널 안의 항목 선택
    const item = target.closest('.tool-dropdown-item') as HTMLElement;
    if (item) {
      const dropdown = item.closest('.tool-dropdown') as HTMLElement;

      // Action dropdown item (data-action) — dispatch action + close panel,
      // do NOT change active tool or swap the main button's icon.
      const itemAction = item.dataset.action;
      if (itemAction) {
        dropdown?.classList.remove('open');
        dispatchToolbarAction(itemAction);
        item.classList.add('flash');
        item.addEventListener('animationend', () => item.classList.remove('flash'), { once: true });
        return;
      }

      const tool = item.dataset.tool;
      if (tool && dropdown) {
        // 그룹 내 active 갱신
        dropdown.querySelectorAll('.tool-dropdown-item').forEach(i => i.classList.remove('active'));
        item.classList.add('active');
        // 대표 버튼의 data-tool 갱신 (다음 클릭이 이 도구를 선택하도록)
        const mainBtn = dropdown.querySelector('.tool-btn') as HTMLElement | null;
        if (mainBtn) {
          mainBtn.dataset.tool = tool;
          // 아이콘 교체 — 항목의 아이콘 SVG 복제
          const srcIcon = item.querySelector('.tdi-icon svg');
          if (srcIcon && mainBtn) {
            mainBtn.innerHTML = srcIcon.outerHTML;
          }
          mainBtn.title = (item.querySelector('.tdi-label')?.textContent ?? '') +
            ((item.querySelector('.tdi-key')?.textContent ?? '').length > 0
              ? ' (' + item.querySelector('.tdi-key')?.textContent + ')'
              : '');
        }
        dropdown.classList.remove('open');
        // 도구 활성화
        toolbar.querySelectorAll('.tool-btn').forEach(b => b.classList.remove('active'));
        if (mainBtn) mainBtn.classList.add('active');
        toolManager.setTool(tool);
        return;
      }
    }

    const btn = target.closest('.tool-btn') as HTMLElement;
    if (!btn) return;

    // Action button (data-action on the main tool-btn) — execute without
    // altering tool selection state. Used by Mirror / Revolve / Subdivide.
    const btnAction = btn.dataset.action;
    if (btnAction) {
      dispatchToolbarAction(btnAction);
      btn.classList.add('flash');
      btn.addEventListener('animationend', () => btn.classList.remove('flash'), { once: true });
      return;
    }

    const tool = btn.dataset.tool;
    if (!tool) return;

    if (tool === 'undo' || tool === 'redo') {
      toolManager.executeAction(tool);
      // 클릭 플래시 효과
      btn.classList.add('flash');
      btn.addEventListener('animationend', () => btn.classList.remove('flash'), { once: true });
      return;
    }

    toolbar.querySelectorAll('.tool-btn').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');
    toolManager.setTool(tool);

    // Update tool label (status-bar command indicator) — shared SSOT names.
    const toolLabel = document.getElementById('tool-label');
    if (toolLabel) {
      toolLabel.textContent = toolDisplayName(tool);
    }
  });

  // 5+6. Keyboard Shortcuts + View Mode — see ui/KeyboardShortcuts.ts
  const viewModeBar = document.getElementById('view-mode-bar');

  // 7. Start render loop
  viewport.start();

  // ─── ADR-135 β — Distance-based LOD chord_tol wiring ───
  //
  // Computes lod_chord_tol(camera_distance) on each frame; pushes via
  // bridge.setRenderChordTol() only when the value changes by more than
  // 5% from previously pushed (avoids per-frame full rebuild thrashing).
  //
  // Near rendering (camera ≤ 100mm): 0.02mm (LOCKED #40 baseline preserved).
  // Far rendering: auto-coarser (0.2mm at 1m, 1.0mm at 5m+) → 10-50× triangle
  // reduction for large primitives (sphere r=1000 at 5m: 2M → 40K tris).
  //
  // Cross-link: ADR-135 §5 Path A, LOCKED #40 §L1 baseline preserved.
  let lodLastPushedTol = 0.02; // baseline (matches engine default)
  // **Demo #2 fix (audit-first canonical 15번째 — PR #146 audit Path A,
  // 2026-05-24)** — RefCell aliasing guard. wasm-bindgen 의 RefCell guard
  // 가 다른 WASM call panic 후 영구 잠김 시 매 frame "recursive use" error
  // 발생 (사용자 demo screenshot #2 evidence: 60fps × 3.3초 = 200 errors).
  // 본 try-catch 가 frame loop 보호 — LOD 일시 비활성으로 graceful
  // degradation (frame loop blocking 보다 안전).
  let lodWarnedOnce = false;
  // **ADR-135 amendment (2026-06-17)** — LOD geometry refresh. `setRenderChordTol`
  // updates the engine's chord_tol + marks the WASM cache dirty, but does NOT
  // invalidate the bridge's TS-side buffer cache, so the visible Three.js
  // geometry never re-tessellates as the camera zooms (a sphere created at the
  // far default camera stays faceted even when zoomed in). Fix: after the LOD
  // chord_tol changes, refresh the geometry (`bridge.markDirty()` +
  // `toolManager.syncMesh()`). DEBOUNCED — re-tessellation is expensive
  // (ADR-111/112), so it fires once after the camera SETTLES, not on every 5%
  // step during a continuous zoom (avoids mid-zoom jank).
  let lodRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  const LOD_REFRESH_DEBOUNCE_MS = 160;
  viewport.onFrame(() => {
    try {
      const camPos = viewport.camera.position;
      // Use orbitTarget proxy via camera's distance to scene origin —
      // approximation since we don't have public orbitTarget accessor.
      // For LOD purposes this is good enough (sketch/primitives are near
      // origin in typical scenes).
      const camDistance = camPos.length();
      if (!Number.isFinite(camDistance) || camDistance <= 0) return;
      const lodTol = bridge.lodChordTol(camDistance);
      // Only push when change is > 5% (avoids per-frame churn on slow zoom).
      if (Math.abs(lodTol / lodLastPushedTol - 1) > 0.05) {
        lodLastPushedTol = lodTol;
        // ADR-286 hardening (dev-preview HeId-panic race) — push
        // `setRenderChordTol` INSIDE the debounced callback, together with the
        // geometry refresh, so cache-invalidation (setRenderChordTol marks the
        // render cache dirty, LOCKED #62 L-135-5) and re-tessellation happen
        // ATOMICALLY in one task. Previously setRenderChordTol fired immediately
        // on every 5% step during a continuous zoom → many cache invalidations,
        // each leaving the cache invalid across a frame boundary until the
        // debounced syncMesh landed 160ms later; a WASM export pull in that
        // window ran on the invalidated cache → `HeId not found` panic →
        // poisoned RefCell → "recursive use" cascade (2026-05-24 audit §2).
        // The visible geometry already only reflected the tol at the (debounced)
        // syncMesh, so folding setRenderChordTol into the same task is a
        // zero-visible-behavior change that closes the stale-cache window.
        if (lodRefreshTimer !== null) clearTimeout(lodRefreshTimer);
        lodRefreshTimer = setTimeout(() => {
          lodRefreshTimer = null;
          try {
            bridge.setRenderChordTol(lodTol);
            bridge.markDirty();
            toolManager.syncMesh();
          } catch {
            // graceful — same RefCell-aliasing guard rationale as the frame loop.
          }
        }, LOD_REFRESH_DEBOUNCE_MS);
      }
    } catch (e) {
      // RefCell 영구 잠김 또는 다른 WASM panic — silent skip per frame.
      // 첫 번째만 console.warn 으로 audit trail (이후 frame 마다 silent).
      // Cross-link: docs/audits/2026-05-24-demo-2-refcell-aliasing-audit.md
      if (!lodWarnedOnce) {
        lodWarnedOnce = true;
        console.warn(
          '[ADR-135 LOD] frame loop WASM call failed (likely RefCell aliasing — see Demo #2 audit). ' +
          'LOD silently disabled. First error:',
          e,
        );
      }
    }
  });

  // 8. Status bar updates — unit/precision are shown on the visible commandbar
  // cb-unit button (StatusBar.updateUnitButton, subscribed to units.onChange).
  // The former hidden #stat-unit/#stat-prec writes were removed (dead churn).

  const undoBtn = toolbar.querySelector('[data-tool="undo"]');
  const redoBtn = toolbar.querySelector('[data-tool="redo"]');

  const statsIntervalId = setInterval(() => {
    const stats = bridge.getStats();
    // (Former hidden #stat-verts/#stat-faces/#stat-tool writes removed — dead
    // churn; verts/faces show in the XIA Inspector, tool in #tool-label.)
    // Undo/Redo 버튼 활성/비활성 (canUndo/canRedo가 없으면 항상 활성)
    if (undoBtn) undoBtn.classList.toggle('disabled', stats.canUndo === false);
    if (redoBtn) redoBtn.classList.toggle('disabled', stats.canRedo === false);
  }, 200);

  // Cleanup on page unload
  window.addEventListener('beforeunload', () => {
    clearInterval(statsIntervalId);
    viewport.stop();
    viewport.dispose();
  });

  // 9. VCB (Value Control Box) — see ui/VCB.ts
  initVCB({ toolManager, units });

  // ═══ Context Menu — see ui/ContextMenu.ts ═══
  initContextMenu({ viewport, bridge, toolManager, viewModeBar, openOsnapPanel });

  // Keyboard Shortcuts (depends on saveProject/openProject)
  initKeyboardShortcuts({
    toolManager, viewport, toolbar, viewModeBar, saveProject, openProject,
    // Same handlers the File menu runs (MenuBar 'file-saveas' / 'file-new'),
    // so the key and the menu item cannot drift apart.
    saveAsProject: () => fileManager.saveAsProject(),
    newProject: () => {
      if (confirm(t('현재 작업을 초기화하시겠습니까?'))) location.reload();
    },
  });

  // ═══ 11. Style Side Panel — see ui/StylePanel.ts ═══
  initStylePanel({
    viewport,
    bridge,
    syncMesh: () => toolManager.syncMesh(),
  });

  // ═══ 12. XIA Inspector Panel — see ui/XiaInspector.ts ═══
  await initXiaInspector({ bridge, viewport, toolManager });

  // ═══ 13. Component Panel (그룹/컴포넌트 아웃라이너) ═══
  {
    const componentPanel = new ComponentPanel(
      viewportEl,
      bridge,
      toolManager.selection,
      {
        onGroupSelect: (groupId) => {
          toolManager.selection.selectGroup(groupId);
          debugLog(`[ComponentPanel] Group-${groupId} selected`);
        },
        onGroupDoubleClick: (groupId) => {
          toolManager.selection.enterGroupEdit(groupId);
          debugLog(`[ComponentPanel] Group-${groupId} edit mode`);
        },
        onGroupDelete: (groupId) => {
          debugLog(`[ComponentPanel] Group-${groupId} deleted`);
        },
        onRefresh: () => {
          // 선택된 면으로 그룹 생성
          toolManager.executeAction('group');
          componentPanel.refresh();
        },
      },
    );

    // 키보드 O → Component Panel 토글
    window.addEventListener('keydown', (e) => {
      if (isTypingInInput(e.target)) return;
      if (e.key === 'o' || e.key === 'O') {
        if (!e.ctrlKey && !e.altKey && !e.shiftKey) {
          componentPanel.toggle();
        }
      }
    });

    // Selection 변경 시 패널 갱신
    toolManager.selection.onChange(() => {
      componentPanel.refresh();
    });
  }

  // ═══ 14. Constraint Panel (파라메트릭 제약 목록) ═══
  {
    const constraintPanel = new ConstraintPanel(
      viewportEl,
      bridge,
      {
        syncMesh: () => toolManager.syncMesh(),
      },
    );
    // 전역 노출 — ToolManager가 제약 변경 후 refresh 호출하도록 함
    (window as unknown as { __axia_constraintPanel?: ConstraintPanel })
      .__axia_constraintPanel = constraintPanel;

    // 키보드 J → Constraint Panel 토글 ('K'는 Inference Lock에서 사용 중)
    window.addEventListener('keydown', (e) => {
      if (isTypingInInput(e.target)) return;
      if ((e.key === 'j' || e.key === 'J') && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        constraintPanel.toggle();
      }
    });
  }

  // ═══ 15. History Panel (Tier 3B — Parametric History MVP) ═══
  {
    const historyPanel = new HistoryPanel(viewportEl, {
      rerun: (kind, params) => toolManager.rerunLoggedOperation(kind, params),
    });
    (window as unknown as { __axia_historyPanel?: HistoryPanel })
      .__axia_historyPanel = historyPanel;

    // 키보드 Shift+H → History Panel 토글
    window.addEventListener('keydown', (e) => {
      if (isTypingInInput(e.target)) return;
      if ((e.key === 'h' || e.key === 'H') && e.shiftKey && !e.ctrlKey && !e.altKey) {
        historyPanel.toggle();
      }
    });
  }

  // ═══ 15b. Capability Explorer Panel (ADR-063 Phase 1 Path Z) ═══
  // §D #1 lock-in: 단일 ActionCatalog 사용 사이트.
  // Step 2 scaffold + Step 3 tree/search + Step 4 invocation form.
  {
    const capabilityExplorerPanel = new CapabilityExplorerPanel(viewportEl, {
      // ADR-063 Step 4 — Action invocation dispatcher.
      // Tier 0 read 는 직접 WASM 호출, Tier 1/2 launcher 는 ToolManager
      // executeAction 경유. 알 수 없는 액션은 명시 거부.
      onActionInvoke: async (actionId, args) => {
        // ADR-069 Step 2 — audit capture wrap. Tier 정보 catalog 에서 조회.
        const allActions = CapabilityExplorerPanel.getAllActions();
        const def = allActions.find((a) => a.id === actionId);
        const tier = (def?.tier ?? 0) as 0 | 1 | 2 | 3;
        const audit = getAuditLog();
        try {
          // 1. Tier 0 read + Phase O Step 6 / P-narrow / Path Z direct dispatch.
          const eng = bridge.engine as unknown as Record<string, (...a: unknown[]) => unknown> | null;
          if (eng) {
            const directDispatch: Record<string, () => unknown> = {
              'edge-curve-info':       () => eng.getEdgeCurveJson?.(Number(args.edgeId)),
              'face-surface-info':     () => eng.getFaceSurfaceJson?.(Number(args.faceId)),
              'face-normals-cached':   () => {
                const arr = eng.getFaceNormalsCached?.(Number(args.faceId)) as Float64Array | undefined;
                return arr ? `Float64Array(len=${arr.length}): [${Array.from(arr.slice(0, 12)).join(', ')}${arr.length > 12 ? ', ...' : ''}]` : null;
              },
              'edge-polyline-cached':  () => {
                const arr = eng.getEdgePolylineCached?.(Number(args.edgeId), Number(args.chordTol ?? 0)) as Float64Array | undefined;
                return arr ? `Float64Array(len=${arr.length})` : null;
              },
              'cache-stats':           () => eng.getCacheStats?.(),
              'migrate-curve-surface': () => eng.migrateCurveSurfaceMandatory?.(),
              'fillet-dispatch':       () => eng.filletEdgeDispatchJson?.(Number(args.edgeId), Number(args.radius), Number(args.segments)),
            };
            const direct = directDispatch[actionId];
            if (direct) {
              const result = direct();
              const text = typeof result === 'string' ? result : JSON.stringify(result, null, 2);
              audit.record({ actionId, tier, result: 'ok', args });
              return { ok: true, result: text ?? '(empty)' };
            }
          }
          // 2. Tier 1/2 launcher.
          //
          // ADR-069 — this used to go straight to a void executeAction and
          // record 'ok' unconditionally, on the strength of a comment claiming
          // "unknown actions surface via Toast (ToolManager internal warning)".
          // They did not: the dispatcher's if/else chain simply ended, so an
          // unknown action did nothing, said nothing, and was written to the
          // audit trail as a success. An audit that invents successes is worse
          // than no audit — it IS the intrusion signal (ADR-041 P26.7).
          //
          // Two fixes, in order:
          //  - Menu-backed ids first, exactly as the Command Palette does.
          //    Measured: 72 of the catalog's 136 `action()` ids (file-open,
          //    export-obj, help, view modes…) have no executeAction branch —
          //    they live as `#menubar [data-action]` items. From the Explorer
          //    they therefore did NOTHING, and were logged as successes. Now
          //    they actually run.
          //  - Then executeAction, which reports whether it had a branch at
          //    all. That boolean is not a general success flag (see its doc) —
          //    it is exactly the case that was being fabricated.
          if (dispatchMenuAction(actionId)) {
            audit.record({ actionId, tier, result: 'ok', args });
            return { ok: true, result: 'Launched (menu dispatch).' };
          }
          if (!toolManager.executeAction(actionId)) {
            const err = t('알 수 없는 명령입니다: {actionId}', { actionId });
            audit.record({ actionId, tier, result: 'error', error: err, args });
            return { ok: false, error: err };
          }
          audit.record({ actionId, tier, result: 'ok', args });
          return { ok: true, result: 'Launched (existing tool dispatch).' };
        } catch (e) {
          const errMsg = e instanceof Error ? e.message : String(e);
          audit.record({ actionId, tier, result: 'error', error: errMsg, args });
          return { ok: false, error: errMsg };
        }
      },
    });
    (window as unknown as { __axia_capabilityExplorer?: CapabilityExplorerPanel })
      .__axia_capabilityExplorer = capabilityExplorerPanel;
    // 단축키 보류 (D-C=(b) 메뉴만). Step 5 종합에서 단축키 결정.
  }

  // ═══ 15c. Invariant Verifier Panel (ADR-068 Phase 1 Path Y B pilot) ═══
  // §D #1 lock-in: WASM verifyInvariants 재사용 (ADR-007), 백엔드 신규 0.
  // §D #2 lock-in: Path Z scope — A/C/D sub-features 별도 ADR.
  {
    const invariantVerifierPanel = new InvariantVerifierPanel(viewportEl, {
      runVerify: () => bridge.verifyInvariants(),
      // 자기교차(self-intersection) 검사 — 위상 검사가 못 잡는 flap/poke-through.
      runSelfIntersect: () => bridge.detectSelfIntersections(),
      jumpToFace: (faceId: number) => {
        // ADR-068 §D #4 lock-in: jump = SelectionManager.selectFaces only.
        // Camera 이동은 Phase 2 enhancement.
        toolManager.selection.clearSelection?.();
        toolManager.selection.selectFaces([faceId]);
      },
      jumpToFaces: (faceIds: number[]) => {
        toolManager.selection.clearSelection?.();
        toolManager.selection.selectFaces(faceIds);
      },
    });
    (window as unknown as { __axia_invariantVerifier?: InvariantVerifierPanel })
      .__axia_invariantVerifier = invariantVerifierPanel;
  }

  // ═══ 15d. Audit Log Viewer Panel (ADR-069 Phase 1 Path Y A pilot) ═══
  // §D #1 lock-in: web-side audit (localStorage 'axia.auditLog').
  // §D #2 lock-in: P26.7 capture policy (Tier 0/1 success skip).
  {
    const auditLogViewerPanel = new AuditLogViewerPanel(viewportEl);
    (window as unknown as { __axia_auditLogViewer?: AuditLogViewerPanel })
      .__axia_auditLogViewer = auditLogViewerPanel;
  }

  // ═══ 15e. Analytic Hover Overlay (ADR-070 Phase 1 Path Y C pilot) ═══
  // §C #1 lock-in: DOM overlay only (Three.js 통합 별도 ADR).
  // §C #3 lock-in: hover read-only — selection / preselect 무관.
  {
    const analyticHoverOverlay = new AnalyticHoverOverlay(document.body, {
      getFaceSurfaceJson: (faceId: number) => {
        const eng = bridge.engine as unknown as { getFaceSurfaceJson?: (id: number) => string };
        return eng?.getFaceSurfaceJson?.(faceId) ?? null;
      },
      getEdgeCurveJson: (edgeId: number) => {
        const eng = bridge.engine as unknown as { getEdgeCurveJson?: (id: number) => string };
        return eng?.getEdgeCurveJson?.(edgeId) ?? null;
      },
    });
    (window as unknown as { __axia_analyticHoverOverlay?: AnalyticHoverOverlay })
      .__axia_analyticHoverOverlay = analyticHoverOverlay;

    // Mousemove → raf-throttled overlay update.
    // Uses faceMap (triangle → FaceId) and edgeMap (segment → EdgeId)
    // from the WasmBridge per ADR-037 Pick→Promote.
    viewportEl.addEventListener('mousemove', (e: MouseEvent) => {
      if (!analyticHoverOverlay.isEnabled()) return;
      const picked = viewport.pickEdgeOrFace(e.clientX, e.clientY, 5);
      let target: { kind: 'face' | 'edge'; id: number } | null = null;
      const tm = toolManager as unknown as {
        faceMap?: Uint32Array | null;
        edgeMap?: Uint32Array | null;
      };
      if (picked && picked.hit.faceIndex != null) {
        const idx = picked.hit.faceIndex;
        if (picked.type === 'face') {
          const fm = tm.faceMap;
          const fid = fm && idx >= 0 && idx < fm.length ? fm[idx] : -1;
          if (fid >= 0) target = { kind: 'face', id: fid };
        } else if (picked.type === 'edge') {
          const em = tm.edgeMap;
          const eid = em && idx >= 0 && idx < em.length ? em[idx] : -1;
          if (eid >= 0) target = { kind: 'edge', id: eid };
        }
      }
      analyticHoverOverlay.update({
        target,
        screenX: e.clientX,
        screenY: e.clientY,
      });
    });
    viewportEl.addEventListener('mouseleave', () => {
      analyticHoverOverlay.update({ target: null, screenX: 0, screenY: 0 });
    });
  }

  // ═══ 14b. (Sun Panel removed 2026-05-16 — shadow system deferred) ═══

  // ═══ 15. Constraint Visual (3D 뷰포트 제약 인디케이터) ═══
  {
    const constraintVisual = new ConstraintVisual(viewportEl, bridge);
    (window as unknown as { __axia_constraintVisual?: ConstraintVisual })
      .__axia_constraintVisual = constraintVisual;

    // 매 프레임 업데이트 (카메라 이동 시 마커 위치 즉시 추적)
    const tickCV = () => {
      constraintVisual.update(viewport.activeCamera);
      requestAnimationFrame(tickCV);
    };
    requestAnimationFrame(tickCV);

    // Shift+J → 인디케이터 토글
    window.addEventListener('keydown', (e) => {
      if (isTypingInInput(e.target)) return;
      if ((e.key === 'j' || e.key === 'J') && e.shiftKey && !e.ctrlKey && !e.altKey) {
        constraintVisual.toggle();
      }
    });
  }

  // ═══ 15b. Dimension Manager (ADR-215 — 영구·편집가능 선형 치수) ═══
  {
    const dimensionManager = new DimensionManager({
      container: viewportEl,
      bridge,
      units,
      getCamera: () => viewport.activeCamera,
      onGeometryEdited: () => toolManager.syncMesh(),
    });
    (window as unknown as { __axia_dimensionManager?: DimensionManager })
      .__axia_dimensionManager = dimensionManager;
    const tickDim = () => {
      dimensionManager.update();
      requestAnimationFrame(tickDim);
    };
    requestAnimationFrame(tickDim);
  }

  // ═══ 15. Toolbar toggle-state sync ═══
  // Phase 1: Inspector/Style/Settings 버튼의 .active 클래스를 실제 패널 상태에
  //   바인딩. MutationObserver로 패널 DOM 변화(class/style)를 감시 → 버튼 갱신.
  // Phase 2: 새 display 토글 버튼(grid/AO/shadow) 클릭 → 대응 setter 호출 +
  //   상태를 .active 클래스로 반영.
  wireToolbarToggleState(viewport, toolManager);

  // Section plane — 단축키 F2로 간단 prompt 기반 토글.
  const sectionPlane = new SectionPlane(viewport);
  (window as unknown as { __axia_section?: SectionPlane }).__axia_section = sectionPlane;

  // Scenes (saved views) — 토글식 floating panel.
  const scenesManager = new ScenesManager(viewportEl, viewport, sectionPlane);
  (window as unknown as { __axia_scenes?: ScenesManager }).__axia_scenes = scenesManager;

  // Solar heatmap — lazy init on first menu use.
  (window as unknown as { __axia_solarHeatmap?: {
    viewport: typeof viewport; bridge: typeof bridge;
  } }).__axia_solarHeatmap = { viewport, bridge };

  debugLog('AXiA 3D ready. OSNAP: F3=Toggle, R=Rect, V=Extrude/Cut, P=Select, I=Inspector, O=Outliner, J=Constraints');
}

/** Phase 1 + Phase 2 — 툴바 버튼이 실제 상태(켜짐/꺼짐)를 시각적으로 반영하게
 *  묶어주는 배선. 패널 세 개는 MutationObserver, display 토글 세 개는 클릭
 *  리스너 + 초기 동기화로 처리. */
function wireToolbarToggleState(viewport: Viewport, _toolManager: ToolManager): void {
  // ── Phase 1: 패널 버튼 3개 ──
  const panelBindings: Array<{ btnId: string; panelId: string; isOpen: (p: HTMLElement) => boolean }> = [
    { btnId: 'inspector-btn', panelId: 'xia-inspector', isOpen: (p) => p.classList.contains('open') },
    { btnId: 'style-btn',     panelId: 'style-panel',   isOpen: (p) => p.classList.contains('open') },
    { btnId: 'settings-btn',  panelId: 'settings-panel', isOpen: (p) => p.style.display !== 'none' && p.style.display !== '' },
  ];
  for (const { btnId, panelId, isOpen } of panelBindings) {
    const btn = document.getElementById(btnId);
    if (!btn) continue;
    const syncFromPanel = () => {
      const panel = document.getElementById(panelId);
      // settings-panel은 클릭 시 lazily 생성되므로 초기엔 없을 수 있음.
      btn.classList.toggle('active', !!panel && isOpen(panel));
    };
    // 패널 존재 여부와 상관없이 document.body 전체를 관찰하면 동적 생성도 캐치.
    const observer = new MutationObserver(syncFromPanel);
    observer.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ['class', 'style'],
      childList: true,
    });
    syncFromPanel();
  }

  // ── Phase 2: display 토글 버튼 3개 ──
  const displayToggles: Array<{ key: string; get: () => boolean; set: (v: boolean) => void }> = [
    {
      key: 'grid',
      get: () => viewport.infiniteGrid.visible,
      set: (v) => viewport.setGridVisible(v),
    },
    {
      key: 'ssao',
      get: () => viewport.isSsaoEnabled(),
      set: (v) => viewport.setSsaoEnabled(v),
    },
    // Shadow toggle removed 2026-05-16 — shadow system deferred to ADR-106.
  ];
  for (const { key, get, set } of displayToggles) {
    const btn = document.querySelector(`.toggle-btn[data-toggle="${key}"]`) as HTMLElement | null;
    if (!btn) continue;
    const sync = () => btn.classList.toggle('active', get());
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      set(!get());
      sync();
    });
    sync();
  }
}

main().catch((err) => {
  console.error('[AXiA 3D] Fatal startup error:', err);
  // Show visible error to user instead of blank screen
  const errorDiv = document.createElement('div');
  errorDiv.style.cssText = `
    position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);
    background:#1a1a2e;color:#ff6b6b;padding:32px;border-radius:12px;
    font-family:'Segoe UI',sans-serif;text-align:center;z-index:99999;
    border:1px solid #ff6b6b33;max-width:480px;
  `;
  errorDiv.innerHTML = `
    <h2 style="margin:0 0 12px">AXiA 3D 시작 실패</h2>
    <p style="color:#ccc;margin:0 0 16px">${err instanceof Error ? err.message : String(err)}</p>
    <button onclick="location.reload()" style="
      background:#4ac1ff;color:#fff;border:none;padding:8px 24px;
      border-radius:6px;cursor:pointer;font-size:14px;
    ">새로고침</button>
  `;
  document.body.appendChild(errorDiv);
});
