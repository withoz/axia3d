/**
 * StepIgesPrewarm — ADR-118 γ-7 (γ-4 component) regression coverage.
 *
 * 사용자 결재 2026-05-17.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('StepIgesPrewarm — ADR-118 γ-7 (pre-warm)', () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  describe('localStorage flag (default ON, opt-out via false)', () => {
    it('getPrewarmEnabled returns true by default', async () => {
      const m = await import('./StepIgesPrewarm');
      expect(m.getPrewarmEnabled()).toBe(true);
    });

    it('localStorage "true" → enabled (matches default)', async () => {
      localStorage.setItem('axia:step-iges-prewarm', 'true');
      const m = await import('./StepIgesPrewarm');
      expect(m.getPrewarmEnabled()).toBe(true);
    });

    it('localStorage "false" → disabled (explicit opt-out)', async () => {
      localStorage.setItem('axia:step-iges-prewarm', 'false');
      const m = await import('./StepIgesPrewarm');
      expect(m.getPrewarmEnabled()).toBe(false);
    });

    it('setPrewarmEnabled persists to localStorage', async () => {
      const m = await import('./StepIgesPrewarm');
      m.setPrewarmEnabled(false);
      expect(localStorage.getItem('axia:step-iges-prewarm')).toBe('false');
      m.setPrewarmEnabled(true);
      expect(localStorage.getItem('axia:step-iges-prewarm')).toBe('true');
    });
  });

  describe('prewarmStepIgesEngine — idle scheduling', () => {
    it('uses requestIdleCallback when available', async () => {
      const ricSpy = vi.fn();
      (window as any).requestIdleCallback = ricSpy;
      const m = await import('./StepIgesPrewarm');
      m.resetPrewarmForTest();
      m.prewarmStepIgesEngine();
      expect(ricSpy).toHaveBeenCalledTimes(1);
      // Second arg should be { timeout: 5000 }
      const opts = ricSpy.mock.calls[0][1];
      expect(opts).toEqual({ timeout: 5000 });
      delete (window as any).requestIdleCallback;
    });

    it('falls back to setTimeout when requestIdleCallback missing', async () => {
      const origRIC = (window as any).requestIdleCallback;
      delete (window as any).requestIdleCallback;
      const setTimeoutSpy = vi.spyOn(window, 'setTimeout');
      const m = await import('./StepIgesPrewarm');
      m.resetPrewarmForTest();
      m.prewarmStepIgesEngine();
      expect(setTimeoutSpy).toHaveBeenCalled();
      // Second arg should be 2000ms
      const delay = setTimeoutSpy.mock.calls[setTimeoutSpy.mock.calls.length - 1][1];
      expect(delay).toBe(2000);
      setTimeoutSpy.mockRestore();
      if (origRIC) (window as any).requestIdleCallback = origRIC;
    });

    it('skips when localStorage disabled (opt-out)', async () => {
      localStorage.setItem('axia:step-iges-prewarm', 'false');
      const ricSpy = vi.fn();
      (window as any).requestIdleCallback = ricSpy;
      const m = await import('./StepIgesPrewarm');
      m.resetPrewarmForTest();
      m.prewarmStepIgesEngine();
      expect(ricSpy).not.toHaveBeenCalled();
      delete (window as any).requestIdleCallback;
    });

    it('idempotent: second call no-op', async () => {
      const ricSpy = vi.fn();
      (window as any).requestIdleCallback = ricSpy;
      const m = await import('./StepIgesPrewarm');
      m.resetPrewarmForTest();
      m.prewarmStepIgesEngine();
      m.prewarmStepIgesEngine();
      m.prewarmStepIgesEngine();
      expect(ricSpy).toHaveBeenCalledTimes(1);
      delete (window as any).requestIdleCallback;
    });

    it('resetPrewarmForTest allows re-trigger (test isolation)', async () => {
      const ricSpy = vi.fn();
      (window as any).requestIdleCallback = ricSpy;
      const m = await import('./StepIgesPrewarm');
      m.resetPrewarmForTest();
      m.prewarmStepIgesEngine();
      expect(ricSpy).toHaveBeenCalledTimes(1);
      m.resetPrewarmForTest();
      m.prewarmStepIgesEngine();
      expect(ricSpy).toHaveBeenCalledTimes(2);
      delete (window as any).requestIdleCallback;
    });
  });

  describe('ADR-118 architectural promise', () => {
    it('does not block main thread synchronously (returns immediately)', async () => {
      const ricSpy = vi.fn();
      (window as any).requestIdleCallback = ricSpy;
      const m = await import('./StepIgesPrewarm');
      m.resetPrewarmForTest();

      const t0 = performance.now();
      m.prewarmStepIgesEngine();
      const t1 = performance.now();

      // Sync return must be < 10ms (no blocking work)
      expect(t1 - t0).toBeLessThan(10);
      delete (window as any).requestIdleCallback;
    });

    it('graceful no-op when localStorage throws (private mode)', async () => {
      const origGetItem = Storage.prototype.getItem;
      Storage.prototype.getItem = vi.fn(() => { throw new Error('private mode'); });
      const m = await import('./StepIgesPrewarm');
      // Should not throw + default to ON (per documented graceful behavior)
      expect(() => m.getPrewarmEnabled()).not.toThrow();
      expect(m.getPrewarmEnabled()).toBe(true);
      Storage.prototype.getItem = origGetItem;
    });
  });
});

/**
 * The deployed build does not pre-warm — and an explicit preference still wins.
 *
 * ⚠ ADR-119 L-119-4 locked the default ON and that is unchanged for anyone
 * running the app locally. What changed is that the *deploy pipeline* asks for
 * OFF, because the bargain is different on a public site: measured on
 * https://withoz.github.io/axia3d/ with `performance.getEntriesByType`, the app
 * is 6.4 MB and the pre-warm adds 115.1 MB to every visit, first request ~1.24 s
 * after load, whether or not the visitor ever opens a STEP file.
 *
 * The last guard is the one that matters most: a flag the pipeline stops setting
 * fails silently everywhere else, because the code would just fall back to ON
 * and nothing would look broken.
 */
describe('StepIgesPrewarm — deployed builds do not pre-warm', () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    vi.unstubAllEnvs();
  });

  it('VITE_STEP_PREWARM=off makes the default OFF', async () => {
    vi.stubEnv('VITE_STEP_PREWARM', 'off');
    const m = await import('./StepIgesPrewarm');
    expect(m.getPrewarmEnabled()).toBe(false);
  });

  it('leaves the default ON when the flag is absent — a local build is untouched', async () => {
    const m = await import('./StepIgesPrewarm');
    expect(m.getPrewarmEnabled()).toBe(true);
  });

  it('any other value is not "off", so a typo cannot silently disable it', async () => {
    vi.stubEnv('VITE_STEP_PREWARM', 'OFF');
    const m = await import('./StepIgesPrewarm');
    expect(m.getPrewarmEnabled()).toBe(true);
  });

  it('an explicit "true" beats a build that defaults OFF', async () => {
    vi.stubEnv('VITE_STEP_PREWARM', 'off');
    localStorage.setItem('axia:step-iges-prewarm', 'true');
    const m = await import('./StepIgesPrewarm');
    expect(m.getPrewarmEnabled()).toBe(true);
  });

  it('an explicit "false" beats a build that defaults ON', async () => {
    localStorage.setItem('axia:step-iges-prewarm', 'false');
    const m = await import('./StepIgesPrewarm');
    expect(m.getPrewarmEnabled()).toBe(false);
  });

  it('deploy.yml actually sets the flag on the step that builds', async () => {
    const { readFileSync } = await import('node:fs');
    const yml = readFileSync('../.github/workflows/deploy.yml', 'utf8');
    // The env has to sit on the Build step. Anywhere else and Vite never sees
    // it, which is exactly the kind of drift that looks fine in a diff.
    const build = yml.slice(yml.indexOf('- name: Build'));
    expect(build).toMatch(/env:\s*\n\s*VITE_STEP_PREWARM:\s*'off'/);
    expect(build.indexOf('VITE_STEP_PREWARM')).toBeLessThan(build.indexOf('run: npm run build'));
  });
});
