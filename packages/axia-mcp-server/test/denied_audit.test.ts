// ADR-041 P26.7 follow-up — denied calls (any tier) MUST appear in audit.
// Intrusion-detection signal.
import { describe, it, expect } from 'vitest';
import { dispatch } from '../src/dispatcher.js';
import { MemoryAuditSink } from '../src/audit.js';
import type { EngineInstance } from '../src/capabilities/types.js';

const VERSIONS = { engine_version: '0.1.0', schema_version: '1.0.0' };

function mockEngine(): EngineInstance {
  return {
    draw_rect_as_shape: () => 1,
    create_solid_extrude: () => true,
    exportSnapshotStrict: () => new Uint8Array(),
  };
}

describe('denied calls are audited (intrusion signal)', () => {
  it('Unknown capability → denied entry with reason', async () => {
    const sink = new MemoryAuditSink();
    await expect(
      dispatch(
        'rm_rf_slash',
        { evil: true },
        { engine: mockEngine(), auditSink: sink, versions: VERSIONS },
      ),
    ).rejects.toThrow();
    await Promise.resolve();
    expect(sink.entries).toHaveLength(1);
    const entry = sink.entries[0]!;
    expect(entry.result).toBe('denied');
    expect(entry.tier).toBeNull();
    expect(entry.capability).toBe('rm_rf_slash');
    expect(entry.reason).toMatch(/Unknown capability/);
    expect(entry.engine_version).toBe('0.1.0');
    expect(entry.schema_version).toBe('1.0.0');
    expect(entry.request_id).toMatch(/^[0-9a-f-]{36}$/);
  });

  // ADR-041 P26.7 — a capability that is DECLARED in tiers.ts but has no
  // handler used to throw with NO audit entry, behind a comment claiming the
  // branch was "unreachable: evaluatePolicy returns unknown=false above".
  // evaluatePolicy checks tiers.ts membership — what is declared — and says
  // nothing about whether a handler exists. Measured: 32 declared, 26 wired.
  // Of the remaining six gaps, export_obj / export_stl / export_step are Tier 1
  // (reachable on the DEFAULT config), so an agent calling an advertised-but-
  // absent capability left no trace at all. (create_xia closed this gap for its
  // own id — 2026-07-18; the export trio still needs an engine-side serializer,
  // export_step is an ADR-035 P20.B non-goal.)
  it('declared-but-unimplemented → denied entry, not a silent throw', async () => {
    const sink = new MemoryAuditSink();
    await expect(
      dispatch(
        'export_obj', // Tier 1 (default-on), declared in tiers.ts, no handler
        {},
        { engine: mockEngine(), auditSink: sink, versions: VERSIONS },
      ),
    ).rejects.toThrow();
    await Promise.resolve();
    expect(sink.entries, 'the throw must not be silent').toHaveLength(1);
    const entry = sink.entries[0]!;
    expect(entry.result).toBe('denied');
    expect(entry.capability).toBe('export_obj');
    // distinguishable from a genuine unknown: this one IS declared, at a tier
    expect(entry.tier).toBe(1);
    expect(entry.reason).toMatch(/declared but not implemented/);
    expect(entry.engine_version).toBe('0.1.0');
    expect(entry.request_id).toMatch(/^[0-9a-f-]{36}$/);
  });

  it('a genuine unknown is still reported as unknown, not as unimplemented', () => {
    // guards the two reasons from collapsing into one
    return dispatch('no_such_thing', {}, {
      engine: mockEngine(), auditSink: new MemoryAuditSink(), versions: VERSIONS,
    }).catch((e: Error) => {
      expect(e.message).toMatch(/Unknown capability/);
    });
  });

  it('Tier blocked → denied entry with config detail', async () => {
    const sink = new MemoryAuditSink();
    await expect(
      dispatch(
        'push_pull',
        { face_id: 1, distance: 10 },
        {
          engine: mockEngine(),
          auditSink: sink,
          versions: VERSIONS,
          // default tiers [0,1] — push_pull (Tier 2) blocked
        },
      ),
    ).rejects.toThrow();
    await Promise.resolve();
    expect(sink.entries).toHaveLength(1);
    expect(sink.entries[0]!.result).toBe('denied');
    expect(sink.entries[0]!.tier).toBe(2);
    expect(sink.entries[0]!.reason).toMatch(/Tier 2 not enabled/);
  });

  it('Input validation failure → denied entry with issue detail', async () => {
    const sink = new MemoryAuditSink();
    await expect(
      dispatch(
        'draw_rect',
        { center: [0, 0, 0], width: -5, height: 10 },
        { engine: mockEngine(), auditSink: sink, versions: VERSIONS },
      ),
    ).rejects.toThrow();
    await Promise.resolve();
    expect(sink.entries).toHaveLength(1);
    expect(sink.entries[0]!.result).toBe('denied');
    expect(sink.entries[0]!.tier).toBe(1);
    expect(sink.entries[0]!.reason).toMatch(/Input validation failed/);
    expect(sink.entries[0]!.reason).toMatch(/width/);
  });

  it('All denied entries carry request_id and version stamps', async () => {
    const sink = new MemoryAuditSink();
    await expect(
      dispatch('unknown_x', {}, { engine: mockEngine(), auditSink: sink, versions: VERSIONS }),
    ).rejects.toThrow();
    await expect(
      dispatch(
        'push_pull',
        { face_id: 1, distance: 1 },
        { engine: mockEngine(), auditSink: sink, versions: VERSIONS },
      ),
    ).rejects.toThrow();
    await Promise.resolve();
    expect(sink.entries).toHaveLength(2);
    for (const e of sink.entries) {
      expect(e.request_id).toMatch(/^[0-9a-f-]{36}$/);
      expect(e.engine_version).toBe('0.1.0');
      expect(e.schema_version).toBe('1.0.0');
    }
    // Request IDs are unique
    expect(sink.entries[0]!.request_id).not.toBe(sink.entries[1]!.request_id);
  });

  it('Caller can override request_id for client correlation', async () => {
    const sink = new MemoryAuditSink();
    await expect(
      dispatch('unknown_x', {}, {
        engine: mockEngine(),
        auditSink: sink,
        versions: VERSIONS,
        request_id: 'caller-supplied-123',
      }),
    ).rejects.toThrow();
    await Promise.resolve();
    expect(sink.entries[0]!.request_id).toBe('caller-supplied-123');
  });
});
