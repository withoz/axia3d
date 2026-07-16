// Capability dispatcher — single ingress point for tool calls.
//
// Order:
//   1. Look up handler (records denied audit + throws UnknownCapabilityError)
//   2. authorizeCapability (records denied audit + throws CapabilityBlockedError)
//   3. Validate input via Zod schema (records denied audit + throws CapabilityInputError)
//   4. Time the handler call
//   5. Append audit entry per shouldAudit() policy
//   6. Return parsed output (or rethrow on error)

import type { z } from 'zod';
import {
  UnknownCapabilityError,
  CapabilityBlockedError,
  type TierConfig,
  tierOf,
  type Tier,
} from './tiers.js';
import {
  evaluatePolicy,
  formatDenialReason,
  DEFAULT_POLICY,
  type CapabilityPolicy,
} from './policy.js';
import {
  type AuditSink,
  NullAuditSink,
  shouldAudit,
  makeAuditEntry,
  newRequestId,
} from './audit.js';
import {
  getCapabilityHandler,
  type CapabilityHandler,
  type EngineInstance,
} from './capabilities/index.js';

export interface VersionInfo {
  schema_version: string;
  engine_version: string;
}

/**
 * The outcome of asking the user to approve a Tier 3 call.
 *
 * `accept` / `decline` / `cancel` mirror MCP elicitation's own actions;
 * `unavailable` is ours — there was no way to ask (no consent channel wired, or
 * a client without elicitation support). It is deliberately NOT folded into
 * `decline`: an operator reading the audit log needs to tell "the user said no"
 * apart from "nobody could be asked", which is a deployment problem.
 */
export type ConsentDecision = 'accept' | 'decline' | 'cancel' | 'unavailable';

export interface ConsentRequest {
  capability: string;
  tier: Tier;
  /** Validated args — what will actually run if approved. */
  args: unknown;
  request_id: string;
  /** The capability's own description, for the prompt. */
  description: string;
}

export type ConsentFn = (req: ConsentRequest) => Promise<ConsentDecision>;

/** A Tier 3 call that the user did not approve — or could not be asked about. */
export class ConsentDeniedError extends Error {
  public readonly capability: string;
  public readonly decision: ConsentDecision;

  constructor(capability: string, decision: ConsentDecision) {
    super(
      decision === 'unavailable'
        ? `Tier 3 capability '${capability}' requires per-call user consent, but no consent channel is available (client must support MCP elicitation).`
        : `Tier 3 capability '${capability}' was not approved by the user (${decision}).`,
    );
    this.name = 'ConsentDeniedError';
    this.capability = capability;
    this.decision = decision;
  }
}

export interface DispatcherOptions {
  engine: EngineInstance;
  /**
   * @deprecated Use `policy` instead. Retained for backward compat in tests
   * (ADR-042 migration). When both `policy` and `config` are absent,
   * `DEFAULT_POLICY` is used.
   */
  config?: TierConfig;
  /** ADR-042 P27 — full capability policy (tiers + ALLOW + DENY). */
  policy?: CapabilityPolicy;
  auditSink?: AuditSink;
  client?: string;
  /** From handshake. Stamped onto every audit entry for drift correlation. */
  versions: VersionInfo;
  /**
   * ADR-041 P26.1 — asks the user to approve a Tier 3 (destructive) call.
   * Absent = no consent channel = every Tier 3 call is denied (fail-closed).
   * `wireTools` supplies one backed by MCP elicitation.
   */
  consent?: ConsentFn;
  /** Override request id for client correlation; auto-generated otherwise. */
  request_id?: string;
}

function resolvePolicy(opts: DispatcherOptions): CapabilityPolicy {
  if (opts.policy) return opts.policy;
  if (opts.config) {
    return {
      enabled_tiers: opts.config.enabled_tiers,
      allow_caps: new Set(),
      deny_caps: new Set(),
    };
  }
  return DEFAULT_POLICY;
}

export class CapabilityInputError extends Error {
  public readonly capability: string;
  public readonly issues: z.ZodIssue[];

  constructor(capability: string, issues: z.ZodIssue[]) {
    const detail = issues.map((i) => `${i.path.join('.')}: ${i.message}`).join('; ');
    super(`Invalid input for "${capability}": ${detail}`);
    this.name = 'CapabilityInputError';
    this.capability = capability;
    this.issues = issues;
  }
}

export interface DispatchResult {
  capability: string;
  output: unknown;
  duration_ms: number;
  request_id: string;
}

/**
 * Fire-and-forget audit append — never fails dispatch on log error.
 */
function recordAudit(sink: AuditSink, draft: Parameters<typeof makeAuditEntry>[0]): void {
  if (!shouldAudit({ tier: draft.tier, result: draft.result })) return;
  void sink.append(makeAuditEntry(draft)).catch(() => {
    /* swallow log errors */
  });
}

export async function dispatch(
  capability: string,
  rawInput: unknown,
  opts: DispatcherOptions,
): Promise<DispatchResult> {
  const policy = resolvePolicy(opts);
  const auditSink = opts.auditSink ?? new NullAuditSink();
  const client = opts.client ?? 'unknown';
  const request_id = opts.request_id ?? newRequestId();
  const versions = opts.versions;
  const start = performance.now();

  // 1+2. Policy evaluation (ADR-042 P27): tier + ALLOW + DENY in one place.
  const decision = evaluatePolicy(capability, policy);
  if (!decision.allowed) {
    const reasonText = decision.reason
      ? formatDenialReason(decision.reason)
      : 'Denied by policy';
    const tierForAudit =
      decision.reason?.kind === 'tier_disabled_no_allow'
        ? decision.reason.tier
        : tierOf(capability) ?? null;

    recordAudit(auditSink, {
      request_id,
      client,
      tier: tierForAudit,
      capability,
      args: rawInput,
      duration_ms: performance.now() - start,
      result: 'denied',
      reason: reasonText,
      engine_version: versions.engine_version,
      schema_version: versions.schema_version,
    });

    // Throw the most specific error for backward compat with existing
    // callers that pattern-match on these classes.
    if (decision.reason?.kind === 'unknown') {
      throw new UnknownCapabilityError(capability);
    }
    if (decision.reason?.kind === 'tier_disabled_no_allow') {
      throw new CapabilityBlockedError({
        capability,
        tier: decision.reason.tier,
        enabled_tiers: decision.reason.enabled_tiers,
      });
    }
    // DENY layer — surfaced as CapabilityBlockedError so MCP tool callers
    // see a consistent error type.
    const t = tierOf(capability);
    throw new CapabilityBlockedError({
      capability,
      tier: t !== undefined ? t : (0 as 0),
      enabled_tiers: policy.enabled_tiers,
    });
  }

  // Lookup handler now that policy passed.
  const handler: CapabilityHandler<unknown, unknown> | undefined =
    getCapabilityHandler(capability);
  if (!handler) {
    // NOT unreachable — the comment here used to claim it was. `evaluatePolicy`
    // checks membership in tiers.ts (what is DECLARED); it says nothing about
    // whether a handler was ever written. Measured: 32 declared, 22 wired, so
    // ten capabilities pass policy and land here — and four of them
    // (create_xia, export_obj, export_stl, export_step) are Tier 1, i.e.
    // reachable on the DEFAULT config.
    //
    // The genuine-unknown path above records an audit entry before throwing;
    // this one threw silently, so the one case an operator most wants to see —
    // an agent calling an advertised capability that does not exist — left no
    // trace at all. Record it the same way, with a reason that says which of
    // the two it is.
    recordAudit(auditSink, {
      request_id,
      client,
      tier: tierOf(capability) ?? null,
      capability,
      args: rawInput,
      duration_ms: performance.now() - start,
      result: 'denied',
      reason: `declared but not implemented: ${capability}`,
      engine_version: versions.engine_version,
      schema_version: versions.schema_version,
    });
    throw new UnknownCapabilityError(capability);
  }

  // 3. Input validation.
  const parsed = handler.inputSchema.safeParse(rawInput);
  if (!parsed.success) {
    const issueDetail = parsed.error.issues
      .map((i) => `${i.path.join('.')}: ${i.message}`)
      .join('; ');
    recordAudit(auditSink, {
      request_id,
      client,
      tier: handler.tier,
      capability,
      args: rawInput,
      duration_ms: performance.now() - start,
      result: 'denied',
      reason: `Input validation failed: ${issueDetail}`,
      engine_version: versions.engine_version,
      schema_version: versions.schema_version,
    });
    throw new CapabilityInputError(capability, parsed.error.issues);
  }

  // 3.5. Tier 3 — per-call user consent.
  //
  // `tiers.ts` has specified "Opt-in + per-call user consent + audit log" for
  // Tier 3 since it was written; the opt-in and the audit log existed, the
  // consent never did. So the tier that erases faces and deletes objects was
  // gated by exactly the same thing as Tier 2 — a config flag — despite the
  // spec promising a person in the loop.
  //
  // Asked here, after validation, so the user is only interrupted for a call
  // that would actually run, and the prompt can show the parsed args rather
  // than whatever arrived on the wire.
  //
  // FAIL CLOSED. No consent channel (`opts.consent` absent, or a client that
  // does not support elicitation) means consent cannot be obtained — not that
  // it can be assumed. Every outcome other than an explicit accept is recorded
  // as a denial, because "the user said no" and "nobody asked" are both things
  // an operator needs to see (ADR-041 P26.7).
  if (handler.tier === 3) {
    const decision: ConsentDecision = opts.consent
      ? await opts.consent({
        capability,
        tier: handler.tier,
        args: parsed.data,
        request_id,
        description: handler.description,
      })
      : 'unavailable';

    if (decision !== 'accept') {
      recordAudit(auditSink, {
        request_id,
        client,
        tier: handler.tier,
        capability,
        args: parsed.data,
        duration_ms: performance.now() - start,
        result: 'denied',
        reason: `Tier 3 consent ${decision}`,
        engine_version: versions.engine_version,
        schema_version: versions.schema_version,
      });
      throw new ConsentDeniedError(capability, decision);
    }
  }

  // 4-5. Run + record outcome.
  let output: unknown;
  let result: 'ok' | 'error' = 'ok';
  let error_message: string | undefined;
  try {
    output = await handler.handler(
      { engine: opts.engine, client },
      parsed.data,
    );
  } catch (e) {
    result = 'error';
    error_message = e instanceof Error ? e.message : String(e);
    throw e;
  } finally {
    const duration_ms = performance.now() - start;
    recordAudit(auditSink, {
      request_id,
      client,
      tier: handler.tier,
      capability,
      args: parsed.success ? parsed.data : rawInput,
      duration_ms,
      result,
      error_message,
      engine_version: versions.engine_version,
      schema_version: versions.schema_version,
    });
  }

  const duration_ms = performance.now() - start;
  return { capability, output, duration_ms, request_id };
}

// Re-export for convenience.
export { tierOf };
