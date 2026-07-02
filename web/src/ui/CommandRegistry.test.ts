import { describe, it, expect, beforeEach, vi } from 'vitest';
import { initCommandRegistry, CommandRegistryDeps } from './CommandRegistry';

function mockDeps(): CommandRegistryDeps {
  return {
    commandInput: {
      registerHandler: vi.fn(),
      toggle: vi.fn(),
      printSuccess: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
    } as any,
    bridge: {
      drawLineAsShape: vi.fn(),
      normalizeForImport: vi.fn().mockReturnValue({
        degenerateRemoved: 0, windingFlipped: 0, normalsRecomputed: 0,
        isolatedVertsRemoved: 0, remainingViolations: 0,
      }),
      countFreeEdges: vi.fn().mockReturnValue(0),
      synthesizeFacesFromFreeEdges: vi.fn().mockReturnValue(0),
      verifyInvariants: vi.fn().mockReturnValue({
        checkedFaces: 0, valid: true, violationCount: 0, violations: [],
      }),
    } as any,
    toolManager: {
      setTool: vi.fn(),
      syncMesh: vi.fn(),
    } as any,
  };
}

describe('CommandRegistry', () => {
  let deps: ReturnType<typeof mockDeps>;

  beforeEach(() => {
    deps = mockDeps();
    initCommandRegistry(deps);
  });

  describe('initCommandRegistry', () => {
    it('registers all Phase H+I + prior handlers', () => {
      expect(deps.commandInput.registerHandler).toHaveBeenCalledTimes(12);
      const calls = (deps.commandInput.registerHandler as any).mock.calls;
      const names = calls.map((c: any) => c[0].name);
      expect(names).toContain('line');
      expect(names).toContain('curves');
      expect(names).toContain('clearcurves');
      expect(names).toContain('mergetol');
      expect(names).toContain('mergemat');
      expect(names).toContain('cadmode');
      expect(names).toContain('normalize');
      expect(names).toContain('synthfaces');
      expect(names).toContain('verify');
      expect(names).toContain('help');
      expect(names).toContain('repair');
      expect(names).toContain('integrity');
    });
  });

  describe('integrity command (ADR-267 δ)', () => {
    function handler() {
      const calls = (deps.commandInput.registerHandler as any).mock.calls;
      return calls.map((c: any) => c[0]).find((h: any) => h.name === 'integrity');
    }

    it('is registered with 무결성 alias', () => {
      const h = handler();
      expect(h).toBeTruthy();
      expect(h.aliases).toContain('무결성');
    });

    it('prints error when engine lacks verifyVolumeIntegrity', () => {
      (deps.bridge as any).engine = {};
      handler().execute([]);
      expect(deps.commandInput.printError).toHaveBeenCalled();
    });

    it('prints success on valid integrity', () => {
      (deps.bridge as any).engine = {
        verifyVolumeIntegrity: vi.fn().mockReturnValue(
          '{"valid":true,"invariantViolations":0,"geometricCracks":0,"openBoundaryEdges":0,"checkedFaces":6}'
        ),
      };
      handler().execute([]);
      expect(deps.commandInput.printSuccess).toHaveBeenCalled();
    });

    it('prints error on integrity violation', () => {
      (deps.bridge as any).engine = {
        verifyVolumeIntegrity: vi.fn().mockReturnValue(
          '{"valid":false,"invariantViolations":2,"geometricCracks":1,"openBoundaryEdges":0,"checkedFaces":6}'
        ),
      };
      handler().execute([]);
      expect(deps.commandInput.printError).toHaveBeenCalled();
    });
  });

  describe('line command', () => {
    let lineHandler: any;

    beforeEach(() => {
      const calls = (deps.commandInput.registerHandler as any).mock.calls;
      lineHandler = calls.find((c: any) => c[0].name === 'line')[0];
    });

    it('has correct aliases', () => {
      expect(lineHandler.aliases).toContain('L');
    });

    it('no args → activates line tool', () => {
      lineHandler.execute([]);
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('line');
    });

    it('single number arg → activates line tool with length', () => {
      lineHandler.execute(['100']);
      expect(deps.toolManager.setTool).toHaveBeenCalledWith('line');
    });

    it('invalid single arg → throws', () => {
      expect(() => lineHandler.execute(['abc'])).toThrow();
    });

    it('zero length → throws', () => {
      expect(() => lineHandler.execute(['0'])).toThrow();
    });

    it('coordinate args → draws line via bridge', () => {
      lineHandler.execute(['0,0,0', '10,20,30']);
      expect(deps.bridge.drawLineAsShape).toHaveBeenCalledWith(0, 0, 0, 10, 20, 30, 0, 0, 0);
      expect(deps.toolManager.syncMesh).toHaveBeenCalled();
    });

    it('bad coordinate format → throws', () => {
      expect(() => lineHandler.execute(['0,0', '10,20'])).toThrow('좌표 형식');
    });

    it('NaN coordinates → throws', () => {
      expect(() => lineHandler.execute(['a,b,c', '1,2,3'])).toThrow('숫자');
    });
  });

  describe('help command', () => {
    let helpHandler: any;

    beforeEach(() => {
      const calls = (deps.commandInput.registerHandler as any).mock.calls;
      helpHandler = calls.find((c: any) => c[0].name === 'help')[0];
    });

    it('has correct aliases', () => {
      expect(helpHandler.aliases).toContain('H');
      expect(helpHandler.aliases).toContain('?');
    });

    it('prints command list', () => {
      helpHandler.execute([]);
      expect(deps.commandInput.printInfo).toHaveBeenCalled();
    });
  });

  describe('keyboard shortcut', () => {
    it('backtick toggles command input', () => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: '`', bubbles: true }));
      expect(deps.commandInput.toggle).toHaveBeenCalled();
    });

    it('Ctrl+K toggles command input', () => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
      expect(deps.commandInput.toggle).toHaveBeenCalled();
    });
  });
});
