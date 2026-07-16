// Tier 3 destructive capabilities — wired 2026-07-16 (user approved exposing
// them to agents; the consent gate landed first, deliberately).
//
// These pin the three properties that make that safe, against the REAL handlers
// and the REAL policy — not stubs:
//   1. hidden on the default config (opt-in),
//   2. nothing runs without an explicit accept,
//   3. a declined call leaves the engine untouched.
import { describe, it, expect, vi } from 'vitest';
import { dispatch, ConsentDeniedError } from '../src/dispatcher.js';
import { MemoryAuditSink } from '../src/audit.js';
import { DEFAULT_TIER_CONFIG, tierOf } from '../src/tiers.js';
import { listRegisteredCapabilities } from '../src/capabilities/index.js';
import { isVisibleInToolsList, DEFAULT_POLICY } from '../src/policy.js';
import type { EngineInstance } from '../src/capabilities/types.js';

const VERSIONS = { engine_version: '0.1.0', schema_version: '1.0.0' };
const T3_POLICY = {
  enabled_tiers: [0, 1, 2, 3] as (0 | 1 | 2 | 3)[],
  allow_caps: new Set<string>(),
  deny_caps: new Set<string>(),
};
const T3 = ['erase_face', 'erase_edge', 'delete_group'] as const;

function spyEngine() {
  return {
    delete_face: vi.fn(() => true),
    deleteEdgeCascade: vi.fn(() => 2),
    delete_group: vi.fn(() => true),
  } as unknown as EngineInstance & {
    delete_face: ReturnType<typeof vi.fn>;
    deleteEdgeCascade: ReturnType<typeof vi.fn>;
    delete_group: ReturnType<typeof vi.fn>;
  };
}
const argsFor = (cap: string) =>
  cap === 'erase_face' ? { face_id: 1 }
    : cap === 'erase_edge' ? { edge_id: 1 }
      : { group_id: 1 };

describe('Tier 3 destructive capabilities are wired', () => {
  it('all three are registered and declared at tier 3', () => {
    const reg = listRegisteredCapabilities();
    for (const cap of T3) {
      expect(reg, `${cap} must have a handler`).toContain(cap);
      expect(tierOf(cap), `${cap} must be declared Tier 3`).toBe(3);
    }
  });

  it('are NOT reachable on the default config — opt-in only', () => {
    expect(DEFAULT_TIER_CONFIG.enabled_tiers).not.toContain(3);
    for (const cap of T3) {
      expect(
        isVisibleInToolsList(cap, DEFAULT_POLICY),
        `${cap} must not be advertised by default`,
      ).toBe(false);
    }
  });

  it('are denied on the default config even if called directly', async () => {
    const engine = spyEngine();
    for (const cap of T3) {
      await expect(
        dispatch(cap, argsFor(cap), {
          engine, auditSink: new MemoryAuditSink(), versions: VERSIONS,
          consent: async () => 'accept', // consent alone must not open the tier
        }),
      ).rejects.toThrow();
    }
    expect(engine.delete_face).not.toHaveBeenCalled();
    expect(engine.deleteEdgeCascade).not.toHaveBeenCalled();
    expect(engine.delete_group).not.toHaveBeenCalled();
  });

  for (const cap of T3) {
    it(`${cap}: nothing runs when the user declines`, async () => {
      const engine = spyEngine();
      await expect(
        dispatch(cap, argsFor(cap), {
          engine, auditSink: new MemoryAuditSink(), versions: VERSIONS,
          policy: T3_POLICY, consent: async () => 'decline',
        }),
      ).rejects.toThrow(ConsentDeniedError);
      expect(engine.delete_face).not.toHaveBeenCalled();
      expect(engine.deleteEdgeCascade).not.toHaveBeenCalled();
      expect(engine.delete_group).not.toHaveBeenCalled();
    });

    it(`${cap}: runs on accept, and the prompt names the target`, async () => {
      const engine = spyEngine();
      let asked: { capability?: string; args?: unknown } = {};
      const res = await dispatch(cap, argsFor(cap), {
        engine, auditSink: new MemoryAuditSink(), versions: VERSIONS,
        policy: T3_POLICY,
        consent: async (r) => { asked = r; return 'accept'; },
      });
      expect(res.output).toMatchObject({ ok: true });
      expect(asked.capability).toBe(cap);
      // approving "erase_face" without saying WHICH face is not consent
      expect(asked.args).toEqual(argsFor(cap));
    });
  }

  it('erase_edge reports how many faces the cascade took', async () => {
    // deleteEdgeCascade removes every face sharing the edge — the count is the
    // whole point of preferring it over the legacy bool-returning delete_edge.
    const engine = spyEngine();
    const res = await dispatch('erase_edge', { edge_id: 7 }, {
      engine, auditSink: new MemoryAuditSink(), versions: VERSIONS,
      policy: T3_POLICY, consent: async () => 'accept',
    });
    expect(engine.deleteEdgeCascade).toHaveBeenCalledWith(7);
    expect(res.output).toMatchObject({ ok: true, cascaded_face_count: 2 });
  });

  it('erase_edge reports failure (-1) as ok=false, not as 0 faces', async () => {
    const engine = spyEngine();
    engine.deleteEdgeCascade.mockReturnValue(-1);
    const res = await dispatch('erase_edge', { edge_id: 7 }, {
      engine, auditSink: new MemoryAuditSink(), versions: VERSIONS,
      policy: T3_POLICY, consent: async () => 'accept',
    });
    expect(res.output).toMatchObject({ ok: false, cascaded_face_count: 0 });
  });

  it('every Tier 3 call is audited', async () => {
    const sink = new MemoryAuditSink();
    await dispatch('erase_face', { face_id: 1 }, {
      engine: spyEngine(), auditSink: sink, versions: VERSIONS,
      policy: T3_POLICY, consent: async () => 'accept',
    });
    await Promise.resolve();
    const entry = sink.entries.at(-1)!;
    expect(entry.capability).toBe('erase_face');
    expect(entry.tier).toBe(3);
    expect(entry.result).toBe('ok');
  });
});
