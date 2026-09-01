/**
 * The desktop shell's config, pinned at the two places it silently broke.
 *
 * ⚠ Tauri resolves `frontendDist` relative to the directory holding
 * tauri.conf.json, and runs `beforeBuildCommand` from the REPOSITORY ROOT.
 * Two fields in one file, two different base directories. Measured 2026-08-31:
 * `npm --prefix ../web run build:desktop` failed with
 * `ENOENT ... open 'E:\web\package.json'` — `E:\AXiA3D\..\web` — while the same
 * command from `desktop/` reads the right package.json, which is how the cwd
 * was identified rather than guessed.
 *
 * Tauri swallows a child process's stdout, so a `process.cwd()` probe printed
 * nothing; the failure message carried the answer the probe could not.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';

const CONF = JSON.parse(readFileSync('../desktop/tauri.conf.json', 'utf8'));

describe('the desktop config builds where Tauri actually stands', () => {
  it('runs its build command from the repository root, not from desktop/', () => {
    // ⚠ A leading `../` here is the exact shape that failed.
    expect(CONF.build.beforeBuildCommand).not.toMatch(/\.\.\//);
    expect(CONF.build.beforeBuildCommand).toContain('web');
  });

  it('points frontendDist relative to the config, which is the other base', () => {
    expect(CONF.build.frontendDist).toBe('../web/dist');
  });

  it('opens the dev server on the port vite is actually configured for', () => {
    // 3000, from `server.port` in vite.config.ts — not Vite's default 5173.
    expect(CONF.build.devUrl).toBe('http://localhost:3000');
  });

  it('names icons that exist, including the .ico Windows needs', () => {
    for (const rel of CONF.bundle.icon) {
      expect(existsSync(`../desktop/${rel}`), `missing ${rel}`).toBe(true);
    }
    expect(CONF.bundle.icon.some((i: string) => i.endsWith('.ico'))).toBe(true);
  });

  it('keeps the workspace out of the desktop crate', () => {
    // ⚠ Without this, `cargo test --workspace` would build 431 Tauri crates to
    // run kernel tests — and the desktop crate would not build at all
    // ("current package believes it's in a workspace when it's not").
    expect(readFileSync('../Cargo.toml', 'utf8')).toMatch(/exclude\s*=\s*\[\s*"desktop"\s*\]/);
  });

  it('builds the frontend into a cleaned dist, so dead copies are not shipped', () => {
    const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));
    expect(pkg.scripts['build:desktop']).toContain('clean-dist');
    expect(pkg.scripts['build:desktop']).toContain('npm run build');
  });
});
