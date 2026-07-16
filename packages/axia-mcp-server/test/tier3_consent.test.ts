// ADR-041 P26.1 — Tier 3 per-call user consent.
//
// tiers.ts has required "Opt-in + per-call user consent + audit log" for Tier 3
// since it was written. The opt-in and the audit log existed; the consent never
// did — so the tier that erases faces and deletes objects was gated by exactly
// the same thing as Tier 2, a config flag, while the spec promised a person in
// the loop.
//
// Tier 3 has no wired handler yet (all five are declared-but-unimplemented), so
// these drive the gate through a stubbed Tier 3 handler. The gate is the
// precondition for wiring those capabilities, so it has to be proven before
// they land, not after.
import { describe, it, expect, beforeEach, vi } from 'vitest';

const stub = {
  name: 'erase_face',
  tier: 3 as const,
  description: 'TEST — erase a face',
  inputSchema: { safeParse: (v: unknown) => ({ success: true as const, data: v }) },
  outputSchema: { parse: (v: unknown) => v },
  handler: vi.fn(async () => ({ erased: true })),
};

// The real REGISTRY is frozen at module load from ALL_CAPABILITY_HANDLERS, so
// the handler has to be injected here rather than pushed at runtime.
const tier2Stub = {
  name: 'push_pull',
  tier: 2 as const,
  description: 'TEST — push/pull a face',
  inputSchema: { safeParse: (v: unknown) => ({ success: true as const, data: v }) },
  outputSchema: { parse: (v: unknown) => v },
  handler: vi.fn(async () => ({ ok: true })),
};

vi.mock('../src/capabilities/index.js', () => ({
  getCapabilityHandler: (name: string) =>
    name === 'erase_face' ? stub : name === 'push_pull' ? tier2Stub : undefined,
}));

const { dispatch, ConsentDeniedError } = await import('../src/dispatcher.js');
const { MemoryAuditSink } = await import('../src/audit.js');
type ConsentDecision = 'accept' | 'decline' | 'cancel' | 'unavailable';

const VERSIONS = { engine_version: '0.1.0', schema_version: '1.0.0' };
const T3_POLICY = {
  enabled_tiers: [0, 1, 2, 3] as (0 | 1 | 2 | 3)[],
  allow_caps: new Set<string>(),
  deny_caps: new Set<string>(),
};

describe('Tier 3 requires per-call consent', () => {
  beforeEach(() => stub.handler.mockClear());

  const run = (consent?: () => Promise<ConsentDecision>, sink = new MemoryAuditSink()) =>
    dispatch('erase_face', { faceId: 1 }, {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      engine: {} as any, auditSink: sink, versions: VERSIONS, policy: T3_POLICY,
      ...(consent ? { consent } : {}),
    });

  it('runs only after an explicit accept', async () => {
    await expect(run(async () => 'accept')).resolves.toBeTruthy();
    expect(stub.handler).toHaveBeenCalledTimes(1);
  });

  for (const decision of ['decline', 'cancel'] as const) {
    it(`refuses and does not run when the user says ${decision}`, async () => {
      const sink = new MemoryAuditSink();
      await expect(run(async () => decision, sink)).rejects.toThrow(ConsentDeniedError);
      expect(stub.handler, 'nothing destructive may run without consent').not.toHaveBeenCalled();
      await Promise.resolve();
      const entry = sink.entries.at(-1)!;
      expect(entry.result).toBe('denied');
      expect(entry.tier).toBe(3);
      expect(entry.reason).toBe(`Tier 3 consent ${decision}`);
    });
  }

  it('FAILS CLOSED with no consent channel — silence is not approval', async () => {
    const sink = new MemoryAuditSink();
    await expect(run(undefined, sink)).rejects.toThrow(ConsentDeniedError);
    expect(stub.handler).not.toHaveBeenCalled();
    await Promise.resolve();
    const entry = sink.entries.at(-1)!;
    expect(entry.result).toBe('denied');
    // distinct from 'decline': nobody could be asked — a deployment problem an
    // operator needs to see, not a user choice
    expect(entry.reason).toBe('Tier 3 consent unavailable');
  });

  it('a consent channel that throws never lets the work through', async () => {
    await expect(run(async () => { throw new Error('transport died'); })).rejects.toThrow();
    expect(stub.handler).not.toHaveBeenCalled();
  });

  it('the prompt is told WHAT will run — capability + validated args', async () => {
    let seen: unknown = null;
    await run(async (...a: unknown[]) => { seen = a[0]; return 'accept'; });
    expect(seen).toMatchObject({ capability: 'erase_face', tier: 3, args: { faceId: 1 } });
    // "approve erase_face" without saying which face is not consent
    expect((seen as { request_id: string }).request_id).toMatch(/^[0-9a-f-]{36}$/);
  });

  it('Tier 2 is NOT prompted — consent is Tier 3 only', () => {
    // Over-prompting would be its own defect: a confirm on every push/pull
    // trains people to click through the one that matters.
    let asked = 0;
    return dispatch('push_pull', { faceId: 1, distance: 10 }, {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      engine: {} as any, auditSink: new MemoryAuditSink(), versions: VERSIONS,
      policy: T3_POLICY,
      consent: async () => { asked += 1; return 'accept'; },
    }).then(() => {
      expect(asked, 'a Tier 2 op must not interrupt the user').toBe(0);
      expect(tier2Stub.handler).toHaveBeenCalledTimes(1);
    });
  });
});
