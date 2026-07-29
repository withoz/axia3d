/**
 * ADR-309 wiring — a lossy IFC import must not be announced as a plain success.
 *
 * The engine now names what a round-trip drops (`…is now FLAT (volume lost)`),
 * but naming it is only half the job: the handler appended those lines to a
 * GREEN success toast, so "your sphere lost its volume" arrived looking like
 * good news. That is the exact misreading ADR-309 exists to prevent.
 *
 * Escalation is deliberately narrow — only a warning that reports lost volume
 * turns the toast yellow. A benign note (an assumed unit, a surface that came
 * back coarser but correct) stays green, because turning every warning yellow
 * trains people to ignore yellow.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const SRC = readFileSync(resolve(__dirname, 'IfcImportHandler.ts'), 'utf8');

describe('ADR-309 — a lossy IFC import is not a green success', () => {
  it('escalates to a warning toast when a warning reports lost volume', () => {
    // The handler boots a lot of engine surface, so this pins the decision at
    // the source rather than by driving the whole import. The behaviour is one
    // branch; what matters is that the branch exists and keys on real loss.
    expect(SRC, 'the handler must decide between success and warning').toMatch(
      /const lostVolume\s*=\s*warnings\.some\(/,
    );
    expect(SRC, "escalation must key on the engine's FLAT marker").toMatch(
      /warnings\.some\(\(w\) => w\.includes\('FLAT'\)\)/,
    );
    const branch = SRC.slice(SRC.indexOf('const lostVolume'));
    expect(branch, 'lost volume → Toast.warning').toMatch(
      /if \(lostVolume\)[\s\S]{0,120}Toast\.warning/,
    );
    expect(branch, 'otherwise → Toast.success').toMatch(/else \{[\s\S]{0,120}Toast\.success/);
  });

  it('still lists the warnings themselves, whichever toast is used', () => {
    // Escalating the colour is no use if the reason stops being printed.
    expect(SRC).toMatch(/for \(const w of warnings\.slice\(0, 3\)\) lines\.push/);
  });

  it('keeps the marker the engine actually emits', () => {
    // If the engine's wording drifts, this handler silently stops escalating.
    // The Rust side builds the string in ifc_geometry.rs; both must agree.
    const rust = readFileSync(
      resolve(__dirname, '../../../crates/axia-ifc/src/ifc_geometry.rs'),
      'utf8',
    );
    expect(
      rust,
      'ifc_geometry.rs must still emit the FLAT marker this handler keys on — ' +
        'if the wording changed, update both sides together',
    ).toContain('FLAT (volume lost)');
  });
});
