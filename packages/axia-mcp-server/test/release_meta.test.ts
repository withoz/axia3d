// ADR-044 P29.7 — release metadata regression (6 회귀).
//
// Lives in axia-mcp-server because it has the test infrastructure already.
// Reads sibling packages from the repo root via fs.

import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import semver from 'semver';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
// __dirname = .../packages/axia-mcp-server/test
const repoRoot = resolve(__dirname, '../../..');

interface PackageJson {
  name: string;
  version: string;
  private?: boolean;
  license?: string;
  author?: string;
  homepage?: string;
  repository?: { type: string; url: string; directory?: string } | string;
  bugs?: { url: string } | string;
  keywords?: string[];
  files?: string[];
  scripts?: Record<string, string>;
  dependencies?: Record<string, string>;
  publishConfig?: { access?: string; provenance?: boolean };
}

interface PublishablePackage {
  /** Folder under packages/ */
  dir: string;
  /** Expected name in package.json */
  expectedName: string;
}

const PUBLISHABLES: PublishablePackage[] = [
  { dir: 'axia-mcp-server', expectedName: '@axia/mcp-server' },
  { dir: 'create-axia-mcp', expectedName: 'create-axia-mcp' },
  // axia-wasm-node is wasm-pack auto-generated and lives at
  // packages/axia-wasm-node/dist/package.json after build. Excluded
  // from this test — covered by a separate post-build patch script
  // (scripts/patch-wasm-package.mjs, ADR-044 R5) which is itself locked by
  // the "ADR-044 R5" describe block below. NOTE: that script did not exist
  // until 2026-07-14, so this exclusion was unjustified for ~2 months and the
  // package shipped with no metadata, no publishConfig and no publish guard.
];

function loadPkg(pkgDir: string): PackageJson {
  const p = resolve(repoRoot, 'packages', pkgDir, 'package.json');
  if (!existsSync(p)) throw new Error(`package.json missing: ${p}`);
  return JSON.parse(readFileSync(p, 'utf8'));
}

describe('ADR-044 P29.7 — release metadata regression', () => {
  // ────────────────────────────────────────────────────────────
  // P29.7 #1
  // ────────────────────────────────────────────────────────────
  describe('release_metadata_complete', () => {
    for (const { dir, expectedName } of PUBLISHABLES) {
      it(`${dir} has license / repository / author / bugs / homepage / keywords`, () => {
        const pkg = loadPkg(dir);
        expect(pkg.name).toBe(expectedName);
        expect(pkg.license).toBe('MIT');
        expect(pkg.author).toBeDefined();
        expect(pkg.homepage).toMatch(/github\.com/);
        expect(pkg.bugs).toBeDefined();
        expect(pkg.keywords).toBeInstanceOf(Array);
        expect(pkg.keywords!.length).toBeGreaterThan(0);
        if (typeof pkg.repository === 'object') {
          expect(pkg.repository.type).toBe('git');
          expect(pkg.repository.url).toMatch(/github\.com/);
          expect(pkg.repository.directory).toBe(`packages/${dir}`);
        } else {
          throw new Error(`${dir} repository must be object form`);
        }
      });
    }
  });

  // ────────────────────────────────────────────────────────────
  // P29.7 #2
  // ────────────────────────────────────────────────────────────
  describe('release_files_whitelist_present', () => {
    for (const { dir } of PUBLISHABLES) {
      it(`${dir} declares non-empty files[] whitelist`, () => {
        const pkg = loadPkg(dir);
        expect(pkg.files).toBeInstanceOf(Array);
        expect(pkg.files!.length).toBeGreaterThan(0);
        // Every publishable must include README.md
        expect(pkg.files).toContain('README.md');
        // None should ship plain src/ (TypeScript source) or test/.
        // create-axia-mcp DOES ship src/template/ — those are runtime
        // assets (intentional, not source).
        for (const f of pkg.files!) {
          expect(f, `${f} smells like TypeScript source`).not.toBe('src');
          expect(f, `${f} smells like TypeScript source`).not.toBe('src/');
          expect(f).not.toBe('test');
          expect(f).not.toBe('test/');
          expect(f, `${f} ships .ts files`).not.toMatch(/\.ts$/);
        }
      });
    }
  });

  // ────────────────────────────────────────────────────────────
  // P29.7 #3
  // ────────────────────────────────────────────────────────────
  it('release_lockstep_versions — all publishables share the same version', () => {
    const versions = PUBLISHABLES.map(({ dir }) => loadPkg(dir).version);
    const unique = new Set(versions);
    expect(
      unique.size,
      `Versions diverged: ${versions.join(', ')}. ADR-044 P29.1 demands lockstep.`,
    ).toBe(1);
    // Each must be valid semver
    for (const v of versions) {
      expect(semver.valid(v), `version ${v} invalid`).toBeTruthy();
    }
  });

  // ────────────────────────────────────────────────────────────
  // P29.7 #4
  // ────────────────────────────────────────────────────────────
  describe('release_prepublish_hook_present', () => {
    for (const { dir } of PUBLISHABLES) {
      it(`${dir} has scripts.prepublishOnly`, () => {
        const pkg = loadPkg(dir);
        expect(pkg.scripts?.prepublishOnly).toBeTruthy();
        // Must invoke guard-publish.mjs (even from sibling package)
        expect(pkg.scripts!.prepublishOnly).toMatch(/guard-publish\.mjs/);
        // Must run tests before publishing
        expect(pkg.scripts!.prepublishOnly).toMatch(/npm (run )?test/);
        // Must build before publishing
        expect(pkg.scripts!.prepublishOnly).toMatch(/npm (run )?build/);
      });
    }
  });

  // ────────────────────────────────────────────────────────────
  // P29.7 #5
  // ────────────────────────────────────────────────────────────
  it('release_schema_pin_consistent — server schema satisfies ^engine schema', () => {
    // Read engine SCHEMA_VERSION from Rust source.
    const wasmRust = resolve(repoRoot, 'crates/axia-wasm/src/lib.rs');
    const wasmText = readFileSync(wasmRust, 'utf8');
    const wasmMatch = wasmText.match(
      /const\s+SCHEMA_VERSION:\s*&str\s*=\s*"([^"]+)"/,
    );
    expect(wasmMatch, 'engine SCHEMA_VERSION not found').toBeTruthy();
    const wasmSchema = wasmMatch![1]!;

    // Read server MCP_SERVER_SCHEMA_VERSION
    const handshake = resolve(
      repoRoot,
      'packages/axia-mcp-server/src/handshake.ts',
    );
    const serverText = readFileSync(handshake, 'utf8');
    const serverMatch = serverText.match(
      /MCP_SERVER_SCHEMA_VERSION\s*=\s*['"]([^'"]+)['"]/,
    );
    expect(serverMatch, 'MCP_SERVER_SCHEMA_VERSION not found').toBeTruthy();
    const serverSchema = serverMatch![1]!;

    expect(
      semver.satisfies(serverSchema, `^${wasmSchema}`),
      `Server ${serverSchema} must satisfy ^${wasmSchema}`,
    ).toBe(true);

    // Read scaffold MCP_SERVER_VERSION_RANGE
    const scaffold = resolve(
      repoRoot,
      'packages/create-axia-mcp/src/scaffold.ts',
    );
    const scaffoldText = readFileSync(scaffold, 'utf8');
    const scaffoldMatch = scaffoldText.match(
      /MCP_SERVER_VERSION_RANGE\s*=\s*['"]([^'"]+)['"]/,
    );
    expect(scaffoldMatch, 'MCP_SERVER_VERSION_RANGE not found').toBeTruthy();
    const scaffoldRange = scaffoldMatch![1]!;

    // Scaffold range must satisfy current server package version.
    const serverPkg = loadPkg('axia-mcp-server');
    expect(
      semver.satisfies(serverPkg.version, scaffoldRange),
      `Scaffold range "${scaffoldRange}" must satisfy server ${serverPkg.version}`,
    ).toBe(true);
  });

  // ────────────────────────────────────────────────────────────
  // P29.7 #6
  // ────────────────────────────────────────────────────────────
  describe('release_no_private_flag_on_publishables', () => {
    for (const { dir } of PUBLISHABLES) {
      it(`${dir} has private:false or no private flag`, () => {
        const pkg = loadPkg(dir);
        expect(pkg.private ?? false).toBe(false);
      });
    }
  });

  // ────────────────────────────────────────────────────────────
  // Bonus: P29.6 publishConfig provenance + access
  // ────────────────────────────────────────────────────────────
  describe('publishConfig — provenance + public access', () => {
    for (const { dir } of PUBLISHABLES) {
      it(`${dir} declares public access + provenance`, () => {
        const pkg = loadPkg(dir);
        expect(pkg.publishConfig?.access).toBe('public');
        expect(pkg.publishConfig?.provenance).toBe(true);
      });
    }
  });

  // ────────────────────────────────────────────────────────────
  // P29.6 — release.yml publish gate is fail-safe (2026-07-14).
  //
  // ADR-044 states the gate three times ("gated by `inputs.publish`",
  // "manual trigger"), but release.yml's publish job condition read
  //   github.event_name == 'push' || (workflow_dispatch && publish=='true')
  // whose leading `push` clause auto-published on ANY `release/v*` tag,
  // bypassing the input — i.e. the code violated the ADR's own success
  // criterion. Harmless only while NPM_TOKEN is absent (npm publish →
  // ENEEDAUTH); the moment a token lands, one tag push would have published
  // all three packages to the public registry. Note guard-publish.mjs does
  // NOT protect here: it allows anything when CI/GITHUB_ACTIONS is set, so
  // this `if:` is the SOLE gate on CI publishes.
  //
  // The P29.7 six read package.json only — nothing locked the workflow, so
  // nothing stopped the clause being re-added. This closes that gap.
  // ────────────────────────────────────────────────────────────
  // ────────────────────────────────────────────────────────────
  // P29 follow-up — wasm-pack must be pinned EVERYWHERE, not just at release.
  //
  // Measured 2026-07-16: release.yml + mcp.yml pinned `--version 0.14.0`, but
  // build.yml (x2), ci.yml, deploy.yml and update-visual-baselines.yml ran a
  // bare `cargo install wasm-pack`. `--locked` does NOT pin the version — it
  // pins the crate's own lockfile — so CI could build the WASM that ships with
  // a different wasm-pack than the one release is verified against.
  // ────────────────────────────────────────────────────────────
  describe('wasm-pack is pinned in every workflow', () => {
    const WORKFLOWS = [
      'build.yml', 'ci.yml', 'deploy.yml', 'mcp.yml',
      'release.yml', 'update-visual-baselines.yml',
    ];

    it('no workflow installs wasm-pack without --version', () => {
      const unpinned: string[] = [];
      for (const f of WORKFLOWS) {
        const p = resolve(repoRoot, '.github/workflows', f);
        if (!existsSync(p)) continue;
        for (const line of readFileSync(p, 'utf8').split('\n')) {
          if (line.includes('cargo install wasm-pack') && !line.includes('--version')) {
            unpinned.push(`${f}: ${line.trim()}`);
          }
        }
      }
      expect(unpinned, 'an unpinned install can ship WASM built by a different toolchain')
        .toEqual([]);
    });

    it('every workflow pins the SAME version', () => {
      const versions = new Set<string>();
      for (const f of WORKFLOWS) {
        const p = resolve(repoRoot, '.github/workflows', f);
        if (!existsSync(p)) continue;
        for (const m of readFileSync(p, 'utf8').matchAll(/cargo install wasm-pack --version (\S+)/g)) {
          versions.add(m[1]!);
        }
      }
      expect(versions.size, `workflows disagree on wasm-pack: ${[...versions].join(', ')}`).toBe(1);
    });
  });

  describe('P29.6 — release.yml publish gate (fail-safe)', () => {
    const releaseYml = () =>
      readFileSync(resolve(repoRoot, '.github/workflows/release.yml'), 'utf8');

    /**
     * The publish JOB block. Anchored on `^  publish:$` — a plain
     * `indexOf('  publish:')` matches the workflow_dispatch INPUT named
     * `publish` first (its 6-space indent contains "  publish:" as a
     * substring), which silently widens the slice to include the preflight
     * job and makes these assertions false-pass. Caught by mutation testing.
     */
    const publishJobSlice = (): string => {
      const yml = releaseYml();
      const m = yml.match(/^ {2}publish:$/m);
      expect(m, 'release.yml must declare a publish job').toBeTruthy();
      return yml.slice(m!.index!);
    };

    /** The `if:` line of the publish job. */
    const publishIf = (): string => {
      const yml = releaseYml();
      const m = yml.match(/^\s*if:.*$/gm);
      expect(m, 'release.yml must declare an if: gate').toBeTruthy();
      // The publish job holds the only `if:` in this workflow.
      expect(m!.length, 'exactly one if: gate expected').toBe(1);
      return m![0];
    };

    it('publish requires an explicit workflow_dispatch input', () => {
      const cond = publishIf();
      expect(cond).toContain("github.event_name == 'workflow_dispatch'");
      expect(cond).toContain("inputs.publish == 'true'");
    });

    it('publish does NOT auto-fire on a tag push (no bare push clause)', () => {
      const cond = publishIf();
      // The regression being locked: a bare `github.event_name == 'push'`
      // disjunct (or any `||`) would re-open tag-push auto-publish.
      expect(cond).not.toMatch(/event_name\s*==\s*'push'/);
      expect(cond, 'no disjunct may widen the gate').not.toContain('||');
    });

    it('the publish input stays `type: choice` (string), not boolean', () => {
      // `inputs` preserves real Booleans, so `type: boolean` would make
      // `inputs.publish == 'true'` a boolean-vs-string compare → always
      // false → publish unreachable forever (fails closed, but silently).
      const yml = releaseYml();
      const block = yml.slice(yml.indexOf('publish:'), yml.indexOf('permissions:'));
      // Strip comments — the workflow explains the boolean footgun in prose,
      // so a naive substring match would hit its own warning comment.
      const code = block
        .split('\n')
        .filter((l) => !l.trim().startsWith('#'))
        .join('\n');
      expect(code).toContain('type: choice');
      expect(code).not.toContain('type: boolean');
      expect(code).toContain("default: 'false'");
    });

    it('publish is bound to an approval environment', () => {
      // The `if:` narrows publishing to a deliberate dispatch, but that is
      // still one click by ANY collaborator with write access. Binding the job
      // to an environment lets the repo owner require reviewers.
      //
      // NOTE this asserts the YAML only. GitHub auto-creates a referenced
      // environment with NO protection rules, so the real gate lives in
      // Settings → Environments → npm-release (required reviewers). This test
      // exists so the binding can't be silently dropped from the workflow.
      const publishJob = publishJobSlice();
      const envIdx = publishJob.indexOf('environment:');
      expect(envIdx, 'publish job must declare an environment').toBeGreaterThan(-1);
      expect(publishJob.slice(envIdx, envIdx + 120)).toContain('name: npm-release');
    });

    it('the patch script scopes the name to @axia/wasm-node (P29.1/P29.3)', () => {
      // wasm-pack emits the Cargo crate name (`axia-wasm`), which would make
      // release.yml's `--access public` a no-op and publish the wrong package.
      // The scoped name is what ADR-041 P26.4 / ADR-043 P28.3 / ADR-044
      // P29.1+P29.3 all assume (사용자 결재 2026-07-14). Asserts the script's
      // source, since dist/ is a build artifact and isn't present in a fresh
      // checkout — the runtime behaviour is covered by the script's own
      // fail-closed name invariant.
      const script = readFileSync(
        resolve(repoRoot, 'scripts/patch-wasm-package.mjs'),
        'utf8',
      );
      expect(script).toContain("name: '@axia/wasm-node'");
      // …and it must hard-fail rather than warn if the name ends up unscoped.
      expect(script).toMatch(/NAME INVARIANT VIOLATED/);
      expect(script).toMatch(/prepublishOnly/);
      expect(script).toMatch(/guard-publish\.mjs/);
    });

    it('axia-wasm-node is patched before it is published (ADR-044 R5)', () => {
      // The publish job regenerates dist/package.json via wasm-pack on every
      // run, so the patch MUST sit between that build and the npm publish —
      // otherwise the tarball ships without the guard/metadata. Locking the
      // ORDER, not just the presence of the step.
      const publishJob = publishJobSlice();
      const buildIdx = publishJob.indexOf('wasm-pack build --target nodejs');
      const patchIdx = publishJob.indexOf('node scripts/patch-wasm-package.mjs');
      const pubIdx = publishJob.indexOf('working-directory: packages/axia-wasm-node/dist');
      expect(buildIdx, 'publish job builds the wasm package').toBeGreaterThan(-1);
      expect(patchIdx, 'publish job runs the ADR-044 R5 patch').toBeGreaterThan(-1);
      expect(pubIdx, 'publish job publishes axia-wasm-node').toBeGreaterThan(-1);
      expect(patchIdx, 'patch must run AFTER wasm-pack build').toBeGreaterThan(buildIdx);
      expect(patchIdx, 'patch must run BEFORE npm publish').toBeLessThan(pubIdx);
    });

    it('npm publish exists only in release.yml, and only in the gated job', () => {
      const wfDir = resolve(repoRoot, '.github/workflows');
      for (const f of ['ci.yml', 'build.yml', 'deploy.yml', 'mcp.yml', 'update-visual-baselines.yml']) {
        const p = resolve(wfDir, f);
        if (!existsSync(p)) continue;
        expect(readFileSync(p, 'utf8'), `${f} must not publish`).not.toMatch(/^\s*run:.*npm publish/m);
      }
      // In release.yml every `npm publish` sits after the gated job header.
      const yml = releaseYml();
      const gateIdx = yml.search(/^\s*if:\s*github\.event_name/m);
      expect(gateIdx).toBeGreaterThan(-1);
      const publishRuns = [...yml.matchAll(/^\s*run:.*npm publish.*$/gm)];
      expect(publishRuns.length, 'three packages are published').toBe(3);
      for (const m of publishRuns) expect(m.index!).toBeGreaterThan(gateIdx);
    });
  });
});
