#!/usr/bin/env node
/**
 * ADR-203 ε — external IFC validation.
 *
 * Everything before this step verified our IFC against *our own* idea of the
 * format (structural well-formedness + live engine counts). This script hands
 * the file to an **independent implementation** — `web-ifc`, the C++ IFC engine
 * behind IFC.js / ThatOpen viewers — and checks it can actually read it back:
 * schema, spatial hierarchy, wall + material names, and real tessellated
 * geometry from our analytic B-rep.
 *
 * Corpus is generated headlessly from the real engine (wasm-pack --target
 * nodejs build), so this is a true round-trip: engine → .ifc → foreign parser.
 *
 *   node scripts/ifc-external-validate.mjs
 *
 * Prereq: packages/axia-wasm-node/dist (see .github/workflows/mcp.yml, or
 *   cd crates/axia-wasm && wasm-pack build --target nodejs \
 *     --out-dir ../../packages/axia-wasm-node/dist)
 *
 * Exit 0 = every expectation held. Exit 1 = a regression (or a missing prereq).
 *
 * KNOWN EXTERNAL LIMITATION (documented, not a failure): web-ifc's geometry
 * kernel implements only IfcPlane + IfcCylindricalSurface for advanced faces.
 * IfcSphericalSurface / IfcConicalSurface / IfcToroidalSurface are valid IFC4
 * and we emit them, but that engine logs "unexpected surface type" and skips
 * them. We assert the entities are present and parseable, not that this
 * particular engine tessellates them.
 */
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const require = createRequire(import.meta.url);
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WASM = path.join(ROOT, 'packages', 'axia-wasm-node', 'dist', 'axia_wasm.js');

const failures = [];
const notes = [];
const check = (ok, label, detail = '') => {
  console.log(`  ${ok ? 'ok  ' : 'FAIL'} ${label}${detail ? `  — ${detail}` : ''}`);
  if (!ok) failures.push(label);
};

if (!fs.existsSync(WASM)) {
  console.error(`axia-wasm-node build missing: ${WASM}\n` +
    'Build it first:\n  cd crates/axia-wasm && wasm-pack build --target nodejs ' +
    '--out-dir ../../packages/axia-wasm-node/dist');
  process.exit(1);
}

let WebIFC;
try {
  WebIFC = require('web-ifc');
} catch {
  console.error('web-ifc not installed. Run `npm install` at the repo root.');
  process.exit(1);
}

const { AxiaEngine } = require(WASM);

/** Production turns Path B on via localStorage; Node has none, so opt in. */
function enablePathB(e) {
  for (const fn of ['setCylinderPathBDefault', 'setSpherePathBDefault',
                    'setConePathBDefault', 'setTorusPathBDefault']) {
    if (typeof e[fn] === 'function') { try { e[fn](true); } catch { /* older build */ } }
  }
}

// ── Corpus: generated from the real engine ────────────────────────────────
const corpus = [
  {
    file: 'box.ifc',
    what: 'planar solid (IfcPlane faces, IfcLine edges)',
    build: (e) => { e.create_box(0, 0, 1500, 2000, 3000, 4000); },
  },
  {
    file: 'curved.ifc',
    what: 'Path B analytic curved (cylinder + sphere)',
    build: (e) => { enablePathB(e); e.create_cylinder(0, 0, 0, 500, 1000, 24); e.create_sphere(3000, 0, 0, 400, 12, 12); },
  },
  {
    file: 'bim.ifc',
    what: 'semantic BIM (Xia → IfcWall + IfcMaterial)',
    build: (e) => {
      e.draw_rect_as_shape(0, 0, 0, 0, 0, 1, 1, 0, 0, 2000, 3000);
      const shapeId = Array.from(e.getShapeIds())[0];
      const faceId = Array.from(e.getShapeFaceIds(shapeId))[0];
      e.create_solid_extrude(faceId, 2000);
      e.promoteShapeToXia(shapeId, 1); // material 1 = 강철 / Steel
    },
  },
  {
    // δ — members export as what they are. Before this, a floor slab and a
    // column both left as IfcWall: geometry right, meaning wrong. Door and
    // window came later: they take thirteen attributes, not nine, and a
    // foreign parser is the real judge of whether we got that shape right.
    file: 'typed.ifc',
    what: 'classified members (slab + column + door + window, not everything a wall)',
    build: (e) => {
      e.create_box(0, 0, 0, 4000, 4000, 200);       // floor
      e.create_box(0, 0, 3000, 300, 300, 3000);     // column
      e.create_box(6000, 0, 1000, 900, 2100, 60);   // door leaf  (X 900, Z 2100)
      e.create_box(9000, 0, 1500, 1200, 900, 60);   // window sash (X 1200, Z 900)
      const ids = Array.from(e.getXiaIds());
      e.setXiaElementKind(ids[0], 'slab');
      e.setXiaElementKind(ids[1], 'column');
      e.setXiaElementKind(ids[2], 'door');
      e.setXiaElementKind(ids[3], 'window');
    },
  },
  {
    // β-3b — a closed-Bezier face (ADR-089 A-ω): its boundary is a spline, so
    // the export must use IFCBSPLINECURVEWITHKNOTS instead of falling back.
    file: 'spline.ifc',
    what: 'spline boundary (Bezier edge → IfcBSplineCurveWithKnots)',
    build: (e) => {
      e.drawClosedBezierAsCurve(new Float64Array([
        1000, 0, 0,
        1000, 1400, 0,
        -1000, 1400, 0,
        -1000, 0, 0,
        1000, 0, 0, // closed: last == first
      ]));
    },
  },
];

const outDir = fs.mkdtempSync(path.join(os.tmpdir(), 'axia-ifc-'));
for (const c of corpus) {
  const e = new AxiaEngine();
  c.build(e);
  c.text = e.exportIfcModel(c.file.replace('.ifc', ''));
  c.path = path.join(outDir, c.file);
  fs.writeFileSync(c.path, c.text, 'utf8');
}

// ── Validate with the foreign parser ──────────────────────────────────────
const api = new WebIFC.IfcAPI();
try { api.SetWasmPath(path.join(ROOT, 'node_modules', 'web-ifc') + path.sep, true); } catch { /* resolved by loader */ }
await api.Init();
console.log(`external parser: web-ifc ${api.GetVersion ? api.GetVersion() : ''}`.trim());

const typeCount = (api, modelID, name) => {
  const t = WebIFC[name];
  if (t === undefined) return 0;
  try { return api.GetLineIDsWithType(modelID, t).size(); } catch { return 0; }
};
const namesOfType = (api, modelID, name) => {
  const out = [];
  const t = WebIFC[name];
  if (t === undefined) return out;
  const ids = api.GetLineIDsWithType(modelID, t);
  for (let i = 0; i < ids.size(); i++) {
    const line = api.GetLine(modelID, ids.get(i));
    out.push(line?.Name?.value ?? '(unnamed)');
  }
  return out;
};

for (const c of corpus) {
  console.log(`\n## ${c.file} — ${c.what}  (${c.text.length} bytes)`);
  let modelID;
  try {
    modelID = api.OpenModel(new Uint8Array(fs.readFileSync(c.path)));
  } catch (err) {
    check(false, 'foreign parser opens the file', String(err?.message ?? err));
    continue;
  }
  check(true, 'foreign parser opens the file');
  check(api.GetModelSchema(modelID) === 'IFC4X3', 'schema is IFC4X3', api.GetModelSchema(modelID));

  // spatial hierarchy + product
  for (const t of ['IFCPROJECT', 'IFCSITE', 'IFCBUILDING', 'IFCBUILDINGSTOREY']) {
    check(typeCount(api, modelID, t) === 1, `exactly one ${t}`);
  }
  // A member is whatever kind it was classified as (δ) — counting only walls
  // would fail the moment a file legitimately has none.
  const MEMBER_TYPES = ['IFCWALL', 'IFCSLAB', 'IFCCOLUMN', 'IFCBEAM', 'IFCROOF',
    'IFCSTAIR', 'IFCRAMP', 'IFCRAILING', 'IFCCOVERING', 'IFCMEMBER', 'IFCPLATE',
    'IFCFOOTING', 'IFCDOOR', 'IFCWINDOW', 'IFCBUILDINGELEMENTPROXY'];
  const members = MEMBER_TYPES.reduce((n, t) => n + typeCount(api, modelID, t), 0);
  check(members >= 1, 'at least one building element', `${members}`);
  check(typeCount(api, modelID, 'IFCADVANCEDBREP') >= 1, 'analytic IfcAdvancedBrep (not faceted)');
  check(typeCount(api, modelID, 'IFCFACETEDBREP') === 0, 'no faceted fallback');

  // names survive the round-trip through a foreign reader
  const memberNames = MEMBER_TYPES.flatMap((t) => namesOfType(api, modelID, t));
  check(memberNames.every((n) => n && n !== '(unnamed)'),
    'every member has a readable name', JSON.stringify(memberNames));

  // geometry: the foreign kernel must tessellate our analytic B-rep
  let tris = 0, meshes = 0;
  try {
    api.StreamAllMeshes(modelID, (mesh) => {
      meshes++;
      const geoms = mesh.geometries;
      for (let i = 0; i < geoms.size(); i++) {
        const g = api.GetGeometry(modelID, geoms.get(i).geometryExpressID);
        tris += api.GetIndexArray(g.GetIndexData(), g.GetIndexDataSize()).length / 3;
      }
    });
  } catch (err) {
    check(false, 'geometry extraction', String(err?.message ?? err));
  }
  check(meshes >= 1 && tris > 0, 'foreign kernel tessellates our geometry', `${meshes} mesh(es), ${tris} triangles`);

  if (c.file === 'box.ifc') {
    check(typeCount(api, modelID, 'IFCPLANE') === 6, 'box = 6 IfcPlane faces');
    check(tris === 12, 'box tessellates to exactly 12 triangles', `${tris}`);
  }

  if (c.file === 'curved.ifc') {
    check(typeCount(api, modelID, 'IFCCYLINDRICALSURFACE') >= 1, 'IfcCylindricalSurface present');
    check(typeCount(api, modelID, 'IFCSPHERICALSURFACE') >= 1, 'IfcSphericalSurface present');
    check(typeCount(api, modelID, 'IFCCIRCLE') >= 1, 'IfcCircle rim edges present');
    notes.push('curved: web-ifc tessellates IfcPlane/IfcCylindricalSurface only; ' +
      'IfcSphericalSurface/Conical/Toroidal are emitted + parsed but skipped by that kernel.');
  }

  if (c.file === 'spline.ifc') {
    const spline = typeCount(api, modelID, 'IFCBSPLINECURVEWITHKNOTS')
      + typeCount(api, modelID, 'IFCRATIONALBSPLINECURVEWITHKNOTS');
    check(spline >= 1, 'foreign parser reads IfcBSplineCurveWithKnots', `${spline}`);
    check(typeCount(api, modelID, 'IFCLINE') === 0, 'spline boundary is not degraded to lines');
  }

  if (c.file === 'typed.ifc') {
    check(typeCount(api, modelID, 'IFCSLAB') === 1, 'the floor is an IfcSlab',
      `${typeCount(api, modelID, 'IFCSLAB')}`);
    check(typeCount(api, modelID, 'IFCCOLUMN') === 1, 'the column is an IfcColumn',
      `${typeCount(api, modelID, 'IFCCOLUMN')}`);
    check(typeCount(api, modelID, 'IFCWALL') === 0, 'nothing was left mislabelled as a wall',
      `${typeCount(api, modelID, 'IFCWALL')}`);
    // Door and window take thirteen attributes, not nine. A foreign parser is
    // the real judge of whether we got that shape right.
    check(typeCount(api, modelID, 'IFCDOOR') === 1, 'the door is an IfcDoor',
      `${typeCount(api, modelID, 'IFCDOOR')}`);
    check(typeCount(api, modelID, 'IFCWINDOW') === 1, 'the window is an IfcWindow',
      `${typeCount(api, modelID, 'IFCWINDOW')}`);
    check(tris > 0, 'the foreign kernel tessellates the typed members too', `${tris}`);
  }

  if (c.file === 'bim.ifc') {
    const mats = namesOfType(api, modelID, 'IFCMATERIAL');
    check(mats.length === 1, 'exactly one IfcMaterial', JSON.stringify(mats));
    check(mats[0] === '강철', 'material name round-trips (Korean \\X2\\ encoding)', JSON.stringify(mats[0]));
    check(typeCount(api, modelID, 'IFCRELASSOCIATESMATERIAL') === 1, 'material associated to the wall');
  }

  api.CloseModel(modelID);
}

fs.rmSync(outDir, { recursive: true, force: true });

if (notes.length) {
  console.log('\nnotes:');
  for (const n of notes) console.log(`  - ${n}`);
}
if (failures.length) {
  console.error(`\n${failures.length} check(s) FAILED:\n${failures.map((f) => `  - ${f}`).join('\n')}`);
  process.exit(1);
}
console.log('\nAll external-parser checks passed.');
