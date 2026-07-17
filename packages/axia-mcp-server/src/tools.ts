// MCP `tools/list` + `tools/call` wiring.
//
// Converts the capability registry to JSON Schema for tools/list, and
// routes tools/call through the dispatcher with full tier authorization
// + audit + input validation.

import type { Server } from '@modelcontextprotocol/sdk/server/index.js';
import {
  ListToolsRequestSchema,
  CallToolRequestSchema,
  type CallToolResult,
} from '@modelcontextprotocol/sdk/types.js';
import { zodToJsonSchema } from './zod_to_json_schema.js';
import {
  dispatch,
  type ConsentFn,
  type ConsentRequest,
  type ConsentDecision,
} from './dispatcher.js';
import {
  ALL_CAPABILITY_HANDLERS,
  type EngineInstance,
} from './capabilities/index.js';
import { isVisibleInToolsList, type CapabilityPolicy } from './policy.js';
import type { AuditSink } from './audit.js';

export interface ToolsWiringOptions {
  engine: EngineInstance;
  /** ADR-042 P27 — full capability policy (replaces tier-only config). */
  policy: CapabilityPolicy;
  auditSink: AuditSink;
  client: string;
  versions: { schema_version: string; engine_version: string };
}

/**
 * P27.4 — `tools/list` filter: a capability appears iff `evaluatePolicy()`
 * would allow it. ALLOW promotes capabilities above their tier; DENY
 * removes them. Defense in depth: dispatcher re-checks at call time.
 */
function visibleCapabilities(policy: CapabilityPolicy) {
  return ALL_CAPABILITY_HANDLERS.filter((h) => isVisibleInToolsList(h.name, policy));
}

/**
 * ADR-041 P26.1 — the Tier 3 consent channel, backed by MCP elicitation.
 *
 * `tiers.ts` has required "per-call user consent" for Tier 3 since it was
 * written, and nothing implemented it: erasing faces and deleting objects was
 * gated by the same config flag as Tier 2. This puts a person back in the loop.
 *
 * A confirmation, not a form: `requestedSchema` carries no properties, so the
 * client renders the message and an accept/decline. The user is shown what will
 * run — capability, description, and the validated arguments — because
 * "approve erase_face" without saying WHICH face is not consent.
 *
 * Fail-closed at every step. A client that does not advertise elicitation makes
 * `elicitInput` throw; a transport error does the same; both become
 * `unavailable` rather than an assumed yes. `unavailable` stays distinct from
 * `decline` so the audit log separates "the user said no" from "nobody could be
 * asked" — the second is a deployment problem, and collapsing them would hide it.
 */
export function makeElicitationConsent(server: Server): ConsentFn {
  return async (req: ConsentRequest): Promise<ConsentDecision> => {
    try {
      const res = await server.elicitInput({
        message:
          `AXiA — 파괴적 작업 승인 요청 (Tier 3)\n\n` +
          `작업: ${req.capability}\n` +
          `${req.description}\n\n` +
          `인자: ${JSON.stringify(req.args)}\n` +
          `요청자: ${req.request_id}\n\n` +
          `실행을 허용하시겠습니까?`,
        requestedSchema: { type: 'object', properties: {} },
      });
      return res.action; // 'accept' | 'decline' | 'cancel'
    } catch {
      // No elicitation support, or the ask never got through.
      return 'unavailable';
    }
  };
}

export function wireTools(server: Server, opts: ToolsWiringOptions): void {
  server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
      tools: visibleCapabilities(opts.policy).map((h) => ({
        name: h.name,
        description: h.description,
        inputSchema: zodToJsonSchema(h.inputSchema),
      })),
    };
  });

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const { name, arguments: rawArgs } = req.params;
    try {
      const result = await dispatch(name, rawArgs ?? {}, {
        engine: opts.engine,
        policy: opts.policy,
        auditSink: opts.auditSink,
        client: opts.client,
        versions: opts.versions,
        consent: makeElicitationConsent(server),
      });
      const response: CallToolResult = {
        content: [
          {
            type: 'text',
            text: JSON.stringify(result.output, null, 2),
          },
        ],
      };
      return response;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      const response: CallToolResult = {
        isError: true,
        content: [
          {
            type: 'text',
            text: `Error in "${name}": ${msg}`,
          },
        ],
      };
      return response;
    }
  });
}
