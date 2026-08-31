/**
 * Turning the engine's triangles into the three interchange formats an agent
 * can ask for.
 *
 * ⚠ Written against the engine's own buffers rather than Three.js. The browser
 * exports through `OBJExporter` / `STLExporter`, which need a `THREE.Object3D`
 * and a DOM-shaped runtime; the MCP server is plain Node. The buffers
 * (`getPositions` / `getNormals` / `getIndices`) carry exactly what those
 * exporters would have read, so this is the same geometry by a shorter route,
 * not a second source of truth.
 *
 * Coordinates are millimetres and Z-up (LOCKED #43). None of the three formats
 * declares a unit or an axis, so nothing is converted — a consumer reading these
 * gets what the engine has.
 */

/** What every writer here needs: a vertex array and triangle indices into it. */
export interface Triangles {
  /** xyz triples, length = 3 × vertexCount. */
  positions: Float32Array;
  /** xyz triples, one per vertex, parallel to `positions`. May be empty. */
  normals: Float32Array;
  /** Vertex indices, three per triangle. */
  indices: Uint32Array;
}

export interface MeshStats {
  vertices: number;
  triangles: number;
}

export function statsOf(t: Triangles): MeshStats {
  return { vertices: t.positions.length / 3, triangles: t.indices.length / 3 };
}

/** The face normal, for formats that store one per facet. Zero if degenerate. */
function facetNormal(
  p: Float32Array,
  a: number,
  b: number,
  c: number,
): [number, number, number] {
  const ux = p[b * 3] - p[a * 3];
  const uy = p[b * 3 + 1] - p[a * 3 + 1];
  const uz = p[b * 3 + 2] - p[a * 3 + 2];
  const vx = p[c * 3] - p[a * 3];
  const vy = p[c * 3 + 1] - p[a * 3 + 1];
  const vz = p[c * 3 + 2] - p[a * 3 + 2];
  const nx = uy * vz - uz * vy;
  const ny = uz * vx - ux * vz;
  const nz = ux * vy - uy * vx;
  const len = Math.hypot(nx, ny, nz);
  // ⚠ A zero here is a degenerate triangle, not an error: the mesh is allowed
  // to hold one (LOCKED #100 — creation is lenient, the verifier is what
  // detects). STL wants three floats regardless, and 0,0,0 is what a reader
  // treats as "recompute it yourself".
  return len > 0 ? [nx / len, ny / len, nz / len] : [0, 0, 0];
}

/** Six significant figures: below the 0.15 μm the mesh dedups at, and short. */
function f(n: number): string {
  return Number.isFinite(n) ? String(Number(n.toFixed(6))) : '0';
}

/**
 * Wavefront OBJ.
 *
 * One `o` group for the whole scene — the buffers arrive already flattened, and
 * the engine's face identity does not survive tessellation in a form OBJ could
 * carry. Normals are written when the engine supplied them, and the `f` lines
 * then use `v//vn`.
 */
export function toObj(t: Triangles, name = 'axia'): string {
  const { positions: p, normals: n, indices: ix } = t;
  const hasNormals = n.length === p.length && n.length > 0;
  const out: string[] = [
    '# AxiA 3D — Wavefront OBJ',
    '# millimetres, Z-up',
    `o ${name}`,
  ];
  for (let i = 0; i < p.length; i += 3) {
    out.push(`v ${f(p[i])} ${f(p[i + 1])} ${f(p[i + 2])}`);
  }
  if (hasNormals) {
    for (let i = 0; i < n.length; i += 3) {
      out.push(`vn ${f(n[i])} ${f(n[i + 1])} ${f(n[i + 2])}`);
    }
  }
  for (let i = 0; i < ix.length; i += 3) {
    // OBJ indices are 1-based.
    const a = ix[i] + 1;
    const b = ix[i + 1] + 1;
    const c = ix[i + 2] + 1;
    out.push(hasNormals ? `f ${a}//${a} ${b}//${b} ${c}//${c}` : `f ${a} ${b} ${c}`);
  }
  return out.join('\n') + '\n';
}

/**
 * Binary STL — 84-byte header plus 50 bytes a facet.
 *
 * Binary rather than ASCII because STL is carried here as base64 either way, and
 * binary is about a fifth the size for the same triangles. The 80-byte header
 * must not begin with "solid": some readers sniff that word and then parse the
 * whole file as ASCII.
 */
export function toStlBinary(t: Triangles, name = 'axia'): Uint8Array {
  const { positions: p, indices: ix } = t;
  const tris = ix.length / 3;
  const buf = new ArrayBuffer(84 + tris * 50);
  const view = new DataView(buf);
  // The 80 bytes are free text and the only place a binary STL can carry a
  // name at all, so the caller's goes here rather than being dropped.
  const header = `AxiA 3D binary STL - ${name} - millimetres, Z-up`;
  for (let i = 0; i < header.length && i < 80; i++) {
    view.setUint8(i, header.charCodeAt(i));
  }
  view.setUint32(80, tris, true);
  let o = 84;
  for (let i = 0; i < ix.length; i += 3) {
    const a = ix[i];
    const b = ix[i + 1];
    const c = ix[i + 2];
    const [nx, ny, nz] = facetNormal(p, a, b, c);
    view.setFloat32(o, nx, true);
    view.setFloat32(o + 4, ny, true);
    view.setFloat32(o + 8, nz, true);
    o += 12;
    for (const v of [a, b, c]) {
      view.setFloat32(o, p[v * 3], true);
      view.setFloat32(o + 4, p[v * 3 + 1], true);
      view.setFloat32(o + 8, p[v * 3 + 2], true);
      o += 12;
    }
    view.setUint16(o, 0, true); // attribute byte count
    o += 2;
  }
  return new Uint8Array(buf);
}

/**
 * STEP AP214 — a faceted `MANIFOLD_SOLID_BREP`, one planar face per triangle.
 *
 * ⚠ AP214 (`AUTOMOTIVE_DESIGN`), not AP203 (`CONFIG_CONTROL_DESIGN`), because
 * that is the schema `FILE_SCHEMA` declares below and the one every CAD
 * package reads. Naming it AP203 in a comment while writing AP214 in the
 * header is how a file ends up trusted for the wrong reason.
 *
 * ⚠ ADR-035 P20.B lists export among the Stage 4 non-goals, so nothing in the
 * engine writes STEP; this is the first. It is written here, in the MCP server,
 * because that is where the capability was declared and because the geometry it
 * needs is triangles — the same ones OBJ and STL take. If STEP export later
 * grows a home in the engine (analytic surfaces rather than facets, assemblies,
 * units), this becomes the caller of it.
 *
 * ⚠ Facets, not surfaces. A cylinder leaves here as its triangles, not as a
 * `CYLINDRICAL_SURFACE`, so a CAD package will read a faceted solid and not a
 * parametric one. That is what AP203 faceted BREP is for and it round-trips as
 * a solid; it is not what ADR-036 P21 means by precision-first promotion.
 *
 * The Part 21 syntax here is written directly rather than through
 * `axia-ifc`'s `StepWriter`: that encoder is Rust and this server is Node, and
 * the entity vocabulary is different anyway (AP214 `CARTESIAN_POINT` and
 * `CLOSED_SHELL` against IFC's `IFCCARTESIANPOINT`). The file syntax — ISO
 * 10303-21 — is the same, which is why IFC export and this one look alike.
 */
export function toStep(t: Triangles, name = 'axia'): string {
  const { positions: p, indices: ix } = t;
  const L: string[] = [];
  let id = 0;
  const next = () => ++id;
  const emit = (n: number, body: string) => {
    L.push(`#${n}=${body};`);
  };

  // Every vertex once, so shared corners stay shared in the file too.
  const pointId: number[] = [];
  const vertexId: number[] = [];
  for (let i = 0; i < p.length; i += 3) {
    const pt = next();
    emit(pt, `CARTESIAN_POINT('',(${f(p[i])},${f(p[i + 1])},${f(p[i + 2])}))`);
    pointId.push(pt);
    const vx = next();
    emit(vx, `VERTEX_POINT('',#${pt})`);
    vertexId.push(vx);
  }

  const faceIds: number[] = [];
  for (let i = 0; i < ix.length; i += 3) {
    const tri = [ix[i], ix[i + 1], ix[i + 2]];
    const [nx, ny, nz] = facetNormal(p, tri[0], tri[1], tri[2]);
    if (nx === 0 && ny === 0 && nz === 0) {
      // A degenerate triangle has no plane to sit on. Dropping it keeps the
      // file valid; keeping it would emit a DIRECTION of zero length, which is
      // out of range for AP203 and rejected by every reader.
      continue;
    }
    // The three edges, each with its own curve-less EDGE_CURVE.
    const edges: number[] = [];
    for (let k = 0; k < 3; k++) {
      const a = tri[k];
      const b = tri[(k + 1) % 3];
      const dirId = next();
      const dx = p[b * 3] - p[a * 3];
      const dy = p[b * 3 + 1] - p[a * 3 + 1];
      const dz = p[b * 3 + 2] - p[a * 3 + 2];
      const dl = Math.hypot(dx, dy, dz) || 1;
      emit(dirId, `DIRECTION('',(${f(dx / dl)},${f(dy / dl)},${f(dz / dl)}))`);
      const vecId = next();
      emit(vecId, `VECTOR('',#${dirId},1.)`);
      const lineId = next();
      emit(lineId, `LINE('',#${pointId[a]},#${vecId})`);
      const ecId = next();
      emit(ecId, `EDGE_CURVE('',#${vertexId[a]},#${vertexId[b]},#${lineId},.T.)`);
      const oeId = next();
      emit(oeId, `ORIENTED_EDGE('',*,*,#${ecId},.T.)`);
      edges.push(oeId);
    }
    const loopId = next();
    emit(loopId, `EDGE_LOOP('',(${edges.map((e) => `#${e}`).join(',')}))`);
    const boundId = next();
    emit(boundId, `FACE_OUTER_BOUND('',#${loopId},.T.)`);

    // The face's plane: origin at the first corner, axis along the normal.
    const orgId = next();
    emit(orgId, `CARTESIAN_POINT('',(${f(p[tri[0] * 3])},${f(p[tri[0] * 3 + 1])},${f(p[tri[0] * 3 + 2])}))`);
    const axisId = next();
    emit(axisId, `DIRECTION('',(${f(nx)},${f(ny)},${f(nz)}))`);
    // A reference direction perpendicular to the normal — the first triangle
    // edge is in the plane by construction, so it always is.
    const rx = p[tri[1] * 3] - p[tri[0] * 3];
    const ry = p[tri[1] * 3 + 1] - p[tri[0] * 3 + 1];
    const rz = p[tri[1] * 3 + 2] - p[tri[0] * 3 + 2];
    const rl = Math.hypot(rx, ry, rz) || 1;
    const refId = next();
    emit(refId, `DIRECTION('',(${f(rx / rl)},${f(ry / rl)},${f(rz / rl)}))`);
    const placeId = next();
    emit(placeId, `AXIS2_PLACEMENT_3D('',#${orgId},#${axisId},#${refId})`);
    const planeId = next();
    emit(planeId, `PLANE('',#${placeId})`);
    const faceId = next();
    emit(faceId, `ADVANCED_FACE('',(#${boundId}),#${planeId},.T.)`);
    faceIds.push(faceId);
  }

  const shellId = next();
  emit(shellId, `CLOSED_SHELL('',(${faceIds.map((x) => `#${x}`).join(',')}))`);
  const brepId = next();
  emit(brepId, `MANIFOLD_SOLID_BREP('${name}',#${shellId})`);

  // The context an AP203 file needs before a shape means anything: a unit
  // system (millimetres, per LOCKED #43) and a tolerance.
  const o = next();
  emit(o, "CARTESIAN_POINT('',(0.,0.,0.))");
  const z = next();
  emit(z, "DIRECTION('',(0.,0.,1.))");
  const x = next();
  emit(x, "DIRECTION('',(1.,0.,0.))");
  const wcs = next();
  emit(wcs, `AXIS2_PLACEMENT_3D('',#${o},#${z},#${x})`);
  const lenUnit = next();
  emit(lenUnit, '(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))');
  const angUnit = next();
  emit(angUnit, '(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))');
  const solidUnit = next();
  emit(solidUnit, '(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())');
  const tolVal = next();
  emit(tolVal, `UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),#${lenUnit},'closure','1e-4 mm — the mesh weld tolerance')`);
  const ctx = next();
  emit(
    ctx,
    `(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#${tolVal}))` +
      `GLOBAL_UNIT_ASSIGNED_CONTEXT((#${lenUnit},#${angUnit},#${solidUnit}))REPRESENTATION_CONTEXT('',''))`,
  );
  const shapeRep = next();
  emit(shapeRep, `ADVANCED_BREP_SHAPE_REPRESENTATION('${name}',(#${wcs},#${brepId}),#${ctx})`);

  // The product structure AP214 wants before a shape means anything. The
  // order matters only through the references; the ids are allocated in the
  // order a reader meets them.
  const appCtx = next();
  emit(appCtx, "APPLICATION_CONTEXT('automotive design')");
  const ap = next();
  emit(
    ap,
    "APPLICATION_PROTOCOL_DEFINITION('international standard'," +
      `'automotive_design',2000,#${appCtx})`,
  );
  const prodCtx = next();
  emit(prodCtx, `PRODUCT_CONTEXT('',#${appCtx},'mechanical')`);
  const product = next();
  emit(product, `PRODUCT('${name}','${name}','',(#${prodCtx}))`);
  const pdf = next();
  emit(pdf, `PRODUCT_DEFINITION_FORMATION('','',#${product})`);
  const pdCtx = next();
  emit(pdCtx, `PRODUCT_DEFINITION_CONTEXT('part definition',#${appCtx},'design')`);
  const pd = next();
  emit(pd, `PRODUCT_DEFINITION('design','',#${pdf},#${pdCtx})`);
  const pds = next();
  emit(pds, `PRODUCT_DEFINITION_SHAPE('','',#${pd})`);
  const sdr = next();
  emit(sdr, `SHAPE_DEFINITION_REPRESENTATION(#${pds},#${shapeRep})`);
  void sdr;
  void ap;

  const stamp = '1970-01-01T00:00:00';
  return [
    'ISO-10303-21;',
    'HEADER;',
    `FILE_DESCRIPTION((''),'2;1');`,
    `FILE_NAME('${name}.step','${stamp}',(''),(''),'AxiA 3D','AxiA 3D','');`,
    `FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }'));`,
    'ENDSEC;',
    'DATA;',
    ...L,
    'ENDSEC;',
    'END-ISO-10303-21;',
    '',
  ].join('\n');
}
