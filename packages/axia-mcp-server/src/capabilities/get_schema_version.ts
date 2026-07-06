// Tier 0 — get_schema_version: expose the engine's MCP schema + build version.
//
// ADR-041 P26.2 — the engine's schema_version is the SSOT the MCP server
// must satisfy (^MAJOR.MINOR). This capability surfaces it (plus the build
// version) to the AI agent for drift awareness. The values are read from
// the scene summary JSON, which already carries both fields.
import { z } from 'zod';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({}).strict();

const OutputSchema = z.object({
  schema_version: z.string(),
  engine_version: z.string(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

interface SummaryVersions {
  schema_version?: unknown;
  engine_version?: unknown;
}

export const getSchemaVersionCapability: CapabilityHandler<Input, Output> = {
  name: 'get_schema_version',
  tier: 0,
  description:
    'Return the engine MCP schema_version (ADR-041 P26.2) and build ' +
    'engine_version. Cheap read; useful for the AI to detect ' +
    'engine/server drift before issuing edits.',
  inputSchema: InputSchema,
  handler: ({ engine }) => {
    const raw = engine.sceneSummary();
    let parsed: SummaryVersions;
    try {
      parsed = JSON.parse(raw) as SummaryVersions;
    } catch (e) {
      throw new Error(
        `Engine returned malformed sceneSummary JSON: ${
          e instanceof Error ? e.message : String(e)
        }`,
      );
    }
    return {
      schema_version:
        typeof parsed.schema_version === 'string' ? parsed.schema_version : 'unknown',
      engine_version:
        typeof parsed.engine_version === 'string' ? parsed.engine_version : 'unknown',
    };
  },
};
