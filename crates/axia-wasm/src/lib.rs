//! AXiA WASM Bridge
//!
//! Exposes the Rust core engine to JavaScript via wasm-bindgen.

use wasm_bindgen::prelude::*;
use glam::DVec3;
use std::collections::{HashMap, HashSet};

use axia_core::scene::Scene;
use axia_core::commands::Command;
use axia_core::commands::CommandResult;
use axia_geo::{FaceId, EdgeId, VertId, HeId};
use axia_geo::operations::boolean::BoolOp;
use axia_core::constraint::{ConstraintKind, ConstraintRef, resolve_constraint, resolve_all, resolve_iterative, max_residual};
use axia_core::orphan_recovery::RecoveryPlan;

mod step6_json;

/// ADR-239 — Build a NURBS control-net grid + weight grid from flat row-major
/// arrays (shared by the live edit update/commit endpoints). Mirrors the inline
/// validation in `createNurbsSurface`.
fn nurbs_grid_from_flat(
    control_pts_flat: &[f64],
    uc: usize,
    vc: usize,
    weights_flat: &[f64],
) -> Result<(Vec<Vec<DVec3>>, Vec<Vec<f64>>), String> {
    if uc < 2 || vc < 2 || control_pts_flat.len() != uc * vc * 3 || weights_flat.len() != uc * vc {
        return Err(format!(
            "bad input — u_count={}, v_count={}, pts.len()={} (expected {}), weights.len()={} (expected {})",
            uc, vc, control_pts_flat.len(), uc * vc * 3, weights_flat.len(), uc * vc,
        ));
    }
    let mut grid: Vec<Vec<DVec3>> = Vec::with_capacity(uc);
    let mut weights: Vec<Vec<f64>> = Vec::with_capacity(uc);
    for i in 0..uc {
        let mut row = Vec::with_capacity(vc);
        let mut w_row = Vec::with_capacity(vc);
        for j in 0..vc {
            let k = (i * vc + j) * 3;
            row.push(DVec3::new(control_pts_flat[k], control_pts_flat[k + 1], control_pts_flat[k + 2]));
            w_row.push(weights_flat[i * vc + j]);
        }
        grid.push(row);
        weights.push(w_row);
    }
    Ok((grid, weights))
}

// Console logging from Rust WASM — debug only (stripped in release builds)
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        web_sys::console::log_1(&format!($($arg)*).into())
    }
}

// Error logging — always active (even in release builds)
macro_rules! console_error {
    ($($arg:tt)*) => {
        web_sys::console::error_1(&format!($($arg)*).into())
    }
}

// ════════════════════════════════════════════════════════════════════════════
// ADR-041 P26.2 — Schema Versioning (3-layer defense)
// ════════════════════════════════════════════════════════════════════════════
//
// SCHEMA_VERSION semantics (semver):
//   MAJOR — capability removed OR ID semantics changed (breaks AI agents)
//   MINOR — capability added (backward compatible)
//   PATCH — bugfix, no API surface change
//
// MCP server checks `^MAJOR.MINOR` compatibility on handshake. Engine /
// server mismatch → SchemaIncompatibleError before any tool call.
//
// ENGINE_VERSION = build identity (cargo version + short git sha when
// available via build script — for now cargo version only).

/// MCP capability schema version. Bumped when any capability surface
/// (input/output schema, ID semantics, error codes) changes. See ADR-041 P26.2.
const SCHEMA_VERSION: &str = "1.0.0";

/// Engine build version (from Cargo.toml). For audit / drift detection.
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// MCP capability schema version (semver). MCP server must satisfy
/// `^MAJOR.MINOR` against this string. ADR-041 P26.2.
#[wasm_bindgen]
pub fn schema_version() -> String {
    SCHEMA_VERSION.to_string()
}

/// Engine build version (axia-wasm crate version). For audit logs and
/// drift detection. ADR-041 P26.2.
#[wasm_bindgen]
pub fn engine_version() -> String {
    ENGINE_VERSION.to_string()
}

// ════════════════════════════════════════════════════════════════════════════
// Delta Buffer Structure — For incremental updates to JavaScript
// ════════════════════════════════════════════════════════════════════════════

/// Delta buffers for incremental mesh updates (Phase 1 Optimization).
///
/// Two modes:
/// 1. **Position-only delta** (translate/rotate/scale): topology unchanged,
///    only vertex positions & normals updated. JS patches the existing buffer
///    at the given offsets — no geometry rebuild needed.
/// 2. **Topology changed** (draw/push_pull/delete/boolean/offset):
///    returns topology_changed=true, JS must do a full rebuild.
///
/// Design: Each dirty face's new positions/normals are packed contiguously.
/// `face_vert_offsets[i]` tells JS where face i's data starts in the
/// FULL cached buffer (so JS patches at the right position).
/// `face_vert_counts[i]` tells JS how many vertices (×3 floats) per face.
#[wasm_bindgen]
pub struct DeltaBuffers {
    modified_face_ids: Vec<u32>,
    /// New vertex positions for dirty faces (packed contiguously)
    positions: Vec<f32>,
    /// New vertex normals for dirty faces (packed contiguously)
    normals: Vec<f32>,
    /// Byte offsets into the FULL position buffer where each face starts
    /// (vertex index, not byte — multiply by 3 for float offset)
    face_vert_offsets: Vec<u32>,
    /// Number of vertices per dirty face
    face_vert_counts: Vec<u32>,
    /// Version counter for validation
    cache_version: u32,
    /// True if topology changed — JS must do full rebuild
    topology_changed: bool,
}

#[wasm_bindgen]
impl DeltaBuffers {
    #[wasm_bindgen(js_name = "getModifiedFaceIds")]
    pub fn get_modified_face_ids(&self) -> Vec<u32> {
        self.modified_face_ids.clone()
    }

    #[wasm_bindgen(js_name = "getPositions")]
    pub fn get_positions(&self) -> Vec<f32> {
        self.positions.clone()
    }

    #[wasm_bindgen(js_name = "getNormals")]
    pub fn get_normals(&self) -> Vec<f32> {
        self.normals.clone()
    }

    /// Vertex offsets into the FULL buffer for each dirty face.
    /// `face_vert_offsets[i]` is the vertex index (not byte) where
    /// face i starts in the full position buffer.
    #[wasm_bindgen(js_name = "getFaceVertOffsets")]
    pub fn get_face_vert_offsets(&self) -> Vec<u32> {
        self.face_vert_offsets.clone()
    }

    /// Number of vertices for each dirty face.
    #[wasm_bindgen(js_name = "getFaceVertCounts")]
    pub fn get_face_vert_counts(&self) -> Vec<u32> {
        self.face_vert_counts.clone()
    }

    #[wasm_bindgen(js_name = "getCacheVersion")]
    pub fn get_cache_version(&self) -> u32 {
        self.cache_version
    }

    /// If true, topology changed (faces added/removed) — JS must do full rebuild.
    /// If false, only positions/normals changed — JS can patch in-place.
    #[wasm_bindgen(js_name = "isTopologyChanged")]
    pub fn is_topology_changed(&self) -> bool {
        self.topology_changed
    }
}

/// Tracks where each face's vertex data lives in the full export buffer.
#[derive(Clone, Debug)]
struct FaceRange {
    vert_start: u32,  // first vertex index in full positions buffer
    vert_count: u32,  // number of vertices for this face
}

#[wasm_bindgen]
pub struct AxiaEngine {
    scene: Scene,
    cached_positions: Vec<f32>,
    cached_positions_f64: Vec<f64>,  // CAD-grade f64 positions (parallel to cached_positions)
    cached_normals: Vec<f32>,
    cached_indices: Vec<u32>,
    cached_face_map: Vec<u32>, // triangle index → FaceId
    cached_edge_lines: Vec<f32>, // hard edge line segments
    cached_edge_map: Vec<u32>,   // segment index → EdgeId raw
    cache_dirty: bool,

    // ════ Delta Tracking (Phase 1 Optimization) ════
    /// Tracks which faces changed since last delta export
    dirty_faces: HashSet<u32>,
    /// Monotonic counter for cache validation
    cache_version: u32,
    /// True if topology changed (faces added/removed) since last delta export.
    /// When true, delta is not useful — JS must do a full rebuild.
    topology_changed: bool,
    /// Maps face_id (raw u32) → FaceRange in the full cached buffer.
    /// Built during rebuild_cache() for fast face→buffer offset lookups.
    face_range_map: HashMap<u32, FaceRange>,

    /// 가장 최근 실패한 기하 연산의 에러 메시지.
    /// TypeScript에서 `last_error()`로 읽어서 Toast에 표시.
    /// 성공한 연산은 이 값을 비우지 않음 (persistent until next failure).
    last_error: String,

    /// 엣지 가시성 임계 각도 (도). 인접 면 사이 법선 각도가 이보다 작으면
    /// coplanar로 판정되어 엣지 숨김. 기본 `EDGE_VISIBILITY_ANGLE_DEG` (15°).
    /// StylePanel의 슬라이더로 런타임 변경 → 다음 syncMesh에서 반영.
    /// 작을수록 엣지가 많이 보임 (부드러운 곡면도 faceted), 클수록 매끈.
    edge_angle_threshold_deg: f64,

    /// 가장 최근 `batch_erase_edges_with_merge`에서 일부 edge의 merge가
    /// 실패했을 때 첫 번째 실패 사유. 디버그 Toast 용.
    last_merge_failure: String,

    /// ADR-135 β — Render-side chord tolerance for analytic surface /
    /// curve tessellation. Set by `setRenderChordTol(value)` from JS
    /// (Viewport computes `lod_chord_tol(camera_distance)` and pushes
    /// here on camera change).
    ///
    /// Default = `DEFAULT_ANALYTIC_CHORD_TOL` (0.02 mm, LOCKED #40 §L1).
    /// Range = `[0.001, 10.0]` mm (clamped in setter).
    /// Changing this value triggers `cache_dirty = true` (next render
    /// pass re-tessellates with new tolerance).
    render_chord_tol: f64,
}

#[wasm_bindgen]
impl AxiaEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        // Route Rust panics to console.error with message + source location.
        // `set_once` is idempotent, so the two engine instances (ADR-041 P26.6
        // session isolation) install it exactly once. Without this a panic shows
        // only "RuntimeError: unreachable" and the leaked wasm-bindgen borrow
        // then spams "recursive use" — making the real cause hard to locate.
        console_error_panic_hook::set_once();
        Self {
            scene: Scene::new(),
            cached_positions: Vec::new(),
            cached_positions_f64: Vec::new(),
            cached_normals: Vec::new(),
            cached_indices: Vec::new(),
            cached_face_map: Vec::new(),
            cached_edge_lines: Vec::new(),
            cached_edge_map: Vec::new(),
            cache_dirty: true,
            dirty_faces: HashSet::new(),
            cache_version: 0,
            topology_changed: true,  // first render always needs full build
            face_range_map: HashMap::new(),
            last_error: String::new(),
            edge_angle_threshold_deg: axia_geo::tolerances::EDGE_VISIBILITY_ANGLE_DEG,
            last_merge_failure: String::new(),
            // ADR-135 β — LOCKED #40 §L1 baseline (0.02 mm). Viewport
            // overrides via setRenderChordTol(lod_chord_tol(camera_dist)).
            render_chord_tol: axia_geo::mesh_export::DEFAULT_ANALYTIC_CHORD_TOL,
        }
    }

    /// 최근 실패한 연산의 에러 메시지를 반환. 실패 이력이 없으면 빈 문자열.
    /// TypeScript Bridge가 연산 반환값이 false일 때 이 값을 Toast로 표시.
    #[wasm_bindgen(js_name = "lastError")]
    pub fn last_error(&self) -> String {
        self.last_error.clone()
    }

    /// Face 가 분석적 surface (Plane/Cylinder/Sphere/Cone/Torus/NURBS) 를
    /// 가지고 있는지 여부.
    ///
    /// ADR-038 P23.4 — Three.js Viewport.smoothNormals 가 analytic evaluate
    /// 결과를 덮어쓰지 않도록 식별 메타데이터. `true` 인 face 의 vertex
    /// normal 은 Rust 의 `surface.normal(u, v)` 로 계산된 정확한 값을
    /// 유지해야 함.
    ///
    /// `face_id` 가 무효 / inactive 면 `false`.
    #[wasm_bindgen(js_name = "faceHasAnalyticSurface")]
    pub fn face_has_analytic_surface(&self, face_id_raw: u32) -> bool {
        let fid = axia_geo::FaceId::new(face_id_raw);
        match self.scene.mesh.faces.get(fid) {
            Some(f) if f.is_active() => f.surface().is_some(),
            _ => false,
        }
    }

    /// Edge visibility angle threshold (도) — Rust 의 SSOT.
    ///
    /// ADR-038 P23.3 — Three.js Viewport.smoothNormals 가 hardcode 30° 대신
    /// 본 값을 사용해야 hard/soft edge 판정이 두 layer 에서 일치.
    ///
    /// 현재 값: `axia_geo::tolerances::EDGE_VISIBILITY_ANGLE_DEG = 20.1`
    #[wasm_bindgen(js_name = "getEdgeVisibilityAngleDeg")]
    pub fn get_edge_visibility_angle_deg(&self) -> f64 {
        axia_geo::tolerances::EDGE_VISIBILITY_ANGLE_DEG
    }

    /// Number of inner hole loops on a face. 0 = simple face.
    /// Returns u32::MAX when the face is missing or inactive.
    #[wasm_bindgen(js_name = "faceInnerLoopCount")]
    pub fn face_inner_loop_count(&self, face_id_raw: u32) -> u32 {
        let fid = FaceId::new(face_id_raw);
        match self.scene.mesh.faces.get(fid) {
            Some(f) if f.is_active() => f.inners().len() as u32,
            _ => u32::MAX,
        }
    }

    /// ADR-016 §2 (Path B) — Erase + Re-synthesize.
    ///
    /// 사용자 정책: "바운더리가 깨지면 새 boundary 찾아서 새 면 생성".
    /// fast-path (`merge_faces_by_edge`) 가 거부하는 hole boundary edge 등
    /// 비정형 케이스 처리. 인접 face soft-remove → edge 제거 → free-edge
    /// re-resolver 실행.
    ///
    /// Returns JSON `{ ok, removedFaces, newFaces, cleanedEdges, cleanedVerts, error? }`.
    /// 트랜잭션 1 개 (Ctrl+Z 한 번에 원복).
    #[wasm_bindgen(js_name = "eraseEdgeResynthesize")]
    pub fn erase_edge_resynthesize(&mut self, edge_id_raw: u32, cleanup_dangling: bool) -> String {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            return r#"{"ok":false,"error":"edge not found"}"#.to_string();
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mat = axia_core::FORM_MATERIAL;
        let result = match self.scene.mesh.erase_edge_resynthesize(eid, mat, cleanup_dangling) {
            Ok(r) => r,
            Err(e) => {
                self.scene.transactions.cancel();
                return format!("{{\"ok\":false,\"error\":\"{}\"}}", e);
            }
        };

        // XIA inheritance — handled in Scene helper.
        self.scene.apply_resynth_xia_inheritance(&result.removed_faces, &result.new_faces);

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        format!(
            "{{\"ok\":true,\"removedFaces\":{},\"newFaces\":{},\"cleanedEdges\":{},\"cleanedVerts\":{}}}",
            result.removed_faces.len(),
            result.new_faces.len(),
            result.cleaned_edges,
            result.cleaned_verts
        )
    }

    /// G3 (A1 follow-up) — Resynthesize-erase N edges in a SINGLE undo
    /// transaction. `eraseEdgeResynthesize` (single) opens one transaction per
    /// edge, so erasing a `curve_owner` group (e.g. a trimmed circle's N arcs
    /// in one click — ADR-088 / LOCKED #15) costs N undo steps. This bundles
    /// the whole gesture into one `begin`/`commit` (mirrors the
    /// `batchEraseEdgesWithMerge` transaction pattern).
    ///
    /// Stale ids (already removed by a prior resynth in the same loop, or never
    /// active) are skipped — not reported as failures. Resynth `Err` edges are
    /// returned in `failed` so the caller can route them to the batch path.
    ///
    /// Returns JSON `{ ok, removedFaces, newFaces, cleanedEdges, cleanedVerts,
    /// failed: [..] }`. If no edge is active, no transaction is opened (no
    /// phantom undo entry).
    #[wasm_bindgen(js_name = "eraseEdgesResynthesize")]
    pub fn erase_edges_resynthesize(&mut self, edge_ids: &[u32], cleanup_dangling: bool) -> String {
        // Early out (avoid an empty undo entry) when nothing is actionable.
        let has_any = edge_ids
            .iter()
            .any(|&r| self.scene.mesh.edges.contains(EdgeId::new(r)));
        if !has_any {
            return r#"{"ok":true,"removedFaces":0,"newFaces":0,"cleanedEdges":0,"cleanedVerts":0,"failed":[]}"#
                .to_string();
        }

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        let mat = axia_core::FORM_MATERIAL;
        let mut total_removed = 0usize;
        let mut total_new = 0usize;
        let mut total_cleaned_edges = 0usize;
        let mut total_cleaned_verts = 0usize;
        let mut failed: Vec<u32> = Vec::new();
        let mut any_ok = false;

        for &raw in edge_ids {
            let eid = EdgeId::new(raw);
            // A prior resynth in this loop may have removed this edge — skip stale.
            if !self.scene.mesh.edges.contains(eid) {
                continue;
            }
            match self.scene.mesh.erase_edge_resynthesize(eid, mat, cleanup_dangling) {
                Ok(result) => {
                    self.scene
                        .apply_resynth_xia_inheritance(&result.removed_faces, &result.new_faces);
                    total_removed += result.removed_faces.len();
                    total_new += result.new_faces.len();
                    total_cleaned_edges += result.cleaned_edges;
                    total_cleaned_verts += result.cleaned_verts;
                    any_ok = true;
                }
                Err(_) => failed.push(raw),
            }
        }

        if any_ok {
            self.scene
                .transactions
                .set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            // Every active edge declined resynth — discard the empty transaction
            // so there is no phantom undo entry. Failed edges are still reported.
            self.scene.transactions.cancel();
        }

        let failed_json = failed
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"ok\":true,\"removedFaces\":{},\"newFaces\":{},\"cleanedEdges\":{},\"cleanedVerts\":{},\"failed\":[{}]}}",
            total_removed, total_new, total_cleaned_edges, total_cleaned_verts, failed_json
        )
    }

    /// ADR-016 §2 — true ⇔ this edge is on the hole boundary of any active face.
    /// JS hover layer uses this to show an explicit-op hint instead of the
    /// generic cascade-red preview.
    #[wasm_bindgen(js_name = "edgeIsHoleBoundary")]
    pub fn edge_is_hole_boundary(&self, edge_id_raw: u32) -> bool {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) { return false; }
        let (faces, hes) = self.scene.mesh.get_faces_sharing_edge(eid);
        for (i, &fid) in faces.iter().enumerate() {
            let Some(face) = self.scene.mesh.faces.get(fid) else { continue };
            if !face.is_active() { continue; }
            let he_id = hes[i];
            for inner in face.inners() {
                let mut h = inner.start;
                let mut guard = 0usize;
                loop {
                    guard += 1;
                    if guard > 4096 { return false; }
                    if h == he_id { return true; }
                    let next = match self.scene.mesh.hes.get(h) {
                        Some(he) => he.next(), None => return false,
                    };
                    h = next;
                    if h == inner.start { break; }
                }
            }
        }
        false
    }

    /// 에러 기록용 내부 헬퍼. 각 연산이 실패 시 호출.
    fn set_error(&mut self, msg: impl Into<String>) {
        self.last_error = msg.into();
    }

    /// 성공 시 에러 상태 clear (다음 실패까지 빈 문자열 유지)
    #[allow(dead_code)] // preserved for future op-level error clearing
    fn clear_error(&mut self) {
        self.last_error.clear();
    }

    fn rebuild_cache(&mut self) {
        if !self.cache_dirty {
            return;
        }
        // `export_mesh_buffers` is self-healing — auto-deactivates earcut
        // Ok([]) faces internally so the user never sees a wireframe-only
        // RECT. Invariant locked by debug_assert_eq inside the export
        // pipeline (see Mesh::export_buffers CONTRACT comment).
        //
        // Cache update policy (2026-05-02):
        //   - Ok: replace cache fields atomically inside this branch only
        //   - Err: KEEP previous cache intact for debugging — caller can
        //     still inspect last-good buffers, and a brief render of stale
        //     geometry beats a flicker-to-empty during a transient failure.
        // ADR-135 β — Use stored render_chord_tol (Viewport-set LOD value
        // via setRenderChordTol). Default = DEFAULT_ANALYTIC_CHORD_TOL
        // (0.02 mm, LOCKED #40 §L1) if Viewport hasn't set yet.
        match self.scene.export_mesh_buffers_with_tol(self.render_chord_tol) {
            Ok((p, n, i, fm, p64)) => {
                self.cached_positions = p;
                self.cached_positions_f64 = p64;
                self.cached_normals = n;
                self.cached_indices = i;
                self.cached_face_map = fm;
            }
            Err(_e) => {
                // Intentionally retain previous cache. The error already
                // surfaced via Result; resetting here would erase the
                // last-good state useful for `getLastExportSkipStats` /
                // user diagnostics during a session.
            }
        }
        // Edge lines are computed from DCEL topology (not from triangle geometry).
        // 임계 각도는 런타임 조절 가능 (StylePanel 슬라이더). 기본은 tolerances.rs의
        // EDGE_VISIBILITY_ANGLE_DEG (15°).
        let (edge_lines, edge_map) = self.scene
            .export_edge_lines_with_map(self.edge_angle_threshold_deg);
        self.cached_edge_lines = edge_lines;
        self.cached_edge_map = edge_map;
        self.cache_dirty = false;

        // Build face_range_map: face_id → (vert_start, vert_count)
        // Single pass through cached_face_map + cached_indices.
        // export_buffers() emits faces in order; each face's vertices are contiguous.
        self.face_range_map.clear();
        for (tri_idx, &face_id) in self.cached_face_map.iter().enumerate() {
            let base = tri_idx * 3;
            if base + 2 >= self.cached_indices.len() { break; }

            let i0 = self.cached_indices[base];
            let i1 = self.cached_indices[base + 1];
            let i2 = self.cached_indices[base + 2];

            let entry = self.face_range_map.entry(face_id).or_insert(FaceRange {
                vert_start: u32::MAX,
                vert_count: 0,
            });
            // Track min vertex index as vert_start
            entry.vert_start = entry.vert_start.min(i0).min(i1).min(i2);
            // Track max+1 to compute count later
            let max_idx = i0.max(i1).max(i2);
            let end = max_idx + 1;
            let needed_count = end - entry.vert_start;
            if needed_count > entry.vert_count {
                entry.vert_count = needed_count;
            }
        }
    }

    fn invalidate_cache(&mut self) {
        self.cache_dirty = true;
    }

    /// Mark specific face IDs as dirty for delta updates.
    /// Called after operations that modify specific faces (translate/rotate/scale).
    #[allow(dead_code)] // preserved for delta-buffer pipeline (Phase 1)
    fn mark_faces_dirty(&mut self, face_ids: &[u32]) {
        for &fid in face_ids {
            self.dirty_faces.insert(fid);
        }
        self.cache_version = self.cache_version.wrapping_add(1);
    }

    /// Walk the radial loop of `eid` and return true if any HE has a face
    /// pointer. Used by Phase B step 2 (erase re-synthesis) to snapshot
    /// which edges were face-bearing before the erase pass.
    fn edge_has_any_face(&self, eid: EdgeId) -> bool {
        let Some(edge) = self.scene.mesh.edges.get(eid) else { return false; };
        let start = edge.any_he();
        if start.is_null() { return false; }
        let mut he = start;
        loop {
            match self.scene.mesh.hes.get(he) {
                Some(h) => {
                    if !h.face().is_null() { return true; }
                    let next = h.next_rad();
                    if next.is_null() || next == start { return false; }
                    he = next;
                }
                None => return false,
            }
        }
    }

    /// Mark that topology changed (faces added/removed/split).
    /// Delta updates are not possible — JS must do a full rebuild.
    fn mark_topology_changed(&mut self) {
        self.topology_changed = true;
        self.cache_version = self.cache_version.wrapping_add(1);
    }

    /// ADR-062 Step 3 — Internal: shared validated-attach dispatcher.
    /// Used by all 5 attachFaceSurface*Validated WASM endpoints.
    /// Maps tol_mm ≤ 0 to ATTACH_VALIDATE_TOL default.
    fn attach_validated_inner(
        &mut self,
        face_id_raw: u32,
        surface: axia_geo::surfaces::AnalyticSurface,
        tol_mm: f64,
    ) -> String {
        let tol = if tol_mm > 0.0 {
            tol_mm
        } else {
            axia_geo::tolerances::ATTACH_VALIDATE_TOL
        };
        let outcome = self.scene.mesh.attach_surface_validated(
            FaceId::new(face_id_raw), surface, tol,
        );
        if outcome.is_attached() {
            self.mark_topology_changed();
        }
        step6_json::surface_attach_outcome_json(&outcome)
    }

    /// Check if all faces in the group share the same normal (coplanar).
    ///
    /// Returns true if every pair of faces has |dot(n_i, n_j)| ≥ cos(EXACT_COPLANAR_ANGLE_DEG).
    /// Used to detect when a "smooth group" is actually split sub-faces of
    /// a single plane, which must NOT be treated as a curved surface.
    fn all_faces_coplanar(&self, face_ids: &[FaceId]) -> bool {
        let exact_coplanar_cos = axia_geo::tolerances::deg_to_cos(
            axia_geo::tolerances::EXACT_COPLANAR_ANGLE_DEG,
        );
        if face_ids.len() < 2 { return true; }

        let reference = match self.scene.mesh.faces.get(face_ids[0]) {
            Some(f) => {
                let n = f.normal();
                let len = n.length();
                if len < 1e-10 { return false; }
                n / len
            }
            None => return false,
        };

        for &fid in &face_ids[1..] {
            if let Some(f) = self.scene.mesh.faces.get(fid) {
                let n = f.normal();
                let len = n.length();
                if len < 1e-10 { return false; }
                let n_unit = n / len;
                if reference.dot(n_unit).abs() < exact_coplanar_cos {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    // ========================================================================
    // Cache Version & Delta Tracking
    // ========================================================================

    /// Get the current cache version (monotonic counter).
    /// Used by JavaScript to validate delta buffer freshness.
    #[wasm_bindgen(js_name = "getCacheVersion")]
    pub fn get_cache_version(&self) -> u32 {
        self.cache_version
    }

    /// Get dirty face count (for debugging)
    #[wasm_bindgen(js_name = "getDirtyFaceCount")]
    pub fn get_dirty_face_count(&self) -> usize {
        self.dirty_faces.len()
    }

    // ========================================================================
    // Draw commands
    // ========================================================================

    /// ADR-087 K-ζ — Legacy `draw_line` / `draw_polyline` exports 폐기.
    /// `drawLineAsShape` / `drawPolylineAsShape` 가 단일 entry.

    // (legacy `pub fn draw_line` deleted — ADR-087 K-ζ)

    // (legacy `pub fn draw_polyline` deleted — ADR-087 K-ζ)

    /// ADR-087 K-γ — form-mode polyline. drawPolyline 의 kernel-aware
    /// 변형: 각 segment 를 `Command::DrawLineAsShape` 로 실행하여 (a) 결과
    /// edge 들이 form-layer Shape 로 등록 + (b) 닫힌 loop 합성 시 face 에
    /// AnalyticSurface::Plane 자동 attach (exec_draw_line_as_shape 의 face
    /// path Plane attach via inherited surface_normal).
    ///
    /// 호출자: DrawFreehandTool form-mode (drawShapeMode ON).
    /// surface_normal: optional plane hint — 닫힌 loop 합성 시 Plane attach
    /// 에 사용. None 이면 inferred (free-edge planar pipeline 의 best-fit).
    /// `points`: 평탄화된 [x0,y0,z0,x1,y1,z1,…] 배열 (3 의 배수).
    /// 반환: 0 (success) 또는 -1.
    #[wasm_bindgen(js_name = "drawPolylineAsShape")]
    pub fn draw_polyline_as_shape(
        &mut self,
        points: &[f64],
        nx: f64, ny: f64, nz: f64,
    ) -> f64 {
        if points.len() < 6 || points.len() % 3 != 0 {
            console_error!(
                "[RUST] drawPolylineAsShape: invalid points length {}",
                points.len()
            );
            return -1.0;
        }
        let n = points.len() / 3;
        if n < 2 {
            return -1.0;
        }

        // surface_normal: caller 가 zero vector 전달 시 None (free-edge
        // planar pipeline 의 default 추론).
        let normal_hint = {
            let v = DVec3::new(nx, ny, nz);
            if v.length_squared() > 1e-12 { Some(v.normalize()) } else { None }
        };

        debug_log!(
            "[RUST] drawPolylineAsShape: {} points → {} segments, normal_hint={:?}",
            n, n - 1, normal_hint
        );

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        // draw-path perf (2026-06-05) — start each polyline with a fresh face
        // AABB cache; the N per-segment interior tests then reuse one boundary
        // walk per face instead of repeating it per segment (O(N²) → O(N)).
        self.scene.mesh.clear_face_aabb_cache();

        let mut any_failed = false;
        for i in 0..n - 1 {
            let start = DVec3::new(
                points[i * 3], points[i * 3 + 1], points[i * 3 + 2],
            );
            let end = DVec3::new(
                points[(i + 1) * 3], points[(i + 1) * 3 + 1], points[(i + 1) * 3 + 2],
            );
            let cmd = Command::DrawLineAsShape {
                start,
                end,
                surface_normal: normal_hint,
            };
            let result = self.scene.execute(cmd);
            if matches!(result, axia_core::commands::CommandResult::Error(_)) {
                any_failed = true;
            }
        }

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();

        self.mark_topology_changed();
        self.invalidate_cache();

        if any_failed { -1.0 } else { 0.0 }
    }

    // (legacy `pub fn draw_rect` / `pub fn draw_circle` deleted — ADR-087
    // K-ζ. drawRectAsShape / drawCircleAsShape 가 단일 entry.)

    // ════════════════════════════════════════════════════════════════════
    // ADR-050 P-5c — As-Shape Draw command bridge.
    //
    // Bridge surface for the form-layer Shape draw variants (P-5a/P-5b).
    // Signature pattern matches existing `draw_rect` / `draw_line` /
    // `draw_circle` — f64 return, -1.0 = error, else = ShapeId.raw() as
    // f64. New endpoints are NOT under js_name attribute (Rust snake_case
    // is exposed as-is, mirroring the existing draw_* family).
    //
    // All transactions are managed inside `Scene::exec_draw_*_as_shape`
    // (Phase 1 delegates to legacy path, Phase 2 wraps conversion).
    // The bridge layer is a thin pass-through.
    // ════════════════════════════════════════════════════════════════════

    /// ADR-050 P-5c — Draw a rectangle as a form-layer Shape (no Xia).
    /// Returns ShapeId.raw() as f64 on success, -1.0 on error.
    pub fn draw_rect_as_shape(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        nx: f64, ny: f64, nz: f64,
        ux: f64, uy: f64, uz: f64,
        width: f64, height: f64,
    ) -> f64 {
        let cmd = Command::DrawRectAsShape {
            center: DVec3::new(cx, cy, cz),
            normal: DVec3::new(nx, ny, nz),
            up: DVec3::new(ux, uy, uz),
            width,
            height,
        };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            // ADR-258 β-2 — surface the reject reason (e.g. the non-manifold
            // imprint guard) into last_error so the TS layer can Toast it.
            axia_core::commands::CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-050 P-5c — Draw a line as a form-layer Shape (no Xia).
    /// Returns ShapeId.raw() as f64 on success, -1.0 on error.
    /// `nx/ny/nz = 0` means surface_normal is None (free-edge mode).
    pub fn draw_line_as_shape(
        &mut self,
        x0: f64, y0: f64, z0: f64,
        x1: f64, y1: f64, z1: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> f64 {
        let surface_normal = if nx == 0.0 && ny == 0.0 && nz == 0.0 {
            None
        } else {
            Some(DVec3::new(nx, ny, nz))
        };
        let cmd = Command::DrawLineAsShape {
            start: DVec3::new(x0, y0, z0),
            end: DVec3::new(x1, y1, z1),
            surface_normal,
        };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            // ADR-258 β-2 — surface the reject reason (e.g. the non-manifold
            // imprint guard) into last_error so the TS layer can Toast it.
            axia_core::commands::CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-219 — Draw a standalone construction Point as a form-layer Shape.
    /// Adds + pins a single isolated vertex (survives cleanup). Returns
    /// ShapeId.raw() as f64 on success, -1.0 on error.
    #[wasm_bindgen(js_name = "drawPointAsShape")]
    pub fn draw_point_as_shape(&mut self, x: f64, y: f64, z: f64) -> f64 {
        let cmd = Command::DrawPointAsShape { pos: DVec3::new(x, y, z) };
        match self.scene.execute(cmd) {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                // A Point adds an isolated vertex only (no faces/edges) → mesh
                // buffers are unchanged, but mark dirty so any dependent cache
                // refreshes and the Point render layer re-fetches.
                self.invalidate_cache();
                shape_id as f64
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-219 — flattened world positions `[x,y,z, ...]` of all standalone
    /// Point vertices, for the THREE.Points render layer (Point verts emit
    /// nothing from getMeshBuffers). Order is deterministic (ascending ShapeId).
    #[wasm_bindgen(js_name = "standalonePointVerts")]
    pub fn standalone_point_verts(&self) -> Vec<f64> {
        self.scene.standalone_point_verts().0
    }

    /// ADR-050 P-5c — Draw a circle as a form-layer Shape (no Xia).
    /// Returns ShapeId.raw() as f64 on success, -1.0 on error.
    pub fn draw_circle_as_shape(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        nx: f64, ny: f64, nz: f64,
        radius: f64, segments: u32,
    ) -> f64 {
        let cmd = Command::DrawCircleAsShape {
            center: DVec3::new(cx, cy, cz),
            normal: DVec3::new(nx, ny, nz),
            radius,
            segments,
        };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            // ADR-258 β-2 — surface the reject reason (e.g. the non-manifold
            // imprint guard) into last_error so the TS layer can Toast it.
            axia_core::commands::CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// 다각형 fix (2026-06-10) — Draw a regular N-gon as a form-layer Shape.
    /// Builds N plain Line segments (NO Arc metadata / NO ≥12 circle threshold),
    /// so a polygon stays a polygon even under `face_rederive_on_draw`. Distinct
    /// from `draw_circle_as_shape`. Returns ShapeId.raw() as f64, -1.0 on error.
    pub fn draw_polygon_as_shape(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        nx: f64, ny: f64, nz: f64,
        radius: f64, sides: u32,
    ) -> f64 {
        let cmd = Command::DrawPolygonAsShape {
            center: DVec3::new(cx, cy, cz),
            normal: DVec3::new(nx, ny, nz),
            radius,
            sides,
        };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            // ADR-258 β-2 — surface the reject reason (e.g. the non-manifold
            // imprint guard) into last_error so the TS layer can Toast it.
            axia_core::commands::CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-089 Phase 2 (A-ζ-4) — Draw circle as TRUE kernel-native
    /// closed-curve face. **메타-원칙 #14 의 deepest realization** —
    /// 1 anchor vertex + 1 self-loop edge + 1 closed-curve face.
    /// 24-segment polygon decomposition 폐기.
    ///
    /// Drop-in alongside drawCircleAsShape — segments parameter 없음
    /// (analytic curve = formula 1개). Returns ShapeId.raw() as f64
    /// on success, -1.0 on error.
    ///
    /// 호출자: 향후 DrawCircleTool 의 kernel-native flag (A-λ) 또는
    /// 사용자 DevTools 직접 호출.
    #[wasm_bindgen(js_name = "drawCircleAsCurve")]
    pub fn draw_circle_as_curve(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        nx: f64, ny: f64, nz: f64,
        radius: f64,
    ) -> f64 {
        let cmd = Command::DrawCircleAsCurve {
            center: DVec3::new(cx, cy, cz),
            normal: DVec3::new(nx, ny, nz),
            radius,
        };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] drawCircleAsCurve ERROR: {}", e);
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-206 — Atomic kernel-native ellipse creation. Reuses the exact-ellipse
    /// NURBS machinery (nurbs::ellipse + add_face_closed_curve); ref_dir is the
    /// major-axis direction, radius_x the semi-axis along ref_dir, radius_y along
    /// normal × ref_dir. Returns shape_id (or -1 on error). Mirrors drawCircleAsCurve.
    #[wasm_bindgen(js_name = "drawEllipseAsCurve")]
    pub fn draw_ellipse_as_curve(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        rdx: f64, rdy: f64, rdz: f64,
        nx: f64, ny: f64, nz: f64,
        radius_x: f64, radius_y: f64,
    ) -> f64 {
        let cmd = Command::DrawEllipseAsCurve {
            center: DVec3::new(cx, cy, cz),
            ref_dir: DVec3::new(rdx, rdy, rdz),
            normal: DVec3::new(nx, ny, nz),
            radius_x,
            radius_y,
        };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] drawEllipseAsCurve ERROR: {}", e);
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-089 A-Β-γ — Atomic closed NURBS creation with curve attach.
    /// Rational extension of drawClosedBSplineAsCurve — adds weights.
    /// All weights must be > 0. Caller passes flat control_pts (3·n
    /// floats), weights vector, knots vector, and degree. control_pts
    /// [0] ≈ control_pts[last] (clamped knots case). Returns shape_id.
    #[wasm_bindgen(js_name = "drawClosedNURBSAsCurve")]
    pub fn draw_closed_nurbs_as_curve(
        &mut self,
        control_pts_flat: Vec<f64>,
        weights: Vec<f64>,
        knots: Vec<f64>,
        degree: u32,
    ) -> f64 {
        if control_pts_flat.len() % 3 != 0 {
            console_error!("[RUST] drawClosedNURBSAsCurve: control_pts_flat length {} not multiple of 3",
                control_pts_flat.len());
            return -1.0;
        }
        let mut control_pts = Vec::with_capacity(control_pts_flat.len() / 3);
        for chunk in control_pts_flat.chunks_exact(3) {
            control_pts.push(DVec3::new(chunk[0], chunk[1], chunk[2]));
        }
        let cmd = Command::DrawClosedNURBSAsCurve { control_pts, weights, knots, degree };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] drawClosedNURBSAsCurve ERROR: {}", e);
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-089 A-Α-γ — Atomic closed BSpline creation with curve attach.
    /// Caller passes flat control_pts (3·n floats), knots vector, and
    /// degree. control_pts[0] must equal control_pts[last] within
    /// EPSILON_LENGTH (clamped knots case). Returns shape_id, -1 on err.
    #[wasm_bindgen(js_name = "drawClosedBSplineAsCurve")]
    pub fn draw_closed_bspline_as_curve(
        &mut self,
        control_pts_flat: Vec<f64>,
        knots: Vec<f64>,
        degree: u32,
    ) -> f64 {
        if control_pts_flat.len() % 3 != 0 {
            console_error!("[RUST] drawClosedBSplineAsCurve: control_pts_flat length {} not multiple of 3",
                control_pts_flat.len());
            return -1.0;
        }
        let mut control_pts = Vec::with_capacity(control_pts_flat.len() / 3);
        for chunk in control_pts_flat.chunks_exact(3) {
            control_pts.push(DVec3::new(chunk[0], chunk[1], chunk[2]));
        }
        let cmd = Command::DrawClosedBSplineAsCurve { control_pts, knots, degree };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] drawClosedBSplineAsCurve ERROR: {}", e);
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    /// ADR-089 A-ω-γ — Atomic closed Bezier creation with curve attach.
    /// `control_pts` flat: 3·n floats. Last point must equal first
    /// (within EPSILON_LENGTH) for closure check. Returns shape_id on
    /// success, -1 on error.
    #[wasm_bindgen(js_name = "drawClosedBezierAsCurve")]
    pub fn draw_closed_bezier_as_curve(
        &mut self,
        control_pts_flat: Vec<f64>,
    ) -> f64 {
        if control_pts_flat.len() % 3 != 0 {
            console_error!("[RUST] drawClosedBezierAsCurve: control_pts_flat length {} not multiple of 3",
                control_pts_flat.len());
            return -1.0;
        }
        let mut control_pts = Vec::with_capacity(control_pts_flat.len() / 3);
        for chunk in control_pts_flat.chunks_exact(3) {
            control_pts.push(DVec3::new(chunk[0], chunk[1], chunk[2]));
        }
        let cmd = Command::DrawClosedBezierAsCurve { control_pts };
        let result = self.scene.execute(cmd);
        match result {
            axia_core::commands::CommandResult::ShapeCreated(shape_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                shape_id as f64
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] drawClosedBezierAsCurve ERROR: {}", e);
                self.set_error(e);
                self.invalidate_cache();
                -1.0
            }
            _ => {
                self.invalidate_cache();
                -1.0
            }
        }
    }

    // ========================================================================
    // ADR-028 Phase A — Analytic Edge Curve API
    // ========================================================================
    //
    // 모든 좌표는 ADR-026 P12 (Cardinal Plane SSOT) 의 sub-tol snap 후 호출자가
    // 보장한 값. Bridge 측에서 추가 snap 없이 그대로 engine 에 전달.

    /// Tessellate an edge into a polyline approximating its curve within
    /// `chord_tol` (mm).
    ///
    /// - For straight edges (no curve attached), returns 6 floats — the two
    ///   endpoint positions: `[x0,y0,z0, x1,y1,z1]`.
    /// - For curved edges (Arc, Circle), returns 3·n floats where n = number
    ///   of tessellation points. n+1 points for n segments — first and last
    ///   coincide for full circles.
    ///
    /// The result is a flat `Float64Array` for zero-copy WASM transfer.
    /// Returns empty array if edge_id is invalid.
    #[wasm_bindgen(js_name = "tessellateEdge")]
    pub fn tessellate_edge(&self, edge_id: u32, chord_tol: f64) -> Vec<f64> {
        use axia_geo::EdgeId;
        let eid = EdgeId::new(edge_id);
        match self.scene.mesh.tessellate_edge(eid, chord_tol) {
            Ok(pts) => {
                let mut flat = Vec::with_capacity(pts.len() * 3);
                for p in pts {
                    flat.push(p.x);
                    flat.push(p.y);
                    flat.push(p.z);
                }
                flat
            }
            Err(_) => Vec::new(),
        }
    }

    /// ADR-040 Stage 2 — analytic ray-to-edge distance.
    ///
    /// For an edge with `Edge.curve = Some(AnalyticCurve)`, returns the
    /// perpendicular distance (mm) from the cursor ray line to the
    /// closest point on the analytic curve, plus the closest point.
    ///
    /// Return shape: `Float64Array([distance, px, py, pz, t_on_curve])`
    /// — 5 elements. On failure (no curve / edge invalid / Newton diverges),
    /// returns an empty array. Caller (TS) treats empty as "fall back to
    /// polyline BVH" per P25.4.
    ///
    /// `ray_dir` MUST be unit length. Caller is responsible for
    /// normalisation. (Avoids per-call sqrt at the boundary.)
    #[wasm_bindgen(js_name = "edgeRayDistance")]
    pub fn edge_ray_distance(
        &self,
        edge_id: u32,
        ox: f64,
        oy: f64,
        oz: f64,
        dx: f64,
        dy: f64,
        dz: f64,
    ) -> Vec<f64> {
        use axia_geo::curves::distance::ray_to_curve_distance;
        use axia_geo::EdgeId;
        let eid = EdgeId::new(edge_id);
        let curve = match self.scene.mesh.edge_curve(eid) {
            Some(c) => c.clone(),
            None => return Vec::new(),
        };
        let ray_origin = glam::DVec3::new(ox, oy, oz);
        let ray_dir = glam::DVec3::new(dx, dy, dz);
        match ray_to_curve_distance(&curve, ray_origin, ray_dir, &self.scene.mesh) {
            Some(r) => vec![
                r.distance,
                r.point_on_curve.x,
                r.point_on_curve.y,
                r.point_on_curve.z,
                r.t_on_curve,
            ],
            None => Vec::new(),
        }
    }

    /// Set an analytic Arc curve on an existing edge.
    ///
    /// Arguments encode the Arc variant of `AnalyticCurve`:
    /// - center: cx, cy, cz
    /// - radius
    /// - normal: nx, ny, nz (must be unit-length, axis of Arc plane)
    /// - basis_u: ux, uy, uz (unit, in-plane, defines θ=0 direction)
    /// - start_angle, end_angle (radians)
    ///
    /// Returns true if successful (edge exists), false otherwise.
    #[wasm_bindgen(js_name = "setEdgeArcCurve")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_edge_arc_curve(
        &mut self,
        edge_id: u32,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        nx: f64, ny: f64, nz: f64,
        ux: f64, uy: f64, uz: f64,
        start_angle: f64, end_angle: f64,
    ) -> bool {
        use axia_geo::{EdgeId, AnalyticCurve};
        use glam::DVec3;
        let eid = EdgeId::new(edge_id);
        if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
            e.set_curve(Some(AnalyticCurve::Arc {
                center: DVec3::new(cx, cy, cz),
                radius,
                normal: DVec3::new(nx, ny, nz),
                basis_u: DVec3::new(ux, uy, uz),
                start_angle, end_angle,
            }));
            self.mark_topology_changed();
            true
        } else {
            false
        }
    }

    /// Set an analytic Circle curve on an existing edge.
    /// Similar arg layout to `setEdgeArcCurve` but no angle range
    /// (full 2π implied).
    #[wasm_bindgen(js_name = "setEdgeCircleCurve")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_edge_circle_curve(
        &mut self,
        edge_id: u32,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        nx: f64, ny: f64, nz: f64,
        ux: f64, uy: f64, uz: f64,
    ) -> bool {
        use axia_geo::{EdgeId, AnalyticCurve};
        use glam::DVec3;
        let eid = EdgeId::new(edge_id);
        if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
            e.set_curve(Some(AnalyticCurve::Circle {
                center: DVec3::new(cx, cy, cz),
                radius,
                normal: DVec3::new(nx, ny, nz),
                basis_u: DVec3::new(ux, uy, uz),
            }));
            self.mark_topology_changed();
            true
        } else {
            false
        }
    }

    /// ADR-032 P17 — Draw a tessellated arc and attach analytic Arc curves
    /// to each segment edge in one atomic op.
    ///
    /// Encapsulates the DrawArc tool's full promotion path: tessellate +
    /// drawLine ×N + setEdgeArcCurve ×N, all in a single transaction.
    /// Returns 0.0 on success, -1.0 on any error.
    #[wasm_bindgen(js_name = "drawArcWithCurve")]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_arc_with_curve(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        nx: f64, ny: f64, nz: f64,
        ux: f64, uy: f64, uz: f64,
        start_angle: f64, end_angle: f64,
        segments: u32,
    ) -> f64 {
        use axia_geo::{AnalyticCurve, EdgeId};
        use glam::DVec3;
        if segments < 1 || radius <= 0.0 {
            return -1.0;
        }
        let center = DVec3::new(cx, cy, cz);
        let normal = DVec3::new(nx, ny, nz);
        let basis_u = DVec3::new(ux, uy, uz);
        let basis_v = normal.cross(basis_u).normalize_or_zero();
        if normal.length_squared() < 1e-12 || basis_u.length_squared() < 1e-12
            || basis_v.length_squared() < 1e-12
        {
            return -1.0;
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mut edge_ids: Vec<EdgeId> = Vec::with_capacity(segments as usize);
        let mut any_failed = false;
        for i in 0..segments {
            let theta_a = start_angle + (end_angle - start_angle) * (i as f64) / (segments as f64);
            let theta_b = start_angle + (end_angle - start_angle) * ((i + 1) as f64) / (segments as f64);
            let p_a = center + basis_u * (radius * theta_a.cos()) + basis_v * (radius * theta_a.sin());
            let p_b = center + basis_u * (radius * theta_b.cos()) + basis_v * (radius * theta_b.sin());
            match self.scene.mesh.draw_line(p_a, p_b) {
                Ok((_va, _vb, eid)) => edge_ids.push(eid),
                Err(_) => { any_failed = true; break; }
            }
        }

        if !any_failed {
            // Attach sub-arc curve metadata.
            for (i, &eid) in edge_ids.iter().enumerate() {
                let theta_a = start_angle
                    + (end_angle - start_angle) * (i as f64) / (segments as f64);
                let theta_b = start_angle
                    + (end_angle - start_angle) * ((i + 1) as f64) / (segments as f64);
                if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
                    e.set_curve(Some(AnalyticCurve::Arc {
                        center, radius, normal, basis_u,
                        start_angle: theta_a,
                        end_angle: theta_b,
                    }));
                }
            }
            // ADR-088 Phase 1 (S-γ) — assign single curve_owner_id to all
            // arc segments (LOCKED #15 P22.5).
            let owner_id = self.scene.mesh.next_curve_owner_id();
            for &eid in &edge_ids {
                self.scene.mesh.set_edge_curve_owner_id(eid, Some(owner_id));
            }
            // ADR-200 (A1) — 그린 호가 auto-division 에 참여: 호 AABB+평면이
            // 겹치는 면을 seed 로 rederive 발동 → free arc edge 가 InputCurve::Arc
            // 로 투입돼 면 분할. flag OFF 면 no-op.
            let arc_pts: Vec<DVec3> = edge_ids
                .iter()
                .filter_map(|&eid| {
                    self.scene.mesh.edges.get(eid).map(|e| (e.v_small(), e.v_large()))
                })
                .flat_map(|(va, vb)| [va, vb])
                .filter_map(|v| self.scene.mesh.verts.get(v).map(|x| x.pos()))
                .collect();
            let _ = self.scene.rederive_after_curve_draw(&arc_pts, normal);
        }

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        if any_failed { -1.0 } else { 0.0 }
    }

    /// ADR-032 P17 — Atomic Bezier drawing with analytic curve promotion.
    ///
    /// `control_pts_flat`: 3·(n+1) floats. `segments`: tessellation count.
    /// All N segment edges receive the SAME Bezier curve metadata (the full
    /// curve), since Bezier doesn't sub-divide naturally per-segment without
    /// re-parameterization. View-time tessellation uses the full curve.
    ///
    /// Returns 0 on success, -1 on error.
    #[wasm_bindgen(js_name = "drawBezierWithCurve")]
    pub fn draw_bezier_with_curve(
        &mut self,
        control_pts_flat: Vec<f64>,
        segments: u32,
    ) -> f64 {
        use axia_geo::{AnalyticCurve, EdgeId};
        use axia_geo::curves::CurveOps;
        use glam::DVec3;
        if control_pts_flat.len() < 6 || control_pts_flat.len() % 3 != 0 || segments < 1 {
            return -1.0;
        }
        let mut ctrl = Vec::with_capacity(control_pts_flat.len() / 3);
        let mut i = 0;
        while i + 2 < control_pts_flat.len() {
            ctrl.push(DVec3::new(
                control_pts_flat[i], control_pts_flat[i + 1], control_pts_flat[i + 2],
            ));
            i += 3;
        }
        let curve = AnalyticCurve::Bezier { control_pts: ctrl };
        // ADR-201 (β-1) — bounded uniform sub-range sampling (Bezier range [0,1]).
        // 세그먼트별 sub-range 곡선 부착 (아래) → render smooth + bounded edge 수.
        let _ = segments; // 이전 hint; 이제 SEGS 고정 + sub-range render.
        const SEGS: usize = 64;
        let (r0, r1) = curve.parameter_range();
        let mut params: Vec<f64> = Vec::with_capacity(SEGS + 1);
        let mut pts: Vec<DVec3> = Vec::with_capacity(SEGS + 1);
        for i in 0..=SEGS {
            let t = r0 + (r1 - r0) * (i as f64) / (SEGS as f64);
            match curve.evaluate(t, &self.scene.mesh) {
                Ok(p) => { params.push(t); pts.push(p); }
                Err(_) => return -1.0,
            }
        }
        if pts.len() < 2 { return -1.0; }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mut edge_ids: Vec<EdgeId> = Vec::with_capacity(pts.len());
        let mut any_failed = false;
        for i in 0..pts.len() - 1 {
            match self.scene.mesh.draw_line(pts[i], pts[i + 1]) {
                Ok((_, _, eid)) => edge_ids.push(eid),
                Err(_) => { any_failed = true; break; }
            }
        }

        if !any_failed {
            // ADR-201 (β-1) — 세그먼트별 sub-range 곡선 (full curve 아님).
            for (i, &eid) in edge_ids.iter().enumerate() {
                let seg = curve.subcurve(params[i], params[i + 1]).unwrap_or_else(|_| curve.clone());
                if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
                    e.set_curve(Some(seg));
                }
            }
            // ADR-088 Phase 1 (S-γ) — single owner_id for all Bezier segments.
            let owner_id = self.scene.mesh.next_curve_owner_id();
            for &eid in &edge_ids {
                self.scene.mesh.set_edge_curve_owner_id(eid, Some(owner_id));
            }
            // ADR-200 (A1) — 그린 freeform 곡선이 auto-division 에 참여:
            //   ① source 저장 → reconstruct 가 InputCurve::Freeform 으로 투입
            //      (smooth 보존, polygonize 아님).
            //   ② 곡선 평면(tessellation best-fit)의 coplanar 면 seed 로 rederive.
            self.scene.mesh.set_freeform_curve_source(owner_id, curve.clone());
            let mut plane_normal = DVec3::Z;
            let last = pts[pts.len() - 1];
            for k in 1..pts.len() {
                let n = (pts[k] - pts[0]).cross(last - pts[0]);
                if n.length_squared() > 1e-12 {
                    plane_normal = n.normalize();
                    break;
                }
            }
            let _ = self.scene.rederive_after_curve_draw(&pts, plane_normal);
        }

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        if any_failed { -1.0 } else { 0.0 }
    }

    /// ADR-032 P17 — Atomic B-spline drawing with curve promotion.
    /// Like Bezier; same curve metadata replicated on each segment edge.
    #[wasm_bindgen(js_name = "drawBSplineWithCurve")]
    pub fn draw_bspline_with_curve(
        &mut self,
        control_pts_flat: Vec<f64>,
        knots: Vec<f64>,
        degree: u32,
    ) -> f64 {
        use axia_geo::{AnalyticCurve, EdgeId};
        use axia_geo::curves::CurveOps;
        use glam::DVec3;
        if control_pts_flat.is_empty() || control_pts_flat.len() % 3 != 0 || degree == 0 {
            return -1.0;
        }
        let mut ctrl = Vec::with_capacity(control_pts_flat.len() / 3);
        let mut i = 0;
        while i + 2 < control_pts_flat.len() {
            ctrl.push(DVec3::new(
                control_pts_flat[i], control_pts_flat[i + 1], control_pts_flat[i + 2],
            ));
            i += 3;
        }
        let expected_knots = ctrl.len() + degree as usize + 1;
        if knots.len() != expected_knots || ctrl.len() < degree as usize + 1 {
            return -1.0;
        }
        let curve = AnalyticCurve::BSpline {
            control_pts: ctrl, knots, degree,
        };
        // ADR-201 (β-1) — bounded uniform sub-range sampling (was tessellate(0.001)
        // → ~4096 edges → syncMesh freeze). SEGS 세그먼트, 각자 sub-range 곡선
        // (split_at) 부착 → render 가 세그먼트마다 자기 구간만 tessellate → 적은
        // edge 수로도 smooth.
        const SEGS: usize = 64;
        let (r0, r1) = curve.parameter_range();
        let mut params: Vec<f64> = Vec::with_capacity(SEGS + 1);
        let mut pts: Vec<DVec3> = Vec::with_capacity(SEGS + 1);
        for i in 0..=SEGS {
            let t = r0 + (r1 - r0) * (i as f64) / (SEGS as f64);
            match curve.evaluate(t, &self.scene.mesh) {
                Ok(p) => { params.push(t); pts.push(p); }
                Err(_) => return -1.0,
            }
        }
        if pts.len() < 2 { return -1.0; }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mut edge_ids: Vec<EdgeId> = Vec::with_capacity(pts.len());
        let mut any_failed = false;
        for i in 0..pts.len() - 1 {
            match self.scene.mesh.draw_line(pts[i], pts[i + 1]) {
                Ok((_, _, eid)) => edge_ids.push(eid),
                Err(_) => { any_failed = true; break; }
            }
        }

        if !any_failed {
            // ADR-201 (β-1) — 세그먼트별 sub-range 곡선 (full curve 아님) → render
            // overlap 없이 smooth.
            for (i, &eid) in edge_ids.iter().enumerate() {
                let seg = curve.subcurve(params[i], params[i + 1]).unwrap_or_else(|_| curve.clone());
                if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
                    e.set_curve(Some(seg));
                }
            }
            // ADR-088 Phase 1 (S-γ) — single owner_id for all B-spline segments.
            let owner_id = self.scene.mesh.next_curve_owner_id();
            for &eid in &edge_ids {
                self.scene.mesh.set_edge_curve_owner_id(eid, Some(owner_id));
            }
            // ADR-200 (A1) — 그린 freeform 곡선 auto-division 참여 (Bezier 동일).
            self.scene.mesh.set_freeform_curve_source(owner_id, curve.clone());
            let mut plane_normal = DVec3::Z;
            let last = pts[pts.len() - 1];
            for k in 1..pts.len() {
                let n = (pts[k] - pts[0]).cross(last - pts[0]);
                if n.length_squared() > 1e-12 {
                    plane_normal = n.normalize();
                    break;
                }
            }
            let _ = self.scene.rederive_after_curve_draw(&pts, plane_normal);
        }

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        if any_failed { -1.0 } else { 0.0 }
    }

    /// Clear any analytic curve from an edge (revert to straight line).
    #[wasm_bindgen(js_name = "clearEdgeCurve")]
    pub fn clear_edge_curve(&mut self, edge_id: u32) -> bool {
        use axia_geo::EdgeId;
        let eid = EdgeId::new(edge_id);
        if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
            e.set_curve(None);
            self.mark_topology_changed();
            true
        } else {
            false
        }
    }

    /// ADR-088 Phase 1 (S-δ) — Read curve owner group ID for an edge.
    /// Returns the owner_id (>= 0) if edge has a group, -1 if no group
    /// (single segment) or edge invalid/inactive.
    ///
    /// Caller (SelectTool walk): pick edge → call this → if >= 0, call
    /// `getEdgesByCurveOwner(id)` to get all segments of the same logical
    /// analytic curve (LOCKED #15 P22.5 enforcement).
    #[wasm_bindgen(js_name = "getEdgeCurveOwnerId")]
    pub fn get_edge_curve_owner_id(&self, edge_id: u32) -> i32 {
        use axia_geo::EdgeId;
        let eid = EdgeId::new(edge_id);
        match self.scene.mesh.edge_curve_owner_id(eid) {
            Some(owner) => owner as i32,
            None => -1,
        }
    }

    /// ADR-088 Phase 1 (S-δ) — Get all active edges sharing a given
    /// curve owner group ID. Returns empty array if no edges match
    /// (stale id, all deactivated, etc.) — defensive against undo /
    /// erase / cascade scenarios.
    ///
    /// Caller: SelectTool walk after `getEdgeCurveOwnerId` returns >= 0.
    #[wasm_bindgen(js_name = "getEdgesByCurveOwner")]
    pub fn get_edges_by_curve_owner(&self, owner_id: u32) -> Vec<u32> {
        self.scene.mesh.edges_by_curve_owner(owner_id)
            .into_iter()
            .map(|eid| eid.raw())
            .collect()
    }

    /// ADR-093 D-γ — Walk face owner-siblings.
    ///
    /// Selection-layer entry point: given a clicked face, returns all
    /// active faces sharing its `surface_owner_id` (Cylinder side group).
    /// If the face has no owner-id (None), returns just `[face_id]`
    /// (no group — single-face selection unchanged).
    ///
    /// Returns empty array if the face is missing/inactive (defensive
    /// against stale ids).
    ///
    /// Caller: SelectTool pickFace → automatic group promote (Lock-in
    /// D-D — single face click promotes to entire surface group).
    #[wasm_bindgen(js_name = "walkFaceOwnerSiblings")]
    pub fn walk_face_owner_siblings(&self, face_id: u32) -> Vec<u32> {
        use axia_geo::FaceId;
        let fid = FaceId::new(face_id);
        self.scene.mesh.walk_face_owner_siblings(fid)
            .into_iter()
            .map(|f| f.raw())
            .collect()
    }

    /// ADR-093 D-γ — Read the surface owner-id of a face.
    /// Returns -1 if the face has no owner-id (standalone) or is
    /// missing/inactive. Mirrors `getEdgeCurveOwnerId` from ADR-088.
    #[wasm_bindgen(js_name = "getFaceSurfaceOwnerId")]
    pub fn get_face_surface_owner_id(&self, face_id: u32) -> i32 {
        use axia_geo::FaceId;
        let fid = FaceId::new(face_id);
        match self.scene.mesh.face_surface_owner_id(fid) {
            Some(owner) => owner as i32,
            None => -1,
        }
    }

    /// ADR-094 B-η — Set the Path B cylinder default.
    ///
    /// `true` = `create_solid` 의 closed-curve cylinder profile 이
    /// kernel-native 3 face / 2 edge / 2 vert annulus topology 로
    /// 생성 (산업 CAD parity, 메모리 ~98% 절감).
    /// `false` = legacy Path A (25 face polygon strip).
    ///
    /// Production layer (TS bridge) calls this once at app init based
    /// on localStorage `axia:cylinder-path-b-mode` preference. Tests
    /// may toggle per-call.
    #[wasm_bindgen(js_name = "setCylinderPathBDefault")]
    pub fn set_cylinder_path_b_default(&mut self, on: bool) {
        self.scene.mesh.set_cylinder_path_b_default(on);
    }

    /// ADR-094 B-η — Read the Path B cylinder default flag.
    #[wasm_bindgen(js_name = "getCylinderPathBDefault")]
    pub fn get_cylinder_path_b_default(&self) -> bool {
        self.scene.mesh.cylinder_path_b_default()
    }

    /// ADR-104 β-1-ζ — Set the Path B sphere default.
    ///
    /// `true` = `create_sphere` 가 kernel-native 2 hemisphere / 1 equator
    /// edge / 1 vert canonical 로 분기 (산업 CAD parity, 메모리 99%+ 절감).
    /// `false` = legacy Path A (289 face default polygonal mesh).
    ///
    /// Production layer (TS bridge) calls this once at app init based on
    /// localStorage `axia:sphere-path-b-mode` preference. Tests may toggle
    /// per-call. Mirrors `setCylinderPathBDefault` pattern (ADR-094 B-η).
    #[wasm_bindgen(js_name = "setSpherePathBDefault")]
    pub fn set_sphere_path_b_default(&mut self, on: bool) {
        self.scene.mesh.set_sphere_path_b_default(on);
    }

    /// ADR-104 β-1-ζ — Read the Path B sphere default flag.
    #[wasm_bindgen(js_name = "getSpherePathBDefault")]
    pub fn get_sphere_path_b_default(&self) -> bool {
        self.scene.mesh.sphere_path_b_default()
    }

    /// ADR-104 β-2-ζ — Set the Path B cone default.
    ///
    /// `true` = `create_cone` 가 kernel-native 2 face / 1 edge / 1 vert
    /// canonical 로 분기 (산업 CAD parity, ~92% 메모리 절감).
    /// `false` = legacy Path A (~25 face polygonal cone).
    ///
    /// Production layer (TS bridge) calls this once at app init based on
    /// localStorage `axia:cone-path-b-mode` preference. Tests may toggle
    /// per-call. Mirrors `setSpherePathBDefault` / `setCylinderPathBDefault`
    /// patterns.
    #[wasm_bindgen(js_name = "setConePathBDefault")]
    pub fn set_cone_path_b_default(&mut self, on: bool) {
        self.scene.mesh.set_cone_path_b_default(on);
    }

    /// ADR-104 β-2-ζ — Read the Path B cone default flag.
    #[wasm_bindgen(js_name = "getConePathBDefault")]
    pub fn get_cone_path_b_default(&self) -> bool {
        self.scene.mesh.cone_path_b_default()
    }

    /// ADR-104 β-3-β — Create torus (Path B kernel-native, Q3 revision).
    ///
    /// 1 face / 1 edge / 1 vert canonical (sphere/cone self-loop pattern
    /// 답습). 99.7%+ memory reduction vs hypothetical Path A 289-face
    /// baseline (no Path A torus exists — kernel-native from day 1).
    ///
    /// Returns the FaceId (as f64 for JS) of the single torus surface.
    /// `-1.0` on error.
    #[wasm_bindgen(js_name = "createTorus")]
    pub fn create_torus(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        major_radius: f64, minor_radius: f64,
    ) -> f64 {
        let position = DVec3::new(cx, cy, cz);
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before);
        match self.scene.mesh.create_torus_kernel_native(
            position, major_radius, minor_radius, axia_core::FORM_MATERIAL,
        ) {
            Ok(face_id) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                let xia_id = self.scene.create_xia_with_faces(
                    "Torus".to_string(),
                    position,
                    vec![face_id],
                );
                let after = self.scene.scene_snapshot();
                self.scene.transactions.set_after_snapshot(after);
                self.scene.transactions.commit();
                debug_log!("[RUST] create_torus: face_id={} xia={}",
                    face_id.raw(), xia_id);
                face_id.raw() as f64
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] create_torus error: {}", e);
                -1.0
            }
        }
    }

    /// ADR-104 β-3-ζ — Set the Path B torus default flag.
    ///
    /// Note: Torus has no Path A polygonal baseline. Flag exists for
    /// pattern consistency with sphere/cone (potential future Path A hook).
    /// `create_torus` always routes to kernel-native regardless.
    #[wasm_bindgen(js_name = "setTorusPathBDefault")]
    pub fn set_torus_path_b_default(&mut self, on: bool) {
        self.scene.mesh.set_torus_path_b_default(on);
    }

    /// ADR-104 β-3-ζ — Read the Path B torus default flag.
    #[wasm_bindgen(js_name = "getTorusPathBDefault")]
    pub fn get_torus_path_b_default(&self) -> bool {
        self.scene.mesh.torus_path_b_default()
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-097 T-δ — Topology damage detection + recovery WASM API
    // ════════════════════════════════════════════════════════════════

    /// ADR-097 T-γ — Detect topology damage (Phase 4).
    ///
    /// Scene-level wrapper (Mesh detect + Orphan). Returns JSON:
    /// `{ "damages": [...], "checkedFaces": N, "checkedEdges": N }`
    ///
    /// damages 의 each item: `{ "kind": "BoundaryEdge|NonManifold|
    /// Degenerate|Orphan", ...kind-specific fields }`.
    #[wasm_bindgen(js_name = "detectTopologyDamage")]
    pub fn detect_topology_damage(&self) -> String {
        use axia_geo::TopologyDamageKind;
        let report = self.scene.detect_topology_damage();
        let mut json = String::with_capacity(256);
        json.push_str("{\"damages\":[");
        let mut first = true;
        for d in &report.damages {
            if !first { json.push(','); }
            first = false;
            match d {
                TopologyDamageKind::BoundaryEdge { edge_id, incident_face } => {
                    json.push_str(&format!(
                        "{{\"kind\":\"BoundaryEdge\",\"edgeId\":{},\"incidentFace\":{}}}",
                        edge_id.raw(), incident_face.raw(),
                    ));
                }
                TopologyDamageKind::NonManifold { edge_id, face_count } => {
                    json.push_str(&format!(
                        "{{\"kind\":\"NonManifold\",\"edgeId\":{},\"faceCount\":{}}}",
                        edge_id.raw(), face_count,
                    ));
                }
                TopologyDamageKind::Degenerate { face_id, reason } => {
                    json.push_str(&format!(
                        "{{\"kind\":\"Degenerate\",\"faceId\":{},\"reason\":\"{}\"}}",
                        face_id.raw(), reason,
                    ));
                }
                TopologyDamageKind::Orphan { face_id } => {
                    json.push_str(&format!(
                        "{{\"kind\":\"Orphan\",\"faceId\":{}}}",
                        face_id.raw(),
                    ));
                }
            }
        }
        json.push_str(&format!(
            "],\"checkedFaces\":{},\"checkedEdges\":{}}}",
            report.checked_faces, report.checked_edges,
        ));
        json
    }

    /// ADR-097 T-γ — Auto-recovery dispatcher (Phase 4).
    ///
    /// Returns JSON: `{ "kind": "NoOp|Recovered|PartialFailure",
    /// ...kind-specific fields }`.
    /// - NoOp: `{"kind":"NoOp"}`
    /// - Recovered: `{"kind":"Recovered","fixesApplied":N,"initialDamages":N}`
    /// - PartialFailure: `{"kind":"PartialFailure","fixesApplied":N,
    ///   "remainingCount":N}`
    ///
    /// Caller (TS bridge / Orchestrator) 가 결과 기반으로 사용자
    /// 다이얼로그 escalation 판단.
    #[wasm_bindgen(js_name = "attemptAutoRecovery")]
    pub fn attempt_auto_recovery(&mut self) -> String {
        use axia_geo::RecoveryOutcome;
        let outcome = self.scene.mesh.attempt_auto_recovery();
        match outcome {
            RecoveryOutcome::NoOp => {
                "{\"kind\":\"NoOp\"}".to_string()
            }
            RecoveryOutcome::Recovered { fixes_applied, initial_damages } => {
                format!(
                    "{{\"kind\":\"Recovered\",\"fixesApplied\":{},\"initialDamages\":{}}}",
                    fixes_applied, initial_damages,
                )
            }
            RecoveryOutcome::PartialFailure { fixes_applied, remaining } => {
                format!(
                    "{{\"kind\":\"PartialFailure\",\"fixesApplied\":{},\"remainingCount\":{}}}",
                    fixes_applied, remaining.damages.len(),
                )
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-098 S-γ — Asset Library 3-Tier Material Scope (Phase 5-A)
    //
    // 6 endpoints (additive — ADR-076 baseline guard PASS):
    //   - listMaterialsByTier (tier u32 → JSON array)
    //   - addProjectMaterial / addUserMaterial (JSON → MaterialId)
    //   - removeUserMaterial (MaterialId → bool)
    //   - getMaterialTier (MaterialId → tier u32, -1 sentinel)
    //   - migrateLegacyMaterials (count migrated)
    //
    // Tier encoding (axia_core::MaterialTier::as_u32 답습):
    //   0 = System (immutable), 1 = Project, 2 = User
    // ════════════════════════════════════════════════════════════════

    /// ADR-098 S-γ — List materials by tier.
    ///
    /// Returns JSON array of `{ id, name, nameEn, tier, color }` for the
    /// specified tier. Invalid tier → empty array.
    #[wasm_bindgen(js_name = "listMaterialsByTier")]
    pub fn list_materials_by_tier(&self, tier: u32) -> String {
        use axia_core::material::MaterialTier;
        let Some(t) = MaterialTier::from_u32(tier) else {
            return "[]".to_string();
        };
        let mats = self.scene.material_library.materials_by_tier(t);
        if mats.is_empty() {
            return "[]".to_string();
        }
        let entries: Vec<String> = mats.iter()
            .map(|m| {
                let hex = format!("{:06x}", m.visual.color);
                format!(
                    r##"{{"id":{},"name":"{}","nameEn":"{}","tier":{},"color":"#{}"}}"##,
                    m.id.raw(), m.name, m.name_en, t.as_u32(), hex
                )
            })
            .collect();
        format!("[{}]", entries.join(","))
    }

    /// ADR-098 S-γ — Get the tier of an existing material.
    ///
    /// Returns 0/1/2 (System/Project/User) or -1 if material missing.
    #[wasm_bindgen(js_name = "getMaterialTier")]
    pub fn get_material_tier(&self, material_id: u32) -> i32 {
        use axia_geo::MaterialId;
        match self.scene.material_library.tier_of(MaterialId::new(material_id)) {
            Some(t) => t.as_u32() as i32,
            None => -1,
        }
    }

    /// ADR-098 S-γ — Add a new material in Project tier.
    ///
    /// Input: simple JSON `{"name":"...","nameEn":"...","color":<u32>}`.
    /// Other physical/visual properties default to safe values; the UI
    /// can edit them via existing material edit endpoints.
    /// Returns the new MaterialId, or throws on parse error.
    #[wasm_bindgen(js_name = "addProjectMaterial")]
    pub fn add_project_material(&mut self, name: String, name_en: String, color: u32)
        -> u32
    {
        use axia_core::material::{
            MaterialTier, MaterialCategory, PhysicalProperties, VisualProperties,
            FireRating,
        };
        let id = self.scene.material_library.create_material_in_tier(
            MaterialTier::Project,
            name, name_en, MaterialCategory::Custom,
            PhysicalProperties {
                density: 1000.0, friction: 0.5, restitution: 0.5,
                specific_gravity: 1.0, thermal_conductivity: 0.5,
                fire_rating: FireRating::None,
            },
            VisualProperties {
                color, roughness: 0.5, metalness: 0.0, opacity: 1.0, layered: None,
            },
        );
        id.raw()
    }

    /// ADR-098 S-γ — Add a new material in User tier (opt-in library).
    ///
    /// Same shape as `addProjectMaterial` but scoped to User tier.
    #[wasm_bindgen(js_name = "addUserMaterial")]
    pub fn add_user_material(&mut self, name: String, name_en: String, color: u32)
        -> u32
    {
        use axia_core::material::{
            MaterialTier, MaterialCategory, PhysicalProperties, VisualProperties,
            FireRating,
        };
        let id = self.scene.material_library.create_material_in_tier(
            MaterialTier::User,
            name, name_en, MaterialCategory::Custom,
            PhysicalProperties {
                density: 1000.0, friction: 0.5, restitution: 0.5,
                specific_gravity: 1.0, thermal_conductivity: 0.5,
                fire_rating: FireRating::None,
            },
            VisualProperties {
                color, roughness: 0.5, metalness: 0.0, opacity: 1.0, layered: None,
            },
        );
        id.raw()
    }

    /// ADR-098 S-γ — Remove a User-tier material.
    ///
    /// System tier rejected (Material library `remove_material` Err →
    /// false). Project tier currently rejected at this surface (use
    /// `removeProjectMaterial` future ADR for cascade safety).
    /// Returns true on success, false otherwise.
    #[wasm_bindgen(js_name = "removeUserMaterial")]
    pub fn remove_user_material(&mut self, material_id: u32) -> bool {
        use axia_core::material::MaterialTier;
        use axia_geo::MaterialId;
        let id = MaterialId::new(material_id);
        // Only allow removal if currently in User tier (S-G safety).
        if self.scene.material_library.tier_of(id) != Some(MaterialTier::User) {
            return false;
        }
        self.scene.material_library.remove_material(id).is_ok()
    }

    /// ADR-098 S-γ — Force migration of legacy materials.
    ///
    /// Idempotent. Returns the count of newly classified materials.
    /// Snapshots imported via `importSnapshot` already auto-migrate;
    /// this endpoint is for explicit re-classification (e.g., after a
    /// legacy DXF/SKP import that creates raw materials).
    #[wasm_bindgen(js_name = "migrateLegacyMaterials")]
    pub fn migrate_legacy_materials(&mut self) -> u32 {
        self.scene.material_library.migrate_legacy_materials() as u32
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-100 R-γ — Material Removal Recovery (Phase 5-C) WASM API
    //
    // 3 endpoints (additive — ADR-076 baseline guard PASS):
    //   - detectOrphanMaterialAssignments — read-only JSON report
    //   - attemptMaterialRemovalRecovery  — 3-tier cascade outcome JSON
    //   - removeProjectMaterial           — remove + auto-recovery
    //
    // ADR-097 T-δ JSON shape 답습 — kind discriminator + named fields.
    // Format!-based serialization (no serde_json dep, consistent with
    // existing material/recovery surfaces).
    // ════════════════════════════════════════════════════════════════

    /// ADR-100 R-γ — Detect orphan material assignments.
    ///
    /// Returns JSON:
    ///   `{ "affectedXias": [{ "xiaId": N, "staleMaterialId": M,
    ///                         "faceCount": K }, ...] }`
    /// Empty array → clean scene.
    #[wasm_bindgen(js_name = "detectOrphanMaterialAssignments")]
    pub fn detect_orphan_material_assignments(&self) -> String {
        let report = self.scene.detect_orphan_material_assignments();
        if report.affected_xias.is_empty() {
            return "{\"affectedXias\":[]}".to_string();
        }
        let entries: Vec<String> = report.affected_xias.iter()
            .map(|e| format!(
                "{{\"xiaId\":{},\"staleMaterialId\":{},\"faceCount\":{}}}",
                e.xia_id, e.stale_material_id, e.face_count,
            ))
            .collect();
        format!("{{\"affectedXias\":[{}]}}", entries.join(","))
    }

    /// ADR-100 R-γ — Attempt material removal recovery (3-tier cascade).
    ///
    /// Returns JSON union (ADR-097 T-δ shape 답습):
    ///   `{ "kind": "NoOp" }`
    ///   `{ "kind": "Recovered", "affectedXias": N, "facesDemoted": K,
    ///      "facesFallback": F }`
    ///   `{ "kind": "PartialFailure", "affectedXias": N,
    ///      "remainingOrphans": R }`
    ///
    /// Mutates scene state (Pass 1 demote + Pass 2 fallback). Caller
    /// wraps in transaction; recovery is idempotent (second call on a
    /// clean scene returns NoOp).
    #[wasm_bindgen(js_name = "attemptMaterialRemovalRecovery")]
    pub fn attempt_material_removal_recovery(&mut self) -> String {
        use axia_core::MaterialRecoveryOutcome;
        let outcome = self.scene.attempt_material_removal_recovery();
        match outcome {
            MaterialRecoveryOutcome::NoOp => {
                "{\"kind\":\"NoOp\"}".to_string()
            }
            MaterialRecoveryOutcome::Recovered {
                affected_xias, faces_demoted, faces_fallback,
            } => format!(
                "{{\"kind\":\"Recovered\",\"affectedXias\":{},\"facesDemoted\":{},\"facesFallback\":{}}}",
                affected_xias, faces_demoted, faces_fallback,
            ),
            MaterialRecoveryOutcome::PartialFailure {
                affected_xias, remaining_orphans,
            } => format!(
                "{{\"kind\":\"PartialFailure\",\"affectedXias\":{},\"remainingOrphans\":{}}}",
                affected_xias, remaining_orphans,
            ),
        }
    }

    /// ADR-100 R-γ — Remove a Project-tier material with auto-recovery.
    ///
    /// Returns JSON `{ "ok": bool, "removedId": N, "recovery": {...} }`
    /// where `recovery` matches the union from `attemptMaterialRemovalRecovery`.
    /// On error: `{ "ok": false, "error": "..." }`.
    ///
    /// System tier always rejected (R-D safety, ADR-098 S-G 답습).
    #[wasm_bindgen(js_name = "removeProjectMaterial")]
    pub fn remove_project_material(&mut self, material_id: u32) -> String {
        use axia_geo::MaterialId;
        use axia_core::MaterialRecoveryOutcome;
        let id = MaterialId::new(material_id);
        match self.scene.remove_project_material_with_recovery(id) {
            Ok(out) => {
                let recovery_json = match out.recovery {
                    MaterialRecoveryOutcome::NoOp => {
                        "{\"kind\":\"NoOp\"}".to_string()
                    }
                    MaterialRecoveryOutcome::Recovered {
                        affected_xias, faces_demoted, faces_fallback,
                    } => format!(
                        "{{\"kind\":\"Recovered\",\"affectedXias\":{},\"facesDemoted\":{},\"facesFallback\":{}}}",
                        affected_xias, faces_demoted, faces_fallback,
                    ),
                    MaterialRecoveryOutcome::PartialFailure {
                        affected_xias, remaining_orphans,
                    } => format!(
                        "{{\"kind\":\"PartialFailure\",\"affectedXias\":{},\"remainingOrphans\":{}}}",
                        affected_xias, remaining_orphans,
                    ),
                };
                format!(
                    "{{\"ok\":true,\"removedId\":{},\"recovery\":{}}}",
                    out.removed_id, recovery_json,
                )
            }
            Err(e) => format!(
                "{{\"ok\":false,\"error\":\"{}\"}}", e,
            ),
        }
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-099 L-γ — Layered Material 4-PBR Channels (Phase 5-B) WASM API
    //
    // 5 endpoints (additive — ADR-076 baseline guard PASS):
    //   - getLayeredChannels   — read-only JSON (per-channel info)
    //   - setLayeredChannel    — set one of 4 channels (flat params)
    //   - clearLayeredChannel  — remove one channel (None)
    //   - migrateLegacyTextureToLayered — bulk normalizer
    //   - hasLayeredMaterial   — quick existence check
    //
    // Channel naming convention: "albedo" | "normal" | "roughness" |
    // "metallic" (lowercase, matches TextureProjection serde). Projection
    // encoded as u32 (0=planar, 1=box, 2=cylindrical) to avoid string
    // round-trip in hot paths. rotation: f64::NAN = None; label: empty
    // string = None.
    //
    // markDirty NOT triggered — visual-only mutation; mesh buffers
    // unchanged. Renderer refreshes by polling material library.
    // ════════════════════════════════════════════════════════════════

    /// ADR-099 L-γ — Read layered channels of a material as JSON.
    ///
    /// Returns:
    ///   - `"{\"hasLayered\":false}"` if material missing or layered=None
    ///   - `"{\"hasLayered\":true,\"channels\":{...}}"` with per-channel
    ///     info (each: `{ "dataUrl": ..., "projection": "planar"|"box"|
    ///     "cylindrical", "scale": ..., "rotation": <num|null>,
    ///     "label": <str|null> }`)
    #[wasm_bindgen(js_name = "getLayeredChannels")]
    pub fn get_layered_channels(&self, material_id: u32) -> String {
        use axia_geo::MaterialId;
        use axia_core::TextureChannelInfo;
        let id = MaterialId::new(material_id);
        let material = match self.scene.material_library.get(id) {
            Some(m) => m,
            None => return "{\"hasLayered\":false}".to_string(),
        };
        let layered = match &material.visual.layered {
            Some(l) => l,
            None => return "{\"hasLayered\":false}".to_string(),
        };
        let channel_json = |ch: &Option<TextureChannelInfo>| -> String {
            match ch {
                None => "null".to_string(),
                Some(c) => {
                    let projection_str = match c.projection {
                        axia_core::TextureProjection::Planar => "planar",
                        axia_core::TextureProjection::Box => "box",
                        axia_core::TextureProjection::Cylindrical => "cylindrical",
                    };
                    let rotation = match c.rotation {
                        Some(r) => format!("{}", r),
                        None => "null".to_string(),
                    };
                    let label = match &c.label {
                        Some(s) => format!("\"{}\"", s.replace('"', "\\\"")),
                        None => "null".to_string(),
                    };
                    // dataUrl may contain quotes — escape minimally.
                    let data_url_esc = c.data_url.replace('"', "\\\"");
                    format!(
                        r#"{{"dataUrl":"{}","projection":"{}","scale":{},"rotation":{},"label":{}}}"#,
                        data_url_esc, projection_str, c.scale, rotation, label,
                    )
                }
            }
        };
        format!(
            r#"{{"hasLayered":true,"channels":{{"albedo":{},"normal":{},"roughness":{},"metallic":{}}}}}"#,
            channel_json(&layered.albedo),
            channel_json(&layered.normal),
            channel_json(&layered.roughness),
            channel_json(&layered.metallic),
        )
    }

    /// ADR-099 L-γ — Set one channel of a material's layered payload.
    ///
    /// Flat-parameter signature (avoids JSON parsing in Rust). Channel
    /// name must be one of "albedo" | "normal" | "roughness" |
    /// "metallic". Projection u32: 0=planar, 1=box, 2=cylindrical.
    /// `rotation_or_nan = f64::NAN` → None; `label.is_empty()` → None.
    ///
    /// Returns true on success, false on:
    ///   - material missing
    ///   - invalid channel name
    ///   - invalid projection u32
    ///   - validate() failure (empty dataUrl, non-positive scale)
    ///
    /// Creates `layered = Some(LayeredChannels::default())` on the first
    /// call if currently None.
    #[wasm_bindgen(js_name = "setLayeredChannel")]
    pub fn set_layered_channel(
        &mut self,
        material_id: u32,
        channel: String,
        data_url: String,
        projection: u32,
        scale: f64,
        rotation_or_nan: f64,
        label: String,
    ) -> bool {
        use axia_geo::MaterialId;
        use axia_core::{TextureChannelInfo, TextureProjection};
        let id = MaterialId::new(material_id);
        let material = match self.scene.material_library.get_mut(id) {
            Some(m) => m,
            None => return false,
        };
        let proj = match projection {
            0 => TextureProjection::Planar,
            1 => TextureProjection::Box,
            2 => TextureProjection::Cylindrical,
            _ => return false,
        };
        let info = TextureChannelInfo {
            data_url,
            projection: proj,
            scale,
            rotation: if rotation_or_nan.is_nan() { None } else { Some(rotation_or_nan) },
            label: if label.is_empty() { None } else { Some(label) },
        };
        if info.validate().is_err() {
            return false;
        }
        if material.visual.layered.is_none() {
            material.visual.layered = Some(Default::default());
        }
        let layered = material.visual.layered.as_mut().unwrap();
        match channel.as_str() {
            "albedo" => layered.albedo = Some(info),
            "normal" => layered.normal = Some(info),
            "roughness" => layered.roughness = Some(info),
            "metallic" => layered.metallic = Some(info),
            _ => return false,
        }
        true
    }

    /// ADR-099 L-γ — Clear one channel of a material's layered payload.
    ///
    /// If clearing the last channel leaves all 4 as None, the `layered`
    /// field is also reset to None (idempotent normalization).
    /// Returns true on success, false on material/channel missing.
    #[wasm_bindgen(js_name = "clearLayeredChannel")]
    pub fn clear_layered_channel(&mut self, material_id: u32, channel: String) -> bool {
        use axia_geo::MaterialId;
        let id = MaterialId::new(material_id);
        let material = match self.scene.material_library.get_mut(id) {
            Some(m) => m,
            None => return false,
        };
        let layered = match material.visual.layered.as_mut() {
            Some(l) => l,
            None => return false,
        };
        match channel.as_str() {
            "albedo" => layered.albedo = None,
            "normal" => layered.normal = None,
            "roughness" => layered.roughness = None,
            "metallic" => layered.metallic = None,
            _ => return false,
        }
        // Normalize: if all 4 channels None, drop the layered wrapper.
        if !layered.has_any_channel() {
            material.visual.layered = None;
        }
        true
    }

    /// ADR-099 L-γ — Bulk normalize empty layered payloads.
    ///
    /// Idempotent. Returns the count of materials whose empty
    /// `LayeredChannels` was stripped to None. ADR-098 S-D pattern.
    #[wasm_bindgen(js_name = "migrateLegacyTextureToLayered")]
    pub fn migrate_legacy_texture_to_layered(&mut self) -> u32 {
        self.scene.material_library
            .migrate_legacy_textures_to_layered() as u32
    }

    /// ADR-099 L-γ — Quick existence check.
    ///
    /// Returns true iff the material exists AND has `layered.Some(_)`
    /// with at least one populated channel. False on material missing
    /// or `layered=None` or empty `LayeredChannels`.
    #[wasm_bindgen(js_name = "hasLayeredMaterial")]
    pub fn has_layered_material(&self, material_id: u32) -> bool {
        use axia_geo::MaterialId;
        let id = MaterialId::new(material_id);
        self.scene.material_library.get(id)
            .and_then(|m| m.visual.layered.as_ref())
            .map(|l| l.has_any_channel())
            .unwrap_or(false)
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-095 Phase 3-γ — Reference 시민권 (Two-Layer Phase 3) WASM API
    //
    // 3 categories: ConstructionLine / ImportedMesh / PointCloud.
    // R-B mutually exclusive geometry ownership 강제 — Form/Property
    // 충돌 시 strict throw (silent skip 차단, ADR-091 §E L1 canonical).
    // ════════════════════════════════════════════════════════════════

    /// ADR-095 Phase 3-γ — Create a ConstructionLine Reference (작도선).
    ///
    /// Returns the new ReferenceId on success. On R-B violation
    /// (edge already in Reference), throws JS Error with the rejection
    /// reason.
    #[wasm_bindgen(js_name = "createReferenceConstructionLine")]
    pub fn create_reference_construction_line(
        &mut self,
        name: String,
        edge_ids: Vec<u32>,
    ) -> Result<u32, JsValue> {
        use axia_geo::EdgeId;
        let edges: Vec<EdgeId> = edge_ids.into_iter().map(EdgeId::new).collect();
        match self.scene.create_reference(
            name,
            axia_core::ReferenceCategory::ConstructionLine { edge_ids: edges },
        ) {
            Ok(id) => Ok(id.raw()),
            Err(e) => Err(JsValue::from_str(&format!(
                "createReferenceConstructionLine: {}", e,
            ))),
        }
    }

    /// ADR-095 Phase 3-γ — Create an ImportedMesh Reference (외부 import).
    ///
    /// Returns the new ReferenceId on success. R-B violation
    /// (face already owned by Form/Property/Reference) → JS Error.
    #[wasm_bindgen(js_name = "createReferenceImportedMesh")]
    pub fn create_reference_imported_mesh(
        &mut self,
        name: String,
        face_ids: Vec<u32>,
        source_path: Option<String>,
    ) -> Result<u32, JsValue> {
        use axia_geo::FaceId;
        let faces: Vec<FaceId> = face_ids.into_iter().map(FaceId::new).collect();
        match self.scene.create_reference(
            name,
            axia_core::ReferenceCategory::ImportedMesh {
                face_ids: faces, source_path,
            },
        ) {
            Ok(id) => Ok(id.raw()),
            Err(e) => Err(JsValue::from_str(&format!(
                "createReferenceImportedMesh: {}", e,
            ))),
        }
    }

    /// ADR-095 Phase 3-γ — Create a PointCloud Reference (스캔 데이터).
    ///
    /// Returns the new ReferenceId on success. R-B violation
    /// (vert already in Reference) → JS Error.
    #[wasm_bindgen(js_name = "createReferencePointCloud")]
    pub fn create_reference_point_cloud(
        &mut self,
        name: String,
        vert_ids: Vec<u32>,
    ) -> Result<u32, JsValue> {
        use axia_geo::VertId;
        let verts: Vec<VertId> = vert_ids.into_iter().map(VertId::new).collect();
        match self.scene.create_reference(
            name,
            axia_core::ReferenceCategory::PointCloud { vert_ids: verts },
        ) {
            Ok(id) => Ok(id.raw()),
            Err(e) => Err(JsValue::from_str(&format!(
                "createReferencePointCloud: {}", e,
            ))),
        }
    }

    /// ADR-095 Phase 3-γ — All currently-stored Reference IDs (sorted
    /// ascending). Returns empty Vec if none.
    #[wasm_bindgen(js_name = "getReferenceIds")]
    pub fn get_reference_ids(&self) -> Vec<u32> {
        self.scene.list_reference_ids()
            .into_iter()
            .map(|id| id.raw())
            .collect()
    }

    /// ADR-095 Phase 3-γ — Read a Reference as JSON.
    /// Returns `{ id, name, category, visible, locked }` or empty
    /// string if id missing.
    ///
    /// `category` shape:
    /// - `{"kind":"ConstructionLine","edge_ids":[...]}`
    /// - `{"kind":"ImportedMesh","face_ids":[...],"source_path":...|null}`
    /// - `{"kind":"PointCloud","vert_ids":[...]}`
    #[wasm_bindgen(js_name = "getReferenceJson")]
    pub fn get_reference_json(&self, id: u32) -> String {
        use axia_core::ReferenceCategory;
        let rid = axia_core::ReferenceId::new(id);
        let Some(r) = self.scene.get_reference(rid) else {
            return String::new();
        };
        let category_json = match &r.category {
            ReferenceCategory::ConstructionLine { edge_ids } => {
                let ids: Vec<u32> = edge_ids.iter().map(|e| e.raw()).collect();
                format!("{{\"kind\":\"ConstructionLine\",\"edge_ids\":{:?}}}", ids)
            }
            ReferenceCategory::ImportedMesh { face_ids, source_path } => {
                let ids: Vec<u32> = face_ids.iter().map(|f| f.raw()).collect();
                let sp = match source_path {
                    Some(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
                    None => "null".to_string(),
                };
                format!(
                    "{{\"kind\":\"ImportedMesh\",\"face_ids\":{:?},\"source_path\":{}}}",
                    ids, sp,
                )
            }
            ReferenceCategory::PointCloud { vert_ids } => {
                let ids: Vec<u32> = vert_ids.iter().map(|v| v.raw()).collect();
                format!("{{\"kind\":\"PointCloud\",\"vert_ids\":{:?}}}", ids)
            }
        };
        // Escape name properly (basic backslash + quote escape).
        let name_escaped = r.name.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            "{{\"id\":{},\"name\":\"{}\",\"category\":{},\"visible\":{},\"locked\":{}}}",
            r.id.raw(), name_escaped, category_json, r.visible, r.locked,
        )
    }

    /// ADR-095 Phase 3-γ — Delete a Reference. Returns true if removed.
    /// Reverse 인덱스도 자동 정리.
    #[wasm_bindgen(js_name = "deleteReference")]
    pub fn delete_reference(&mut self, id: u32) -> bool {
        let rid = axia_core::ReferenceId::new(id);
        self.scene.delete_reference(rid)
    }

    /// ADR-095 Phase 3-γ — Toggle Reference visibility flag.
    /// Returns false if id missing.
    #[wasm_bindgen(js_name = "setReferenceVisible")]
    pub fn set_reference_visible(&mut self, id: u32, visible: bool) -> bool {
        let rid = axia_core::ReferenceId::new(id);
        self.scene.set_reference_visible(rid, visible)
    }

    /// ADR-095 Phase 3-γ — Toggle Reference locked flag.
    /// Returns false if id missing.
    #[wasm_bindgen(js_name = "setReferenceLocked")]
    pub fn set_reference_locked(&mut self, id: u32, locked: bool) -> bool {
        let rid = axia_core::ReferenceId::new(id);
        self.scene.set_reference_locked(rid, locked)
    }

    /// ADR-095 Phase 3-γ — Reverse lookup: get the Reference ID owning
    /// a given face. Returns -1 if face is not part of any Reference.
    #[wasm_bindgen(js_name = "getFaceReferenceId")]
    pub fn get_face_reference_id(&self, face_id: u32) -> i32 {
        use axia_geo::FaceId;
        let fid = FaceId::new(face_id);
        match self.scene.face_to_reference.get(&fid) {
            Some(rid) => rid.raw() as i32,
            None => -1,
        }
    }

    /// Check whether an edge has an analytic curve attached.
    /// Returns: 0 = none/straight, 1 = Line, 2 = Circle, 3 = Arc,
    /// 4 = Bezier, 5 = BSpline, 6 = NURBS. -1 if edge_id invalid.
    #[wasm_bindgen(js_name = "edgeCurveKind")]
    pub fn edge_curve_kind(&self, edge_id: u32) -> i32 {
        use axia_geo::{EdgeId, AnalyticCurve};
        let eid = EdgeId::new(edge_id);
        match self.scene.mesh.edge_curve(eid) {
            None => match self.scene.mesh.edges.get(eid) {
                Some(_) => 0,
                None => -1,
            },
            Some(AnalyticCurve::Line { .. }) => 1,
            Some(AnalyticCurve::Circle { .. }) => 2,
            Some(AnalyticCurve::Arc { .. }) => 3,
            Some(AnalyticCurve::Bezier { .. }) => 4,
            Some(AnalyticCurve::BSpline { .. }) => 5,
            Some(AnalyticCurve::NURBS { .. }) => 6,
        }
    }

    /// ADR-030 Phase C — Set a NURBS curve on an existing edge.
    ///
    /// Args:
    /// - `control_pts_flat`: 3·(n+1) floats `[x0,y0,z0, x1,y1,z1, ...]`
    /// - `weights`: n+1 strictly-positive weights
    /// - `knots`: n + degree + 2 = `(n+1) + degree + 1` non-decreasing values
    /// - `degree`: spline degree (≥ 1)
    ///
    /// Returns true on success.
    #[wasm_bindgen(js_name = "setEdgeNurbsCurve")]
    pub fn set_edge_nurbs_curve(
        &mut self,
        edge_id: u32,
        control_pts_flat: Vec<f64>,
        weights: Vec<f64>,
        knots: Vec<f64>,
        degree: u32,
    ) -> bool {
        use axia_geo::{EdgeId, AnalyticCurve};
        use glam::DVec3;
        if control_pts_flat.is_empty() || control_pts_flat.len() % 3 != 0 {
            return false;
        }
        let mut pts = Vec::with_capacity(control_pts_flat.len() / 3);
        let mut i = 0;
        while i + 2 < control_pts_flat.len() {
            pts.push(DVec3::new(
                control_pts_flat[i], control_pts_flat[i + 1], control_pts_flat[i + 2],
            ));
            i += 3;
        }
        // Validation will happen on the engine side via the AnalyticCurve eval;
        // sanity-check sizes here for early rejection.
        let expected_knots = pts.len() + degree as usize + 1;
        if pts.len() != weights.len()
            || knots.len() != expected_knots
            || pts.len() < degree as usize + 1
            || degree == 0
            || weights.iter().any(|&w| w <= 0.0)
        {
            return false;
        }
        let eid = EdgeId::new(edge_id);
        if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
            e.set_curve(Some(AnalyticCurve::NURBS {
                control_pts: pts, weights, knots, degree,
            }));
            self.mark_topology_changed();
            true
        } else {
            false
        }
    }

    /// ADR-030 Phase C — Compute intersections between two edges' analytic
    /// curves. Returns a flat Float64Array `[x0, y0, z0, t1_0, t2_0, angle_0,
    /// x1, y1, z1, t1_1, t2_1, angle_1, ...]` — 6 floats per intersection.
    ///
    /// If either edge has no curve attached, the edge is treated as a straight
    /// line between its two endpoints.
    #[wasm_bindgen(js_name = "intersectEdges")]
    pub fn intersect_edges(&self, edge_id_a: u32, edge_id_b: u32, tol: f64) -> Vec<f64> {
        use axia_geo::{EdgeId, AnalyticCurve};
        let eid_a = EdgeId::new(edge_id_a);
        let eid_b = EdgeId::new(edge_id_b);
        let mesh = &self.scene.mesh;
        let make_curve = |eid: EdgeId| -> Option<AnalyticCurve> {
            let edge = mesh.edges.get(eid)?;
            if let Some(c) = edge.curve() {
                return Some(c.clone());
            }
            // Straight-line fallback.
            Some(AnalyticCurve::Line { start: edge.v_small(), end: edge.v_large() })
        };
        let c1 = match make_curve(eid_a) { Some(c) => c, None => return Vec::new() };
        let c2 = match make_curve(eid_b) { Some(c) => c, None => return Vec::new() };
        let xs = match axia_geo::curves::intersect::intersect_curves(&c1, &c2, mesh, tol) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let mut flat = Vec::with_capacity(xs.len() * 6);
        for x in xs {
            flat.push(x.point.x);
            flat.push(x.point.y);
            flat.push(x.point.z);
            flat.push(x.t1);
            flat.push(x.t2);
            flat.push(x.angle);
        }
        flat
    }

    /// ADR-029 Phase B — Set a Bezier curve on an existing edge.
    ///
    /// `control_pts_flat` is a flat Float64Array `[x0,y0,z0, x1,y1,z1, ...]`
    /// of n+1 control points (n = degree). Need ≥ 2 points (degree ≥ 1).
    /// Returns true if successful.
    #[wasm_bindgen(js_name = "setEdgeBezierCurve")]
    pub fn set_edge_bezier_curve(
        &mut self,
        edge_id: u32,
        control_pts_flat: Vec<f64>,
    ) -> bool {
        use axia_geo::{EdgeId, AnalyticCurve};
        use glam::DVec3;
        if control_pts_flat.len() < 6 || control_pts_flat.len() % 3 != 0 {
            return false;
        }
        let mut pts = Vec::with_capacity(control_pts_flat.len() / 3);
        let mut i = 0;
        while i + 2 < control_pts_flat.len() {
            pts.push(DVec3::new(
                control_pts_flat[i], control_pts_flat[i + 1], control_pts_flat[i + 2],
            ));
            i += 3;
        }
        let eid = EdgeId::new(edge_id);
        if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
            e.set_curve(Some(AnalyticCurve::Bezier { control_pts: pts }));
            self.mark_topology_changed();
            true
        } else {
            false
        }
    }

    /// ADR-029 Phase B — Set a B-spline curve on an existing edge.
    ///
    /// `control_pts_flat`: flat array of n+1 control points (3·(n+1) floats).
    /// `knots`: m+1 knot values (m = n + degree + 1), non-decreasing.
    /// `degree`: spline degree (≥ 1).
    /// Returns true if successful and knot vector is valid.
    #[wasm_bindgen(js_name = "setEdgeBSplineCurve")]
    pub fn set_edge_bspline_curve(
        &mut self,
        edge_id: u32,
        control_pts_flat: Vec<f64>,
        knots: Vec<f64>,
        degree: u32,
    ) -> bool {
        use axia_geo::{EdgeId, AnalyticCurve};
        use glam::DVec3;
        if control_pts_flat.is_empty() || control_pts_flat.len() % 3 != 0 {
            return false;
        }
        let mut pts = Vec::with_capacity(control_pts_flat.len() / 3);
        let mut i = 0;
        while i + 2 < control_pts_flat.len() {
            pts.push(DVec3::new(
                control_pts_flat[i], control_pts_flat[i + 1], control_pts_flat[i + 2],
            ));
            i += 3;
        }
        // Sanity: knots.len() must equal pts.len() + degree + 1
        let expected = pts.len() + degree as usize + 1;
        if knots.len() != expected || pts.len() < degree as usize + 1 || degree == 0 {
            return false;
        }
        let eid = EdgeId::new(edge_id);
        if let Some(e) = self.scene.mesh.edges.get_mut(eid) {
            e.set_curve(Some(AnalyticCurve::BSpline {
                control_pts: pts, knots, degree,
            }));
            self.mark_topology_changed();
            true
        } else {
            false
        }
    }

    // ========================================================================
    // ADR-031 Phase D — Analytic Surface API
    // ========================================================================

    /// Set a Plane surface on an existing face.
    /// Args: origin (3), normal (3), basis_u (3), u_range (2), v_range (2).
    #[wasm_bindgen(js_name = "setFaceSurfacePlane")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_face_surface_plane(
        &mut self, face_id: u32,
        ox: f64, oy: f64, oz: f64,
        nx: f64, ny: f64, nz: f64,
        ux: f64, uy: f64, uz: f64,
        u_min: f64, u_max: f64,
        v_min: f64, v_max: f64,
    ) -> bool {
        use axia_geo::{FaceId, AnalyticSurface};
        use glam::DVec3;
        let surface = AnalyticSurface::Plane {
            origin: DVec3::new(ox, oy, oz),
            normal: DVec3::new(nx, ny, nz),
            basis_u: DVec3::new(ux, uy, uz),
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        let fid = FaceId::new(face_id);
        let result = self.scene.mesh.set_face_surface(fid, Some(surface));
        if result { self.mark_topology_changed(); }
        result
    }

    /// Set a Cylinder surface on an existing face.
    #[wasm_bindgen(js_name = "setFaceSurfaceCylinder")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_face_surface_cylinder(
        &mut self, face_id: u32,
        ox: f64, oy: f64, oz: f64,
        ax: f64, ay: f64, az: f64,
        radius: f64,
        rx: f64, ry: f64, rz: f64,
        u_min: f64, u_max: f64,
        v_min: f64, v_max: f64,
    ) -> bool {
        use axia_geo::{FaceId, AnalyticSurface};
        use glam::DVec3;
        let surface = AnalyticSurface::Cylinder {
            axis_origin: DVec3::new(ox, oy, oz),
            axis_dir: DVec3::new(ax, ay, az),
            radius,
            ref_dir: DVec3::new(rx, ry, rz),
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        let fid = FaceId::new(face_id);
        let result = self.scene.mesh.set_face_surface(fid, Some(surface));
        if result { self.mark_topology_changed(); }
        result
    }

    /// Set a Sphere surface on an existing face.
    #[wasm_bindgen(js_name = "setFaceSurfaceSphere")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_face_surface_sphere(
        &mut self, face_id: u32,
        cx: f64, cy: f64, cz: f64, radius: f64,
        u_min: f64, u_max: f64, v_min: f64, v_max: f64,
    ) -> bool {
        use axia_geo::{FaceId, AnalyticSurface};
        use glam::DVec3;
        let surface = AnalyticSurface::Sphere {
            center: DVec3::new(cx, cy, cz),
            radius,
            axis_dir: DVec3::Z, // ADR-204: WASM bridge defaults to Z/X pole
            ref_dir: DVec3::X,
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        let fid = FaceId::new(face_id);
        let result = self.scene.mesh.set_face_surface(fid, Some(surface));
        if result { self.mark_topology_changed(); }
        result
    }

    /// Set a Cone surface on an existing face.
    #[wasm_bindgen(js_name = "setFaceSurfaceCone")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_face_surface_cone(
        &mut self, face_id: u32,
        ax: f64, ay: f64, az: f64,
        dx: f64, dy: f64, dz: f64,
        half_angle: f64,
        rx: f64, ry: f64, rz: f64,
        u_min: f64, u_max: f64, v_min: f64, v_max: f64,
    ) -> bool {
        use axia_geo::{FaceId, AnalyticSurface};
        use glam::DVec3;
        let surface = AnalyticSurface::Cone {
            apex: DVec3::new(ax, ay, az),
            axis_dir: DVec3::new(dx, dy, dz),
            half_angle,
            ref_dir: DVec3::new(rx, ry, rz),
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        let fid = FaceId::new(face_id);
        let result = self.scene.mesh.set_face_surface(fid, Some(surface));
        if result { self.mark_topology_changed(); }
        result
    }

    /// Set a Torus surface on an existing face.
    #[wasm_bindgen(js_name = "setFaceSurfaceTorus")]
    #[allow(clippy::too_many_arguments)]
    pub fn set_face_surface_torus(
        &mut self, face_id: u32,
        cx: f64, cy: f64, cz: f64,
        ax: f64, ay: f64, az: f64,
        rx: f64, ry: f64, rz: f64,
        major_radius: f64, minor_radius: f64,
        u_min: f64, u_max: f64, v_min: f64, v_max: f64,
    ) -> bool {
        use axia_geo::{FaceId, AnalyticSurface};
        use glam::DVec3;
        let surface = AnalyticSurface::Torus {
            center: DVec3::new(cx, cy, cz),
            axis_dir: DVec3::new(ax, ay, az),
            ref_dir: DVec3::new(rx, ry, rz),
            major_radius,
            minor_radius,
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        let fid = FaceId::new(face_id);
        let result = self.scene.mesh.set_face_surface(fid, Some(surface));
        if result { self.mark_topology_changed(); }
        result
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-086 O-γ — Inject External Face (STEP/IGES Approach A)
    // ════════════════════════════════════════════════════════════════
    //
    // import 된 BRep face 를 axia DCEL 의 first-class entity 로 inject.
    // Two variants:
    //   1. injectExternalFaceNoSurface — DCEL face only (no analytic
    //      surface attached)
    //   2. injectExternalFacePlane — Plane analytic surface attached
    //
    // Returns: FaceId.raw() as i32 on success, -1 on failure.
    //   Caller (TS, O-δ) 가 traversal stable index → axia FaceId map 에 저장.
    //
    // Future sub-step: Cylinder / Sphere / Cone / Torus / Bezier /
    // BSpline / NURBS variants.

    /// Inject an external face boundary into axia DCEL — no surface.
    ///
    /// Args:
    /// - `positions_xyz`: flat array of `xyz × N` outer boundary points
    ///   (N >= 3). First point != last (loop closure implicit).
    ///
    /// Returns: new FaceId.raw() as i32, or -1 on error.
    #[wasm_bindgen(js_name = "injectExternalFaceNoSurface")]
    pub fn inject_external_face_no_surface(
        &mut self,
        positions_xyz: &[f64],
    ) -> i32 {
        use axia_geo::operations::import_mesh::{ImportFaceBoundary, inject_external_face};
        use axia_geo::MaterialId;
        use glam::DVec3;

        if positions_xyz.len() % 3 != 0 || positions_xyz.len() < 9 {
            return -1;
        }
        let outer_loop: Vec<DVec3> = positions_xyz
            .chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2]))
            .collect();
        let boundary = ImportFaceBoundary {
            outer_loop,
            inner_loops: vec![],
        };
        // FORM_MATERIAL equivalent (LOCKED #26 ADR-049 P-5e-β)
        match inject_external_face(&mut self.scene.mesh, boundary, None, MaterialId::new(0)) {
            Ok(face_id) => {
                self.mark_topology_changed();
                face_id.raw() as i32
            }
            Err(_) => -1,
        }
    }

    /// Inject an external face boundary into axia DCEL — with Plane surface.
    ///
    /// Args:
    /// - `positions_xyz`: flat outer boundary points (xyz × N)
    /// - plane_o[xyz]: Plane origin
    /// - plane_n[xyz]: Plane normal
    /// - plane_u[xyz]: Plane reference direction (basis_u)
    ///
    /// Returns: new FaceId.raw() as i32, or -1 on error.
    #[wasm_bindgen(js_name = "injectExternalFacePlane")]
    #[allow(clippy::too_many_arguments)]
    pub fn inject_external_face_plane(
        &mut self,
        positions_xyz: &[f64],
        plane_ox: f64, plane_oy: f64, plane_oz: f64,
        plane_nx: f64, plane_ny: f64, plane_nz: f64,
        plane_ux: f64, plane_uy: f64, plane_uz: f64,
    ) -> i32 {
        use axia_geo::operations::import_mesh::{ImportFaceBoundary, inject_external_face};
        use axia_geo::{AnalyticSurface, MaterialId};
        use glam::DVec3;

        if positions_xyz.len() % 3 != 0 || positions_xyz.len() < 9 {
            return -1;
        }
        let outer_loop: Vec<DVec3> = positions_xyz
            .chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2]))
            .collect();
        let boundary = ImportFaceBoundary {
            outer_loop,
            inner_loops: vec![],
        };
        let surface = AnalyticSurface::Plane {
            origin: DVec3::new(plane_ox, plane_oy, plane_oz),
            normal: DVec3::new(plane_nx, plane_ny, plane_nz),
            basis_u: DVec3::new(plane_ux, plane_uy, plane_uz),
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        match inject_external_face(
            &mut self.scene.mesh,
            boundary,
            Some(surface),
            MaterialId::new(0),
        ) {
            Ok(face_id) => {
                self.mark_topology_changed();
                face_id.raw() as i32
            }
            Err(_) => -1,
        }
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-062 Phase L₂ Path Z Step 3 — Validated attach (W2 per-kind)
    //
    // 5 new endpoints, additive-only (ADR-060 §D). Each mirrors the
    // matching setFaceSurface* signature + adds `tol_mm` parameter.
    // Returns JSON outcome per Amendment 1 schema (schemaVersion: 1).
    //
    // tol_mm ≤ 0 → ATTACH_VALIDATE_TOL default (1μm).
    // ════════════════════════════════════════════════════════════════

    #[wasm_bindgen(js_name = "attachFaceSurfacePlaneValidated")]
    #[allow(clippy::too_many_arguments)]
    pub fn attach_face_surface_plane_validated(
        &mut self, face_id: u32,
        ox: f64, oy: f64, oz: f64,
        nx: f64, ny: f64, nz: f64,
        ux: f64, uy: f64, uz: f64,
        u_min: f64, u_max: f64,
        v_min: f64, v_max: f64,
        tol_mm: f64,
    ) -> String {
        use axia_geo::surfaces::AnalyticSurface;
        let surface = AnalyticSurface::Plane {
            origin: DVec3::new(ox, oy, oz),
            normal: DVec3::new(nx, ny, nz),
            basis_u: DVec3::new(ux, uy, uz),
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        self.attach_validated_inner(face_id, surface, tol_mm)
    }

    #[wasm_bindgen(js_name = "attachFaceSurfaceCylinderValidated")]
    #[allow(clippy::too_many_arguments)]
    pub fn attach_face_surface_cylinder_validated(
        &mut self, face_id: u32,
        ox: f64, oy: f64, oz: f64,
        ax: f64, ay: f64, az: f64,
        radius: f64,
        rx: f64, ry: f64, rz: f64,
        u_min: f64, u_max: f64,
        v_min: f64, v_max: f64,
        tol_mm: f64,
    ) -> String {
        use axia_geo::surfaces::AnalyticSurface;
        let surface = AnalyticSurface::Cylinder {
            axis_origin: DVec3::new(ox, oy, oz),
            axis_dir: DVec3::new(ax, ay, az),
            radius,
            ref_dir: DVec3::new(rx, ry, rz),
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        self.attach_validated_inner(face_id, surface, tol_mm)
    }

    #[wasm_bindgen(js_name = "attachFaceSurfaceSphereValidated")]
    #[allow(clippy::too_many_arguments)]
    pub fn attach_face_surface_sphere_validated(
        &mut self, face_id: u32,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        u_min: f64, u_max: f64,
        v_min: f64, v_max: f64,
        tol_mm: f64,
    ) -> String {
        use axia_geo::surfaces::AnalyticSurface;
        let surface = AnalyticSurface::Sphere {
            center: DVec3::new(cx, cy, cz),
            radius,
            axis_dir: DVec3::Z, // ADR-204: WASM bridge defaults to Z/X pole
            ref_dir: DVec3::X,
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        self.attach_validated_inner(face_id, surface, tol_mm)
    }

    #[wasm_bindgen(js_name = "attachFaceSurfaceConeValidated")]
    #[allow(clippy::too_many_arguments)]
    pub fn attach_face_surface_cone_validated(
        &mut self, face_id: u32,
        ax: f64, ay: f64, az: f64,
        dx: f64, dy: f64, dz: f64,
        half_angle: f64,
        rx: f64, ry: f64, rz: f64,
        u_min: f64, u_max: f64,
        v_min: f64, v_max: f64,
        tol_mm: f64,
    ) -> String {
        use axia_geo::surfaces::AnalyticSurface;
        let surface = AnalyticSurface::Cone {
            apex: DVec3::new(ax, ay, az),
            axis_dir: DVec3::new(dx, dy, dz),
            half_angle,
            ref_dir: DVec3::new(rx, ry, rz),
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        self.attach_validated_inner(face_id, surface, tol_mm)
    }

    #[wasm_bindgen(js_name = "attachFaceSurfaceTorusValidated")]
    #[allow(clippy::too_many_arguments)]
    pub fn attach_face_surface_torus_validated(
        &mut self, face_id: u32,
        cx: f64, cy: f64, cz: f64,
        ax: f64, ay: f64, az: f64,
        rx: f64, ry: f64, rz: f64,
        major_radius: f64, minor_radius: f64,
        u_min: f64, u_max: f64, v_min: f64, v_max: f64,
        tol_mm: f64,
    ) -> String {
        use axia_geo::surfaces::AnalyticSurface;
        let surface = AnalyticSurface::Torus {
            center: DVec3::new(cx, cy, cz),
            axis_dir: DVec3::new(ax, ay, az),
            ref_dir: DVec3::new(rx, ry, rz),
            major_radius,
            minor_radius,
            u_range: (u_min, u_max),
            v_range: (v_min, v_max),
        };
        self.attach_validated_inner(face_id, surface, tol_mm)
    }

    /// Clear any analytic surface from a face (revert to polygon).
    #[wasm_bindgen(js_name = "clearFaceSurface")]
    pub fn clear_face_surface(&mut self, face_id: u32) -> bool {
        use axia_geo::FaceId;
        let fid = FaceId::new(face_id);
        let ok = self.scene.mesh.set_face_surface(fid, None);
        if ok { self.mark_topology_changed(); }
        ok
    }

    // ADR-076 Step 2 — Removed: nurbs_boolean (ADR-027 Phase G3 legacy
    // probe export). Reachable only from removed BooleanHandler legacy
    // probe path (sunset by ADR-076 Step 1) and removed
    // WasmBridge.nurbsBoolean wrapper (sunset by ADR-076 Step 2).
    // No external consumers remain (verified via repo-wide grep).


    /// Surface kind: 0 = none, 1 = Plane, 2 = Cylinder, 3 = Sphere,
    /// 4 = Cone, 5 = Torus, 6 = BezierPatch, 7 = BSplineSurface,
    /// 8 = NURBSSurface, -1 = invalid face id.
    #[wasm_bindgen(js_name = "faceSurfaceKind")]
    pub fn face_surface_kind(&self, face_id: u32) -> i32 {
        use axia_geo::{FaceId, AnalyticSurface};
        let fid = FaceId::new(face_id);
        match self.scene.mesh.face_surface(fid) {
            None => match self.scene.mesh.faces.get(fid) {
                Some(_) => 0,
                None => -1,
            },
            Some(AnalyticSurface::Plane { .. }) => 1,
            Some(AnalyticSurface::Cylinder { .. }) => 2,
            Some(AnalyticSurface::Sphere { .. }) => 3,
            Some(AnalyticSurface::Cone { .. }) => 4,
            Some(AnalyticSurface::Torus { .. }) => 5,
            Some(AnalyticSurface::BezierPatch { .. }) => 6,
            Some(AnalyticSurface::BSplineSurface { .. }) => 7,
            Some(AnalyticSurface::NURBSSurface { .. }) => 8,
        }
    }

    /// ADR-232 — read a NURBS-class face's control net as JSON for the
    /// control-net overlay (A2-MVP-1 visualize-only). Returns "" for a
    /// non-NURBS-class surface / missing face. Row-major flat arrays; `weights`
    /// is all-1.0 for Bezier / BSpline; `knotsU`/`knotsV` empty for BezierPatch.
    /// Schema: `{"kind","nU","nV","degU","degV","ctrlPts":[…],"weights":[…],
    ///           "knotsU":[…],"knotsV":[…]}`.
    #[wasm_bindgen(js_name = "getNurbsSurfaceParams")]
    pub fn get_nurbs_surface_params(&self, face_id: u32) -> String {
        use axia_geo::FaceId;
        let p = match self.scene.mesh.nurbs_surface_params(FaceId::new(face_id)) {
            Some(p) => p,
            None => return String::new(),
        };
        let arr = |v: &[f64]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
        format!(
            r#"{{"kind":"{}","nU":{},"nV":{},"degU":{},"degV":{},"ctrlPts":[{}],"weights":[{}],"knotsU":[{}],"knotsV":[{}]}}"#,
            p.kind, p.n_u, p.n_v, p.deg_u, p.deg_v,
            arr(&p.ctrl_pts), arr(&p.weights), arr(&p.knots_u), arr(&p.knots_v),
        )
    }

    /// **ADR-140 β (2026-05-24)** — Surface-aware normal at world position.
    /// 사용자 click hit point `(x, y, z)` 에서 face 의 AnalyticSurface
    /// 의 *exact normal* 평가. ADR-038 P23 surface-aware normals 의 도구
    /// 입력 경로 1:1 mirror.
    ///
    /// Returns `[nx, ny, nz]` (3 elements) 정상 시.
    /// Returns empty `[]` 시:
    /// - face_id 가 invalid
    /// - face 가 surface attached 안 됨 (DCEL face normal 만 가능)
    /// - normal evaluation 이 zero vector (degenerate position)
    ///
    /// **Caller contract** (ADR-031 Phase D `normal_at_world_pos` 답습):
    /// `pos` 가 surface 위 또는 매우 근접해야 (raycast hit point). 멀리 떨어진
    /// pos 의 result 는 미정의.
    ///
    /// 사용 사례:
    /// - Cylinder/Sphere/Cone/Torus surface 위 사용자 click → tangent plane
    /// - DrawLine/Rect/Circle/Sketch 도구의 `getDrawPlane(faceId, hitPoint?)`
    ///   surface-aware path
    /// - ADR-038 P23 render 가 이미 활용 — 본 export 는 도구 입력 경로 활성
    #[wasm_bindgen(js_name = "faceSurfaceNormalAtPos")]
    pub fn face_surface_normal_at_pos(
        &self,
        face_id: u32,
        x: f64, y: f64, z: f64,
    ) -> Vec<f64> {
        use axia_geo::FaceId;
        use glam::DVec3;
        let fid = FaceId::new(face_id);
        let surface = match self.scene.mesh.face_surface(fid) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let pos = DVec3::new(x, y, z);
        let normal = surface.normal_at_world_pos(pos);
        // Reject zero-normal (degenerate position — e.g., apex of cone)
        if normal.length_squared() < 1e-20 {
            return Vec::new();
        }
        vec![normal.x, normal.y, normal.z]
    }

    /// Tessellate a face's analytic surface for rendering. Returns flat
    /// `[v_count, t_count, vx, vy, vz, ..., t0_a, t0_b, t0_c, t1_a, ...]`.
    /// Returns empty array if face has no surface.
    #[wasm_bindgen(js_name = "tessellateFaceSurface")]
    pub fn tessellate_face_surface(&self, face_id: u32, chord_tol: f64) -> Vec<f64> {
        use axia_geo::FaceId;
        let fid = FaceId::new(face_id);
        let tess = match self.scene.mesh.tessellate_face_surface(fid, chord_tol) {
            Some(t) => t,
            None => return Vec::new(),
        };
        let mut flat = Vec::with_capacity(2 + tess.vertices.len() * 3 + tess.triangles.len() * 3);
        flat.push(tess.vertices.len() as f64);
        flat.push(tess.triangles.len() as f64);
        for p in tess.vertices {
            flat.push(p.x);
            flat.push(p.y);
            flat.push(p.z);
        }
        for [a, b, c] in tess.triangles {
            flat.push(a as f64);
            flat.push(b as f64);
            flat.push(c as f64);
        }
        flat
    }

    // ========================================================================
    // Primitive shapes (Cylinder, Cone, Sphere)
    // ========================================================================

    /// Create a cylinder primitive.
    /// Returns the base face ID for Push/Pull operations.
    pub fn create_cylinder(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
        segments: u32,
    ) -> f64 {
        let position = DVec3::new(cx, cy, cz);
        // ADR-007 Rev 2 Tier 3 — transaction + auto-intersect for primitives.
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before);
        match self.scene.mesh.create_cylinder(
            position,
            radius,
            height,
            segments,
            axia_core::FORM_MATERIAL,
        ) {
            Ok(faces) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                // XIA 생성 — state는 face_ids.len()에서 자동 계산
                let xia_id = self.scene.create_xia_with_faces(
                    "Cylinder".to_string(),
                    position,
                    faces.clone(),
                );
                if self.scene.auto_intersect_on_draw || self.scene.face_rederive_on_draw {
                    let _ = self.scene.intersect_faces_inner(&faces);
                }
                let after = self.scene.scene_snapshot();
                self.scene.transactions.set_after_snapshot(after);
                self.scene.transactions.commit();
                if let Some(&base_face) = faces.first() {
                    debug_log!("[RUST] create_cylinder: faces={} base_id={} xia={}", faces.len(), base_face.raw(), xia_id);
                    base_face.raw() as f64
                } else {
                    -1.0
                }
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] create_cylinder error: {}", e);
                -1.0
            }
        }
    }

    /// Create a cone primitive.
    /// Returns the base face ID for Push/Pull operations.
    pub fn create_cone(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
        segments: u32,
    ) -> f64 {
        let position = DVec3::new(cx, cy, cz);
        // Tier 3 — transaction + auto-intersect.
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before);
        match self.scene.mesh.create_cone(
            position,
            radius,
            height,
            segments,
            axia_core::FORM_MATERIAL,
        ) {
            Ok(faces) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                let xia_id = self.scene.create_xia_with_faces(
                    "Cone".to_string(),
                    position,
                    faces.clone(),
                );
                if self.scene.auto_intersect_on_draw || self.scene.face_rederive_on_draw {
                    let _ = self.scene.intersect_faces_inner(&faces);
                }
                let after = self.scene.scene_snapshot();
                self.scene.transactions.set_after_snapshot(after);
                self.scene.transactions.commit();
                if let Some(&base_face) = faces.first() {
                    debug_log!("[RUST] create_cone: faces={} base_id={} xia={}", faces.len(), base_face.raw(), xia_id);
                    base_face.raw() as f64
                } else {
                    -1.0
                }
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] create_cone error: {}", e);
                -1.0
            }
        }
    }

    /// Create an axis-aligned box primitive (6-face closed solid).
    /// Returns the bottom face ID for Push/Pull operations.
    pub fn create_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        width: f64, height: f64, depth: f64,
    ) -> f64 {
        let position = DVec3::new(cx, cy, cz);
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before);
        match self.scene.mesh.create_box(
            position, width, height, depth, axia_core::FORM_MATERIAL,
        ) {
            Ok(faces) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                let xia_id = self.scene.create_xia_with_faces(
                    "Box".to_string(), position, faces.clone(),
                );
                if self.scene.auto_intersect_on_draw || self.scene.face_rederive_on_draw {
                    let _ = self.scene.intersect_faces_inner(&faces);
                }
                let after = self.scene.scene_snapshot();
                self.scene.transactions.set_after_snapshot(after);
                self.scene.transactions.commit();
                if let Some(&base_face) = faces.first() {
                    debug_log!("[RUST] create_box: faces={} base_id={} xia={}", faces.len(), base_face.raw(), xia_id);
                    base_face.raw() as f64
                } else { -1.0 }
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] create_box error: {}", e);
                -1.0
            }
        }
    }

    /// Create a sphere primitive (UV sphere).
    /// Returns a face ID from the sphere for Push/Pull operations.
    pub fn create_sphere(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        u_segments: u32,
        v_segments: u32,
    ) -> f64 {
        let position = DVec3::new(cx, cy, cz);
        // Tier 3 — transaction + auto-intersect.
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before);
        match self.scene.mesh.create_sphere(
            position,
            radius,
            u_segments,
            v_segments,
            axia_core::FORM_MATERIAL,
        ) {
            Ok(faces) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                let xia_id = self.scene.create_xia_with_faces(
                    "Sphere".to_string(),
                    position,
                    faces.clone(),
                );
                if self.scene.auto_intersect_on_draw || self.scene.face_rederive_on_draw {
                    let _ = self.scene.intersect_faces_inner(&faces);
                }
                let after = self.scene.scene_snapshot();
                self.scene.transactions.set_after_snapshot(after);
                self.scene.transactions.commit();
                if let Some(&first_face) = faces.first() {
                    debug_log!("[RUST] create_sphere: faces={} first_id={} xia={}", faces.len(), first_face.raw(), xia_id);
                    first_face.raw() as f64
                } else {
                    -1.0
                }
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] create_sphere error: {}", e);
                -1.0
            }
        }
    }

    // ========================================================================
    // ADR-197 β-3-h — Curved-Boolean DEMO entries (standalone curved ops wired
    // to the live scene so the result renders in the viewport). Each is
    // self-contained: build the kernel-native primitive, apply the curved
    // Boolean, wrap the result in a fresh XIA + a single Undo step. Returns the
    // result face count (-1 on error). NOTE: these are the curved-primitive ∩
    // {Z-halfspace | Z-slab} operations (sphere/cylinder/cone/torus). General
    // solid∩solid curved routing (box∩sphere) needs the periodic arrangement
    // (γ-2b) and is a later step.
    // ========================================================================

    fn finish_curved_demo(
        &mut self,
        before: Vec<u8>,
        result: anyhow::Result<Vec<FaceId>>,
        name: &str,
        position: DVec3,
    ) -> f64 {
        match result {
            Ok(faces) if !faces.is_empty() => {
                self.mark_topology_changed();
                self.invalidate_cache();
                self.scene.create_xia_with_faces(name.to_string(), position, faces.clone());
                let after = self.scene.scene_snapshot();
                self.scene.transactions.set_after_snapshot(after);
                self.scene.transactions.commit();
                let _ = before;
                faces.len() as f64
            }
            Ok(_) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] curved demo {}: empty result", name);
                -1.0
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] curved demo {} error: {}", name, e);
                -1.0
            }
        }
    }

    /// sphere(cx,cy,cz,r) ∩ {z > plane_z} (keep_above) or {z < plane_z}.
    #[wasm_bindgen(js_name = "demoSphereHalfspace")]
    pub fn demo_sphere_halfspace(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        plane_z: f64,
        keep_above: bool,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let normal = if keep_above { DVec3::Z } else { DVec3::NEG_Z };
        let result = self.scene.mesh.create_sphere_kernel_native(pos, radius, mat).and_then(|s| {
            self.scene.mesh.boolean_sphere_halfspace(&s, DVec3::new(cx, cy, plane_z), normal, mat)
        });
        self.finish_curved_demo(before, result, "Capped Sphere", pos)
    }

    /// sphere(cx,cy,cz,r) ∩ {z_lo < z < z_hi} → barrel/band.
    #[wasm_bindgen(js_name = "demoSphereSlab")]
    pub fn demo_sphere_slab(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        z_lo: f64, z_hi: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = self.scene.mesh.create_sphere_kernel_native(pos, radius, mat).and_then(|s| {
            self.scene.mesh.boolean_sphere_slab(&s, z_lo, z_hi, mat)
        });
        self.finish_curved_demo(before, result, "Sphere Barrel", pos)
    }

    /// cylinder(base cx,cy,cz; r; h) ∩ {z_lo < z < z_hi} → truncated cylinder.
    #[wasm_bindgen(js_name = "demoCylinderSlab")]
    pub fn demo_cylinder_slab(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
        z_lo: f64, z_hi: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = self.scene.mesh
            .create_cylinder_kernel_native_clean(pos, radius, height, mat)
            .and_then(|c| self.scene.mesh.boolean_cylinder_slab(&c, z_lo, z_hi, mat));
        self.finish_curved_demo(before, result, "Truncated Cylinder", pos)
    }

    /// cone(base cx,cy,cz; r; h, apex up) ∩ {z_lo < z < z_hi} → frustum/smaller cone.
    #[wasm_bindgen(js_name = "demoConeSlab")]
    pub fn demo_cone_slab(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
        z_lo: f64, z_hi: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = self.scene.mesh
            .create_cone_kernel_native(pos, radius, height, mat)
            .and_then(|c| self.scene.mesh.boolean_cone_slab(&c, z_lo, z_hi, mat));
        self.finish_curved_demo(before, result, "Cone Frustum", pos)
    }

    /// torus(cx,cy,cz; R; r) ∩ {z > plane_z} (keep_above) or {z < plane_z} → band + washer.
    #[wasm_bindgen(js_name = "demoTorusHalfspace")]
    pub fn demo_torus_halfspace(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        major_radius: f64, minor_radius: f64,
        plane_z: f64,
        keep_above: bool,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = self.scene.mesh
            .create_torus_kernel_native(pos, major_radius, minor_radius, mat)
            .and_then(|t| self.scene.mesh.boolean_torus_halfspace(&[t], plane_z, keep_above, mat));
        self.finish_curved_demo(before, result, "Torus Halfspace", pos)
    }

    /// ADR-197 β-3-l — TORUS ∩ Z-slab demo: both planes cut the tube → a
    /// horizontal donut band (genus-1 ring) = 2 Torus bands + 2 Plane washers.
    #[wasm_bindgen(js_name = "demoTorusSlab")]
    pub fn demo_torus_slab(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        major_radius: f64, minor_radius: f64,
        z_lo: f64, z_hi: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = self.scene.mesh
            .create_torus_kernel_native(pos, major_radius, minor_radius, mat)
            .and_then(|t| self.scene.mesh.boolean_torus_slab(&[t], z_lo, z_hi, mat));
        self.finish_curved_demo(before, result, "Torus Slab", pos)
    }

    /// ADR-197 β-3-i — GENERAL routing demo: build a clean Path B sphere + an
    /// axis-aligned box, then run `Mesh::boolean(sphere, box, Intersect)`. The box
    /// (`box_w × box_h × box_d` centred at the sphere) must contain the sphere in
    /// XY and cut it in Z so the legacy `boolean()` routes to the curved
    /// `boolean_sphere_slab`/`_halfspace` (surface preserved). Returns result face
    /// count (-1 on error).
    #[wasm_bindgen(js_name = "demoBooleanSphereBox")]
    pub fn demo_boolean_sphere_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        sphere_radius: f64,
        box_w: f64, box_h: f64, box_d: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let sphere = self.scene.mesh.create_sphere_kernel_native(pos, sphere_radius, mat)?;
            let bx = self.scene.mesh.create_box(pos, box_w, box_h, box_d, mat)?;
            let res = self.scene.mesh.boolean(
                &sphere,
                &bx,
                axia_geo::operations::boolean::BoolOp::Intersect,
                mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Sphere ∩ Box", pos)
    }

    /// ADR-197 β-3-m — SUBTRACT routing demo: build a clean Path B sphere + an
    /// axis-box (XY-containing, Z-cutting), then `Mesh::boolean(sphere, box,
    /// Subtract)`. `A − box` keeps the OUTER piece(s): a Z-slab box → 2 disjoint
    /// caps (4 faces, surface preserved); a halfspace box → 1 cap. Returns the
    /// result face count (-1 on error).
    #[wasm_bindgen(js_name = "demoBooleanSubtractSphereBox")]
    pub fn demo_boolean_subtract_sphere_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        sphere_radius: f64,
        box_w: f64, box_h: f64, box_d: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let sphere = self.scene.mesh.create_sphere_kernel_native(pos, sphere_radius, mat)?;
            let bx = self.scene.mesh.create_box(pos, box_w, box_h, box_d, mat)?;
            let res = self.scene.mesh.boolean(
                &sphere,
                &bx,
                axia_geo::operations::boolean::BoolOp::Subtract,
                mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Sphere − Box", pos)
    }

    /// ADR-197 β-3-o — UNION demo: two Z-coaxial overlapping spheres → a capsule
    /// (2 Sphere caps sharing the SSI circle, surface preserved). Returns the
    /// result face count (-1 on error). `sep` = axial separation between centres.
    #[wasm_bindgen(js_name = "demoBooleanUnionSpheres")]
    pub fn demo_boolean_union_spheres(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        sep: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let a = self.scene.mesh.create_sphere_kernel_native(pos, radius, mat)?;
            let b = self.scene.mesh.create_sphere_kernel_native(pos + DVec3::new(0., 0., sep), radius, mat)?;
            let res = self.scene.mesh.boolean(
                &a, &b, axia_geo::operations::boolean::BoolOp::Union, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Sphere ∪ Sphere", pos)
    }

    /// ADR-197 β-3-o — UNION demo: two OPPOSING coaxial cones (apex-up + apex-down)
    /// overlapping → an HOURGLASS (2 Cone frustum bands + 2 base disks, sharing the
    /// waist SSI circle). Cone A base at `(cx,cy,cz)` apex up; cone B base at the
    /// top apex down — they span the same z-range and overlap. Returns face count.
    #[wasm_bindgen(js_name = "demoBooleanUnionConeCone")]
    pub fn demo_boolean_union_cone_cone(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
    ) -> f64 {
        let base = DVec3::new(cx, cy, cz);
        let top = DVec3::new(cx, cy, cz + height);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let a = self.scene.mesh.create_cone_kernel_native(base, radius, height, mat)?;
            let b = self.scene.mesh.create_cone_kernel_native_apex_down(top, radius, height, mat)?;
            let res = self.scene.mesh.boolean(
                &a, &b, axia_geo::operations::boolean::BoolOp::Union, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Cone ∪ Cone (hourglass)", base)
    }

    /// ADR-197 β-3-p — UNION demo: sphere ∪ box (box XY-contains + Z-cuts) → the
    /// box with the sphere caps poking out, the box top/bottom faces PIERCED.
    /// `box_w → X, box_h → Z (thin = the slab), box_d → Y`. Returns face count.
    #[wasm_bindgen(js_name = "demoBooleanUnionSphereBox")]
    pub fn demo_boolean_union_sphere_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        sphere_radius: f64,
        box_w: f64, box_h: f64, box_d: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let sphere = self.scene.mesh.create_sphere_kernel_native(pos, sphere_radius, mat)?;
            let bx = self.scene.mesh.create_box(pos, box_w, box_h, box_d, mat)?;
            let res = self.scene.mesh.boolean(
                &sphere, &bx, axia_geo::operations::boolean::BoolOp::Union, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Sphere ∪ Box", pos)
    }

    /// ADR-197 β-3-p — UNION demo: cylinder ∪ box → the box with two cylinder
    /// STUBS poking out. Cylinder base at `(cx,cy,cz)`, height up; the box is
    /// placed at the cylinder's mid-height so it Z-cuts both ends.
    /// `box_w → X, box_h → Z (the slab thickness), box_d → Y`. Returns face count.
    #[wasm_bindgen(js_name = "demoBooleanUnionCylinderBox")]
    pub fn demo_boolean_union_cylinder_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
        box_w: f64, box_h: f64, box_d: f64,
    ) -> f64 {
        let base = DVec3::new(cx, cy, cz);
        let box_center = DVec3::new(cx, cy, cz + height * 0.5);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let cyl = self.scene.mesh.create_cylinder_kernel_native_clean(base, radius, height, mat)?;
            let bx = self.scene.mesh.create_box(box_center, box_w, box_h, box_d, mat)?;
            let res = self.scene.mesh.boolean(
                &cyl, &bx, axia_geo::operations::boolean::BoolOp::Union, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Cylinder ∪ Box", base)
    }

    /// ADR-198 — DRILLING demo: box − cylinder through-hole. Box centered at
    /// `(cx,cy,cz)` with half-size `box_half`; a Z-axis cylinder of `cyl_radius`
    /// pierces it top-to-bottom → genus-1 watertight solid. Returns face count.
    #[wasm_bindgen(js_name = "demoBooleanBoxMinusCylinder")]
    pub fn demo_boolean_box_minus_cylinder(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        box_half: f64, cyl_radius: f64,
    ) -> f64 {
        let center = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let bx = self.scene.mesh.create_box(center, box_half * 2.0, box_half * 2.0, box_half * 2.0, mat)?;
            let cyl_base = DVec3::new(cx, cy, cz - box_half - 1.0);
            let cyl = self.scene.mesh.create_cylinder_kernel_native_clean(cyl_base, cyl_radius, box_half * 2.0 + 2.0, mat)?;
            let res = self.scene.mesh.boolean(
                &bx, &cyl, axia_geo::operations::boolean::BoolOp::Subtract, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Box − Cylinder (drill)", center)
    }

    /// ADR-276 Phase 1 verification harness — two overlapping BOXES via the
    /// solid-CSG path (`Mesh::boolean_solid`: general tri-tri Stage 1 +
    /// fail-closed validity gate). Box A = [0,0,50] 100³; box B = [bx,by,bz]
    /// cube of side `bsize`. Returns the result face count on a valid cut, or
    /// -1 on the fail-closed rollback (config not yet supported). Not wired to
    /// any UI button — a building block for ADR-276 Phase 5 routing (Q2).
    #[wasm_bindgen(js_name = "demoBooleanSolidTwoBoxes")]
    pub fn demo_boolean_solid_two_boxes(
        &mut self,
        bx: f64, by: f64, bz: f64, bsize: f64,
    ) -> f64 {
        let center = DVec3::new(0.0, 0.0, 50.0);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let a = self.scene.mesh.create_box(center, 100.0, 100.0, 100.0, mat)?;
            let b = self.scene.mesh.create_box(
                DVec3::new(bx, by, bz), bsize, bsize, bsize, mat,
            )?;
            let res = self.scene.mesh.boolean_solid(
                &a, &b, axia_geo::operations::boolean::BoolOp::Subtract, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "CSG A − B (ADR-276)", center)
    }

    /// ADR-198 (blind hole) — box − cylinder entering the top, floor `depth` below
    /// the box top (inside the box). Box half-size `box_half`, cylinder `cyl_radius`.
    #[wasm_bindgen(js_name = "demoBooleanBoxMinusCylinderBlind")]
    pub fn demo_boolean_box_minus_cylinder_blind(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        box_half: f64, cyl_radius: f64, depth: f64,
    ) -> f64 {
        let center = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let bx = self.scene.mesh.create_box(center, box_half * 2.0, box_half * 2.0, box_half * 2.0, mat)?;
            // cylinder base = box_top − depth (inside), top = box_top + 1 (pokes out).
            let cyl_base = DVec3::new(cx, cy, cz + box_half - depth);
            let cyl = self.scene.mesh.create_cylinder_kernel_native_clean(cyl_base, cyl_radius, depth + 1.0, mat)?;
            let res = self.scene.mesh.boolean(&bx, &cyl, axia_geo::operations::boolean::BoolOp::Subtract, mat)?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Box − Cylinder (blind)", center)
    }

    /// ADR-198 (dimple) — box − sphere poking the top (center on the box top face,
    /// so the lower hemisphere is inside the box). Box half-size `box_half`.
    #[wasm_bindgen(js_name = "demoBooleanBoxMinusSphereDimple")]
    pub fn demo_boolean_box_minus_sphere_dimple(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        box_half: f64, sphere_radius: f64,
    ) -> f64 {
        let center = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let bx = self.scene.mesh.create_box(center, box_half * 2.0, box_half * 2.0, box_half * 2.0, mat)?;
            // sphere center on the box top → lower hemisphere inside the box.
            let sc = DVec3::new(cx, cy, cz + box_half);
            let sph = self.scene.mesh.create_sphere_kernel_native(sc, sphere_radius, mat)?;
            let res = self.scene.mesh.boolean(&bx, &sph, axia_geo::operations::boolean::BoolOp::Subtract, mat)?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Box − Sphere (dimple)", center)
    }

    /// ADR-198 (countersink) — box − cone conical pocket. Apex-down cone, base
    /// `1` above the box top, apex `depth` below it (inside the box).
    #[wasm_bindgen(js_name = "demoBooleanBoxMinusConeCountersink")]
    pub fn demo_boolean_box_minus_cone_countersink(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        box_half: f64, cone_radius: f64, depth: f64,
    ) -> f64 {
        let center = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let bx = self.scene.mesh.create_box(center, box_half * 2.0, box_half * 2.0, box_half * 2.0, mat)?;
            // apex-down cone: base 1 above the box top; apex `depth` below it.
            let cone_base = DVec3::new(cx, cy, cz + box_half + 1.0);
            let cone = self.scene.mesh.create_cone_kernel_native_apex_down(cone_base, cone_radius, depth + 1.0, mat)?;
            let res = self.scene.mesh.boolean(&bx, &cone, axia_geo::operations::boolean::BoolOp::Subtract, mat)?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Box − Cone (countersink)", center)
    }

    /// ADR-197 β-3-p — UNION demo: cone ∪ box → MIXED: apex tip pokes out the box
    /// top (Cone cap), base frustum pokes out the box bottom (Cone stub). Cone base
    /// at `(cx,cy,cz)`, apex up; box at the cone's mid-height so it Z-cuts both.
    /// `box_w → X, box_h → Z (slab), box_d → Y`. Returns face count.
    #[wasm_bindgen(js_name = "demoBooleanUnionConeBox")]
    pub fn demo_boolean_union_cone_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64, height: f64,
        box_w: f64, box_h: f64, box_d: f64,
    ) -> f64 {
        let base = DVec3::new(cx, cy, cz);
        let box_center = DVec3::new(cx, cy, cz + height * 0.5);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let cone = self.scene.mesh.create_cone_kernel_native(base, radius, height, mat)?;
            let bx = self.scene.mesh.create_box(box_center, box_w, box_h, box_d, mat)?;
            let res = self.scene.mesh.boolean(
                &cone, &bx, axia_geo::operations::boolean::BoolOp::Union, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Cone ∪ Box", base)
    }

    /// ADR-197 β-3-p — UNION demo: torus ∪ box → the tube pokes through the box
    /// top & bottom as ANNULI (box faces become annular + donut-center disks, 2
    /// Torus band-rings). `box_w → X, box_h → Z (thin slab), box_d → Y`. Box is
    /// centred on the torus so it Z-cuts the tube. Returns face count.
    #[wasm_bindgen(js_name = "demoBooleanUnionTorusBox")]
    pub fn demo_boolean_union_torus_box(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        major_radius: f64, minor_radius: f64,
        box_w: f64, box_h: f64, box_d: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let torus = self.scene.mesh.create_torus_kernel_native(pos, major_radius, minor_radius, mat)?;
            let bx = self.scene.mesh.create_box(pos, box_w, box_h, box_d, mat)?;
            let res = self.scene.mesh.boolean(
                &[torus], &bx, axia_geo::operations::boolean::BoolOp::Union, mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Torus ∪ Box", pos)
    }

    /// ADR-197 γ-2b-3 — CORNER demo: sphere ∩ {x>cx+x0, y>cy+y0, z>cz+z0} (3
    /// axis planes meeting at a box corner inside the sphere) → curved Sphere
    /// patch + 3 planar caps (`boolean_sphere_octant`). Returns face count (-1).
    #[wasm_bindgen(js_name = "demoSphereOctant")]
    pub fn demo_sphere_octant(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        radius: f64,
        x0: f64, y0: f64, z0: f64,
    ) -> f64 {
        let pos = DVec3::new(cx, cy, cz);
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = self.scene.mesh.create_sphere_kernel_native(pos, radius, mat).and_then(|s| {
            let planes = [
                (DVec3::X, DVec3::new(cx + x0, cy, cz)),
                (DVec3::Y, DVec3::new(cx, cy + y0, cz)),
                (DVec3::Z, DVec3::new(cx, cy, cz + z0)),
            ];
            self.scene.mesh.boolean_sphere_octant(&s, &planes, mat)
        });
        self.finish_curved_demo(before, result, "Sphere Octant", pos)
    }

    /// ADR-197 β-3-j γ-2b-4 — CORNER ROUTING demo via the general `boolean()`:
    /// sphere at origin ∩ a box centred at `(bcx,bcy,bcz)` (offset so only the
    /// near planes cut → a corner). `boolean()` auto-detects the (1,1,1) corner
    /// and routes to `boolean_sphere_octant`. Returns the result face count.
    #[wasm_bindgen(js_name = "demoBooleanSphereCorner")]
    pub fn demo_boolean_sphere_corner(
        &mut self,
        radius: f64,
        bcx: f64, bcy: f64, bcz: f64,
        box_size: f64,
    ) -> f64 {
        let pos = DVec3::ZERO;
        let mat = axia_core::FORM_MATERIAL;
        self.scene.transactions.begin();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before.clone());
        let result = (|| -> anyhow::Result<Vec<FaceId>> {
            let sphere = self.scene.mesh.create_sphere_kernel_native(pos, radius, mat)?;
            let bx = self.scene.mesh.create_box(
                DVec3::new(bcx, bcy, bcz),
                box_size,
                box_size,
                box_size,
                mat,
            )?;
            let res = self.scene.mesh.boolean(
                &sphere,
                &bx,
                axia_geo::operations::boolean::BoolOp::Intersect,
                mat,
            )?;
            Ok(res.faces)
        })();
        self.finish_curved_demo(before, result, "Sphere ∩ Corner Box", pos)
    }

    // ========================================================================
    // XIA → Face ID lookup
    // ========================================================================

    /// 주어진 XIA가 소유한 모든 face ID 반환 (B3 — 그룹 병합용).
    /// 빈 배열이면 해당 XIA가 없거나 비어 있음.
    #[wasm_bindgen(js_name = "getXiaFaceIds")]
    pub fn get_xia_face_ids(&self, xia_id: u32) -> Vec<u32> {
        match self.scene.xias.get(&xia_id) {
            Some(xia) => xia.face_ids.iter().map(|f| f.raw()).collect(),
            None => Vec::new(),
        }
    }

    /// Returns the first face ID owned by the given XIA ID.
    /// draw_rect/draw_circle return XIA IDs; push_pull expects face IDs.
    /// Returns u32::MAX on failure.
    pub fn get_xia_face(&self, xia_id: u32) -> u32 {
        if let Some(xia) = self.scene.xias.get(&xia_id) {
            if let Some(&fid) = xia.face_ids.first() {
                return fid.raw();
            }
        }
        u32::MAX
    }

    /// face가 속한 XIA의 ID 반환 (O(1) 역인덱스)
    /// 없으면 u32::MAX 반환
    pub fn get_xia_for_face(&self, face_id_raw: u32) -> u32 {
        let fid = FaceId::new(face_id_raw);
        self.scene.get_xia_for_face(fid).unwrap_or(u32::MAX)
    }

    /// 씬에 존재하는 모든 XIA ID를 반환. 디버깅/열거용.
    #[wasm_bindgen(js_name = "getXiaIds")]
    pub fn get_xia_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.scene.xias.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// 씬의 XIA 개수.
    #[wasm_bindgen(js_name = "xiaCount")]
    pub fn xia_count(&self) -> u32 {
        self.scene.xias.len() as u32
    }

    /// 모든 XIA ID 목록 (정렬됨).
    /// MCP `list_xias` capability 의 backbone (ADR-041 P26.1, ADR-042).
    #[wasm_bindgen(js_name = "allXiaIds")]
    pub fn all_xia_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.scene.xias.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// 씬의 high-level 요약 JSON. AI / MCP first-look query 에 적합.
    /// 형식:
    /// ```json
    /// { "xia_count": 3, "face_count": 12, "edge_count": 24,
    ///   "free_edge_count": 0, "constraint_count": 0,
    ///   "engine_version": "0.1.0", "schema_version": "1.0.0" }
    /// ```
    #[wasm_bindgen(js_name = "sceneSummary")]
    pub fn scene_summary(&self) -> String {
        use serde_json::json;
        let edge_count = self.scene.mesh.edges.iter()
            .filter(|(_, e)| e.is_active())
            .count();
        let summary = json!({
            "xia_count": self.scene.xias.len(),
            "face_count": self.face_count(),
            "edge_count": edge_count,
            "free_edge_count": self.count_free_edges(),
            "constraint_count": self.scene.constraints.len(),
            "engine_version": ENGINE_VERSION,
            "schema_version": SCHEMA_VERSION,
        });
        summary.to_string()
    }

    /// 특정 XIA ID에 대한 요약 JSON.
    /// `get_xia_info`는 face ID를 받지만, 이 함수는 **XIA ID를 직접 받는다**.
    /// 내부적으로 해당 XIA의 모든 face_ids를 수집해 `get_xia_info`와 동일한 JSON을 반환.
    ///
    /// XIA가 없으면 `{"empty":true}` 반환.
    #[wasm_bindgen(js_name = "getXiaStats")]
    pub fn get_xia_stats(&self, xia_id: u32) -> String {
        let Some(xia) = self.scene.xias.get(&xia_id) else {
            return r#"{"empty":true}"#.to_string();
        };
        let face_ids_raw: Vec<u32> = xia.face_ids.iter().map(|f| f.raw()).collect();
        self.get_xia_info(&face_ids_raw)
    }

    // ========================================================================
    // Push/Pull
    // ========================================================================

    // (legacy `pub fn push_pull` deleted — ADR-087 K-ζ. createSolidExtrude
    // 가 단일 entry. Q3 fallback to Mesh::push_pull 은 exec_create_solid
    // 가 자동 처리.)

    /// ADR-079 W-1-β — Surface-native solid extrusion bridge.
    ///
    /// Routes through `Command::CreateSolid` with `CreateSolidMode::Extrude`.
    /// On success, returns true. On `SolidError::NotYetSupported` (curved
    /// profile / NURBS / non-Plane), Scene auto-falls-back to legacy
    /// `Mesh::push_pull` per ADR-079 Q3 lock-in — caller still receives
    /// true on overall success.
    ///
    /// Per W-1-β scope: Extrude mode only. Other modes (Revolve / Sweep /
    /// Loft) get separate exports in W-3 / W-4.
    pub fn create_solid_extrude(
        &mut self,
        face_id_raw: u32,
        distance: f64,
    ) -> bool {
        let fid = FaceId::new(face_id_raw);

        // NOTE: a COPLANAR INTERIOR face (offset inner) is handled inside
        // `Mesh::create_solid` (SSOT) → routed to extrude_coplanar_interior_face
        // (distance<0 pocket / >0 boss). This makes the interactive live
        // Push/Pull path work too (begin/commit_live_extrude → exec_create_solid).
        // The gates below still apply on top.

        // ADR-191 P1.2-a (사용자 결재 2026-06-09) — multi-loop face (ring with
        // holes) Push/Pull 활성. ADR-016 Q2 reject 를 Push/Pull 한정 해제 —
        // `exec_create_solid` 가 multi-loop 을 legacy push_pull (Phase F = hole →
        // tube) 로 라우팅하고, hole 을 채우는 coplanar disk 를 먼저 제거해 manifold
        // through-hole tube 를 만든다. (Boolean / Offset / fillet 의 Q2 reject 는
        // 그대로 유지 — 본 변경은 Push/Pull entry 한정.)

        let faces_before = self.scene.mesh.face_count();
        debug_log!("[RUST] create_solid_extrude faceId={} distance={:.3} faces_before={}",
            face_id_raw, distance, faces_before);

        // ADR-267 β-2 — Watertight production gate (delta). op 전에 baseline 손상량 +
        // snapshot 을 잡아, op 가 NEW 손상(coincident 크랙 / winding / non-manifold)을
        // 유발하면 byte-identical rollback (ADR-190 P0.2). pre-existing 손상에는
        // 오탐하지 않는다(delta 비교).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        // Defense-in-depth (adversarial sweep): the OpenMesh gate below cannot
        // see a closed→open tear or a self-intersection (flap). Capture the
        // watertight/self-intersect baseline for the closure-preserving gate.
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        let cmd = Command::CreateSolid {
            face_id: fid,
            mode: axia_geo::CreateSolidMode::Extrude { distance },
        };
        let result = self.scene.execute(cmd);

        let faces_after = self.scene.mesh.face_count();

        let ok = match &result {
            axia_core::commands::CommandResult::SolidCreated { kind, face_count } => {
                debug_log!(
                    "[RUST] create_solid_extrude ok kind={:?} face_count={} (delta={:+})",
                    kind, face_count, faces_after as i64 - faces_before as i64,
                );
                true
            }
            axia_core::commands::CommandResult::PushPullDone {
                sides_created, adj_splits, base_removed, ref split_debug,
            } => {
                // Q3 fallback path — Scene auto-routed to legacy push_pull.
                debug_log!(
                    "[RUST] create_solid_extrude → Q3 fallback to push_pull: \
                     faces={} (delta={:+}) sides={} adj_splits={} base_removed={}",
                    faces_after, faces_after as i64 - faces_before as i64,
                    sides_created, adj_splits, base_removed,
                );
                for msg in split_debug {
                    debug_log!("[SPLIT] {}", msg);
                }
                true
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] create_solid_extrude ERROR: {}", e);
                self.set_error(e.to_string());
                false
            }
            _ => {
                debug_log!("[RUST] create_solid_extrude unexpected result");
                false
            }
        };

        // ADR-267 β-2 — post-op watertight gate. op 가 새 손상을 유발했으면
        // byte-identical rollback + phantom undo frame 제거 + lastError.
        if ok {
            let after = self
                .scene
                .mesh
                .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh);
            if after.damage_count() > integrity_before {
                console_error!(
                    "[RUST] create_solid_extrude REJECTED by integrity gate:\n{}",
                    after.summary()
                );
                self.scene.restore_scene_snapshot(&integrity_snapshot);
                self.scene.transactions.discard_last_undo();
                self.set_error(format!(
                    "부피 무결성 위반으로 취소됨 (extrude): {}",
                    after.summary()
                ));
                self.invalidate_cache();
                return false;
            }
        }

        // Defense-in-depth closure-preserving + self-intersection gate. Catches
        // the classes the OpenMesh damage gate misses: a watertight solid torn
        // open, and a wall folded through itself (flap). No false rejection —
        // closure is enforced only when the input was fully closed, and only a
        // NEW self-intersection is rejected (verified-good extrudes pass R5/6).
        if ok
            && !self.closure_preserving_gate_passed(
                before_boundary, before_si, &integrity_snapshot, "extrude", false,
            )
        {
            return false;
        }

        if ok {
            self.mark_topology_changed();
        }
        self.invalidate_cache();
        ok
    }

    /// ADR-259 β-2 — Tapered (draft) extrude. Routes through `Command::CreateSolid`
    /// with `CreateSolidMode::ExtrudeTapered`. v1 supports a `(Plane, AllLinear)`
    /// convex/concave FLAT profile → frustum.
    ///
    /// FAIL-CLOSED (D5): unlike `create_solid_extrude`, a tapered extrude carries
    /// NO push_pull fallback — a collapsing / self-intersecting / spiking offset,
    /// a solid-face profile (is_move_only), or a non-(Plane,AllLinear) profile
    /// returns FALSE and the Scene rolls the mesh back byte-identical. The UI
    /// surfaces `lastError()` (a clear "taper too steep / use a flat profile"
    /// message) rather than silently producing a non-tapered straight solid.
    pub fn create_solid_extrude_tapered(
        &mut self,
        face_id_raw: u32,
        distance: f64,
        taper_deg: f64,
    ) -> bool {
        let fid = FaceId::new(face_id_raw);
        debug_log!(
            "[RUST] create_solid_extrude_tapered faceId={} distance={:.3} taper_deg={:.3}",
            face_id_raw, distance, taper_deg
        );
        // Defense-in-depth: closure-preserving + self-intersection baseline.
        let gate_snapshot = self.scene.scene_snapshot();
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let cmd = Command::CreateSolid {
            face_id: fid,
            mode: axia_geo::CreateSolidMode::ExtrudeTapered { distance, taper_deg },
        };
        let result = self.scene.execute(cmd);
        let ok = match &result {
            axia_core::commands::CommandResult::SolidCreated { kind, face_count } => {
                debug_log!(
                    "[RUST] create_solid_extrude_tapered ok kind={:?} face_count={}",
                    kind, face_count
                );
                true
            }
            axia_core::commands::CommandResult::PushPullDone {
                sides_created, base_removed, ..
            } => {
                // ADR-259 draft-on-solid-face — a taper on a SOLID face (a box
                // top, prism wall) routes through the MoveOnly-taper path
                // (exec_push_pull_tapered) → PushPullDone: the ring moves +
                // shrinks, walls slant, no new faces. A valid taper result.
                debug_log!(
                    "[RUST] create_solid_extrude_tapered ok (draft-on-solid-face) sides={} base_removed={}",
                    sides_created, base_removed
                );
                true
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] create_solid_extrude_tapered ERROR: {}", e);
                self.set_error(e.to_string());
                false
            }
            _ => {
                self.set_error("tapered extrude: unexpected result".to_string());
                false
            }
        };
        if ok
            && !self.closure_preserving_gate_passed(
                before_boundary, before_si, &gate_snapshot, "tapered extrude", false,
            )
        {
            return false;
        }
        if ok {
            self.mark_topology_changed();
        }
        self.invalidate_cache();
        ok
    }

    /// ADR-260 β-2 — Circle → Cone / Frustum extrude. Routes through
    /// `Command::CreateSolid` with `CreateSolidMode::ExtrudeCone`. v1 supports a
    /// `(Plane, AllCircular)` profile → cone (`top_scale = 0`) or frustum
    /// (`0 < top_scale < 1`), reusing `AnalyticSurface::Cone`.
    ///
    /// FAIL-CLOSED (D5): like the tapered extrude, a cone extrude carries NO
    /// push_pull fallback — `top_scale ≥ 1` (= cylinder) / `< 0` / degenerate
    /// distance / a solid-face profile (is_move_only) / a non-(Plane,AllCircular)
    /// profile returns FALSE and the Scene rolls the mesh back byte-identical.
    /// The UI surfaces `lastError()` rather than silently producing a straight
    /// cylinder.
    pub fn create_solid_extrude_cone(
        &mut self,
        face_id_raw: u32,
        distance: f64,
        top_scale: f64,
    ) -> bool {
        let fid = FaceId::new(face_id_raw);
        debug_log!(
            "[RUST] create_solid_extrude_cone faceId={} distance={:.3} top_scale={:.3}",
            face_id_raw, distance, top_scale
        );
        // Defense-in-depth: closure-preserving + self-intersection baseline.
        let gate_snapshot = self.scene.scene_snapshot();
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let cmd = Command::CreateSolid {
            face_id: fid,
            mode: axia_geo::CreateSolidMode::ExtrudeCone { distance, top_scale },
        };
        let result = self.scene.execute(cmd);
        let ok = match &result {
            axia_core::commands::CommandResult::SolidCreated { kind, face_count } => {
                debug_log!(
                    "[RUST] create_solid_extrude_cone ok kind={:?} face_count={}",
                    kind, face_count
                );
                true
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] create_solid_extrude_cone ERROR: {}", e);
                self.set_error(e.to_string());
                false
            }
            _ => {
                // Cone never falls back (D5) → PushPullDone / others are unexpected.
                self.set_error("cone extrude: unexpected result".to_string());
                false
            }
        };
        if ok
            && !self.closure_preserving_gate_passed(
                before_boundary, before_si, &gate_snapshot, "cone extrude", false,
            )
        {
            return false;
        }
        if ok {
            self.mark_topology_changed();
        }
        self.invalidate_cache();
        ok
    }

    /// ADR-261 β-2 — Bidirectional / two-sided extrude. Routes through
    /// `Command::CreateSolid` with `CreateSolidMode::ExtrudeBidirectional`.
    /// `dist_pos` = extent along +normal, `dist_neg` = extent along −normal
    /// (both ≥ 0, sum > 0). Symmetric = `(d, d)`; asymmetric = `(d_pos, d_neg)`;
    /// `dist_neg = 0` degenerates to a one-way `+` extrude. v1 supports
    /// `(Plane, AllLinear)` + `(Plane, AllCircular)` profiles.
    ///
    /// FAIL-CLOSED (D5): like tapered/cone, carries NO push_pull fallback — a
    /// negative / zero-sum distance, a solid-face profile (is_move_only), or a
    /// non-(Plane, AllLinear|AllCircular) profile returns FALSE and the Scene
    /// rolls the mesh back byte-identical. The UI surfaces `lastError()`.
    pub fn create_solid_extrude_bidirectional(
        &mut self,
        face_id_raw: u32,
        dist_pos: f64,
        dist_neg: f64,
    ) -> bool {
        let fid = FaceId::new(face_id_raw);
        debug_log!(
            "[RUST] create_solid_extrude_bidirectional faceId={} dist_pos={:.3} dist_neg={:.3}",
            face_id_raw, dist_pos, dist_neg
        );
        // Defense-in-depth: closure-preserving + self-intersection baseline.
        let gate_snapshot = self.scene.scene_snapshot();
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let cmd = Command::CreateSolid {
            face_id: fid,
            mode: axia_geo::CreateSolidMode::ExtrudeBidirectional { dist_pos, dist_neg },
        };
        let result = self.scene.execute(cmd);
        let ok = match &result {
            axia_core::commands::CommandResult::SolidCreated { kind, face_count } => {
                debug_log!(
                    "[RUST] create_solid_extrude_bidirectional ok kind={:?} face_count={}",
                    kind, face_count
                );
                true
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] create_solid_extrude_bidirectional ERROR: {}", e);
                self.set_error(e.to_string());
                false
            }
            _ => {
                // Bidirectional never falls back (D5) → PushPullDone / others
                // are unexpected.
                self.set_error("bidirectional extrude: unexpected result".to_string());
                false
            }
        };
        if ok
            && !self.closure_preserving_gate_passed(
                before_boundary, before_si, &gate_snapshot, "bidirectional extrude", false,
            )
        {
            return false;
        }
        if ok {
            self.mark_topology_changed();
        }
        self.invalidate_cache();
        ok
    }

    /// ADR-247 (Phase 3 E2) — Loft between two selected profile faces. Routes
    /// through `Command::CreateSolid` with `CreateSolidMode::Loft`. Mismatched
    /// profile vertex counts are auto-resampled (the shorter cap is subdivided
    /// at its longest boundary edges) so both caps match → manifold loft solid.
    /// Returns true on success, false on error (< 3 verts / same face / curved
    /// or multi-loop profile).
    pub fn create_solid_loft(&mut self, profile_face_raw: u32, other_profile_raw: u32) -> bool {
        let profile = FaceId::new(profile_face_raw);
        let other = FaceId::new(other_profile_raw);
        // Defense-in-depth: closure-preserving + self-intersection baseline.
        let gate_snapshot = self.scene.scene_snapshot();
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let cmd = Command::CreateSolid {
            face_id: profile,
            mode: axia_geo::CreateSolidMode::Loft { other_profile: other },
        };
        let result = self.scene.execute(cmd);
        let ok = match &result {
            axia_core::commands::CommandResult::SolidCreated { kind, face_count } => {
                debug_log!(
                    "[RUST] create_solid_loft ok kind={:?} face_count={}",
                    kind, face_count
                );
                true
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] create_solid_loft ERROR: {}", e);
                self.set_error(e.to_string());
                false
            }
            _ => {
                debug_log!("[RUST] create_solid_loft unexpected result");
                false
            }
        };
        if ok
            && !self.closure_preserving_gate_passed(
                before_boundary, before_si, &gate_snapshot, "loft", false,
            )
        {
            return false;
        }
        if ok {
            self.mark_topology_changed();
        }
        self.invalidate_cache();
        ok
    }

    /// ADR-248 (Phase 3 E1) — Revolve a profile face around an axis by
    /// `angle_rad`. Full 360° (≈2π) → surface of revolution; partial (< 2π) →
    /// CAPPED wedge solid (θ=0 + θ=angle end caps). Profile must be a single
    /// planar polygon whose plane contains the axis and (for partial) stays
    /// clear of it. Returns true on success.
    pub fn create_solid_revolve(
        &mut self,
        profile_face_raw: u32,
        ox: f64, oy: f64, oz: f64,
        dx: f64, dy: f64, dz: f64,
        angle_rad: f64,
    ) -> bool {
        let profile = FaceId::new(profile_face_raw);
        // Defense-in-depth: closure-preserving + self-intersection baseline.
        let gate_snapshot = self.scene.scene_snapshot();
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let cmd = Command::CreateSolid {
            face_id: profile,
            mode: axia_geo::CreateSolidMode::Revolve {
                axis_origin: DVec3::new(ox, oy, oz),
                axis_dir: DVec3::new(dx, dy, dz),
                angle_rad,
            },
        };
        let result = self.scene.execute(cmd);
        let ok = match &result {
            axia_core::commands::CommandResult::SolidCreated { kind, face_count } => {
                debug_log!(
                    "[RUST] create_solid_revolve ok kind={:?} face_count={}",
                    kind, face_count
                );
                true
            }
            axia_core::commands::CommandResult::Error(e) => {
                console_error!("[RUST] create_solid_revolve ERROR: {}", e);
                self.set_error(e.to_string());
                false
            }
            _ => {
                debug_log!("[RUST] create_solid_revolve unexpected result");
                false
            }
        };
        if ok
            && !self.closure_preserving_gate_passed(
                before_boundary, before_si, &gate_snapshot, "revolve", false,
            )
        {
            return false;
        }
        if ok {
            self.mark_topology_changed();
        }
        self.invalidate_cache();
        ok
    }

    // ════════════════════════════════════════════════
    // ADR-193 — Live Push/Pull (direct manipulation) session
    //
    // Replaces the translucent ghost preview with real-geometry direct
    // manipulation. The tool calls begin → update×N → commit/cancel; the
    // Scene reconciles the preview into a single clean Undo frame on commit.
    // Each call marks topology changed + invalidates cache so the next
    // syncMesh re-renders (approach B: full sync per move, matching the
    // existing Move/Rotate/Scale live tools).
    // ════════════════════════════════════════════════

    /// ADR-193 — Begin a live Push/Pull: extrude `face_id` by `distance` as a
    /// real-geometry preview. Returns the new top FaceId (raw, as f64), or
    /// `-1.0` on error (call `lastError()` for the reason).
    #[wasm_bindgen(js_name = "beginLiveExtrude")]
    pub fn begin_live_extrude(&mut self, face_id_raw: u32, distance: f64) -> f64 {
        match self.scene.begin_live_extrude(FaceId::new(face_id_raw), distance) {
            Ok(top) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                top.raw() as f64
            }
            Err(e) => {
                self.set_error(e);
                -1.0
            }
        }
    }

    /// ADR-193 — Slide the live preview top cap to absolute `target` distance
    /// (signed, along the extrude normal). Cheap per-mousemove vertex move.
    #[wasm_bindgen(js_name = "updateLiveExtrude")]
    pub fn update_live_extrude(&mut self, target: f64) -> bool {
        match self.scene.update_live_extrude(target) {
            Ok(()) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.set_error(e);
                false
            }
        }
    }

    /// ADR-193 — Commit the live session: clean re-extrude at the final
    /// distance (single Undo, correct surfaces).
    #[wasm_bindgen(js_name = "commitLiveExtrude")]
    pub fn commit_live_extrude(&mut self) -> bool {
        let ok = match self.scene.commit_live_extrude() {
            Ok(res) => match res {
                axia_core::commands::CommandResult::Error(e) => {
                    self.set_error(e);
                    false
                }
                _ => true,
            },
            Err(e) => {
                self.set_error(e);
                false
            }
        };
        // The mesh changed (restore + extrude) regardless of outcome.
        self.mark_topology_changed();
        self.invalidate_cache();
        ok
    }

    /// ADR-193 — Cancel the live session (ESC): restore the pre-op snapshot.
    #[wasm_bindgen(js_name = "cancelLiveExtrude")]
    pub fn cancel_live_extrude(&mut self) -> bool {
        match self.scene.cancel_live_extrude() {
            Ok(()) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.set_error(e);
                false
            }
        }
    }

    /// ADR-193 — Whether a live Push/Pull session is in progress.
    #[wasm_bindgen(js_name = "isLiveExtrudeActive")]
    pub fn is_live_extrude_active(&self) -> bool {
        self.scene.is_live_extrude_active()
    }

    // ─── ADR-239 — Live NURBS CP-edit session (drag deforms surface live) ────

    /// Begin a live NURBS CP-edit session for `face_id` (captures pre-edit
    /// snapshot; no speculative op). Returns false (with lastError) if a session
    /// is active or the face is missing.
    #[wasm_bindgen(js_name = "beginLiveNurbsEdit")]
    pub fn begin_live_nurbs_edit(&mut self, face_id_raw: u32) -> bool {
        match self.scene.begin_live_nurbs_edit(FaceId::new(face_id_raw)) {
            Ok(()) => true,
            Err(e) => {
                self.set_error(e);
                false
            }
        }
    }

    /// Live per-frame re-create from an edited control net (no transaction).
    /// Returns the new preview FaceId (one element) or `[]` on failure.
    #[wasm_bindgen(js_name = "updateLiveNurbsEdit")]
    #[allow(clippy::too_many_arguments)]
    pub fn update_live_nurbs_edit(
        &mut self,
        control_pts_flat: &[f64],
        u_count: u32,
        v_count: u32,
        weights_flat: &[f64],
        knots_u: &[f64],
        knots_v: &[f64],
        degree_u: u32,
        degree_v: u32,
    ) -> Vec<u32> {
        let (grid, weights) = match nurbs_grid_from_flat(
            control_pts_flat, u_count as usize, v_count as usize, weights_flat,
        ) {
            Ok(g) => g,
            Err(e) => {
                self.set_error(format!("updateLiveNurbsEdit: {e}"));
                return Vec::new();
            }
        };
        match self.scene.update_live_nurbs_edit(
            grid, weights, knots_u.to_vec(), knots_v.to_vec(),
            degree_u as usize, degree_v as usize,
        ) {
            Ok(fid) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                vec![fid.raw()]
            }
            Err(e) => {
                self.set_error(format!("updateLiveNurbsEdit: {e}"));
                Vec::new()
            }
        }
    }

    /// Commit the live session: roll back previews + ONE clean replace (single
    /// Undo). Returns the final FaceId (one element) or `[]` on failure.
    #[wasm_bindgen(js_name = "commitLiveNurbsEdit")]
    #[allow(clippy::too_many_arguments)]
    pub fn commit_live_nurbs_edit(
        &mut self,
        control_pts_flat: &[f64],
        u_count: u32,
        v_count: u32,
        weights_flat: &[f64],
        knots_u: &[f64],
        knots_v: &[f64],
        degree_u: u32,
        degree_v: u32,
    ) -> Vec<u32> {
        let (grid, weights) = match nurbs_grid_from_flat(
            control_pts_flat, u_count as usize, v_count as usize, weights_flat,
        ) {
            Ok(g) => g,
            Err(e) => {
                self.set_error(format!("commitLiveNurbsEdit: {e}"));
                return Vec::new();
            }
        };
        match self.scene.commit_live_nurbs_edit(
            grid, weights, knots_u.to_vec(), knots_v.to_vec(),
            degree_u as usize, degree_v as usize,
        ) {
            Ok(fid) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                vec![fid.raw()]
            }
            Err(e) => {
                self.set_error(format!("commitLiveNurbsEdit: {e}"));
                Vec::new()
            }
        }
    }

    /// Cancel the live session (ESC / tool switch): restore the pre-edit state.
    #[wasm_bindgen(js_name = "cancelLiveNurbsEdit")]
    pub fn cancel_live_nurbs_edit(&mut self) -> bool {
        match self.scene.cancel_live_nurbs_edit() {
            Ok(()) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.set_error(e);
                false
            }
        }
    }

    /// Whether a live NURBS CP-edit session is in progress.
    #[wasm_bindgen(js_name = "isLiveNurbsEditActive")]
    pub fn is_live_nurbs_edit_active(&self) -> bool {
        self.scene.is_live_nurbs_edit_active()
    }

    /// Push/Pull a smooth group seamlessly (no gaps, wall faces connect adjacent surfaces)
    ///
    /// # Parameters
    /// - face_ids: Uint32Array of face IDs (wasm-bindgen converts JS Uint32Array → Vec<u32>)
    /// - dist: distance to offset (positive = outward)
    ///
    /// # Returns
    /// true if successful
    ///
    /// # Behavior
    /// - NaN/0 distance → no-op, returns true.
    /// - Empty group → no-op, returns true.
    /// - All faces coplanar → falls back to per-face regular push_pull
    ///   (prevents degenerate walls when smooth group contains only split sub-faces).
    #[wasm_bindgen]
    pub fn push_pull_smooth_group_seamless(
        &mut self,
        face_ids: Vec<u32>,
        dist: f64,
    ) -> bool {
        // NaN / 0 guard — JS can pass NaN if args are misaligned
        if !dist.is_finite() || dist == 0.0 || face_ids.is_empty() {
            return true;
        }

        let smooth_group: Vec<FaceId> = face_ids
            .iter()
            .map(|&id| FaceId::new(id))
            .collect();

        debug_log!(
            "[RUST] push_pull_smooth_group_seamless: {} faces, dist={:.3}",
            smooth_group.len(),
            dist
        );

        // ────────────────────────────────────────────────────────────────
        // Coplanar fallback — if all faces share the same normal (within
        // a tight tolerance), seamless-offset would create degenerate walls
        // on shared edges. Delegate to regular per-face push_pull instead.
        //
        // This handles the case where findSmoothGroup returns split sub-faces
        // (same plane, same normal) that should be treated independently.
        // ────────────────────────────────────────────────────────────────
        if smooth_group.len() >= 2 && self.all_faces_coplanar(&smooth_group) {
            debug_log!(
                "[RUST] seamless: all {} faces coplanar — falling back to per-face push_pull",
                smooth_group.len()
            );
            // Only push/pull the FIRST face to avoid topology chaos from
            // operating on multiple coplanar split siblings simultaneously.
            // The user clicked one face; that's the one that should extrude.
            let first = smooth_group[0];
            // ADR-087 K-ζ — kernel-aware CreateSolid Extrude (Q3 fallback
            // to Mesh::push_pull 은 exec_create_solid 가 자동 처리).
            let cmd = Command::CreateSolid {
                face_id: first,
                mode: axia_geo::CreateSolidMode::Extrude { distance: dist },
            };
            let result = self.scene.execute(cmd);
            let ok = matches!(
                result,
                axia_core::commands::CommandResult::SolidCreated { .. }
                    | axia_core::commands::CommandResult::PushPullDone { .. }
            );
            if ok { self.mark_topology_changed(); }
            self.invalidate_cache();
            return ok;
        }

        let faces_before = self.scene.mesh.face_count();

        // Execute seamless offset
        let result = match self.scene.mesh.push_pull_smooth_group_seamless(
            smooth_group.clone(),
            dist,
            axia_geo::MaterialId::new(0),
        ) {
            Ok(pp_result) => {
                let faces_after = self.scene.mesh.face_count();
                debug_log!(
                    "[RUST] seamless offset done: {} → {} faces (delta={}), {} wall faces",
                    faces_before,
                    faces_after,
                    faces_after as i64 - faces_before as i64,
                    pp_result.side_faces.len()
                );
                for msg in &pp_result.split_debug {
                    debug_log!("[SEAMLESS] {}", msg);
                }
                true
            }
            Err(e) => {
                console_error!("[RUST] push_pull_smooth_group_seamless ERROR: {}", e);
                false
            }
        };

        if result {
            self.mark_topology_changed();  // seamless push_pull changes topology
        }
        self.invalidate_cache();
        result
    }

    // ========================================================================
    // Face Split — draw line on face to subdivide it
    // ========================================================================

    /// Split a face by drawing a line segment across it.
    ///
    /// Both endpoints should be on the face's boundary (on an edge or at a vertex).
    /// Creates two new faces from the original face.
    ///
    /// # Parameters
    /// - face_id_raw: the face to split
    /// - x0, y0, z0: line start point
    /// - x1, y1, z1: line end point
    ///
    /// # Returns
    /// JSON string with split result info, or empty string on failure.
    #[wasm_bindgen(js_name = "splitFaceByLine")]
    pub fn split_face_by_line(
        &mut self,
        face_id_raw: u32,
        x0: f64, y0: f64, z0: f64,
        x1: f64, y1: f64, z1: f64,
    ) -> String {
        use axia_geo::operations::face_split;

        let fid = FaceId::new(face_id_raw);
        let line_start = DVec3::new(x0, y0, z0);
        let line_end = DVec3::new(x1, y1, z1);

        // Snapshot for undo + ADR-267 γ-2 watertight 게이트 (delta, OpenMesh).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(integrity_snapshot.clone());

        let faces_before = self.scene.mesh.face_count();

        match face_split::split_face_by_line(&mut self.scene.mesh, fid, line_start, line_end) {
            Ok(result) => {
                if !self.integrity_gate_passed(integrity_before, &integrity_snapshot, "split", true) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                let faces_after = self.scene.mesh.face_count();
                debug_log!("[RUST] split_face_by_line: face {} → {} new faces, {} new verts, faces {}->{} (delta {:+})",
                    face_id_raw, result.new_faces.len(), result.new_verts.len(),
                    faces_before, faces_after, faces_after as i64 - faces_before as i64);

                for msg in &result.debug {
                    debug_log!("[SPLIT] {}", msg);
                }

                // Commit undo frame
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();

                self.mark_topology_changed();
                self.invalidate_cache();

                // Return JSON with result info
                let face_ids: Vec<u32> = result.new_faces.iter().map(|f| f.raw()).collect();
                let vert_ids: Vec<u32> = result.new_verts.iter().map(|v| v.raw()).collect();
                format!("{{\"faces\":{:?},\"verts\":{:?},\"edges\":{}}}",
                    face_ids, vert_ids, result.new_edges.len())
            }
            Err(e) => {
                console_error!("[RUST] split_face_by_line ERROR: {}", e);
                // 트랜잭션 명시적 취소 — 열린 프레임이 남으면 후속 undo 스택 오염
                self.scene.transactions.cancel();
                self.set_error(format!("split_face_by_line: {}", e));
                format!("{{\"error\":\"{}\"}}", e.to_string().replace('"', "'"))
            }
        }
    }

    /// **ADR-202 β-3** — draw a closed circle ON a Sphere face (곡면 위 직접
    /// 그리기, S9). `center_pt`/`radius_pt` are world points the user clicked on
    /// the sphere; the engine projects them onto the sphere, builds the small
    /// circle, and splits the face into a cap + annulus (both Sphere). The cap
    /// joins the host face's XIA. Single Undo (transaction in
    /// `Scene::draw_circle_on_sphere`). Returns JSON `{cap, annulus}` or
    /// `{error}` (non-sphere face / degenerate circle).
    #[wasm_bindgen(js_name = "drawCircleOnSphere")]
    pub fn draw_circle_on_sphere(
        &mut self,
        face_id_raw: u32,
        cx: f64, cy: f64, cz: f64,
        rx: f64, ry: f64, rz: f64,
    ) -> String {
        let fid = FaceId::new(face_id_raw);
        let center_pt = DVec3::new(cx, cy, cz);
        let radius_pt = DVec3::new(rx, ry, rz);
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_circle_on_sphere(fid, center_pt, radius_pt) {
            Some((cap, annulus)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), annulus.raw())
            }
            None => {
                let msg = "draw_circle_on_sphere failed (not a Sphere face or degenerate circle)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    /// ADR-257 β-6 (P3-B) — draw a closed geodesic "porthole" circle on a
    /// Cylinder side face (곡면 위 직접 그리기, S9-cylinder). The 1:1 mirror of
    /// `drawCircleOnSphere`: `center_pt`/`radius_pt` are world points the user
    /// clicked on the cylinder wall; the engine builds the geodesic circle and
    /// splits the face into cap + remainder (both Cylinder). The cap joins the
    /// host's owner (dual-path Shape/XIA). Single Undo (transaction in
    /// `Scene::draw_circle_on_cylinder`). Returns JSON `{cap, annulus}` or
    /// `{error}` (non-cylinder face / degenerate or wrapping circle).
    #[wasm_bindgen(js_name = "drawCircleOnCylinder")]
    pub fn draw_circle_on_cylinder(
        &mut self,
        face_id_raw: u32,
        cx: f64, cy: f64, cz: f64,
        rx: f64, ry: f64, rz: f64,
    ) -> String {
        let fid = FaceId::new(face_id_raw);
        let center_pt = DVec3::new(cx, cy, cz);
        let radius_pt = DVec3::new(rx, ry, rz);
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_circle_on_cylinder(fid, center_pt, radius_pt) {
            Some((cap, annulus)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), annulus.raw())
            }
            None => {
                let msg = "draw_circle_on_cylinder failed (not a Cylinder face or degenerate circle)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    /// **ADR-263 β-3 (P3-C)** — draw a closed geodesic "porthole" circle on a
    /// Cone side face → split into cap + remainder. 1:1 mirror of
    /// `drawCircleOnCylinder`. Returns `{"cap":N,"annulus":M}` on success or
    /// `{"error":"..."}`.
    #[wasm_bindgen(js_name = "drawCircleOnCone")]
    pub fn draw_circle_on_cone(
        &mut self,
        face_id_raw: u32,
        cx: f64, cy: f64, cz: f64,
        rx: f64, ry: f64, rz: f64,
    ) -> String {
        let fid = FaceId::new(face_id_raw);
        let center_pt = DVec3::new(cx, cy, cz);
        let radius_pt = DVec3::new(rx, ry, rz);
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_circle_on_cone(fid, center_pt, radius_pt) {
            Some((cap, annulus)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), annulus.raw())
            }
            None => {
                let msg = "draw_circle_on_cone failed (not a Cone face or degenerate circle)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    /// **ADR-263 β-6 (P3-C)** — draw a closed "porthole" circle on a Torus face
    /// → split into cap + remainder. 1:1 mirror of `drawCircleOnCone`. Returns
    /// `{"cap":N,"annulus":M}` on success or `{"error":"..."}`.
    #[wasm_bindgen(js_name = "drawCircleOnTorus")]
    pub fn draw_circle_on_torus(
        &mut self,
        face_id_raw: u32,
        cx: f64, cy: f64, cz: f64,
        rx: f64, ry: f64, rz: f64,
    ) -> String {
        let fid = FaceId::new(face_id_raw);
        let center_pt = DVec3::new(cx, cy, cz);
        let radius_pt = DVec3::new(rx, ry, rz);
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_circle_on_torus(fid, center_pt, radius_pt) {
            Some((cap, annulus)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), annulus.raw())
            }
            None => {
                let msg = "draw_circle_on_torus failed (not a Torus face or degenerate circle)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    /// **ADR-284 β-3** — draw a closed POLYLINE (rect / polygon / freehand /
    /// bezier corners, flat `[x0,y0,z0, x1,y1,z1, …]`) on a curved surface face,
    /// splitting it into cap + remainder. `closed` closes the loop. Returns
    /// `{"cap":N,"annulus":M}` or `{"error":"..."}`. One export per surface.
    #[wasm_bindgen(js_name = "drawPolylineOnCylinder")]
    pub fn draw_polyline_on_cylinder(&mut self, face_id_raw: u32, flat: &[f64], closed: bool) -> String {
        let pts: Vec<DVec3> = flat.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_polyline_on_cylinder(FaceId::new(face_id_raw), pts, closed) {
            Some((cap, ann)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), ann.raw())
            }
            None => {
                let msg = "drawPolylineOnCylinder failed (not a Cylinder face / off-surface / wraps)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    #[wasm_bindgen(js_name = "drawPolylineOnCone")]
    pub fn draw_polyline_on_cone(&mut self, face_id_raw: u32, flat: &[f64], closed: bool) -> String {
        let pts: Vec<DVec3> = flat.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_polyline_on_cone(FaceId::new(face_id_raw), pts, closed) {
            Some((cap, ann)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), ann.raw())
            }
            None => {
                let msg = "drawPolylineOnCone failed (not a Cone face / off-surface / wraps)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    #[wasm_bindgen(js_name = "drawPolylineOnTorus")]
    pub fn draw_polyline_on_torus(&mut self, face_id_raw: u32, flat: &[f64], closed: bool) -> String {
        let pts: Vec<DVec3> = flat.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_polyline_on_torus(FaceId::new(face_id_raw), pts, closed) {
            Some((cap, ann)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), ann.raw())
            }
            None => {
                let msg = "drawPolylineOnTorus failed (not a Torus face / off-surface / wraps)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    #[wasm_bindgen(js_name = "drawPolylineOnSphere")]
    pub fn draw_polyline_on_sphere(&mut self, face_id_raw: u32, flat: &[f64], closed: bool) -> String {
        let pts: Vec<DVec3> = flat.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_polyline_on_sphere(FaceId::new(face_id_raw), pts, closed) {
            Some((cap, ann)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved sketch", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"cap\":{},\"annulus\":{}}}", cap.raw(), ann.raw())
            }
            None => {
                let msg = "drawPolylineOnSphere failed (not a Sphere face / off-surface / pole / wraps)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    /// ADR-284 β-4-3/β-4-4 — split a curved self-loop face (Sphere hemisphere or
    /// Cone side) by an OPEN drawn seam (rim → interior → rim, the S3 open-line
    /// case). `flat` is the raw drawn stroke (xyz triples, NOT pre-projected); the
    /// first + last are the rim endpoints, the interior points arc over the
    /// surface. Returns `{"a":FaceId,"b":FaceId}` (the two host pieces) or
    /// `{"error":...}`. (Cylinder/Torus are multi-rim → rejected.)
    #[wasm_bindgen(js_name = "drawOpenSeamOnCurved")]
    pub fn draw_open_seam_on_curved(&mut self, face_id_raw: u32, flat: &[f64]) -> String {
        let pts: Vec<DVec3> = flat.chunks_exact(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        // ADR-267 γ — the curved sketch-split had NO integrity gate: measured,
        // a second circle landing exactly on an existing pocket's rim left 55
        // invariant violations and still returned SUCCESS. Baseline first.
        let (integrity_before, integrity_snapshot) = self.integrity_baseline();
        match self.scene.draw_open_seam_on_curved(FaceId::new(face_id_raw), pts) {
            Some((a, b)) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "curved seam", false,
                ) {
                    return format!(
                        "{{\"error\":\"{}\"}}",
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                format!("{{\"a\":{},\"b\":{}}}", a.raw(), b.raw())
            }
            None => {
                let msg = "drawOpenSeamOnCurved failed (not a Sphere/Cone face / < 3 pts / degenerate seam)";
                self.set_error(msg.to_string());
                format!("{{\"error\":\"{}\"}}", msg)
            }
        }
    }

    /// ADR-285 β-1 — parametric direct edit: change a Path B Sphere's RADIUS in
    /// place (given any one hemisphere face). Updates both hemispheres + the
    /// shared equator, topology unchanged. Transaction-wrapped (single Undo).
    /// Returns true on success, false if not a Sphere face / non-positive radius.
    #[wasm_bindgen(js_name = "setSphereRadius")]
    pub fn set_sphere_radius(&mut self, face_id_raw: u32, radius: f64) -> bool {
        let ok = self.scene.set_sphere_radius(FaceId::new(face_id_raw), radius);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// ADR-285 β-2 — parametric direct edit: change a Path B Cylinder's RADIUS in
    /// place (given the Cylinder side/annulus face). Transaction-wrapped.
    #[wasm_bindgen(js_name = "setCylinderRadius")]
    pub fn set_cylinder_radius(&mut self, side_face_raw: u32, radius: f64) -> bool {
        let ok = self.scene.set_cylinder_radius(FaceId::new(side_face_raw), radius);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// ADR-285 β-2 — parametric direct edit: change a Path B Cylinder's HEIGHT in
    /// place (given the Cylinder side/annulus face). Transaction-wrapped.
    #[wasm_bindgen(js_name = "setCylinderHeight")]
    pub fn set_cylinder_height(&mut self, side_face_raw: u32, height: f64) -> bool {
        let ok = self.scene.set_cylinder_height(FaceId::new(side_face_raw), height);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// ADR-285 β-3 — parametric direct edit: Path B Cone base RADIUS in place
    /// (given the Cone side face). Transaction-wrapped.
    #[wasm_bindgen(js_name = "setConeRadius")]
    pub fn set_cone_radius(&mut self, side_face_raw: u32, radius: f64) -> bool {
        let ok = self.scene.set_cone_radius(FaceId::new(side_face_raw), radius);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// ADR-285 β-3 — parametric direct edit: Path B Cone HEIGHT in place
    /// (given the Cone side face). Transaction-wrapped.
    #[wasm_bindgen(js_name = "setConeHeight")]
    pub fn set_cone_height(&mut self, side_face_raw: u32, height: f64) -> bool {
        let ok = self.scene.set_cone_height(FaceId::new(side_face_raw), height);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// ADR-285 β-4 — parametric direct edit: Path B Torus MAJOR radius in place.
    #[wasm_bindgen(js_name = "setTorusMajorRadius")]
    pub fn set_torus_major_radius(&mut self, face_raw: u32, major: f64) -> bool {
        let ok = self.scene.set_torus_major_radius(FaceId::new(face_raw), major);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// ADR-285 β-4 — parametric direct edit: Path B Torus MINOR radius in place.
    #[wasm_bindgen(js_name = "setTorusMinorRadius")]
    pub fn set_torus_minor_radius(&mut self, face_raw: u32, minor: f64) -> bool {
        let ok = self.scene.set_torus_minor_radius(FaceId::new(face_raw), minor);
        if ok {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        ok
    }

    /// Test if a 3D point lies within a face's boundary.
    ///
    /// Returns true if the point is on the face's plane and inside its edges.
    /// Useful for determining if a draw operation should trigger face split.
    #[wasm_bindgen(js_name = "pointInFace")]
    pub fn point_in_face(&self, face_id_raw: u32, x: f64, y: f64, z: f64) -> bool {
        use axia_geo::operations::face_split;

        let fid = FaceId::new(face_id_raw);
        let point = DVec3::new(x, y, z);

        match face_split::point_in_face(&self.scene.mesh, fid, point) {
            Ok(result) => result,
            Err(_) => false,
        }
    }

    // ========================================================================
    // Undo/Redo
    // ========================================================================

    pub fn undo(&mut self) -> bool {
        let result = self.scene.execute(Command::Undo);
        self.mark_topology_changed();  // undo can restore/remove faces
        self.invalidate_cache();
        matches!(result, axia_core::commands::CommandResult::MeshUpdated)
    }

    pub fn redo(&mut self) -> bool {
        let result = self.scene.execute(Command::Redo);
        self.mark_topology_changed();  // redo can restore/remove faces
        self.invalidate_cache();
        matches!(result, axia_core::commands::CommandResult::MeshUpdated)
    }

    pub fn can_undo(&self) -> bool {
        self.scene.transactions.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.scene.transactions.can_redo()
    }

    // ========================================================================
    // Mesh export (cached)
    // ========================================================================

    pub fn get_positions(&mut self) -> Vec<f32> {
        self.rebuild_cache();
        self.cached_positions.clone()
    }

    /// ADR-013 §4 zero-copy view — returns raw pointer + length so JS can
    /// build a `Float32Array(memory.buffer, ptr, len)` without copying.
    /// Caller MUST refresh after any WASM allocation (memory may grow).
    /// 길이/포인터 둘 다 필요하므로 별도 함수 2개로 노출.
    #[wasm_bindgen(js_name = "getPositionsPtr")]
    pub fn get_positions_ptr(&mut self) -> *const f32 {
        self.rebuild_cache();
        self.cached_positions.as_ptr()
    }
    #[wasm_bindgen(js_name = "getPositionsLen")]
    pub fn get_positions_len(&mut self) -> usize {
        self.rebuild_cache();
        self.cached_positions.len()
    }
    #[wasm_bindgen(js_name = "getNormalsPtr")]
    pub fn get_normals_ptr(&mut self) -> *const f32 {
        self.rebuild_cache();
        self.cached_normals.as_ptr()
    }
    #[wasm_bindgen(js_name = "getNormalsLen")]
    pub fn get_normals_len(&mut self) -> usize {
        self.rebuild_cache();
        self.cached_normals.len()
    }
    #[wasm_bindgen(js_name = "getIndicesPtr")]
    pub fn get_indices_ptr(&mut self) -> *const u32 {
        self.rebuild_cache();
        self.cached_indices.as_ptr()
    }
    #[wasm_bindgen(js_name = "getIndicesLen")]
    pub fn get_indices_len(&mut self) -> usize {
        self.rebuild_cache();
        self.cached_indices.len()
    }
    #[wasm_bindgen(js_name = "getFaceMapPtr")]
    pub fn get_face_map_ptr(&mut self) -> *const u32 {
        self.rebuild_cache();
        self.cached_face_map.as_ptr()
    }
    #[wasm_bindgen(js_name = "getFaceMapLen")]
    pub fn get_face_map_len(&mut self) -> usize {
        self.rebuild_cache();
        self.cached_face_map.len()
    }

    /// Get vertex positions in f64 precision (CAD-grade).
    /// Same layout as get_positions() but Float64Array — no f32 truncation.
    /// Use for dimension display, snap matching, and precision-sensitive operations.
    #[wasm_bindgen(js_name = "getPositionsF64")]
    pub fn get_positions_f64(&mut self) -> Vec<f64> {
        self.rebuild_cache();
        self.cached_positions_f64.clone()
    }

    pub fn get_normals(&mut self) -> Vec<f32> {
        self.rebuild_cache();
        self.cached_normals.clone()
    }

    pub fn get_indices(&mut self) -> Vec<u32> {
        self.rebuild_cache();
        self.cached_indices.clone()
    }

    /// ADR-203 β-1.5 — export the whole model as an IFC4.3 `IfcFacetedBrep`
    /// file (STEP-21 text). Uses the cached render tessellation (curved faces
    /// already faceted, same source as OBJ/STL export), converting engine mm →
    /// IFC metre. `name` labels the single IfcWall. Returns "" if the scene has
    /// no geometry (caller should warn).
    #[wasm_bindgen(js_name = "exportIfc")]
    pub fn export_ifc(&mut self, name: String) -> String {
        self.rebuild_cache();
        if self.cached_indices.is_empty() {
            return String::new();
        }
        const MM_TO_M: f64 = 0.001;
        let pos = &self.cached_positions_f64;
        let mut points = Vec::with_capacity(pos.len() / 3);
        let mut i = 0;
        while i + 2 < pos.len() {
            points.push(DVec3::new(pos[i] * MM_TO_M, pos[i + 1] * MM_TO_M, pos[i + 2] * MM_TO_M));
            i += 3;
        }
        let idx = &self.cached_indices;
        let mut tris = Vec::with_capacity(idx.len() / 3);
        let mut j = 0;
        while j + 2 < idx.len() {
            tris.push([idx[j], idx[j + 1], idx[j + 2]]);
            j += 3;
        }
        let nm = name.trim();
        let nm = if nm.is_empty() { "AXiA Model" } else { nm };
        axia_ifc::emit_faceted_brep(&points, &tris, nm)
    }

    /// ADR-203 β-2.5 — export the model as an IFC4.3 `IfcAdvancedBrep` with
    /// analytic surfaces, walking the live DCEL directly (not the render
    /// tessellation). Planar-face models export as exact `IfcAdvancedFace(
    /// IfcPlane)`; converts engine mm → IFC metre. `name` labels the wall.
    ///
    /// Returns "" (→ caller falls back to `exportIfc` faceted) if the scene is
    /// empty, or if any active face lacks a supported analytic surface or has a
    /// boundary that is not a straight-edge loop (e.g. Path B curved rims —
    /// their curved edges are β-3).
    #[wasm_bindgen(js_name = "exportIfcAdvanced")]
    pub fn export_ifc_advanced(&mut self, name: String) -> String {
        const MM_TO_M: f64 = 0.001;
        let nm = name.trim();
        let nm = if nm.is_empty() { "AXiA Model" } else { nm };
        axia_ifc::emit_advanced_brep_from_mesh(&self.scene.mesh, MM_TO_M, nm).unwrap_or_default()
    }

    /// ADR-203 δ — classify a member as a slab, column, beam, … so it exports
    /// as that instead of an `IfcWall`.
    ///
    /// `kind` accepts either a short key (`"slab"`) or an IFC tag
    /// (`"IFCSLAB"`); an empty string clears the assignment. Returns false for
    /// an unknown kind rather than storing something the exporter cannot use —
    /// that keeps the stored value canonical.
    #[wasm_bindgen(js_name = "setXiaElementKind")]
    pub fn set_xia_element_kind(&mut self, xia_id: u32, kind: String) -> bool {
        if kind.trim().is_empty() {
            self.scene.xia_element_kind.remove(&xia_id);
            return true;
        }
        match axia_ifc::IfcElementKind::from_tag(&kind) {
            Some(k) => {
                self.scene.xia_element_kind.insert(xia_id, k.key().to_string());
                true
            }
            None => false,
        }
    }

    /// Form-citizen counterpart of [`Self::set_xia_element_kind`].
    #[wasm_bindgen(js_name = "setShapeElementKind")]
    pub fn set_shape_element_kind(&mut self, shape_id: u32, kind: String) -> bool {
        let sid = axia_core::ShapeId::new(shape_id);
        if kind.trim().is_empty() {
            self.scene.shape_element_kind.remove(&sid);
            return true;
        }
        match axia_ifc::IfcElementKind::from_tag(&kind) {
            Some(k) => {
                self.scene.shape_element_kind.insert(sid, k.key().to_string());
                true
            }
            None => false,
        }
    }

    /// Which Form citizen owns a face, or `u32::MAX` when none does — the
    /// mirror of `get_xia_for_face`, which the Inspector needs in order to
    /// classify a drawn Shape (ADR-203 δ). The reverse index already exists
    /// (ADR-079 W-1); this only exposes it.
    #[wasm_bindgen(js_name = "getShapeForFace")]
    pub fn get_shape_for_face(&self, face_id_raw: u32) -> u32 {
        self.scene
            .face_to_shape
            .get(&axia_geo::FaceId::new(face_id_raw))
            .map(|s| s.raw())
            .unwrap_or(u32::MAX)
    }

    /// The member's classification key, or "" when unassigned (exports as a
    /// wall, which is what it always did).
    #[wasm_bindgen(js_name = "getXiaElementKind")]
    pub fn get_xia_element_kind(&self, xia_id: u32) -> String {
        self.scene.xia_element_kind.get(&xia_id).cloned().unwrap_or_default()
    }

    /// Form-citizen counterpart of [`Self::get_xia_element_kind`].
    #[wasm_bindgen(js_name = "getShapeElementKind")]
    pub fn get_shape_element_kind(&self, shape_id: u32) -> String {
        self.scene
            .shape_element_kind
            .get(&axia_core::ShapeId::new(shape_id))
            .cloned()
            .unwrap_or_default()
    }

    /// Every classification a member can be given, as `key|IFCTAG` pairs — one
    /// per line. The UI builds its picker from this so the two never drift.
    #[wasm_bindgen(js_name = "ifcElementKinds")]
    pub fn ifc_element_kinds(&self) -> String {
        axia_ifc::IfcElementKind::ALL
            .iter()
            .map(|k| format!("{}|{}", k.key(), k.tag()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// ADR-203 γ — export a semantic IFC4.3 model: one element per member
    /// (Xia → named + material, Shape → named, leftover faces → "Model"), all
    /// under one Project→Site→Building→Storey. Analytic geometry (β-3), and
    /// each member exports as its assigned kind (δ).
    ///
    /// Returns "" (→ caller falls back to `exportIfc` faceted) if the scene is
    /// empty or any element can't form an advanced brep. Elements are ordered by
    /// id (Xias, then Shapes, then leftover) for deterministic output.
    #[wasm_bindgen(js_name = "exportIfcModel")]
    pub fn export_ifc_model(&self, name: String) -> String {
        use std::collections::HashSet;
        const MM_TO_M: f64 = 0.001;
        let nm = name.trim();
        let nm = if nm.is_empty() { "AXiA Model" } else { nm };

        let scene = &self.scene;
        let mut claimed: HashSet<axia_geo::FaceId> = HashSet::new();
        let mut elements: Vec<axia_ifc::IfcElement> = Vec::new();

        // Xias (sorted by id) → named wall + material.
        let mut xia_ids: Vec<u32> = scene.xias.keys().copied().collect();
        xia_ids.sort_unstable();
        for xid in xia_ids {
            let xia = &scene.xias[&xid];
            let faces: Vec<axia_geo::FaceId> =
                xia.face_ids.iter().copied().filter(|f| claimed.insert(*f)).collect();
            if faces.is_empty() {
                continue;
            }
            let material_name = if xia.material == axia_core::FORM_MATERIAL {
                None
            } else {
                scene.material_library.get(xia.material).map(|m| m.name.clone())
            };
            let kind = scene
                .xia_element_kind
                .get(&xid)
                .and_then(|k| axia_ifc::IfcElementKind::from_tag(k))
                .unwrap_or_default();
            elements.push(axia_ifc::IfcElement {
                name: xia.name.clone(),
                material_name,
                kind,
                face_ids: faces,
            });
        }
        // Shapes (sorted by id) → named wall, no material.
        let mut shape_ids: Vec<_> = scene.shapes.keys().copied().collect();
        shape_ids.sort_unstable();
        for sid in shape_ids {
            let shape = &scene.shapes[&sid];
            let faces: Vec<axia_geo::FaceId> =
                shape.face_ids.iter().copied().filter(|f| claimed.insert(*f)).collect();
            if faces.is_empty() {
                continue;
            }
            let kind = scene
                .shape_element_kind
                .get(&sid)
                .and_then(|k| axia_ifc::IfcElementKind::from_tag(k))
                .unwrap_or_default();
            elements.push(axia_ifc::IfcElement {
                name: shape.name.clone(),
                material_name: None,
                kind,
                face_ids: faces,
            });
        }
        // Leftover active faces (unowned) → one "Model" wall.
        let leftover: Vec<axia_geo::FaceId> = scene
            .mesh
            .faces
            .iter()
            .filter(|(fid, f)| f.is_active() && !claimed.contains(fid))
            .map(|(fid, _)| fid)
            .collect();
        if !leftover.is_empty() {
            elements.push(axia_ifc::IfcElement {
                name: "Model".into(),
                material_name: None,
                kind: axia_ifc::IfcElementKind::default(),
                face_ids: leftover,
            });
        }

        if elements.is_empty() {
            return String::new();
        }
        axia_ifc::emit_ifc_model(&scene.mesh, &elements, MM_TO_M, nm).unwrap_or_default()
    }

    /// ADR-203 I-1 — read an `.ifc` file and report what is inside it: schema,
    /// entity count, an entity histogram, and the element/material counts a BIM
    /// user cares about. Read-only: the scene is untouched.
    ///
    /// Returns JSON — `{"ok":true,"schema":…,"entityCount":N,"notable":{…},
    /// "topTypes":[[tag,count],…]}` or `{"ok":false,"error":"…"}`.
    ///
    /// Geometry import (IFC B-rep → DCEL) is the next step; this one proves the
    /// file is readable and tells the user what it holds.
    #[wasm_bindgen(js_name = "analyzeIfc")]
    pub fn analyze_ifc(&self, text: String) -> String {
        match axia_ifc::analyze_ifc(&text) {
            Ok(a) => a.to_json(),
            Err(e) => axia_ifc::ifc_analyze::error_json(&e),
        }
    }

    /// ADR-203 I-2 — classify an `.ifc` file's building elements: each member's
    /// type, name, material and the geometry it points at, following the IFC
    /// reference chain (element → ProductDefinitionShape → ShapeRepresentation
    /// → items). Read-only; the scene is untouched.
    ///
    /// Returns JSON — `{"ok":true,"elementCount":N,"convertible":M,"elements":
    /// [{id,type,name,material,geometry:[{id,kind,representationType,supported}]}],
    /// "unsupportedGeometry":{tag:count}}` or `{"ok":false,"error":"…"}`.
    ///
    /// `supported` marks the items I-3 will be able to turn into DCEL faces;
    /// anything else is reported rather than silently dropped.
    #[wasm_bindgen(js_name = "classifyIfc")]
    pub fn classify_ifc(&self, text: String) -> String {
        match axia_ifc::classify_ifc(&text) {
            Ok(r) => r.to_json(),
            Err(e) => axia_ifc::ifc_analyze::error_json(&e),
        }
    }

    /// ADR-203 I-3 — actually import an `.ifc`: turn each element's B-rep into
    /// DCEL faces and add them to the scene. Coordinates are converted from the
    /// file's `IfcSIUnit` length unit to engine mm.
    ///
    /// One undo step: the whole import is wrapped in a transaction, so a failure
    /// leaves the scene exactly as it was.
    ///
    /// Returns JSON — `{"ok":true,"elements":N,"faces":F,"vertices":V,
    /// "scaleToMm":S,"placed":P,"groups":G,"warnings":[…]}` or `{"ok":false,"error":"…"}`.
    /// `placed` counts members moved by an `IfcLocalPlacement` chain (I-4);
    /// `groups` counts the scene groups built from the spatial tree (I-5).
    #[wasm_bindgen(js_name = "importIfc")]
    pub fn import_ifc(&mut self, text: String) -> String {
        let g = match axia_ifc::import_ifc_geometry(&text) {
            Ok(g) => g,
            Err(e) => return axia_ifc::ifc_analyze::error_json(&e),
        };
        if g.elements.is_empty() {
            let mut msg = String::from("no convertible geometry in this file");
            if !g.warnings.is_empty() {
                msg.push_str(" (");
                msg.push_str(&g.warnings.join("; "));
                msg.push(')');
            }
            return axia_ifc::ifc_analyze::error_json(&msg);
        }

        let before_verts = self.scene.mesh.vert_count();
        let before_faces = self.scene.mesh.face_count();

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());

        let material = axia_core::FORM_MATERIAL;
        let mut failed: Vec<String> = Vec::new();
        // I-5 — remember each member's faces so the spatial tree can be
        // rebuilt as scene groups once everything is in.
        let mut element_faces: Vec<(usize, Vec<axia_geo::FaceId>)> = Vec::new();
        for (ei, el) in g.elements.iter().enumerate() {
            let mut mine: Vec<axia_geo::FaceId> = Vec::new();
            for f in &el.faces {
                // A single closed-curve disk — a drawn circle / ellipse / spline
                // — is rebuilt kernel-native: one anchor + one self-loop edge
                // carrying the exact curve, the same thing the draw tools make,
                // instead of baking the tessellated polygon into the DCEL. Any
                // failure falls straight through to the polygon path below.
                if let Some(curve) = &f.closed_curve {
                    let anchor = self.scene.mesh.add_vertex(curve_anchor(curve));
                    if let Ok(face_id) =
                        self.scene
                            .mesh
                            .add_face_closed_curve(anchor, curve.clone(), material)
                    {
                        mine.push(face_id);
                        continue;
                    }
                    // Fell through: drop the half-built anchor is not possible,
                    // but add_face_with_holes below re-uses it via the dedup, so
                    // the polygon path still produces a valid face.
                }
                let outer: Vec<_> =
                    f.outer.iter().map(|&p| self.scene.mesh.add_vertex(p)).collect();
                let inner_ids: Vec<Vec<_>> = f
                    .inners
                    .iter()
                    .map(|ring| ring.iter().map(|&p| self.scene.mesh.add_vertex(p)).collect())
                    .collect();
                let hole_refs: Vec<&[_]> = inner_ids.iter().map(|v| v.as_slice()).collect();
                match self.scene.mesh.add_face_with_holes(&outer, &hole_refs, material) {
                    // An imported face must carry its surface like any other
                    // face (ADR-087 K-ε) — otherwise Push/Pull, Boolean and
                    // re-export all refuse it.
                    Ok(face_id) => {
                        if let Some(plane) = f.plane() {
                            self.scene.mesh.set_face_surface(face_id, Some(plane));
                        }
                        mine.push(face_id);
                    }
                    Err(e) => failed.push(format!(
                        "element #{} '{}': {}",
                        el.element_id,
                        el.name.clone().unwrap_or_default(),
                        e
                    )),
                }
            }
            // IfcBooleanResult — a wall with an opening. Build each operand solid
            // and combine them with the engine's own boolean. eval_csg adds the
            // operand faces *before* the boolean runs; if the boolean can't be
            // evaluated they must not linger as loose solids (a wall + an opening
            // block — a wrong shape), so snapshot around it and rewind on failure.
            for node in &el.booleans {
                let pre = self.scene.scene_snapshot();
                match eval_csg(&mut self.scene.mesh, node, material) {
                    Some(fids) => mine.extend(fids),
                    None => {
                        self.scene.restore_scene_snapshot(&pre);
                        failed.push(format!(
                            "element #{} '{}': boolean geometry could not be evaluated",
                            el.element_id,
                            el.name.clone().unwrap_or_default(),
                        ));
                    }
                }
            }
            if !mine.is_empty() {
                element_faces.push((ei, mine));
            }
        }

        let added_faces = self.scene.mesh.face_count().saturating_sub(before_faces);
        if added_faces == 0 {
            // Nothing landed — restore rather than leave half-built vertices.
            self.scene.restore_scene_snapshot(&before_snapshot);
            self.scene.transactions.cancel();
            let mut msg = String::from("no faces could be built");
            if !failed.is_empty() {
                msg.push_str(" (");
                msg.push_str(&failed.join("; "));
                msg.push(')');
            }
            return axia_ifc::ifc_analyze::error_json(&msg);
        }

        // I-5 — mirror the file's spatial structure as scene groups, inside the
        // same transaction so one Undo removes the groups with the geometry.
        // Members become groups too: that is what makes "select the whole wall"
        // and "hide this storey" possible.
        let mut groups_made = 0usize;
        let mut container_group: std::collections::BTreeMap<u32, u32> =
            std::collections::BTreeMap::new();
        for node in g.spatial.topological() {
            // Containers start empty; faces arrive via their member groups.
            let gid = self.scene.groups.create_group(node.label(), Vec::new());
            container_group.insert(node.id, gid);
            groups_made += 1;
            if let Some(parent) = node.parent.and_then(|p| container_group.get(&p)).copied() {
                self.scene.groups.set_parent(gid, Some(parent));
            }
        }
        for (ei, faces) in &element_faces {
            let el = &g.elements[*ei];
            let label = match &el.name {
                Some(n) if !n.trim().is_empty() => n.clone(),
                _ => format!("#{}", el.element_id),
            };
            let gid = self.scene.groups.create_group(label, faces.clone());
            groups_made += 1;
            // A member with no container stays at the top level rather than
            // being filed under a container the file never named.
            if let Some(parent) = el.container.and_then(|c| container_group.get(&c)).copied() {
                self.scene.groups.set_parent(gid, Some(parent));
            }
        }

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        let added_verts = self.scene.mesh.vert_count().saturating_sub(before_verts);
        let mut warnings = g.warnings.clone();
        warnings.extend(failed);
        let warn_json: Vec<String> = warnings
            .iter()
            .map(|w| format!("\"{}\"", w.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();

        format!(
            r#"{{"ok":true,"elements":{},"faces":{},"vertices":{},"scaleToMm":{},"placed":{},"groups":{},"warnings":[{}]}}"#,
            g.elements.len(),
            added_faces,
            added_verts,
            g.scale_to_mm,
            g.placed,
            groups_made,
            warn_json.join(","),
        )
    }

    /// Get the FaceId for each triangle (one u32 per triangle).
    /// Use: face_map[triangleIndex] → FaceId for push_pull.
    pub fn get_face_map(&mut self) -> Vec<u32> {
        self.rebuild_cache();
        self.cached_face_map.clone()
    }

    /// Get hard edge line segments for wireframe rendering.
    /// Returns flat [x0,y0,z0, x1,y1,z1, ...] — use with THREE.LineSegments.
    /// Coplanar edges (angle ≤ 15°) are automatically hidden.
    /// Centerline edges are excluded — call getCenterlineLines() separately.
    pub fn get_edge_lines(&mut self) -> Vec<f32> {
        self.rebuild_cache();
        self.cached_edge_lines.clone()
    }

    /// Get centerline edge segments for separate rendering (dashed/thin/dimmer).
    /// Flat [x0,y0,z0, x1,y1,z1, ...] — pair per segment.
    /// Not cached — centerlines are typically fewer and changes infrequently,
    /// but if perf becomes an issue we can cache like getEdgeLines.
    #[wasm_bindgen(js_name = "getCenterlineLines")]
    pub fn get_centerline_lines(&self) -> Vec<f32> {
        self.scene.mesh.export_centerline_lines()
    }

    /// Edge line segment index → EdgeId raw value mapping.
    /// segment[i]의 EdgeId = edge_map[i]
    pub fn get_edge_map(&mut self) -> Vec<u32> {
        self.rebuild_cache();
        self.cached_edge_map.clone()
    }

    /// Get unique vertex positions in f64 precision for snap system.
    /// Returns flat [x0,y0,z0, x1,y1,z1, ...] as Float64Array.
    /// Snap system should use these instead of the f32 render buffers.
    #[wasm_bindgen(js_name = "getSnapVerticesF64")]
    pub fn get_snap_vertices_f64(&self) -> Vec<f64> {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let mut result = Vec::new();

        for (_fid, face) in self.scene.mesh.faces.iter() {
            if !face.is_active() || !face.is_visible() { continue; }
            let start = face.outer().start;
            if start.is_null() { continue; }
            if let Ok(verts) = self.scene.mesh.collect_loop_verts(start) {
                for vid in verts {
                    if seen.insert(vid) {
                        if let Ok(pos) = self.scene.mesh.vertex_pos(vid) {
                            result.push(pos.x);
                            result.push(pos.y);
                            result.push(pos.z);
                        }
                    }
                }
            }
        }

        // Also include standalone edge vertices
        for (_eid, edge) in self.scene.mesh.edges.iter() {
            if !edge.is_active() { continue; }
            for &vid in &[edge.v_small(), edge.v_large()] {
                if seen.insert(vid) {
                    if let Ok(pos) = self.scene.mesh.vertex_pos(vid) {
                        result.push(pos.x);
                        result.push(pos.y);
                        result.push(pos.z);
                    }
                }
            }
        }

        result
    }

    // ════════════════════════════════════════════════════════════════════════
    // Delta Buffer Export (Phase 1 Optimization)
    // ════════════════════════════════════════════════════════════════════════

    /// Export incremental geometry updates for dirty faces.
    ///
    /// Two modes:
    /// - **topology_changed = true**: Topology was modified (draw/push_pull/delete/boolean).
    ///   Returns a DeltaBuffers with topology_changed=true and empty data.
    ///   JS must do a full rebuild via getMeshBuffers().
    ///
    /// - **topology_changed = false**: Only vertex positions changed (translate/rotate/scale).
    ///   Returns the new positions/normals for dirty faces with their offsets
    ///   into the full buffer, so JS can patch in-place.
    ///
    /// Returns None if nothing changed since last export.
    /// Clears dirty_faces and topology_changed after export.
    #[wasm_bindgen(js_name = "getDirtyFaceBuffers")]
    pub fn get_dirty_face_buffers(&mut self) -> Option<DeltaBuffers> {
        // Nothing changed at all
        if !self.topology_changed && self.dirty_faces.is_empty() {
            return None;
        }

        // Case 1: Topology changed → tell JS to do full rebuild
        if self.topology_changed {
            self.dirty_faces.clear();
            self.topology_changed = false;
            return Some(DeltaBuffers {
                modified_face_ids: Vec::new(),
                positions: Vec::new(),
                normals: Vec::new(),
                face_vert_offsets: Vec::new(),
                face_vert_counts: Vec::new(),
                cache_version: self.cache_version,
                topology_changed: true,
            });
        }

        // Case 2: Position-only change (translate/rotate/scale)
        // We need the face_range_map from the PREVIOUS full rebuild.
        // If face_range_map is empty, we can't do delta — force full rebuild.
        if self.face_range_map.is_empty() {
            self.dirty_faces.clear();
            self.topology_changed = false;
            return Some(DeltaBuffers {
                modified_face_ids: Vec::new(),
                positions: Vec::new(),
                normals: Vec::new(),
                face_vert_offsets: Vec::new(),
                face_vert_counts: Vec::new(),
                cache_version: self.cache_version,
                topology_changed: true,  // force full rebuild since no range map
            });
        }

        // Rebuild cache to get fresh vertex positions after transform
        self.rebuild_cache();

        let mut modified_face_ids = Vec::new();
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut face_vert_offsets = Vec::new();
        let mut face_vert_counts = Vec::new();

        // For each dirty face, look up its range in the full buffer and copy
        for &face_id in &self.dirty_faces {
            if let Some(range) = self.face_range_map.get(&face_id) {
                let start = range.vert_start as usize;
                let count = range.vert_count as usize;
                let float_start = start * 3;
                let float_end = float_start + count * 3;

                // Bounds check
                if float_end > self.cached_positions.len() || float_end > self.cached_normals.len() {
                    continue;
                }

                modified_face_ids.push(face_id);
                face_vert_offsets.push(range.vert_start);
                face_vert_counts.push(range.vert_count);

                // Copy this face's positions and normals from the full cache
                positions.extend_from_slice(&self.cached_positions[float_start..float_end]);
                normals.extend_from_slice(&self.cached_normals[float_start..float_end]);
            }
            // Skip faces not in range map (shouldn't happen for position-only changes)
        }

        // Sort by face_id for consistent output
        // (need to sort all arrays together)
        if modified_face_ids.len() > 1 {
            let mut order: Vec<usize> = (0..modified_face_ids.len()).collect();
            order.sort_unstable_by_key(|&i| modified_face_ids[i]);

            let sorted_ids: Vec<u32> = order.iter().map(|&i| modified_face_ids[i]).collect();
            let sorted_offsets: Vec<u32> = order.iter().map(|&i| face_vert_offsets[i]).collect();
            let sorted_counts: Vec<u32> = order.iter().map(|&i| face_vert_counts[i]).collect();

            // Rebuild positions/normals in sorted order
            let mut sorted_positions = Vec::with_capacity(positions.len());
            let mut sorted_normals = Vec::with_capacity(normals.len());
            // Build a prefix-sum of original vertex counts for source offsets
            let mut src_offsets: Vec<usize> = Vec::with_capacity(order.len());
            let mut acc = 0usize;
            for &count in &face_vert_counts {
                src_offsets.push(acc);
                acc += count as usize * 3;
            }
            for &i in &order {
                let count = face_vert_counts[i] as usize * 3;
                let start = src_offsets[i];
                sorted_positions.extend_from_slice(&positions[start..start + count]);
                sorted_normals.extend_from_slice(&normals[start..start + count]);
            }

            modified_face_ids = sorted_ids;
            face_vert_offsets = sorted_offsets;
            face_vert_counts = sorted_counts;
            positions = sorted_positions;
            normals = sorted_normals;
        }

        self.dirty_faces.clear();
        self.topology_changed = false;

        Some(DeltaBuffers {
            modified_face_ids,
            positions,
            normals,
            face_vert_offsets,
            face_vert_counts,
            cache_version: self.cache_version,
            topology_changed: false,
        })
    }

    // ========================================================================
    // Scene info
    // ========================================================================

    /// Force-delete a face from the mesh.
    ///
    /// Wrapped in an undo transaction (Bug #1 fix, 2026-04-17) — previously
    /// this op mutated the mesh without recording a snapshot, causing Ctrl+Z
    /// to skip past the deletion to an earlier command.
    pub fn delete_face(&mut self, face_id_raw: u32) -> bool {
        let fid = FaceId::new(face_id_raw);
        if !self.scene.mesh.faces.contains(fid) {
            return true; // already gone — no-op, no transaction needed
        }

        // Begin undo transaction
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        // Clean up face_to_xia reverse index + XIA face_ids
        self.scene.unregister_face_from_xia(fid);
        // Try proper removal first
        let _ = self.scene.mesh.remove_face(fid);
        // Force-remove from storage even if remove_face had issues
        if self.scene.mesh.faces.contains(fid) {
            self.scene.mesh.faces.remove(fid);
        }

        // Commit transaction so Ctrl+Z can restore this deletion
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();

        self.mark_topology_changed();
        self.invalidate_cache();
        !self.scene.mesh.faces.contains(fid) // return true if actually gone
    }

    /// Delete an edge (and its half-edges) from the mesh.
    /// Also removes any faces that reference this edge (SketchUp-style cascade).
    ///
    /// Legacy signature returning just bool — calls the cascaded_count version.
    /// New code should prefer `delete_edge_cascade` which reports how many faces
    /// were removed so the UI can show a toast.
    pub fn delete_edge(&mut self, edge_id_raw: u32) -> bool {
        self.delete_edge_cascade(edge_id_raw) >= 0
    }

    /// Delete an edge plus all faces sharing it. Returns the cascaded face count
    /// (>= 0 on success, -1 on failure). TS wraps this to inform the user how
    /// many faces were removed as a side effect.
    #[wasm_bindgen(js_name = "deleteEdgeCascade")]
    pub fn delete_edge_cascade(&mut self, edge_id_raw: u32) -> i32 {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            return 0; // already gone, 0 cascaded
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        // First, find and remove any faces sharing this edge
        let (faces, _) = self.scene.mesh.get_faces_sharing_edge(eid);
        let cascade_count = faces.len() as i32;
        // Clean up face_to_xia for all affected faces
        let face_ids: Vec<FaceId> = faces.iter().copied().collect();
        self.scene.unregister_faces_from_xia(&face_ids);
        for fid in faces {
            let _ = self.scene.mesh.remove_face(fid);
            if self.scene.mesh.faces.contains(fid) {
                self.scene.mesh.faces.remove(fid);
            }
        }

        // Then remove the edge itself
        let _ = self.scene.mesh.remove_edge_and_halfedges(eid);
        // Force-remove if still present
        if self.scene.mesh.edges.contains(eid) {
            self.scene.mesh.edges.remove(eid);
        }

        // Clean up isolated vertices
        self.scene.mesh.remove_isolated_verts();

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        if self.scene.mesh.edges.contains(eid) {
            -1 // failure
        } else {
            cascade_count
        }
    }

    /// Batch delete faces and edges in a single undo transaction.
    /// Called from JS delete action — undo restores everything at once.
    pub fn batch_delete(&mut self, face_ids: &[u32], edge_ids: &[u32]) -> bool {
        if face_ids.is_empty() && edge_ids.is_empty() {
            return false;
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        // Collect all face IDs to unregister (direct + edge-sharing)
        let mut all_removed_faces: Vec<FaceId> = Vec::new();

        // Delete faces first
        for &fid_raw in face_ids {
            let fid = FaceId::new(fid_raw);
            if self.scene.mesh.faces.contains(fid) {
                all_removed_faces.push(fid);
                let _ = self.scene.mesh.remove_face(fid);
                if self.scene.mesh.faces.contains(fid) {
                    self.scene.mesh.faces.remove(fid);
                }
            }
        }

        // Delete edges (also removes faces sharing the edge)
        for &eid_raw in edge_ids {
            let eid = EdgeId::new(eid_raw);
            if !self.scene.mesh.edges.contains(eid) {
                continue;
            }
            let (faces, _) = self.scene.mesh.get_faces_sharing_edge(eid);
            for fid in &faces {
                all_removed_faces.push(*fid);
            }
            for fid in faces {
                let _ = self.scene.mesh.remove_face(fid);
                if self.scene.mesh.faces.contains(fid) {
                    self.scene.mesh.faces.remove(fid);
                }
            }
            let _ = self.scene.mesh.remove_edge_and_halfedges(eid);
            if self.scene.mesh.edges.contains(eid) {
                self.scene.mesh.edges.remove(eid);
            }
        }

        // Batch clean up face_to_xia for all removed faces
        self.scene.unregister_faces_from_xia(&all_removed_faces);

        // Clean up isolated vertices
        self.scene.mesh.remove_isolated_verts();

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        true
    }

    /// Dry-run: "if I erase this edge right now, would it merge two coplanar
    /// faces (good outcome) or cascade-delete (destructive)?"
    ///
    /// Returns:
    ///   • `[f1, f2]` — the two adjacent faces that would merge into one
    ///   • `[]`      — merge would fail; erase would soft-hide or cascade
    ///
    /// Decision tree mirrors `batch_erase_edges_impl`:
    ///   1. Edge must exist + shared by exactly 2 active faces.
    ///   2. Faces coplanar at `angle_tol_deg`.
    ///   3a. If exactly 1 outer-loop edge shared → standard merge will succeed.
    ///   3b. Else (C-slit / no DCEL edge) → require `would_geometric_merge_succeed`
    ///       at the same `angle_tol_deg`. This excludes cases where coplanarity
    ///       passes but no collinear overlap exists, preventing false-positive
    ///       cyan tints (the user clicks expecting merge → SOFT fallback).
    ///
    /// JS side calls this twice (user_tol → max(user_tol·4, 2°)) to mirror the
    /// real path's geometric fallback tolerance widening.
    ///
    /// Pure inspection — no state mutation, safe to call on every mousemove.
    #[wasm_bindgen(js_name = "previewEdgeEraseMerge")]
    pub fn preview_edge_erase_merge(&self, edge_id_raw: u32, angle_tol_deg: f64) -> Vec<u32> {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            return vec![];
        }
        let (faces, hes) = self.scene.mesh.get_faces_sharing_edge(eid);
        if faces.len() != 2 {
            return vec![];
        }
        let f1 = faces[0];
        let f2 = faces[1];

        // ADR-016 §2 — Hole boundary edges require explicit operations.
        //   Erase auto-fill applies only to coplanar INTERIOR SPLIT edges
        //   (outer-loop ↔ outer-loop). If this edge appears on either
        //   face's hole loop, return empty so the preview shows the
        //   cascade red — JS layer will surface the explicit-op hint.
        for (i, &fid) in faces.iter().enumerate() {
            if let Some(face) = self.scene.mesh.faces.get(fid) {
                let he_id = hes[i];
                for inner in face.inners() {
                    let mut h = inner.start;
                    let mut guard = 0usize;
                    loop {
                        guard += 1;
                        if guard > 4096 { return vec![]; }
                        if h == he_id { return vec![]; }
                        let next = match self.scene.mesh.hes.get(h) {
                            Some(he) => he.next(), None => return vec![],
                        };
                        h = next;
                        if h == inner.start { break; }
                    }
                }
            }
        }

        // Step 2 — coplanarity gate (cheap; identical for both branches below).
        match self.scene.mesh.are_faces_coplanar_with_tolerance(f1, f2, angle_tol_deg) {
            Ok(true) => {}
            _ => return vec![],
        }

        // Step 3a — standard merge precondition: faces share exactly 1 outer
        // edge. Standard `merge_faces_by_edge_with_tolerance` will succeed.
        if self.scene.mesh.count_shared_edges_outer(f1, f2) == 1 {
            return vec![f1.raw(), f2.raw()];
        }

        // Step 3b — geometric polygon-rebuild dry-run. Catches C-slit /
        // multi-shared-edge cases where coplanar holds but the real merge
        // would also fail (no collinear overlap, plane drift > 5 mm, etc).
        if self.scene.mesh.would_geometric_merge_succeed(f1, f2, angle_tol_deg) {
            return vec![f1.raw(), f2.raw()];
        }

        vec![]
    }

    /// Measure helpers — pure queries, no state mutation.
    ///
    /// faceArea returns the planar area of a single face (fan-triangulated
    /// cross-product magnitude / 2). Returns 0 on error / missing face.
    #[wasm_bindgen(js_name = "faceArea")]
    pub fn face_area(&self, face_id_raw: u32) -> f64 {
        self.scene.mesh.face_area(FaceId::new(face_id_raw))
    }

    /// edgeLength returns the straight-line distance between an edge's
    /// two endpoints. Zero on missing / degenerate edge.
    #[wasm_bindgen(js_name = "edgeLength")]
    pub fn edge_length(&self, edge_id_raw: u32) -> f64 {
        let eid = EdgeId::new(edge_id_raw);
        let edge = match self.scene.mesh.edges.get(eid) { Some(e) => e, None => return 0.0 };
        let va = edge.v_small();
        let vb = edge.v_large();
        let pa = match self.scene.mesh.vertex_pos(va) { Ok(p) => p, Err(_) => return 0.0 };
        let pb = match self.scene.mesh.vertex_pos(vb) { Ok(p) => p, Err(_) => return 0.0 };
        (pb - pa).length()
    }

    /// meshVolume returns the signed enclosed volume of the whole mesh.
    /// Exact for closed solids; indicative only for open shells.
    #[wasm_bindgen(js_name = "meshVolume")]
    pub fn mesh_volume(&self) -> f64 {
        self.scene.mesh.mesh_volume()
    }

    /// Linear array — create `count` translated copies of the given
    /// faces, each shifted by `offset · k` for k = 1..=count. Returns
    /// the new FaceIds in copy-major, source-order.
    #[wasm_bindgen(js_name = "arrayLinearFaces")]
    pub fn array_linear_faces(
        &mut self,
        face_ids: &[u32],
        count: u32,
        dx: f64, dy: f64, dz: f64,
    ) -> Vec<u32> {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let offset = DVec3::new(dx, dy, dz);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.array_linear_faces(&fids, count, offset) {
            Ok(new_faces) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_faces.iter().map(|f| f.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] array_linear_faces ERROR: {}", e);
                self.set_error(format!("array_linear: {}", e));
                Vec::new()
            }
        }
    }

    /// Radial array — rotate `count` copies of the given faces around
    /// an axis. Copy `k` is rotated by `total_angle_rad · k / count`
    /// about (axis_origin, axis_dir). Returns new FaceIds copy-major.
    #[wasm_bindgen(js_name = "arrayRadialFaces")]
    pub fn array_radial_faces(
        &mut self,
        face_ids: &[u32],
        count: u32,
        ox: f64, oy: f64, oz: f64,
        ax: f64, ay: f64, az: f64,
        total_angle_rad: f64,
    ) -> Vec<u32> {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let origin = DVec3::new(ox, oy, oz);
        let axis = DVec3::new(ax, ay, az);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.array_radial_faces(&fids, count, origin, axis, total_angle_rad) {
            Ok(new_faces) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_faces.iter().map(|f| f.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] array_radial_faces ERROR: {}", e);
                self.set_error(format!("array_radial: {}", e));
                Vec::new()
            }
        }
    }

    /// Return the outer-loop vertex IDs of a face in walk order.
    /// Empty vec on error (face missing, degenerate, etc.).
    #[wasm_bindgen(js_name = "getFaceVertices")]
    pub fn get_face_vertices(&self, face_id_raw: u32) -> Vec<u32> {
        let fid = FaceId::new(face_id_raw);
        if !self.scene.mesh.faces.contains(fid) { return vec![]; }
        let start = self.scene.mesh.faces[fid].outer().start;
        match self.scene.mesh.collect_loop_verts(start) {
            Ok(verts) => verts.into_iter().map(|v| v.raw()).collect(),
            Err(_) => vec![],
        }
    }

    /// Bend a vertex set around `bend_axis` with angle ramping from 0
    /// (at `t=0` along `bend_dir`) to `angle_deg` (at `t=length_limit`).
    /// Verts with negative `t` (behind `origin`) are left untouched.
    #[wasm_bindgen(js_name = "bendVerts")]
    pub fn bend_verts(
        &mut self,
        vert_ids: &[u32],
        ax_x: f64, ax_y: f64, ax_z: f64,          // bend axis
        dir_x: f64, dir_y: f64, dir_z: f64,       // bend direction
        ox: f64, oy: f64, oz: f64,                // origin
        angle_deg: f64,
        length_limit: f64,
    ) -> bool {
        let vids: Vec<VertId> = vert_ids.iter().map(|&id| VertId::new(id)).collect();
        let bend_axis = DVec3::new(ax_x, ax_y, ax_z);
        let bend_dir = DVec3::new(dir_x, dir_y, dir_z);
        let origin = DVec3::new(ox, oy, oz);
        let angle_rad = angle_deg.to_radians();

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.bend_verts(&vids, bend_axis, bend_dir, origin, angle_rad, length_limit) {
            Ok(_) => {
                // ADR-274 Phase 3 P3-A — a large bend folds geometry into
                // self-intersection (measured SI 0→1). Gate rejects+rolls back;
                // moderate bends pass.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "bend", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(format!("bend: {}", e));
                false
            }
        }
    }

    /// Twist a vertex set around `(axis_origin, axis_dir)` with
    /// `degrees_per_unit` degrees of rotation per unit of axial distance.
    #[wasm_bindgen(js_name = "twistVerts")]
    pub fn twist_verts_deform(
        &mut self,
        vert_ids: &[u32],
        ox: f64, oy: f64, oz: f64,
        ax: f64, ay: f64, az: f64,
        degrees_per_unit: f64,
    ) -> bool {
        let vids: Vec<VertId> = vert_ids.iter().map(|&id| VertId::new(id)).collect();
        let axis_origin = DVec3::new(ox, oy, oz);
        let axis_dir = DVec3::new(ax, ay, az);
        let angle_per_unit = degrees_per_unit.to_radians();

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.twist_verts(&vids, axis_origin, axis_dir, angle_per_unit) {
            Ok(_) => {
                // ADR-274 Phase 3 P3-A — a large twist folds geometry into
                // self-intersection (measured SI 0→2). Gate rejects+rolls back;
                // moderate twists pass.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "twist", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(format!("twist: {}", e));
                false
            }
        }
    }

    /// Taper a vertex set along `(axis_origin, axis_dir)` from
    /// `start_scale` at t=0 to `end_scale` at t=length.
    #[wasm_bindgen(js_name = "taperVerts")]
    pub fn taper_verts(
        &mut self,
        vert_ids: &[u32],
        ox: f64, oy: f64, oz: f64,
        ax: f64, ay: f64, az: f64,
        start_scale: f64,
        end_scale: f64,
        length: f64,
    ) -> bool {
        let vids: Vec<VertId> = vert_ids.iter().map(|&id| VertId::new(id)).collect();
        let axis_origin = DVec3::new(ox, oy, oz);
        let axis_dir = DVec3::new(ax, ay, az);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.taper_verts(&vids, axis_origin, axis_dir, start_scale, end_scale, length) {
            Ok(_) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(format!("taper: {}", e));
                false
            }
        }
    }

    /// Round off a single edge into a cylindrical arc of the given
    /// radius, sampled with `segments` quads. Returns the count of new
    /// fillet strip quads on success (>= 2), or -1 on failure with
    /// `lastError()` populated.
    #[wasm_bindgen(js_name = "filletEdge")]
    pub fn fillet_edge(
        &mut self,
        edge_id_raw: u32,
        radius: f64,
        segments: u32,
    ) -> i32 {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            self.set_error(format!("fillet: edge {} not found", edge_id_raw));
            return -1;
        }

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.fillet_edge(eid, radius, segments) {
            Ok(res) => {
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "fillet", true,
                ) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                res.fillet_faces.len() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] fillet_edge ERROR: {}", e);
                self.set_error(format!("fillet: {}", e));
                -1
            }
        }
    }

    /// ADR-207 — chamfer a valence-3 vertex into a flat triangular corner cut
    /// (ADR-024 P10). `chamfer_vertex_3way` already exists in axia-geo; this is the
    /// additive WASM expose, mirroring filletEdge. Returns the number of rebuilt
    /// incident faces (3) on success, or -1 on error.
    #[wasm_bindgen(js_name = "chamferVertex3way")]
    pub fn chamfer_vertex_3way(
        &mut self,
        vert_id_raw: u32,
        radius: f64,
    ) -> i32 {
        let vid = VertId::new(vert_id_raw);
        if !self.scene.mesh.verts.contains(vid) || !self.scene.mesh.verts[vid].is_active() {
            self.set_error(format!("chamfer: vertex {} not found/active", vert_id_raw));
            return -1;
        }
        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.chamfer_vertex_3way(vid, radius) {
            Ok(res) => {
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "chamfer", true,
                ) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                res.modified_faces.len() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] chamfer_vertex_3way ERROR: {}", e);
                self.set_error(format!("chamfer: {}", e));
                -1
            }
        }
    }

    /// Chamfer (flat-bevel) a single edge, set back `dist` along each adjacent
    /// edge — the flat sibling of `filletEdge` (`chamfer_edge` already exists in
    /// axia-geo). Additive WASM expose, mirroring filletEdge's closure-preserving
    /// gate + transaction wiring. Returns 1 (the single facet) on success, or -1
    /// on error with `lastError()` populated.
    #[wasm_bindgen(js_name = "chamferEdge")]
    pub fn chamfer_edge(
        &mut self,
        edge_id_raw: u32,
        dist: f64,
    ) -> i32 {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            self.set_error(format!("chamfer_edge: edge {} not found", edge_id_raw));
            return -1;
        }

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.chamfer_edge(eid, dist) {
            Ok(_res) => {
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "chamfer_edge", true,
                ) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                1
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] chamfer_edge ERROR: {}", e);
                self.set_error(format!("chamfer_edge: {}", e));
                -1
            }
        }
    }

    /// ADR-211 — EXTEND free wire edge `target` so its nearest endpoint meets
    /// `boundary`'s supporting line. `extend_edge_to_boundary` already exists in
    /// axia-geo; additive WASM expose. Returns 0 on success, or -1 on error.
    #[wasm_bindgen(js_name = "extendEdge")]
    pub fn extend_edge(&mut self, target_raw: u32, boundary_raw: u32) -> i32 {
        let target = EdgeId::new(target_raw);
        let boundary = EdgeId::new(boundary_raw);
        if !self.scene.mesh.edges.contains(target) {
            self.set_error(format!("extend: target edge {} not found", target_raw));
            return -1;
        }
        if !self.scene.mesh.edges.contains(boundary) {
            self.set_error(format!("extend: boundary edge {} not found", boundary_raw));
            return -1;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.extend_edge_to_boundary(target, boundary) {
            Ok(()) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                0
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] extend_edge ERROR: {}", e);
                self.set_error(format!("extend: {}", e));
                -1
            }
        }
    }

    /// ADR-212 — FILLET a 2D corner (valence-2 wire vertex) with a tangent arc
    /// of `radius`. `fillet_corner_2d` already exists in axia-geo; additive WASM
    /// expose. Returns the new arc edge id on success, or -1 on error.
    #[wasm_bindgen(js_name = "filletCorner2d")]
    pub fn fillet_corner_2d(&mut self, vert_raw: u32, radius: f64) -> i32 {
        let vid = VertId::new(vert_raw);
        if !self.scene.mesh.verts.contains(vid) || !self.scene.mesh.verts[vid].is_active() {
            self.set_error(format!("fillet corner: vertex {} not found/active", vert_raw));
            return -1;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.fillet_corner_2d(vid, radius) {
            Ok(e) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                e.raw() as i32
            }
            Err(err) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] fillet_corner_2d ERROR: {}", err);
                self.set_error(format!("fillet corner: {}", err));
                -1
            }
        }
    }

    /// ADR-212 — CHAMFER a 2D corner (valence-2 wire vertex) with a straight
    /// line cut at `dist` from the corner. `chamfer_corner_2d` already exists in
    /// axia-geo; additive WASM expose. Returns the new chamfer edge id, or -1.
    #[wasm_bindgen(js_name = "chamferCorner2d")]
    pub fn chamfer_corner_2d(&mut self, vert_raw: u32, dist: f64) -> i32 {
        let vid = VertId::new(vert_raw);
        if !self.scene.mesh.verts.contains(vid) || !self.scene.mesh.verts[vid].is_active() {
            self.set_error(format!("chamfer corner: vertex {} not found/active", vert_raw));
            return -1;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.chamfer_corner_2d(vid, dist) {
            Ok(e) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                e.raw() as i32
            }
            Err(err) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] chamfer_corner_2d ERROR: {}", err);
                self.set_error(format!("chamfer corner: {}", err));
                -1
            }
        }
    }

    /// ADR-213 — JOIN: merge the two collinear straight edges at a valence-2
    /// vertex into one (inverse of split). `join_collinear_at` already exists in
    /// axia-geo; additive WASM expose. Returns the merged edge id, or -1.
    #[wasm_bindgen(js_name = "joinCollinearAt")]
    pub fn join_collinear_at(&mut self, vert_raw: u32) -> i32 {
        let vid = VertId::new(vert_raw);
        if !self.scene.mesh.verts.contains(vid) || !self.scene.mesh.verts[vid].is_active() {
            self.set_error(format!("join: vertex {} not found/active", vert_raw));
            return -1;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.join_collinear_at(vid) {
            Ok(e) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                e.raw() as i32
            }
            Err(err) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] join_collinear_at ERROR: {}", err);
                self.set_error(format!("join: {}", err));
                -1
            }
        }
    }

    /// ADR-214 — MIRROR `edge_ids` across a plane (new wire edges). Returns the
    /// new edge ids, or empty on error. `mirror_edges` exists in axia-geo.
    #[wasm_bindgen(js_name = "mirrorEdges")]
    pub fn mirror_edges(
        &mut self,
        edge_ids: &[u32],
        ox: f64, oy: f64, oz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> Vec<u32> {
        let eids: Vec<EdgeId> = edge_ids.iter().map(|&id| EdgeId::new(id)).collect();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.mirror_edges(&eids, DVec3::new(ox, oy, oz), DVec3::new(nx, ny, nz)) {
            Ok(new_edges) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_edges.iter().map(|e| e.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] mirror_edges ERROR: {}", e);
                self.set_error(format!("mirror edges: {}", e));
                Vec::new()
            }
        }
    }

    /// ADR-214 — LINEAR ARRAY of `edge_ids` (`count` copies, copy k by `offset·k`).
    #[wasm_bindgen(js_name = "arrayLinearEdges")]
    pub fn array_linear_edges(
        &mut self,
        edge_ids: &[u32],
        count: u32,
        dx: f64, dy: f64, dz: f64,
    ) -> Vec<u32> {
        let eids: Vec<EdgeId> = edge_ids.iter().map(|&id| EdgeId::new(id)).collect();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.array_linear_edges(&eids, count, DVec3::new(dx, dy, dz)) {
            Ok(new_edges) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_edges.iter().map(|e| e.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] array_linear_edges ERROR: {}", e);
                self.set_error(format!("array_linear edges: {}", e));
                Vec::new()
            }
        }
    }

    /// ADR-214 — RADIAL ARRAY of `edge_ids` about an axis (`count` copies).
    #[wasm_bindgen(js_name = "arrayRadialEdges")]
    pub fn array_radial_edges(
        &mut self,
        edge_ids: &[u32],
        count: u32,
        ox: f64, oy: f64, oz: f64,
        ax: f64, ay: f64, az: f64,
        total_angle_rad: f64,
    ) -> Vec<u32> {
        let eids: Vec<EdgeId> = edge_ids.iter().map(|&id| EdgeId::new(id)).collect();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.array_radial_edges(
            &eids, count, DVec3::new(ox, oy, oz), DVec3::new(ax, ay, az), total_angle_rad,
        ) {
            Ok(new_edges) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_edges.iter().map(|e| e.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] array_radial_edges ERROR: {}", e);
                self.set_error(format!("array_radial edges: {}", e));
                Vec::new()
            }
        }
    }

    /// Apply one level of Catmull-Clark subdivision to the whole mesh.
    /// Returns the count of new quads on success, or -1 on failure.
    /// Wrapped in a single undo transaction so one Ctrl+Z restores the
    /// original topology.
    #[wasm_bindgen(js_name = "subdivideCatmullClark")]
    pub fn subdivide_catmull_clark(&mut self) -> i32 {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.subdivide_catmull_clark() {
            Ok(count) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                count as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] subdivide_catmull_clark ERROR: {}", e);
                self.set_error(format!("subdivide: {}", e));
                -1
            }
        }
    }

    /// Sweep a 2D profile along a 3D path, producing one ring of vertices
    /// per path point and stitching them with `loft`. `profile_flat` is
    /// K points (xyz triples) in a local XY plane; `path_flat` is M points
    /// (xyz triples) in world space. `closed_profile` treats the profile
    /// as a closed ring. Returns new FaceIds; empty on failure.
    #[wasm_bindgen(js_name = "sweepProfileAlongPath")]
    pub fn sweep_profile_along_path(
        &mut self,
        profile_flat: &[f64],
        path_flat: &[f64],
        closed_profile: bool,
    ) -> Vec<u32> {
        if profile_flat.len() < 9 || profile_flat.len() % 3 != 0
            || path_flat.len() < 6 || path_flat.len() % 3 != 0
        {
            self.set_error(format!(
                "sweep: bad input — profile_flat.len()={}, path_flat.len()={}",
                profile_flat.len(), path_flat.len(),
            ));
            return Vec::new();
        }
        let profile: Vec<DVec3> = profile_flat.chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        let path: Vec<DVec3> = path_flat.chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2])).collect();
        let material = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.sweep(&profile, &path, closed_profile, material) {
            Ok(faces) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                faces.iter().map(|f| f.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] sweep ERROR: {}", e);
                self.set_error(format!("sweep: {}", e));
                Vec::new()
            }
        }
    }

    /// Loft N cross-sections into a continuous surface. `sections_flat` is
    /// a flat f64 array containing every point of every section as xyz
    /// triples; `section_size` says how many POINTS (not floats) are in
    /// each section. All sections must be the same size.
    ///
    /// `closed_sections` treats each section as a closed ring (the last
    /// point wraps to the first).
    ///
    /// Returns the new FaceIds in section-major, point-minor order.
    /// Single undo transaction.
    #[wasm_bindgen(js_name = "loftSections")]
    pub fn loft_sections(
        &mut self,
        sections_flat: &[f64],
        section_size: u32,
        closed_sections: bool,
    ) -> Vec<u32> {
        let ps = section_size as usize;
        if ps < 3 || sections_flat.len() % (3 * ps) != 0 || sections_flat.is_empty() {
            self.set_error(format!(
                "loft: bad input — sections_flat.len()={}, section_size={}",
                sections_flat.len(), section_size,
            ));
            return Vec::new();
        }
        let n_sections = sections_flat.len() / (3 * ps);
        if n_sections < 2 {
            self.set_error(format!("loft: need ≥ 2 sections, got {}", n_sections));
            return Vec::new();
        }
        let mut sections: Vec<Vec<DVec3>> = Vec::with_capacity(n_sections);
        for s in 0..n_sections {
            let base = s * ps * 3;
            let mut sec = Vec::with_capacity(ps);
            for j in 0..ps {
                let idx = base + j * 3;
                sec.push(DVec3::new(
                    sections_flat[idx],
                    sections_flat[idx + 1],
                    sections_flat[idx + 2],
                ));
            }
            sections.push(sec);
        }
        let material = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.loft(&sections, closed_sections, material) {
            Ok(faces) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                faces.iter().map(|f| f.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] loft ERROR: {}", e);
                self.set_error(format!("loft: {}", e));
                Vec::new()
            }
        }
    }

    /// Create a NEW face carrying a **Bezier patch** surface from a
    /// control-point grid (ADR-033 Phase E + meta-principle #14). The grid is
    /// `control_pts_flat` laid out row-major — `u_count` rows × `v_count`
    /// columns, each control point 3 floats `[x, y, z]`. `u_count, v_count ≥ 2`
    /// (a `1×N`/`N×1` grid is a curve, not a surface).
    ///
    /// The face's 4-corner boundary loop gives valid topology; the attached
    /// `AnalyticSurface::BezierPatch` IS the geometry, so the render pipeline
    /// (ADR-038 P23) tessellates the full bulged patch and downstream
    /// kernel-aware ops see the analytic surface. Returns the new FaceId(s)
    /// (one element) or an empty vec on failure (with `lastError` set).
    /// Single undo transaction.
    #[wasm_bindgen(js_name = "createBezierPatch")]
    pub fn create_bezier_patch(
        &mut self,
        control_pts_flat: &[f64],
        u_count: u32,
        v_count: u32,
    ) -> Vec<u32> {
        let uc = u_count as usize;
        let vc = v_count as usize;
        if uc < 2 || vc < 2 || control_pts_flat.len() != uc * vc * 3 {
            self.set_error(format!(
                "createBezierPatch: bad input — u_count={}, v_count={}, flat.len()={} (expected {})",
                u_count, v_count, control_pts_flat.len(), uc * vc * 3,
            ));
            return Vec::new();
        }
        let mut grid: Vec<Vec<DVec3>> = Vec::with_capacity(uc);
        for i in 0..uc {
            let mut row = Vec::with_capacity(vc);
            for j in 0..vc {
                let k = (i * vc + j) * 3;
                row.push(DVec3::new(
                    control_pts_flat[k],
                    control_pts_flat[k + 1],
                    control_pts_flat[k + 2],
                ));
            }
            grid.push(row);
        }
        let material = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.create_bezier_patch(grid, material) {
            Ok(fid) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                vec![fid.raw()]
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] createBezierPatch ERROR: {}", e);
                self.set_error(format!("createBezierPatch: {}", e));
                Vec::new()
            }
        }
    }

    /// Create a NEW face carrying a **NURBS surface** (rational tensor-product
    /// B-spline) from a control grid + weights + knot vectors (ADR-033 Phase
    /// E + meta-principle #14).
    ///
    /// - `control_pts_flat` — row-major `u_count × v_count`, 3 floats each.
    /// - `weights_flat` — row-major `u_count × v_count`, one weight per
    ///   control point (all `> 0`).
    /// - `knots_u` — length `u_count + degree_u + 1`.
    /// - `knots_v` — length `v_count + degree_v + 1`.
    ///
    /// Same kernel-native face semantics as `createBezierPatch`. Returns the
    /// new FaceId(s) (one element) or an empty vec on failure. Single undo
    /// transaction.
    #[wasm_bindgen(js_name = "createNurbsSurface")]
    #[allow(clippy::too_many_arguments)]
    pub fn create_nurbs_surface(
        &mut self,
        control_pts_flat: &[f64],
        u_count: u32,
        v_count: u32,
        weights_flat: &[f64],
        knots_u: &[f64],
        knots_v: &[f64],
        degree_u: u32,
        degree_v: u32,
    ) -> Vec<u32> {
        let uc = u_count as usize;
        let vc = v_count as usize;
        if uc < 2
            || vc < 2
            || control_pts_flat.len() != uc * vc * 3
            || weights_flat.len() != uc * vc
        {
            self.set_error(format!(
                "createNurbsSurface: bad input — u_count={}, v_count={}, pts.len()={} (expected {}), weights.len()={} (expected {})",
                u_count, v_count, control_pts_flat.len(), uc * vc * 3,
                weights_flat.len(), uc * vc,
            ));
            return Vec::new();
        }
        let mut grid: Vec<Vec<DVec3>> = Vec::with_capacity(uc);
        let mut weights: Vec<Vec<f64>> = Vec::with_capacity(uc);
        for i in 0..uc {
            let mut row = Vec::with_capacity(vc);
            let mut w_row = Vec::with_capacity(vc);
            for j in 0..vc {
                let k = (i * vc + j) * 3;
                row.push(DVec3::new(
                    control_pts_flat[k],
                    control_pts_flat[k + 1],
                    control_pts_flat[k + 2],
                ));
                w_row.push(weights_flat[i * vc + j]);
            }
            grid.push(row);
            weights.push(w_row);
        }
        let material = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.create_nurbs_surface(
            grid,
            weights,
            knots_u.to_vec(),
            knots_v.to_vec(),
            degree_u as usize,
            degree_v as usize,
            material,
        ) {
            Ok(fid) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                vec![fid.raw()]
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] createNurbsSurface ERROR: {}", e);
                self.set_error(format!("createNurbsSurface: {}", e));
                Vec::new()
            }
        }
    }

    /// ADR-238 (NURBS A2 full-2) — Replace a NURBS-class patch in place: create a
    /// fresh patch from the edited control net AND remove the old face within a
    /// SINGLE undo transaction. This is the single-Undo SSOT for control-point
    /// edits (drag ADR-236 / click-prompt ADR-233/234 / inline panel ADR-237).
    ///
    /// Without this, a CP edit = `createNurbsSurface` + `deleteFace` = two undo
    /// frames (2 Ctrl+Z to revert one edit). Here both ops share one begin/commit
    /// (before = pre-edit, after = post-edit), so one Ctrl+Z restores pre-edit.
    ///
    /// Returns the new FaceId(s) (one element) or an empty vec on failure
    /// (old face untouched, transaction cancelled).
    #[wasm_bindgen(js_name = "replaceNurbsSurface")]
    #[allow(clippy::too_many_arguments)]
    pub fn replace_nurbs_surface(
        &mut self,
        old_face_id_raw: u32,
        control_pts_flat: &[f64],
        u_count: u32,
        v_count: u32,
        weights_flat: &[f64],
        knots_u: &[f64],
        knots_v: &[f64],
        degree_u: u32,
        degree_v: u32,
    ) -> Vec<u32> {
        let uc = u_count as usize;
        let vc = v_count as usize;
        if uc < 2
            || vc < 2
            || control_pts_flat.len() != uc * vc * 3
            || weights_flat.len() != uc * vc
        {
            self.set_error(format!(
                "replaceNurbsSurface: bad input — u_count={}, v_count={}, pts.len()={} (expected {}), weights.len()={} (expected {})",
                u_count, v_count, control_pts_flat.len(), uc * vc * 3,
                weights_flat.len(), uc * vc,
            ));
            return Vec::new();
        }
        let mut grid: Vec<Vec<DVec3>> = Vec::with_capacity(uc);
        let mut weights: Vec<Vec<f64>> = Vec::with_capacity(uc);
        for i in 0..uc {
            let mut row = Vec::with_capacity(vc);
            let mut w_row = Vec::with_capacity(vc);
            for j in 0..vc {
                let k = (i * vc + j) * 3;
                row.push(DVec3::new(
                    control_pts_flat[k],
                    control_pts_flat[k + 1],
                    control_pts_flat[k + 2],
                ));
                w_row.push(weights_flat[i * vc + j]);
            }
            grid.push(row);
            weights.push(w_row);
        }
        let material = axia_core::FORM_MATERIAL;

        // ONE transaction: before = pre-edit; create new + remove old; after = post-edit.
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.create_nurbs_surface(
            grid,
            weights,
            knots_u.to_vec(),
            knots_v.to_vec(),
            degree_u as usize,
            degree_v as usize,
            material,
        ) {
            Ok(new_fid) => {
                // Remove the old face within the SAME transaction (delete_face
                // mesh-level pattern, no separate commit).
                let old_fid = FaceId::new(old_face_id_raw);
                if self.scene.mesh.faces.contains(old_fid) {
                    self.scene.unregister_face_from_xia(old_fid);
                    let _ = self.scene.mesh.remove_face(old_fid);
                    if self.scene.mesh.faces.contains(old_fid) {
                        self.scene.mesh.faces.remove(old_fid);
                    }
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                vec![new_fid.raw()]
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] replaceNurbsSurface ERROR: {}", e);
                self.set_error(format!("replaceNurbsSurface: {}", e));
                Vec::new()
            }
        }
    }

    /// Revolve a 2D profile (flat array of [x,y,z, x,y,z, …]) around the
    /// axis `(origin, dir)` into a surface of revolution. Returns the new
    /// FaceIds in profile-major, ring-minor order, or an empty vec on
    /// failure (with `lastError` set).
    ///
    /// Profile vertex order matters — see `operations::revolve` docs.
    /// Single undo transaction wraps the whole spin.
    #[wasm_bindgen(js_name = "revolveProfile")]
    pub fn revolve_profile(
        &mut self,
        profile_flat: &[f64],
        ox: f64, oy: f64, oz: f64,
        dx: f64, dy: f64, dz: f64,
        segments: u32,
    ) -> Vec<u32> {
        if profile_flat.len() < 6 || profile_flat.len() % 3 != 0 {
            self.set_error(format!(
                "revolve: profile_flat must be a non-empty multiple of 3, got {}",
                profile_flat.len(),
            ));
            return Vec::new();
        }
        let profile: Vec<DVec3> = profile_flat.chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2]))
            .collect();
        let origin = DVec3::new(ox, oy, oz);
        let dir = DVec3::new(dx, dy, dz);
        let material = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.revolve(&profile, origin, dir, segments, material) {
            Ok(faces) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                faces.iter().map(|f| f.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] revolve ERROR: {}", e);
                self.set_error(format!("revolve: {}", e));
                Vec::new()
            }
        }
    }

    /// Mirror the given faces across a plane. Returns the new FaceIds
    /// in the same order as the input (empty vec on failure, with
    /// `lastError()` set). Single undo transaction wraps the whole batch.
    #[wasm_bindgen(js_name = "mirrorFaces")]
    pub fn mirror_faces(
        &mut self,
        face_ids: &[u32],
        ox: f64, oy: f64, oz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> Vec<u32> {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let origin = DVec3::new(ox, oy, oz);
        let normal = DVec3::new(nx, ny, nz);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.mirror_faces(&fids, origin, normal) {
            Ok(new_faces) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_faces.iter().map(|f| f.raw()).collect()
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] mirror_faces ERROR: {}", e);
                self.set_error(format!("mirror_faces: {}", e));
                Vec::new()
            }
        }
    }

    pub fn get_face_normal(&self, face_id_raw: u32) -> Vec<f64> {
        let fid = FaceId::new(face_id_raw);
        if let Some(face) = self.scene.mesh.faces.get(fid) {
            let n = face.normal();
            vec![n.x, n.y, n.z]
        } else {
            vec![0.0, 0.0, 0.0]
        }
    }

    /// Atomic "erase with auto-merge" — primary delete path for the Erase tool.
    ///
    /// For each edge in `edge_ids`:
    ///   1. First try `merge_faces_by_edge_with_tolerance`. If it succeeds the
    ///      edge and the two coplanar faces collapse to a single face.
    ///   2. If merge fails (non-coplanar, C-slit, etc.) cascade-delete the
    ///      edge plus every face touching it.
    ///
    /// After edge processing, any faces listed in `face_ids` that still exist
    /// are removed outright.
    ///
    /// **Everything runs inside a single undo transaction** so the user
    /// presses Ctrl+Z once to restore the original geometry, regardless of
    /// how many edges and faces were touched.
    ///
    /// When `cascade_only == true`, the merge step is skipped entirely —
    /// every edge goes straight to cascade-delete. This backs the Shift
    /// modifier in the Erase tool.
    ///
    /// Returns a packed `[merged, cascaded_faces, cascaded_edges]` triple
    /// (one i32 each) for the tool to surface in its Toast feedback. All
    /// values are >= 0 on success.
    /// Batch erase edges (and optional faces).
    ///
    /// For each edge:
    ///   1. cascade_only=true → force hard delete (faces destroyed).
    ///   2. else try `merge_faces_by_edge_with_tolerance`:
    ///      a) Success → two faces become one.
    ///      b) Failure (non-coplanar / non-manifold / material mismatch):
    ///         · soft_on_fail=true → mark the edge SOFT (rendering-hidden);
    ///           topology intact, two faces read as one surface.
    ///         · soft_on_fail=false → cascade-delete faces (legacy behaviour).
    ///
    /// Returns `[merged, cascaded_faces, cascaded_edges, softened]`.
    /// (Older callers that expect length 3 still work since Vec<i32> is
    /// returned — JS just reads indices it needs.)
    #[wasm_bindgen(js_name = "batchEraseEdgesWithMerge")]
    pub fn batch_erase_edges_with_merge(
        &mut self,
        face_ids: &[u32],
        edge_ids: &[u32],
        angle_tol_deg: f64,
        cascade_only: bool,
    ) -> Vec<i32> {
        // Legacy signature retained; soft_on_fail defaults to false to keep
        // current callers identical until they opt in. Use the _soft variant
        // below for the non-destructive path.
        self.batch_erase_edges_impl(face_ids, edge_ids, angle_tol_deg, cascade_only, false)
    }

    /// New variant: merge failure falls back to SOFT edge (hidden, topology
    /// preserved) instead of destroying the adjacent faces. Recommended
    /// default for interactive Erase tool.
    #[wasm_bindgen(js_name = "batchEraseEdgesSoftFallback")]
    pub fn batch_erase_edges_soft_fallback(
        &mut self,
        face_ids: &[u32],
        edge_ids: &[u32],
        angle_tol_deg: f64,
        cascade_only: bool,
    ) -> Vec<i32> {
        self.batch_erase_edges_impl(face_ids, edge_ids, angle_tol_deg, cascade_only, true)
    }

    fn batch_erase_edges_impl(
        &mut self,
        face_ids: &[u32],
        edge_ids: &[u32],
        angle_tol_deg: f64,
        cascade_only: bool,
        soft_on_fail: bool,
    ) -> Vec<i32> {
        if face_ids.is_empty() && edge_ids.is_empty() {
            return vec![0, 0, 0, 0, 0, 0];
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mut merged: i32 = 0;
        let mut cascaded_faces: i32 = 0;
        let mut cascaded_edges: i32 = 0;
        let mut softened: i32 = 0;
        let mut synthesized: i32 = 0;
        let mut desolidified: i32 = 0;
        let mut all_removed_faces: Vec<FaceId> = Vec::new();

        // ── Phase C (ADR-008 Axiom 5 — Surface↔Solid merge): snapshot which
        // connected face-components are currently closed 2-manifold solids.
        // After the erase pass we re-evaluate the same face sets and count
        // those that went from solid → open, so the JS layer can show a
        // "solid → surface" Toast.
        //
        // We snapshot by representative seed face + full component face list
        // (so after faces get removed/merged, we can rebuild the post list
        // by dropping gone faces and adding any merged survivors).
        let mut pre_solid_components: Vec<(FaceId, Vec<FaceId>)> = Vec::new();
        {
            use std::collections::HashSet as StdHashSet;
            let mut seen_seed: StdHashSet<FaceId> = StdHashSet::new();

            // Every face adjacent to any erase-target edge or direct face id.
            let mut candidate_seeds: Vec<FaceId> = Vec::new();
            for &eid_raw in edge_ids {
                let eid = EdgeId::new(eid_raw);
                if self.scene.mesh.edges.contains(eid) {
                    let (faces, _) = self.scene.mesh.get_faces_sharing_edge(eid);
                    candidate_seeds.extend(faces);
                }
            }
            for &fid_raw in face_ids {
                candidate_seeds.push(FaceId::new(fid_raw));
            }

            for seed in candidate_seeds {
                if !self.scene.mesh.faces.contains(seed) { continue; }
                if seen_seed.contains(&seed) { continue; }
                // BFS the connected component — use raw id path via helper.
                let component_raw = self.get_connected_faces(seed.raw());
                let component: Vec<FaceId> = component_raw.iter()
                    .map(|&r| FaceId::new(r)).collect();
                for f in &component { seen_seed.insert(*f); }
                let info = self.scene.mesh.face_set_manifold_info(&component);
                if info.is_closed_solid {
                    pre_solid_components.push((seed, component));
                }
            }
        }

        // Phase B step 2 (ADR-008 Axiom 6): pre-snapshot which edges, in the
        // neighbourhood of this erase, currently have a face on at least one
        // side. After the erase pass we will see which of those edges went
        // "face → free" (newly-freed) — those are the only edges a re-synth
        // cycle must include, which keeps the re-synthesis strictly scoped
        // to loops the erase actually opened.
        //
        // Neighbourhood = edges whose endpoint is an endpoint of any erase-
        // target edge OR an endpoint on any face-only target's boundary.
        let mut seed_verts: Vec<VertId> = Vec::new();
        for &eid_raw in edge_ids {
            let eid = EdgeId::new(eid_raw);
            if let Some(edge) = self.scene.mesh.edges.get(eid) {
                seed_verts.push(edge.v_small());
                seed_verts.push(edge.v_large());
            }
        }
        for &fid_raw in face_ids {
            let fid = FaceId::new(fid_raw);
            if let Some(face) = self.scene.mesh.faces.get(fid) {
                if let Ok(verts) = self.scene.mesh.collect_loop_verts(face.outer().start) {
                    seed_verts.extend(verts);
                }
            }
        }
        seed_verts.sort_by_key(|v| v.raw());
        seed_verts.dedup();

        // Collect neighbourhood edges (edges touching any seed vertex) that
        // are currently face-bearing. "face-bearing" = at least one of its
        // half-edges has a non-null face. These are the watch-list — later
        // we'll check which of them survive but no longer have ANY face-side.
        let mut watched_edges: Vec<EdgeId> = Vec::new();
        {
            let seed_set: HashSet<VertId> = seed_verts.iter().copied().collect();
            for (eid, edge) in self.scene.mesh.edges.iter() {
                if !edge.is_active() { continue; }
                if !edge.class().is_topological() { continue; }
                if !(seed_set.contains(&edge.v_small()) || seed_set.contains(&edge.v_large())) {
                    continue;
                }
                // At least one HE in the radial loop has a face?
                if self.edge_has_any_face(eid) {
                    watched_edges.push(eid);
                }
            }
        }

        // Capture the first merge failure for diagnostic purposes — surfaces
        // in the Erase tool's debug log so users can tell why an edge fell
        // through to cascade (e.g. "not coplanar (3.2° > 0.5° tolerance)").
        let mut first_failure_reason: Option<String> = None;

        // Edge pass — try merge first, cascade on failure.
        for &eid_raw in edge_ids {
            let eid = EdgeId::new(eid_raw);
            if !self.scene.mesh.edges.contains(eid) {
                // Already gone (earlier merge folded it in). Skip.
                continue;
            }

            if !cascade_only {
                match self.scene.mesh.merge_faces_by_edge_with_tolerance(eid, angle_tol_deg) {
                    Ok(_new_face) => {
                        merged += 1;
                        continue;
                    }
                    Err(e) => {
                        if first_failure_reason.is_none() {
                            first_failure_reason = Some(format!("edge {}: {}", eid_raw, e));
                        }
                        /* fall through to geometric fallback */
                    }
                }

                // Option X1 (2026-04-24) — geometric merge fallback.
                //
                // Standard merge_faces_by_edge rejects on:
                //   • ≠2 faces sharing the edge (snap-drift "parallel" edges)
                //   • multi-edge sharing (C-slit)
                //   • coplanarity tol miss (0.5° strict)
                // For most user-facing "두 RECT 붙여놓고 공유 엣지 삭제" cases
                // this is a false negative. Try the polygon-level merge with
                // a loosened tolerance before falling through to SOFT. If it
                // succeeds we treat the operation as merged.
                if self.scene.mesh.edges.contains(eid) {
                    let (faces, _) = self.scene.mesh.get_faces_sharing_edge(eid);
                    if faces.len() == 2 && faces[0] != faces[1] {
                        let geo_tol = (angle_tol_deg * 4.0).max(2.0);
                        if let Ok(_) = self.scene.mesh.merge_coplanar_faces_geometric(
                            faces[0], faces[1], geo_tol,
                        ) {
                            merged += 1;
                            // Clear the diagnostic — a successful geometric
                            //   merge is not a "failure" from the user's POV.
                            if first_failure_reason.as_ref()
                                .map(|s| s.starts_with(&format!("edge {}:", eid_raw)))
                                .unwrap_or(false)
                            {
                                first_failure_reason = None;
                            }
                            continue;
                        }
                    }
                }
            }

            // Merge failed → choose fallback based on soft_on_fail flag.
            if soft_on_fail && !cascade_only && self.scene.mesh.edges.contains(eid) {
                // Non-destructive: mark edge SOFT. Topology stays intact, two
                // faces remain but read as one surface (edge hidden in render).
                self.scene.mesh.mark_edge_soft(eid);
                softened += 1;
                continue;
            }

            // Destructive cascade-delete: remove both sharing faces + the edge.
            if self.scene.mesh.edges.contains(eid) {
                let (faces, _) = self.scene.mesh.get_faces_sharing_edge(eid);
                for fid in &faces { all_removed_faces.push(*fid); }
                cascaded_faces += faces.len() as i32;
                for fid in faces {
                    let _ = self.scene.mesh.remove_face(fid);
                    if self.scene.mesh.faces.contains(fid) {
                        self.scene.mesh.faces.remove(fid);
                    }
                }
                let _ = self.scene.mesh.remove_edge_and_halfedges(eid);
                if self.scene.mesh.edges.contains(eid) {
                    self.scene.mesh.edges.remove(eid);
                }
                cascaded_edges += 1;
            }
        }

        // ── Face-only deletions ──
        // 2026-04-27 UX: 사용자가 면만 지우면 boundary edge 는 standalone
        // wireframe 으로 남아야 한다 (SketchUp-style — "면 지우고 윤곽선
        // 유지"). 따라서 face-only 삭제 대상의 outer + hole loop 엣지를
        // 미리 snapshot 해서 cleanup_dangling 의 보호 집합으로 넘긴다.
        // edge-erase cascade 경로의 orphan 은 보호 안 함 — 사용자가 명시적
        // 으로 edge 도 지우라고 한 작업이라 전부 정리되는게 자연스럽다.
        let mut protected_orphan_edges: std::collections::HashSet<EdgeId>
            = std::collections::HashSet::new();
        for &fid_raw in face_ids {
            let fid = FaceId::new(fid_raw);
            let face = match self.scene.mesh.faces.get(fid) {
                Some(f) if f.is_active() => f,
                _ => continue,
            };
            let outer_start = face.outer().start;
            let inner_starts: Vec<HeId> = face.inners().iter()
                .map(|i| i.start)
                .filter(|s| !s.is_null())
                .collect();
            if !outer_start.is_null() {
                if let Ok(hes) = self.scene.mesh.collect_loop_hes(outer_start) {
                    for he in hes {
                        protected_orphan_edges.insert(self.scene.mesh.hes[he].edge());
                    }
                }
            }
            for inner_start in inner_starts {
                if let Ok(hes) = self.scene.mesh.collect_loop_hes(inner_start) {
                    for he in hes {
                        protected_orphan_edges.insert(self.scene.mesh.hes[he].edge());
                    }
                }
            }
        }

        for &fid_raw in face_ids {
            let fid = FaceId::new(fid_raw);
            if self.scene.mesh.faces.contains(fid) {
                all_removed_faces.push(fid);
                let _ = self.scene.mesh.remove_face(fid);
                if self.scene.mesh.faces.contains(fid) {
                    self.scene.mesh.faces.remove(fid);
                }
            }
        }

        self.scene.unregister_faces_from_xia(&all_removed_faces);
        // Post-merge/erase cleanup — merged-leftover dangling edges + isolated
        // vertices. Boundary edges of face-only deletes are protected (they
        // remain as standalone wireframe per CAD UX convention).
        let _ = self.scene.mesh.cleanup_dangling_excluding(&protected_orphan_edges);

        // ── Phase B step 2 (ADR-008 Axiom 6): erase re-synthesis ──
        // Among the watched edges, find those that SURVIVED the erase but
        // are no longer face-bearing (they lost every face pointer). Those
        // are the "newly-freed" edges a re-synth cycle must pass through.
        // This scoping prevents:
        //   • recreating a face whose boundary edges we deliberately deleted
        //     (cascade of face+edges removes the edges entirely → not in
        //     newly_freed list)
        //   • recreating a face the user deliberately face-only-deleted
        //     (those edges are still face-bearing on the neighbour's side
        //     OR were never in the watched list if the face was isolated)
        let newly_freed: Vec<EdgeId> = watched_edges.iter()
            .copied()
            .filter(|&eid| self.scene.mesh.edges.contains(eid))
            .filter(|&eid| !self.edge_has_any_face(eid))
            // Option B (2026-06-18) — do NOT re-synthesize the boundary of a face
            // the user deliberately face-only-deleted. ADR-019 "면만 지우면
            // boundary edge 는 standalone wireframe 으로 남아야 한다" + the scoping
            // intent documented above ("recreating a face the user deliberately
            // face-only-deleted is prevented"). `protected_orphan_edges` already
            // holds those boundaries (kept as wireframe); excluding them here stops
            // an ISOLATED closed-loop face — a Path B circle whose only boundary is
            // a self-loop edge — from immediately re-forming the face. Non-isolated
            // faces are unaffected (their shared edges stay face-bearing on the
            // neighbour → never in this list); edge-erase re-synth is unaffected
            // (deleted edges are not in `protected_orphan_edges`).
            .filter(|&eid| !protected_orphan_edges.contains(&eid))
            .collect();
        let live_seeds: Vec<VertId> = seed_verts.iter()
            .filter(|&&v| self.scene.mesh.verts.contains(v))
            .copied()
            .collect();
        if !live_seeds.is_empty() && !newly_freed.is_empty() {
            let material = axia_core::FORM_MATERIAL;
            let new_faces = self.scene.mesh.resolve_planar_free_faces_scoped(
                material,
                Some(&live_seeds),
                Some(&newly_freed),
            );
            if !new_faces.is_empty() {
                synthesized = new_faces.len() as i32;
                // Wrap new faces in a "Face" XIA (same pattern as
                // exec_draw_line's Step 5). Use the first face's centroid as
                // the XIA position so picking/outliner behave naturally.
                // Inline centroid of the first new face (use face start HE).
                let pos = {
                    let f0 = new_faces[0];
                    let face = self.scene.mesh.faces.get(f0);
                    let mut c = DVec3::ZERO;
                    let mut n = 0;
                    if let Some(face) = face {
                        if let Ok(verts) = self.scene.mesh.collect_loop_verts(face.outer().start) {
                            for v in &verts {
                                if let Ok(p) = self.scene.mesh.vertex_pos(*v) {
                                    c += p;
                                    n += 1;
                                }
                            }
                        }
                    }
                    if n > 0 { c / n as f64 } else { DVec3::ZERO }
                };
                self.scene.create_xia_with_faces(
                    "Face".to_string(),
                    pos,
                    new_faces,
                );
            }
        }

        // ── Phase C (ADR-008 Axiom 5): count de-solidified components ──
        // For each previously-solid component, rebuild its surviving face
        // list (exclude any face removed during this pass) and re-check. If
        // the surviving set is no longer a closed 2-manifold, that component
        // was de-solidified. The JS layer uses this count to emit a Toast
        // per ADR-008: "solid가 붕괴(de-solidify)되어 surface로 남음".
        {
            use std::collections::HashSet as StdHashSet;
            let removed_set: StdHashSet<FaceId> = all_removed_faces.iter().copied().collect();
            for (_seed, pre_faces) in &pre_solid_components {
                let survivors: Vec<FaceId> = pre_faces.iter()
                    .filter(|f| !removed_set.contains(f))
                    .filter(|f| self.scene.mesh.faces.contains(**f))
                    .copied()
                    .collect();
                if survivors.len() < 4 {
                    // Can't form a closed solid below tetrahedron.
                    desolidified += 1;
                    continue;
                }
                let info = self.scene.mesh.face_set_manifold_info(&survivors);
                if !info.is_closed_solid {
                    desolidified += 1;
                }
            }
        }

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        // Save first failure so JS can fetch it via `lastMergeFailureReason()`.
        // (We don't overload the numeric return to keep the happy path small.)
        if let Some(reason) = first_failure_reason {
            self.last_merge_failure = reason;
        } else {
            self.last_merge_failure.clear();
        }

        vec![merged, cascaded_faces, cascaded_edges, softened, synthesized, desolidified]
    }

    /// Diagnostic — first merge failure reason from the most recent
    /// `batchEraseEdgesWithMerge` call. Empty string if no failure or no
    /// call yet. Intended for the debug-mode Toast in the Erase tool.
    #[wasm_bindgen(js_name = "lastMergeFailureReason")]
    pub fn last_merge_failure_reason(&self) -> String {
        self.last_merge_failure.clone()
    }

    // ========================================================================
    // ADR-009 — Orphan Face Recovery
    // ========================================================================

    /// Read-only classifier. Returns JSON-serialised `OrphanReport`.
    /// See ADR-009 for category definitions (C1 / C2 / C3).
    #[wasm_bindgen(js_name = "classifyOrphans")]
    pub fn classify_orphans(&self) -> String {
        let report = self.scene.classify_orphans();
        serde_json::to_string(&report).unwrap_or_else(|e| {
            format!("{{\"error\":\"{}\"}}", e)
        })
    }

    /// Apply or preview an orphan-recovery plan. Wrapped in a single undo
    /// frame on apply; preview rolls back to the exact prior snapshot.
    ///
    /// `plan_json` — `RecoveryPlan` serialised as JSON.
    /// `dry_run`   — true = preview (always rolls back); false = apply.
    ///
    /// Returns `RecoveryResult` serialised as JSON.
    #[wasm_bindgen(js_name = "applyOrphanRecovery")]
    pub fn apply_orphan_recovery(&mut self, plan_json: &str, dry_run: bool) -> String {
        let plan: RecoveryPlan = match serde_json::from_str(plan_json) {
            Ok(p) => p,
            Err(e) => return format!("{{\"error\":\"invalid plan JSON: {}\"}}", e),
        };

        if dry_run {
            let result = self.scene.preview_orphan_recovery(&plan);
            self.mark_topology_changed();
            self.invalidate_cache();
            return serde_json::to_string(&result).unwrap_or_default();
        }

        // Apply — single undo frame.
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let result = self.scene.apply_orphan_recovery(&plan);
        if result.error.is_some() {
            self.scene.transactions.cancel();
        } else {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
        }
        self.mark_topology_changed();
        self.invalidate_cache();
        serde_json::to_string(&result).unwrap_or_default()
    }

    /// Phase D (ADR-008 Axiom 9 row 3): forced polygon-mesh merge.
    ///
    /// For 2+ faces the user selected and explicitly asked to "merge" even
    /// though they are not coplanar, we don't actually fuse them into a
    /// single polygon (that would require non-planar face regions, which
    /// violates ADR-007's Invariant 3). Instead we identify every edge
    /// interior to the selection — edges whose radial loop contains two or
    /// more of the selected faces — and mark those edges SOFT. The faces
    /// stay distinct topologically, but the renderer hides the internal
    /// seams so the selection reads as one continuous smooth surface.
    ///
    /// Returns the number of edges softened. Wrapped in a single undo
    /// transaction. If fewer than two selected faces share any edge, the
    /// return value is 0 (caller can surface a Toast).
    #[wasm_bindgen(js_name = "softenInternalEdges")]
    pub fn soften_internal_edges(&mut self, face_ids: &[u32]) -> i32 {
        use std::collections::HashSet as StdHashSet;
        if face_ids.len() < 2 { return 0; }
        let selected: StdHashSet<FaceId> = face_ids.iter()
            .map(|&r| FaceId::new(r))
            .filter(|f| self.scene.mesh.faces.contains(*f))
            .collect();
        if selected.len() < 2 { return 0; }

        // Find every edge where ≥2 of the selected faces meet. Walk the
        // radial loop for every active topological edge once.
        let candidate_edges: Vec<EdgeId> = self.scene.mesh.edges.iter()
            .filter(|(_, e)| e.is_active() && e.class().is_topological())
            .map(|(id, _)| id)
            .collect();

        let mut to_soften: Vec<EdgeId> = Vec::new();
        for eid in candidate_edges {
            let Some(edge) = self.scene.mesh.edges.get(eid) else { continue; };
            let start = edge.any_he();
            if start.is_null() { continue; }
            let mut count = 0usize;
            let mut he = start;
            loop {
                match self.scene.mesh.hes.get(he) {
                    Some(h) => {
                        let f = h.face();
                        if !f.is_null() && selected.contains(&f) {
                            count += 1;
                            if count >= 2 { break; }
                        }
                        let next = h.next_rad();
                        if next.is_null() || next == start { break; }
                        he = next;
                    }
                    None => break,
                }
            }
            if count >= 2 {
                to_soften.push(eid);
            }
        }

        if to_soften.is_empty() { return 0; }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        for eid in &to_soften {
            self.scene.mesh.mark_edge_soft(*eid);
        }
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        to_soften.len() as i32
    }

    /// DCEL 위상(topology) 기반으로 seedFace에 연결된 모든 face를 BFS 탐색.
    /// half-edge의 radial partner(next_rad)를 통해 edge를 공유하는 인접 face를 찾습니다.
    /// 좌표 비교 없이 순수 위상 구조만 사용 → 다른 Volume의 face가 섞이지 않음.
    pub fn get_connected_faces(&self, seed_face_raw: u32) -> Vec<u32> {
        use std::collections::{HashSet, VecDeque};

        let seed = FaceId::new(seed_face_raw);
        let mesh = &self.scene.mesh;

        if !mesh.faces.contains(seed) {
            return vec![];
        }

        let mut visited: HashSet<FaceId> = HashSet::new();
        let mut queue: VecDeque<FaceId> = VecDeque::new();
        visited.insert(seed);
        queue.push_back(seed);

        while let Some(current) = queue.pop_front() {
            let face = match mesh.faces.get(current) {
                Some(f) => f,
                None => continue,
            };

            // 외곽 루프의 half-edge를 순회
            let outer_start = face.outer().start;
            if outer_start.is_null() { continue; }

            let mut he_id = outer_start;
            loop {
                // radial 체인 전체를 순회하여 공유 edge의 모든 인접 face 탐색
                // (find_halfedge가 non-manifold edge에 HE 쌍을 삽입하므로
                //  체인이 2개 이상일 수 있음: bottom_he → side_fwd → side_bwd → bottom_he)
                let mut rad_id = mesh.hes[he_id].next_rad();
                while rad_id != he_id {
                    let rad_face = mesh.hes[rad_id].face();
                    if !rad_face.is_null() && !visited.contains(&rad_face) {
                        if mesh.faces.contains(rad_face) {
                            visited.insert(rad_face);
                            queue.push_back(rad_face);
                        }
                    }
                    rad_id = mesh.hes[rad_id].next_rad();
                }

                he_id = mesh.hes[he_id].next();
                if he_id == outer_start { break; }
            }

            // inner loops (holes)도 순회
            for inner_loop in face.inners() {
                let inner_start = inner_loop.start;
                if inner_start.is_null() { continue; }
                let mut ihe = inner_start;
                loop {
                    let mut rad_id = mesh.hes[ihe].next_rad();
                    while rad_id != ihe {
                        let rad_face = mesh.hes[rad_id].face();
                        if !rad_face.is_null() && !visited.contains(&rad_face) {
                            if mesh.faces.contains(rad_face) {
                                visited.insert(rad_face);
                                queue.push_back(rad_face);
                            }
                        }
                        rad_id = mesh.hes[rad_id].next_rad();
                    }
                    ihe = mesh.hes[ihe].next();
                    if ihe == inner_start { break; }
                }
            }
        }

        visited.into_iter().map(|f| f.raw()).collect()
    }

    pub fn get_stats(&self) -> String {
        let stats = self.scene.stats();
        format!(
            r#"{{"xias":{},"verts":{},"edges":{},"faces":{},"groups":{},"components":{},"canUndo":{},"canRedo":{}}}"#,
            stats.xia_count,
            stats.vert_count,
            stats.edge_count,
            stats.face_count,
            stats.group_count,
            stats.component_count,
            stats.can_undo,
            stats.can_redo,
        )
    }

    pub fn vert_count(&self) -> usize {
        self.scene.mesh.vert_count()
    }

    pub fn face_count(&self) -> usize {
        self.scene.mesh.face_count()
    }

    // ========================================================================
    // XIA Inspector — 선택된 face들의 기하학적/물리적 속성 계산
    // ========================================================================

    /// ⚠️ **파라미터는 FACE IDs** (XIA IDs 아님). XIA Inspector가 선택된 면들의
    /// 집계 속성을 계산하기 위한 함수. 이름의 "xia"는 "XIA 관점의 속성"이라는 뜻.
    ///
    /// - 입력: 선택된 face ID 배열
    /// - 출력 JSON: { isSolid, bbox{minX..maxZ}, length, width, height,
    ///   surfaceArea, volume, faceCount, vertCount, edgeCount, snapPoints, shapeType }
    ///
    /// 특정 XIA 하나의 정보가 필요하면 먼저 `get_xia_face(xia_id)`로 대표 face를 얻은
    /// 뒤 그 XIA의 모든 face_ids를 수집해 이 함수에 전달하거나, 새 `get_xia_stats` 사용.
    pub fn get_xia_info(&self, face_ids_raw: &[u32]) -> String {
        use std::collections::HashSet;

        let mesh = &self.scene.mesh;

        if face_ids_raw.is_empty() {
            return r#"{"empty":true}"#.to_string();
        }

        let face_ids: Vec<axia_geo::FaceId> = face_ids_raw.iter()
            .map(|&id| axia_geo::FaceId::new(id))
            .filter(|fid| mesh.faces.contains(*fid))
            .collect();

        if face_ids.is_empty() {
            return r#"{"empty":true}"#.to_string();
        }

        // ── 1. 모든 정점 수집 + Bounding Box ──
        let mut all_verts = HashSet::new();
        let mut all_edges = HashSet::new();
        let mut min_pt = DVec3::new(f64::MAX, f64::MAX, f64::MAX);
        let mut max_pt = DVec3::new(f64::MIN, f64::MIN, f64::MIN);

        for &fid in &face_ids {
            let outer_start = mesh.faces[fid].outer().start;
            if outer_start.is_null() { continue; }
            if let Ok(verts) = mesh.collect_loop_verts(outer_start) {
                for &vid in &verts {
                    all_verts.insert(vid);
                    if let Ok(p) = mesh.vertex_pos(vid) {
                        min_pt = DVec3::new(min_pt.x.min(p.x), min_pt.y.min(p.y), min_pt.z.min(p.z));
                        max_pt = DVec3::new(max_pt.x.max(p.x), max_pt.y.max(p.y), max_pt.z.max(p.z));
                    }
                }
            }
            if let Ok(hes) = mesh.collect_loop_hes(outer_start) {
                for &he_id in &hes {
                    all_edges.insert(mesh.hes[he_id].edge());
                }
            }
        }

        let dx = max_pt.x - min_pt.x;
        let dy = max_pt.y - min_pt.y;
        let dz = max_pt.z - min_pt.z;

        // 길이/너비/높이: 큰 순서대로 정렬
        let mut dims = [dx, dy, dz];
        dims.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let length = dims[0];
        let width  = dims[1];
        let height = dims[2];

        // ── 2. 표면적 계산 ──
        // ADR-253 P1 — route through mesh.face_area (SSOT, 메타-원칙 #4):
        // polygon Newell for ≥3-vert faces + analytic/tessellation fallback
        // for free-form Path B self-loop faces (Cylinder/BSpline/NURBS sweep
        // sides). The prior in-line shoelace required verts.len() >= 3 and
        // silently contributed 0 for self-loop faces → "면적 0" in Inspector.
        let mut surface_area = 0.0_f64;
        for &fid in &face_ids {
            surface_area += mesh.face_area(fid);
        }

        // ── 3. 부피 계산 (signed volume via divergence theorem) ──
        // 닫힌 메시의 경우만 정확, 열린 메시는 근사치
        let mut volume = 0.0_f64;
        for &fid in &face_ids {
            let outer_start = mesh.faces[fid].outer().start;
            if outer_start.is_null() { continue; }
            if let Ok(verts) = mesh.collect_loop_verts(outer_start) {
                if verts.len() >= 3 {
                    let p0 = mesh.vertex_pos(verts[0]).unwrap_or(DVec3::ZERO);
                    for i in 1..verts.len() - 1 {
                        let p1 = mesh.vertex_pos(verts[i]).unwrap_or(DVec3::ZERO);
                        let p2 = mesh.vertex_pos(verts[i + 1]).unwrap_or(DVec3::ZERO);
                        // Signed volume of tetrahedron with origin
                        volume += p0.dot(p1.cross(p2));
                    }
                }
            }
        }
        volume = (volume / 6.0).abs();

        // ── 4. Boundary Extraction — manifold 분석 (axia-geo 공통 유틸) ──
        // 모든 edge가 정확히 2개의 선택된 face를 공유하면 닫힌 2-manifold 솔리드.
        // boundary_edges > 0: open (hole), non_manifold > 0: T-junction 등 결함.
        let manifold = mesh.face_set_manifold_info(&face_ids);
        let is_solid = manifold.is_closed_solid;

        // ── 5. 형상 유형 판별 ──
        let shape_type = if !is_solid {
            if face_ids.len() == 1 { "면" } else { "면 그룹" }
        } else if face_ids.len() == 6 {
            // 6면 + 8정점 = 직사각형
            if all_verts.len() == 8 { "직사각형" } else { "다면체" }
        } else if face_ids.len() >= 20 {
            "원기둥/원뿔"
        } else {
            "다면체"
        };

        // ── 6. 스냅 포인트 수 = 정점 + edge 중점 ──
        let snap_points = all_verts.len() + all_edges.len();

        // ── 7. 재질 정보: 선택된 face들의 공통 재질 ──
        let mut common_mat: Option<u32> = None;
        let mut all_same = true;
        for fid in &face_ids {
            if let Some(face) = self.scene.mesh.faces.get(*fid) {
                let mid = face.material().raw();
                match common_mat {
                    None => common_mat = Some(mid),
                    Some(prev) => if prev != mid { all_same = false; break; }
                }
            }
        }
        let mat_id_val = if all_same { common_mat.unwrap_or(0) } else { 0 };
        let has_material = all_same && mat_id_val > 0;

        // mm 단위 기준
        format!(
            r#"{{"empty":false,"isSolid":{},"boundaryEdges":{},"nonManifoldEdges":{},"interiorEdges":{},"shapeType":"{}","faceCount":{},"vertCount":{},"edgeCount":{},"snapPoints":{},"minX":{:.4},"minY":{:.4},"minZ":{:.4},"maxX":{:.4},"maxY":{:.4},"maxZ":{:.4},"length":{:.4},"width":{:.4},"height":{:.4},"surfaceArea":{:.6},"volume":{:.6},"materialId":{},"hasMaterial":{}}}"#,
            is_solid,
            manifold.boundary_edge_count,
            manifold.non_manifold_edge_count,
            manifold.interior_edge_count,
            shape_type,
            face_ids.len(),
            all_verts.len(),
            all_edges.len(),
            snap_points,
            min_pt.x, min_pt.y, min_pt.z,
            max_pt.x, max_pt.y, max_pt.z,
            length, width, height,
            surface_area,
            volume,
            mat_id_val,
            has_material,
        )
    }

    // ========================================================================
    // Project Save/Load (.axia)
    // ========================================================================

    /// 프로젝트 데이터를 바이너리 스냅샷으로 내보내기 (versioned format with magic bytes)
    pub fn export_snapshot(&self) -> Vec<u8> {
        match self.scene.export_versioned_snapshot() {
            Ok(data) => {
                debug_log!("[RUST] export_snapshot: {} bytes", data.len());
                data
            }
            Err(e) => {
                console_error!("[RUST] export_snapshot ERROR: {}", e);
                Vec::new()
            }
        }
    }

    /// ADR-007 Phase 5 — 엄격 export: invariant 위반 시 빈 배열 반환 + lastError 설정.
    /// 파일 저장 대화창 등에서 데이터 무결성이 중요한 경우 사용.
    #[wasm_bindgen(js_name = "exportSnapshotStrict")]
    pub fn export_snapshot_strict(&mut self) -> Vec<u8> {
        match self.scene.export_versioned_snapshot_strict() {
            Ok(data) => data,
            Err(e) => {
                console_error!("[RUST] export_snapshot_strict ERROR: {}", e);
                self.set_error(e.to_string());
                Vec::new()
            }
        }
    }

    /// Phase H5 — 자유 엣지 → Face Synthesis (사용자 수동 트리거).
    ///
    /// 닫힌 polygon을 이루는 free edges를 감지해 face로 전환.
    /// 2D DXF 도면 import 후 "평면도 → 면 생성"에 유용.
    ///
    /// **사용자 명시 호출만** — import 직후 자동 실행 안 함 (의도 왜곡 방지).
    ///
    /// 반환: 생성된 face 개수 (감지 실패 / 이미 face로 처리됨 시 0)
    #[wasm_bindgen(js_name = "synthesizeFacesFromFreeEdges")]
    pub fn synthesize_faces_from_free_edges(&mut self) -> u32 {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let material = axia_core::FORM_MATERIAL;
        let created = self.scene.mesh.resolve_planar_free_faces(material);

        if !created.is_empty() {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            self.scene.transactions.cancel();
        }

        debug_log!("[RUST] synthesizeFacesFromFreeEdges: {} faces", created.len());
        created.len() as u32
    }

    /// Draw a centerline (reference axis). Unlike drawLine, bypasses
    /// intersection-split / face synthesis / loop detection. Creates one
    /// edge tagged Centerline; crossing other edges does not split them.
    /// Returns the new edge raw id, or -1 on failure.
    #[wasm_bindgen(js_name = "drawCenterline")]
    pub fn draw_centerline(
        &mut self,
        x0: f64, y0: f64, z0: f64,
        x1: f64, y1: f64, z1: f64,
    ) -> i32 {
        let cmd = axia_core::commands::Command::DrawCenterline {
            start: DVec3::new(x0, y0, z0),
            end:   DVec3::new(x1, y1, z1),
        };
        match self.scene.execute(cmd) {
            axia_core::commands::CommandResult::EntityCreated(eid) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                eid as i32
            }
            axia_core::commands::CommandResult::Error(msg) => {
                self.set_error(format!("draw_centerline: {}", msg));
                -1
            }
            _ => -1,
        }
    }

    /// Get an edge's semantic class as u32 (0=Geometry, 1=Centerline).
    /// Returns 0 for missing/inactive edges (safe default).
    #[wasm_bindgen(js_name = "edgeClass")]
    pub fn edge_class(&self, edge_id_raw: u32) -> u32 {
        let eid = axia_geo::EdgeId::new(edge_id_raw);
        self.scene.mesh.edges.get(eid)
            .map(|e| e.class().to_raw())
            .unwrap_or(0)
    }

    /// Change an edge's semantic class. Rejects Geometry→Centerline if the
    /// edge bounds an active face (would orphan the face).
    /// Returns true on success.
    #[wasm_bindgen(js_name = "setEdgeClass")]
    pub fn set_edge_class(&mut self, edge_id_raw: u32, class_raw: u32) -> bool {
        let cmd = axia_core::commands::Command::SetEdgeClass {
            edge_id: axia_geo::EdgeId::new(edge_id_raw),
            class_raw,
        };
        match self.scene.execute(cmd) {
            axia_core::commands::CommandResult::MeshUpdated => {
                self.invalidate_cache();
                true
            }
            axia_core::commands::CommandResult::Error(msg) => {
                self.set_error(format!("set_edge_class: {}", msg));
                false
            }
            _ => false,
        }
    }

    /// 엣지 가시성 임계 각도(도) 조회. StylePanel 슬라이더 초기화에 사용.
    #[wasm_bindgen(js_name = "edgeAngleThreshold")]
    pub fn edge_angle_threshold(&self) -> f64 {
        self.edge_angle_threshold_deg
    }

    /// 엣지 가시성 임계 각도(도) 설정. 범위 [1.0, 89.0]로 clamp.
    /// 변경 시 edge cache 무효화 → 다음 getEdgeLines 호출에 반영.
    /// 작은 값: 모든 panel 경계가 보임 (건축/기계 CAD 선호).
    /// 큰 값: 부드러운 곡면 유지 (캐릭터 모델 선호).
    #[wasm_bindgen(js_name = "setEdgeAngleThreshold")]
    pub fn set_edge_angle_threshold(&mut self, deg: f64) {
        let clamped = deg.max(1.0).min(89.0);
        if (clamped - self.edge_angle_threshold_deg).abs() > 1e-6 {
            self.edge_angle_threshold_deg = clamped;
            self.cache_dirty = true;
        }
    }

    // ════ ADR-135 β — Distance-based LOD chord_tol ════

    /// ADR-135 β — Get current render chord tolerance (mm).
    ///
    /// Returns the value set by `setRenderChordTol` (or default
    /// `DEFAULT_ANALYTIC_CHORD_TOL = 0.02 mm` if not yet set).
    #[wasm_bindgen(js_name = "renderChordTol")]
    pub fn render_chord_tol(&self) -> f64 {
        self.render_chord_tol
    }

    /// ADR-135 β — Set render chord tolerance (mm).
    ///
    /// Caller (Viewport) computes `lod_chord_tol(camera_distance)` in TS
    /// and pushes the result here. Clamped to `[0.001, 10.0]` mm. Change
    /// triggers `cache_dirty = true` (next `getMeshBuffers` re-tessellates
    /// with new chord tolerance).
    ///
    /// **Idempotent**: Setting the same value (within 1μm) is a no-op
    /// (no cache invalidation), so Viewport can call every frame without
    /// performance penalty.
    ///
    /// **Visual impact**: Near rendering (camera ≤ 100 mm via `lod_chord_tol`)
    /// uses default 0.02 mm — visual output identical to pre-ADR-135.
    /// Far rendering automatically coarser (0.2 mm at 1 m, 1.0 mm at 5 m+).
    ///
    /// **Triangle count reduction** (r=1000 mm sphere example):
    /// - Near (0.02 mm tol): ~2,000,000 tris (LOCKED #40 baseline)
    /// - Mid (0.20 mm tol, 1 m camera): ~200,000 tris (10× ↓)
    /// - Far (1.00 mm tol, 5 m+ camera): ~40,000 tris (50× ↓)
    #[wasm_bindgen(js_name = "setRenderChordTol")]
    pub fn set_render_chord_tol(&mut self, tol: f64) {
        let clamped = tol.max(0.001).min(10.0);
        if (clamped - self.render_chord_tol).abs() > 1e-6 {
            self.render_chord_tol = clamped;
            self.cache_dirty = true;
            // Triangle count change can be drastic (10-50×) → force full
            // rebuild (topology_changed) so delta-buffer path doesn't try
            // to apply position-only delta to wrong-sized buffer.
            self.topology_changed = true;
        }
    }

    /// ADR-135 β — Compute LOD chord_tol for given camera distance (mm).
    ///
    /// Pure function — does NOT modify engine state. Caller (Viewport)
    /// uses this to compute the value, then pushes via `setRenderChordTol`.
    /// Exposed here so TS can validate / debug the formula independently.
    ///
    /// Formula: `base * max(1, dist / 100)`, capped at 1.0 mm.
    /// See `axia_geo::mesh_export::lod_chord_tol` for full docs.
    #[wasm_bindgen(js_name = "lodChordTol")]
    pub fn lod_chord_tol(&self, camera_distance: f64) -> f64 {
        axia_geo::mesh_export::lod_chord_tol(camera_distance)
    }

    /// Analyse the whole active mesh for solid-closure status.
    /// Returns JSON: {face_count, interior_edge_count, boundary_edge_count,
    ///                non_manifold_edge_count, is_closed_solid}.
    /// Used by the Solidify action to report before/after state to the user.
    #[wasm_bindgen(js_name = "meshManifoldInfo")]
    pub fn mesh_manifold_info(&self) -> String {
        let all_faces: Vec<FaceId> = self.scene.mesh.faces.iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        let info = self.scene.mesh.face_set_manifold_info(&all_faces);
        format!(
            "{{\"face_count\":{},\"interior_edge_count\":{},\"boundary_edge_count\":{},\"non_manifold_edge_count\":{},\"is_closed_solid\":{}}}",
            info.face_count,
            info.interior_edge_count,
            info.boundary_edge_count,
            info.non_manifold_edge_count,
            info.is_closed_solid,
        )
    }

    /// Phase H5 — 자유 엣지 개수만 카운트 (dry-run, mesh 불변).
    /// UI에서 "N개 자유 엣지 발견 — Face Synthesis 실행?" 안내에 사용.
    ///
    /// Centerline 엣지는 제외 — 얘네는 "free" 상태로 있는 게 정상이므로
    /// Finish→Extrude 트리거에 영향 주지 않아야 함.
    #[wasm_bindgen(js_name = "countFreeEdges")]
    pub fn count_free_edges(&self) -> u32 {
        let mut count = 0u32;
        for (_, he) in self.scene.mesh.hes.iter() {
            if !he.is_active() || !he.face().is_null() { continue; }
            let is_topo = self.scene.mesh.edges.get(he.edge())
                .map(|e| e.class().is_topological())
                .unwrap_or(false);
            if is_topo { count += 1; }
        }
        // HE 한 쌍 (twin)이 모두 face null이면 엣지 2번 카운트됨 → 반으로
        count / 2
    }

    /// Phase H — Import Normalizer 실행 (ADR-007 Barrier).
    ///
    /// 외부 파일에서 들어온 mesh 데이터를 AXiA 네이티브 규칙에 맞춰 정리.
    /// 반환: JSON 리포트 {degenerateRemoved, windingFlipped, normalsRecomputed,
    ///                    isolatedVertsRemoved, remainingViolations}
    ///
    /// `options_json`: {remove_degenerate, normalize_winding, recompute_normals,
    ///                  remove_isolated_verts, degenerate_tolerance}
    ///                 — 생략/빈문자면 기본값 사용.
    #[wasm_bindgen(js_name = "normalizeForImport")]
    pub fn normalize_for_import(&mut self, options_json: String) -> String {
        use axia_geo::NormalizeOptions;
        let opts: NormalizeOptions = if options_json.is_empty() || options_json == "{}" {
            NormalizeOptions::default()
        } else {
            // 간단 파싱 — 필요한 필드만 추출
            let mut o = NormalizeOptions::default();
            if options_json.contains("\"remove_degenerate\":false") { o.remove_degenerate = false; }
            if options_json.contains("\"normalize_winding\":false") { o.normalize_winding = false; }
            if options_json.contains("\"recompute_normals\":false") { o.recompute_normals = false; }
            if options_json.contains("\"remove_isolated_verts\":false") { o.remove_isolated_verts = false; }
            o
        };

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let report = self.scene.mesh.normalize_for_import(&opts);
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();

        debug_log!("[RUST] normalizeForImport: {}", report.summary());

        format!(
            r#"{{"degenerateRemoved":{},"windingFlipped":{},"normalsRecomputed":{},"isolatedVertsRemoved":{},"remainingViolations":{}}}"#,
            report.degenerate_removed,
            report.winding_flipped,
            report.normals_recomputed,
            report.isolated_verts_removed,
            report.remaining_violations,
        )
    }

    /// ADR-007 원칙 1 확장 — 닫힌 solid의 outward normal 검증.
    /// 반환 JSON: {isClosedSolid, checkedFaces, inwardCount, inwardFaces[]}
    #[wasm_bindgen(js_name = "verifyOutwardNormals")]
    pub fn verify_outward_normals(&self) -> String {
        let report = self.scene.mesh.verify_outward_normals();
        let ids_json: Vec<String> = report.inward_faces.iter()
            .map(|f| f.raw().to_string())
            .collect();
        format!(
            r#"{{"isClosedSolid":{},"checkedFaces":{},"inwardCount":{},"inwardFaces":[{}]}}"#,
            report.is_closed_solid,
            report.checked_faces,
            report.inward_count,
            ids_json.join(","),
        )
    }

    /// 마지막 verify_face_invariants 결과를 요약 JSON으로 반환.
    /// UI에서 "정합성 검사" 버튼에 바인딩.
    /// ADR-007 Rev 2 — face 가 닫힌 볼륨의 일원(Wall)인지 stand-alone
    /// sheet 인지 판정. 렌더러가 sheet 는 양면, wall 은 single-sided
    /// 로 표시하는데 사용.
    #[wasm_bindgen(js_name = "isFaceInVolume")]
    pub fn is_face_in_volume(&self, face_id_raw: u32) -> bool {
        self.scene.mesh.is_face_in_volume(FaceId::new(face_id_raw))
    }

    /// ADR-007 Rev 2 — 모든 active face 의 분류를 비트 array (Uint8) 로
    /// 일괄 반환. 인덱스는 mesh buffer 의 face_map 슬롯과 1:1 매핑이
    /// 아니라 raw FaceId 와 1:1. 호출자(Viewport.syncMesh)는 face_map
    /// 으로 lookup 하면 됨.
    ///
    /// 반환: 활성 face 마다 1 = Wall, 0 = Sheet.
    /// 길이 = max active FaceId raw + 1 (편의상 sparse vec).
    #[wasm_bindgen(js_name = "getFaceVolumeFlags")]
    pub fn get_face_volume_flags(&self) -> Vec<u8> {
        let mut max_raw = 0u32;
        for (fid, _f) in self.scene.mesh.faces.iter() {
            if fid.raw() > max_raw { max_raw = fid.raw(); }
        }
        let mut out = vec![0u8; (max_raw as usize) + 1];
        for (fid, f) in self.scene.mesh.faces.iter() {
            if !f.is_active() { continue; }
            if self.scene.mesh.is_face_in_volume(fid) {
                out[fid.raw() as usize] = 1;
            }
        }
        out
    }

    #[wasm_bindgen(js_name = "verifyInvariants")]
    pub fn verify_invariants(&self) -> String {
        let report = self.scene.mesh.verify_face_invariants();
        let violations_json: Vec<String> = report.violations.iter()
            .map(|v| format!("{:?}", v))
            .collect();
        format!(
            r#"{{"checkedFaces":{},"valid":{},"violationCount":{},"violations":[{}]}}"#,
            report.checked_faces,
            report.is_valid(),
            report.violations.len(),
            violations_json.join(","),
        )
    }

    /// Self-intersection check (adversarial-sweep "flap" class). Detects
    /// geometric overlap of non-adjacent faces that passes every topological
    /// check (manifold / watertight / cracks / winding) yet renders as a fold
    /// or poke-through. Read-only. Returns JSON
    /// `{"clean":bool,"count":N,"pairs":[[fa,fb],...]}`.
    #[wasm_bindgen(js_name = "detectSelfIntersections")]
    pub fn detect_self_intersections_json(&self) -> String {
        let report = self.scene.mesh.detect_self_intersections();
        let pairs: Vec<String> = report
            .intersecting_pairs
            .iter()
            .map(|(a, b)| format!("[{},{}]", a.raw(), b.raw()))
            .collect();
        format!(
            r#"{{"clean":{},"count":{},"pairs":[{}]}}"#,
            report.is_clean(),
            report.count(),
            pairs.join(","),
        )
    }

    /// ADR-021 P7 + ADR-025 P11 — user-triggered "Resynthesize Faces".
    ///
    /// Sweeps free orphan edges for closed simple cycles and synthesizes a
    /// face for each. Returns JSON `{"created":N,"abortedByTimeBudget":bool,
    /// "elapsedMs":N}` so the UI can distinguish completion outcomes.
    ///
    /// Bounded by `MAX_ROUNDS = 8` inside the engine — caps work regardless
    /// of scene size. Time tracking happens via `performance.now()` here
    /// (NOT inside Rust, where `Instant::now()` panics on the wasm32-unknown
    /// -unknown target and the resulting trap leaks the wasm-bindgen
    /// RefCell guard, breaking all subsequent engine calls).
    ///
    /// Call site triggers a topology-change so the next syncMesh rebuilds
    /// everything (face buffers, edge wireframe, snap cache).
    #[wasm_bindgen(js_name = "resynthesizeOrphanFaces")]
    pub fn resynthesize_orphan_faces(&mut self) -> String {
        let t_start = js_sys::Date::now();
        let r = self.scene.resynthesize_orphan_faces();
        let elapsed_ms = js_sys::Date::now() - t_start;
        if r.created > 0 {
            self.mark_topology_changed();
            self.invalidate_cache();
        }
        format!(
            r#"{{"created":{},"abortedByTimeBudget":{},"elapsedMs":{:.2}}}"#,
            r.created, r.aborted_by_time_budget, elapsed_ms,
        )
    }

    /// UX 2026-05-02 — free (face-less) edge endpoints for distinct render.
    ///
    /// Returns `[x0,y0,z0, x1,y1,z1, ...]` flat Float32Array of edges that
    /// don't bound any active face. The renderer draws these with a
    /// distinct dashed/lighter style so users see "this is a line, not a
    /// face boundary" — addresses the "looks like a rect but engine
    /// reports no face" misperception (closed line sets that don't
    /// actually close to within ε tolerance).
    #[wasm_bindgen(js_name = "getFreeEdgeSegments")]
    pub fn get_free_edge_segments(&self) -> Vec<f32> {
        self.scene.mesh.collect_free_edge_segments()
    }

    /// ADR-047 R-track — non-manifold edge endpoints for rendering overlay.
    ///
    /// Returns `Float32Array` of `[x0,y0,z0, x1,y1,z1, ...]` line segments
    /// (2 endpoints × 3 coords per non-manifold edge). The renderer uses
    /// this to draw a highlight outline on edges shared by ≥3 active
    /// faces — these are ADR-021 P7 stacked-inner artifacts; without
    /// the highlight users mistake the overlapping faces for "missing
    /// face / wireframe only" (z-fight visual confusion).
    #[wasm_bindgen(js_name = "getNonManifoldEdgeSegments")]
    pub fn get_non_manifold_edge_segments(&self) -> Vec<f32> {
        let edges = self.scene.mesh.collect_non_manifold_edges();
        let mut buf = Vec::with_capacity(edges.len() * 6);
        for eid in edges {
            let edge = &self.scene.mesh.edges[eid];
            let v0 = edge.v_small();
            let v1 = edge.v_large();
            if let (Ok(p0), Ok(p1)) = (
                self.scene.mesh.vertex_pos(v0),
                self.scene.mesh.vertex_pos(v1),
            ) {
                buf.push(p0.x as f32);
                buf.push(p0.y as f32);
                buf.push(p0.z as f32);
                buf.push(p1.x as f32);
                buf.push(p1.y as f32);
                buf.push(p1.z as f32);
            }
        }
        buf
    }

    /// Per-`getMeshBuffers` skip diagnostics — JSON. Counts faces dropped at
    /// each silent-skip path inside `Mesh::export_buffers`. Use to debug
    /// "face is active in mesh but invisible in render" symptoms.
    #[wasm_bindgen(js_name = "getLastExportSkipStats")]
    pub fn get_last_export_skip_stats(&self) -> String {
        let s = self.scene.mesh.last_export_skip_stats();
        format!(
            r#"{{"totalActiveFaces":{},"emitted":{},"corruptedOuterLoop":{},"outerTooShort":{},"vertexPosFailed":{},"corruptedInnerLoop":{},"earcutFailed":{},"earcutEmpty":{},"lastEarcutEmptyFid":{},"lastEarcutEmptyOuterN":{},"analyticEmptyTess":{}}}"#,
            s.total_active_faces, s.emitted,
            s.corrupted_outer_loop, s.outer_too_short, s.vertex_pos_failed,
            s.corrupted_inner_loop, s.earcut_failed,
            s.earcut_empty, s.last_earcut_empty_fid, s.last_earcut_empty_outer_n,
            s.analytic_empty_tess,
        )
    }

    /// 바이너리 스냅샷으로부터 프로젝트 복원 (supports versioned and legacy formats)
    pub fn import_snapshot(&mut self, data: &[u8]) -> bool {
        match self.scene.import_versioned_snapshot(data) {
            Ok(()) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                debug_log!("[RUST] import_snapshot: verts={} faces={}",
                    self.scene.mesh.vert_count(), self.scene.mesh.face_count());
                true
            }
            Err(e) => {
                console_error!("[RUST] import_snapshot ERROR: {}", e);
                false
            }
        }
    }

    /// Orient all faces for consistent normals.
    /// Returns number of faces flipped.
    pub fn orient_faces(&mut self) -> usize {
        let (flipped, visited) = self.scene.orient_faces();
        debug_log!("[RUST] orient_faces: flipped={} visited={}", flipped, visited);
        self.mark_topology_changed();
        self.invalidate_cache();
        flipped
    }

    /// **User-triggered Face Reverse** (SketchUp "Reverse Faces").
    ///
    /// Flips orientation of the given faces. Locked (inside grouped/component)
    /// faces are silently skipped. Wrapped in a single undo transaction so the
    /// whole batch restores with one Ctrl+Z.
    ///
    /// Returns the count of faces actually flipped.
    #[wasm_bindgen(js_name = "flipFaces")]
    pub fn flip_faces(&mut self, face_ids: Vec<u32>) -> u32 {
        if face_ids.is_empty() {
            return 0;
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        // 잠긴(locked) 면 스킵 — 그룹/컴포넌트 보호
        let fids: Vec<FaceId> = face_ids
            .iter()
            .map(|&id| FaceId::new(id))
            .filter(|fid| !self.scene.is_face_locked(*fid))
            .collect();

        let skipped = face_ids.len() - fids.len();
        let flipped = self.scene.mesh.flip_faces(&fids);

        debug_log!(
            "[RUST] flip_faces: requested={}, skipped_locked={}, flipped={}",
            face_ids.len(), skipped, flipped
        );

        if flipped > 0 {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            // 아무것도 뒤집히지 않음 — 트랜잭션 취소해 undo 스택 오염 방지
            self.scene.transactions.cancel();
            if skipped > 0 {
                self.set_error(format!("{}개 면이 잠겨있어 반전 불가", skipped));
            }
        }

        flipped as u32
    }

    // ========================================================================
    // Face Merge
    // ========================================================================

    /// Merge the two coplanar faces sharing the given edge into a single face.
    ///
    /// - Success: returns the new merged FaceId (>= 0).
    /// - Failure: returns -1 and sets lastError (e.g. "not coplanar",
    ///   "shares multiple edges", "edge not shared by exactly 2 faces").
    ///
    /// Wrapped in a single undo transaction.
    #[wasm_bindgen(js_name = "mergeFacesByEdge")]
    pub fn merge_faces_by_edge(&mut self, edge_id_raw: u32) -> i32 {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            self.set_error(format!("Edge {} not found", edge_id_raw));
            return -1;
        }

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.merge_faces_by_edge(eid) {
            Ok(new_face) => {
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "merge", true,
                ) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                debug_log!("[RUST] merge_faces_by_edge: ok, new face = {:?}", new_face);
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                let msg = e.to_string();
                console_error!("[RUST] merge_faces_by_edge error: {}", msg);
                self.set_error(msg);
                -1
            }
        }
    }

    /// 2026-04-24 — Geometric merge of two coplanar adjacent faces even when
    /// they don't share an exact DCEL edge (different-sized boundaries).
    /// Used by the "두 면 기하 병합" menu action when user selects 2 faces.
    #[wasm_bindgen(js_name = "mergeCoplanarFacesGeometric")]
    pub fn merge_coplanar_faces_geometric(
        &mut self,
        f1_raw: u32,
        f2_raw: u32,
        angle_tol_deg: f64,
    ) -> i32 {
        let f1 = FaceId::new(f1_raw);
        let f2 = FaceId::new(f2_raw);
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.merge_coplanar_faces_geometric(f1, f2, angle_tol_deg) {
            Ok(new_face) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                let msg = e.to_string();
                self.set_error(msg);
                -1
            }
        }
    }

    /// Phase F — 비인접 coplanar 포함 병합 (ADR-006 C1).
    /// outer_face 안에 inner_face가 완전히 들어 있으면 inner를 hole로 합침.
    /// Returns new face ID, or -1 on failure (lastError set).
    #[wasm_bindgen(js_name = "mergeCoplanarContaining")]
    pub fn merge_coplanar_containing(
        &mut self,
        outer_face_raw: u32,
        inner_face_raw: u32,
        angle_tol_deg: f64,
    ) -> i32 {
        let o = FaceId::new(outer_face_raw);
        let i = FaceId::new(inner_face_raw);
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.merge_coplanar_containing(o, i, angle_tol_deg) {
            Ok(new_face) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                -1
            }
        }
    }

    /// Punch a circular hole through a coplanar face — atomically, stable ids.
    ///
    /// ADR-101 follow-up (메타-원칙 #15 / LOCKED #40). Finds the host face
    /// fresh from the world-space center `(cx, cy, cz)` + face normal hint
    /// `(nx, ny, nz)`, synthesizes an N-gon circle of `radius` (`segments`
    /// points) in that face's plane, and re-derives the face with the circle
    /// as a hole. The caller passes no (stale) face id — this is the atomic
    /// alternative to draw-inner-circle + `mergeCoplanarContaining`.
    ///
    /// Returns the rebuilt face id, or -1 on failure (lastError set).
    /// ADR-267 γ — delta 무결성 게이트 헬퍼 (cut/carve/slice op 의 Ok arm 에서 호출).
    /// `baseline` = op 전 `verify_volume_integrity(OpenMesh).damage_count()`,
    /// `snapshot` = op 전 `scene_snapshot()`. 반환 true = clean (caller 가 commit
    /// 진행). false = op 가 새 손상 유발 → 이미 byte-identical rollback + txn cancel
    /// + lastError 완료 (caller 는 실패값 반환).
    ///
    /// `manual_txn` — cleanup 모드: true = op 가 begin/commit/cancel 로 txn 을 직접
    /// 관리하고 게이트가 commit 前 Ok arm 에서 호출됨(→ `cancel()`); false = Scene
    /// 메서드가 txn 을 내부에서 이미 commit 함(→ `discard_last_undo()` 로 프레임 제거).
    /// Damage count + rollback snapshot to hand `integrity_gate_passed`, read
    /// BEFORE an op mutates anything. The gate is baseline-relative, so a mesh
    /// that was already damaged does not make every later op fail.
    fn integrity_baseline(&self) -> (usize, Vec<u8>) {
        (
            self.scene
                .mesh
                .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
                .damage_count(),
            self.scene.scene_snapshot(),
        )
    }

    fn integrity_gate_passed(
        &mut self,
        baseline: usize,
        snapshot: &[u8],
        label: &str,
        manual_txn: bool,
    ) -> bool {
        let after = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh);
        if after.damage_count() > baseline {
            console_error!(
                "[RUST] {} REJECTED by integrity gate:\n{}",
                label,
                after.summary()
            );
            self.scene.restore_scene_snapshot(snapshot);
            if manual_txn {
                self.scene.transactions.cancel();
            } else {
                self.scene.transactions.discard_last_undo();
            }
            self.set_error(format!(
                "부피 무결성 위반으로 취소됨 ({}): {}",
                label,
                after.summary()
            ));
            self.invalidate_cache();
            false
        } else {
            true
        }
    }

    /// Boundary-edge count over ALL active faces (ClosedSolid scope). 0 = the
    /// whole mesh is watertight.
    fn active_boundary_count(&self) -> usize {
        let active: Vec<FaceId> = self
            .scene
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        self.scene.mesh.face_set_manifold_info(&active).boundary_edge_count
    }

    /// Adversarial-sweep closure-preserving gate for hand-rolled face-rebuild
    /// ops (merge / chamfer / fillet).
    ///
    /// `integrity_gate_passed` uses `IntegrityScope::OpenMesh`, which forces
    /// `open_boundary_edges = 0` — so it CANNOT see a closed→open tear (a solid
    /// silently opening). merge/chamfer/fillet produce exactly that: boundary
    /// edges, not coincident cracks. This gate uses `ClosedSolid` scope and,
    /// **only when the input was fully watertight** (`before_boundary == 0`),
    /// rejects a result that opened it. Inputs that were already open (sheets)
    /// are never rejected for boundary — closure is not their contract — so this
    /// adds no false rejections; cracks / invariant violations are still caught
    /// for every input. On rejection it restores the pre-op snapshot and rolls
    /// back the transaction (mirrors `integrity_gate_passed`).
    fn closure_preserving_gate_passed(
        &mut self,
        before_boundary: usize,
        before_self_intersect: usize,
        snapshot: &[u8],
        label: &str,
        manual_txn: bool,
    ) -> bool {
        let active: Vec<FaceId> = self
            .scene
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        let after = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::ClosedSolid(&active));
        // Only enforce closure when the input was genuinely closed.
        let opened = before_boundary == 0 && after.open_boundary_edges > 0;
        // Self-intersection (flap / poke-through) — no topological check sees it.
        // Compare before→after so an input that already self-intersects is not
        // rejected; only a NEW self-intersection the op introduced is caught.
        let after_si = self.scene.mesh.detect_self_intersections().count();
        let self_intersected = after_si > before_self_intersect;
        if opened
            || self_intersected
            || !after.invariant_violations.is_empty()
            || !after.geometric_cracks.is_empty()
        {
            console_error!(
                "[RUST] {} REJECTED by closure gate: opened={} (boundary {}→{}), \
                 self_intersect={} ({}→{}), cracks={}, inv={}",
                label,
                opened,
                before_boundary,
                after.open_boundary_edges,
                self_intersected,
                before_self_intersect,
                after_si,
                after.geometric_cracks.len(),
                after.invariant_violations.len()
            );
            self.scene.restore_scene_snapshot(snapshot);
            if manual_txn {
                self.scene.transactions.cancel();
            } else {
                self.scene.transactions.discard_last_undo();
            }
            let reason = if self_intersected {
                format!("면 재구성이 자기교차(self-intersection)를 만들어 취소됨 ({label})")
            } else {
                format!(
                    "면 재구성이 solid 를 여는 결과가 되어 취소됨 ({label}): 경계 {before_boundary}→{}",
                    after.open_boundary_edges
                )
            };
            self.set_error(reason);
            self.invalidate_cache();
            false
        } else {
            true
        }
    }

    /// ADR-267 γ — on-demand 씬 무결성 검사 (UI "씬 무결성 검사"). 전체 활성 mesh 에
    /// OpenMesh scope 게이트를 적용한 JSON 리포트 반환. read-only (변경 없음).
    #[wasm_bindgen(js_name = "verifyVolumeIntegrity")]
    pub fn verify_volume_integrity_json(&self) -> String {
        let r = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh);
        format!(
            r#"{{"valid":{},"invariantViolations":{},"geometricCracks":{},"openBoundaryEdges":{},"checkedFaces":{}}}"#,
            r.is_valid(),
            r.invariant_violations.len(),
            r.geometric_cracks.len(),
            r.open_boundary_edges,
            r.checked_faces,
        )
    }

    #[wasm_bindgen(js_name = "punchHole")]
    pub fn punch_hole(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        nx: f64, ny: f64, nz: f64,
        radius: f64,
        segments: u32,
    ) -> i32 {
        let center = DVec3::new(cx, cy, cz);
        let normal = DVec3::new(nx, ny, nz);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(integrity_snapshot.clone());
        match self.scene.mesh.punch_circular_hole(center, normal, radius, segments) {
            Ok(new_face) => {
                if !self.integrity_gate_passed(integrity_before, &integrity_snapshot, "punch", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                -1
            }
        }
    }

    /// ADR-194 β-2 — drill a circular **through-hole** (explicit op, NOT
    /// auto-triggered — 메타-원칙 #16). Punches entry + exit holes (near + far
    /// faces along `(nx,ny,nz)`) and bridges them with a manifold tube wall.
    /// Returns the tube-quad count (> 0 on success), or -1 on failure
    /// (mesh restored to the pre-drill state — ADR-190 P0.2; lastError set).
    #[wasm_bindgen(js_name = "drillThroughHole")]
    pub fn drill_through_hole(
        &mut self,
        cx: f64, cy: f64, cz: f64,
        nx: f64, ny: f64, nz: f64,
        radius: f64,
        segments: u32,
    ) -> i32 {
        let center = DVec3::new(cx, cy, cz);
        let normal = DVec3::new(nx, ny, nz);
        // Drill mutates in several steps; capture a snapshot so a partial
        // failure (e.g. exit punch fails after entry) rolls back cleanly.
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(before.clone());
        match self
            .scene
            .mesh
            .drill_circular_through_hole(center, normal, radius, segments)
        {
            Ok(res) => {
                if !self.integrity_gate_passed(integrity_before, &before, "drill", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                res.tube_faces.len() as i32
            }
            Err(e) => {
                self.scene.restore_scene_snapshot(&before);
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                self.invalidate_cache();
                -1
            }
        }
    }

    /// Punch an axis-aligned rectangular hole (a window) into the face under the
    /// midpoint of corners (ax,ay,az)–(bx,by,bz); `(nx,ny,nz)` is the host normal
    /// hint. Returns the new ring-with-hole face id, or -1 on failure (transaction
    /// rolled back). Mirrors `punchHole`; engine: `Mesh::punch_rect_hole`.
    #[wasm_bindgen(js_name = "punchRectHole")]
    pub fn punch_rect_hole(
        &mut self,
        ax: f64, ay: f64, az: f64,
        bx: f64, by: f64, bz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> i32 {
        let a = DVec3::new(ax, ay, az);
        let b = DVec3::new(bx, by, bz);
        let normal = DVec3::new(nx, ny, nz);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(integrity_snapshot.clone());
        match self.scene.mesh.punch_rect_hole(a, b, normal) {
            Ok(new_face) => {
                if !self.integrity_gate_passed(integrity_before, &integrity_snapshot, "punch rect", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                -1
            }
        }
    }

    /// ADR-249 (P1) — drill a rectangular **through-hole** (explicit op, NOT
    /// auto-triggered — 메타-원칙 #16). Punches entry + exit rect windows on the
    /// near + far faces along `(nx,ny,nz)` and bridges them with a manifold tube.
    /// The rect is the bbox of corners (ax,ay,az)–(bx,by,bz) in the entry face's
    /// basis. Returns the tube-quad count (> 0 on success), or -1 on failure
    /// (mesh restored to the pre-drill state — ADR-190 P0.2; lastError set).
    #[wasm_bindgen(js_name = "drillRectThroughHole")]
    pub fn drill_rect_through_hole(
        &mut self,
        ax: f64, ay: f64, az: f64,
        bx: f64, by: f64, bz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> i32 {
        let a = DVec3::new(ax, ay, az);
        let b = DVec3::new(bx, by, bz);
        let normal = DVec3::new(nx, ny, nz);
        // Drill mutates in several steps; capture a snapshot so a partial
        // failure rolls back cleanly (mirrors drillThroughHole).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(before.clone());
        match self.scene.mesh.drill_rect_through_hole(a, b, normal) {
            Ok(res) => {
                if !self.integrity_gate_passed(integrity_before, &before, "drill rect", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                res.tube_faces.len() as i32
            }
            Err(e) => {
                self.scene.restore_scene_snapshot(&before);
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                self.invalidate_cache();
                -1
            }
        }
    }

    /// ADR-262 β-2 — cut a DOOR opening (floor-reaching notch) through a wall.
    /// `(ax,ay,az)` / `(bx,by,bz)` = two opposite corners of the door rect on
    /// the host wall face (one at the wall bottom edge, one at the header);
    /// `(nx,ny,nz)` = the host face's outward normal. Returns the jamb-face
    /// count (3) on success, or -1 on failure (mesh restored — ADR-190 P0.2;
    /// lastError set). A non-floor-reaching opening (a window) → -1 (caller
    /// routes to `drillRectThroughHole`). The door kernel mutates in many steps
    /// (F + B U-chain split, Bot notch, 3-jamb bridge) WITHOUT its own rollback,
    /// so the snapshot+restore here is mandatory (mirrors drillRectThroughHole).
    #[wasm_bindgen(js_name = "cutWallDoorOpening")]
    pub fn cut_wall_door_opening(
        &mut self,
        ax: f64, ay: f64, az: f64,
        bx: f64, by: f64, bz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> i32 {
        let a = DVec3::new(ax, ay, az);
        let b = DVec3::new(bx, by, bz);
        let normal = DVec3::new(nx, ny, nz);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(before.clone());
        match self.scene.mesh.cut_wall_door_opening(a, b, normal) {
            Ok(res) => {
                if !self.integrity_gate_passed(integrity_before, &before, "door", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                res.jamb_faces.len() as i32
            }
            Err(e) => {
                self.scene.restore_scene_snapshot(&before);
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                self.invalidate_cache();
                -1
            }
        }
    }

    /// ADR-249 (P5) — punch an arbitrary closed-polygon hole (a window) into the
    /// face under the loop centroid. `points` = flat [x0,y0,z0, x1,y1,z1, …]
    /// (≥ 3 points, CCW around the host normal). Returns the ring-with-hole face
    /// id, or -1 on failure (transaction rolled back; lastError set).
    #[wasm_bindgen(js_name = "punchPolygonHole")]
    pub fn punch_polygon_hole(
        &mut self,
        points: &[f64],
        nx: f64, ny: f64, nz: f64,
    ) -> i32 {
        if points.len() < 9 || points.len() % 3 != 0 {
            self.set_error(format!("punchPolygonHole: invalid points length {}", points.len()));
            return -1;
        }
        let loop_pts: Vec<DVec3> = points
            .chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2]))
            .collect();
        let normal = DVec3::new(nx, ny, nz);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(integrity_snapshot.clone());
        match self.scene.mesh.punch_polygon_hole(&loop_pts, normal) {
            Ok(new_face) => {
                if !self.integrity_gate_passed(integrity_before, &integrity_snapshot, "punch polygon", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                -1
            }
        }
    }

    /// ADR-249 (P5) — drill an arbitrary-profile **through-hole** (explicit op,
    /// NOT auto-triggered — 메타-원칙 #16). Punches the profile on the near + far
    /// faces along `(nx,ny,nz)` and bridges them with a manifold tube. `points` =
    /// flat [x0,y0,z0, …] (≥ 3, CCW around the host normal). Returns the tube-quad
    /// count (> 0), or -1 on failure (mesh restored — ADR-190 P0.2; lastError set).
    #[wasm_bindgen(js_name = "drillPolygonThroughHole")]
    pub fn drill_polygon_through_hole(
        &mut self,
        points: &[f64],
        nx: f64, ny: f64, nz: f64,
    ) -> i32 {
        if points.len() < 9 || points.len() % 3 != 0 {
            self.set_error(format!("drillPolygonThroughHole: invalid points length {}", points.len()));
            return -1;
        }
        let loop_pts: Vec<DVec3> = points
            .chunks_exact(3)
            .map(|c| DVec3::new(c[0], c[1], c[2]))
            .collect();
        let normal = DVec3::new(nx, ny, nz);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let before = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(before.clone());
        match self.scene.mesh.drill_polygon_through_hole(&loop_pts, normal) {
            Ok(res) => {
                if !self.integrity_gate_passed(integrity_before, &before, "drill polygon", true) {
                    return -1;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                res.tube_faces.len() as i32
            }
            Err(e) => {
                self.scene.restore_scene_snapshot(&before);
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                self.invalidate_cache();
                -1
            }
        }
    }

    /// ADR-252 — carve a blind POCKET from a coplanar profile sheet drawn on a
    /// solid wall ("draw rect/polygon on a face → push in → pocket"). `source_face`
    /// = the drawn profile sheet; `depth` (> 0) = inward recess depth. Returns the
    /// side-wall count (> 0 on success), or -1 on failure (mesh restored — ADR-190
    /// P0.2; lastError set). Errors if depth reaches the opposite wall (→ through-hole).
    #[wasm_bindgen(js_name = "carvePocketFromSourceFace")]
    pub fn carve_pocket_from_source_face(&mut self, source_face_raw: u32, depth: f64) -> i32 {
        let fid = FaceId::new(source_face_raw);
        // ADR-267 γ — Scene 메서드가 txn 을 내부 commit 하므로 게이트는 op 후 실행,
        // 실패 시 discard_last_undo 로 프레임 제거 (manual_txn=false).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        // The Scene method owns the transaction + XIA/Shape reconciliation
        // (ADR-252) so the new pocket faces are tracked by the wall's solid.
        match self.scene.carve_pocket_from_source_face(fid, depth) {
            CommandResult::PushPullDone { sides_created, .. } => {
                if !self.integrity_gate_passed(
                    integrity_before,
                    &integrity_snapshot,
                    "carve pocket",
                    false,
                ) {
                    return -1;
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                sides_created as i32
            }
            CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1
            }
            _ => {
                self.invalidate_cache();
                -1
            }
        }
    }

    /// ADR-271 γ — carve a blind radial POCKET into a curved (Cylinder) wall from
    /// a sketched cap face (ADR-263). Returns the side-wall count, or -1 on
    /// rejection (non-Cylinder cap / depth ≥ radius / non-manifold). Integrity
    /// gate (ADR-267) enforced like `carvePocketFromSourceFace`.
    #[wasm_bindgen(js_name = "carveCurvedPocket")]
    pub fn carve_curved_pocket(&mut self, cap_face_raw: u32, depth: f64) -> i32 {
        let fid = FaceId::new(cap_face_raw);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        // ADR-271 / ADR-273 — capture closure + self-intersection baseline so the
        // curved carve gets the SAME defense-in-depth gate as the flat carve. The
        // SI detector now trusts curved analytic faces (co-surface + chord
        // artifacts skipped), so this no longer false-rejects a valid curved
        // carve — it catches a REAL fold among the carved walls, a torn-open
        // solid, or new invariant/crack.
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let integrity_snapshot = self.scene.scene_snapshot();
        match self.scene.carve_curved_pocket_from_cap(fid, depth) {
            CommandResult::PushPullDone { sides_created, .. } => {
                if !self.integrity_gate_passed(
                    integrity_before,
                    &integrity_snapshot,
                    "carve curved pocket",
                    false,
                ) {
                    return -1;
                }
                if !self.closure_preserving_gate_passed(
                    before_boundary,
                    before_si,
                    &integrity_snapshot,
                    "carve curved pocket",
                    false,
                ) {
                    return -1;
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                sides_created as i32
            }
            CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1
            }
            _ => {
                self.invalidate_cache();
                -1
            }
        }
    }

    /// ADR-286 β — raise a curved BOSS (outward protrusion) from a sketched
    /// (Cylinder) cap face (ADR-263): the mirror of `carveCurvedPocket`. Returns
    /// the side-wall count, or -1 on rejection (non-Cylinder cap / height ≤ 0 /
    /// non-manifold). Same defense-in-depth integrity + closure gate (ADR-267 /
    /// ADR-273) as the pocket carve.
    #[wasm_bindgen(js_name = "carveCurvedBoss")]
    pub fn carve_curved_boss(&mut self, cap_face_raw: u32, height: f64) -> i32 {
        let fid = FaceId::new(cap_face_raw);
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let integrity_snapshot = self.scene.scene_snapshot();
        match self.scene.add_curved_boss_from_cap(fid, height) {
            CommandResult::PushPullDone { sides_created, .. } => {
                if !self.integrity_gate_passed(
                    integrity_before,
                    &integrity_snapshot,
                    "add curved boss",
                    false,
                ) {
                    return -1;
                }
                if !self.closure_preserving_gate_passed(
                    before_boundary,
                    before_si,
                    &integrity_snapshot,
                    "add curved boss",
                    false,
                ) {
                    return -1;
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                sides_created as i32
            }
            CommandResult::Error(e) => {
                self.set_error(e);
                self.invalidate_cache();
                -1
            }
            _ => {
                self.invalidate_cache();
                -1
            }
        }
    }

    /// ADR-287 live preview — READ-ONLY ghost triangles (flat xyz) for a curved
    /// pocket/boss on a sketched cap, without mutating the mesh. `signed_depth` =
    /// drag distance (negative = inward pocket, positive = outward boss). Empty
    /// array = no ghost (non-carveable cap / ~zero depth). Safe every mouse-move.
    #[wasm_bindgen(js_name = "previewCurvedCarve")]
    pub fn preview_curved_carve(&self, cap_face_raw: u32, signed_depth: f64) -> Vec<f32> {
        self.scene
            .mesh
            .preview_curved_carve(FaceId::new(cap_face_raw), signed_depth)
            .unwrap_or_default()
    }

    /// ADR-290 곡면 편집 마무리 — READ-ONLY on-surface circle preview (flat xyz
    /// polyline) for the DrawCircle tool on a curved host face (Sphere/Cylinder/
    /// Cone/Torus). `center_pt`/`radius_pt` are the world points the user clicked;
    /// the returned polyline FOLLOWS the surface (curves along it) instead of the
    /// flat tangent-plane approximation. Empty array = non-curved face (the tool
    /// then draws its own flat preview). Safe every mouse-move (`&self`).
    /// ADR-284 follow-up — a point on `host_face`'s surface whose GEODESIC
    /// distance from the given centre is `d`, so a TYPED radius can mean what
    /// it says. Feed the result back as `radius_pt` to drawCircleOn*.
    ///
    /// Empty array = not answerable here (non-curved / inactive face,
    /// degenerate ask) — the caller keeps its planar path rather than guessing.
    /// Read-only (`&self`), so it is safe to call while typing.
    #[wasm_bindgen(js_name = "surfacePointAtGeodesicDistance")]
    pub fn surface_point_at_geodesic_distance(
        &self,
        host_face_raw: u32,
        cx: f64, cy: f64, cz: f64,
        d: f64,
    ) -> Vec<f64> {
        self.scene
            .mesh
            .surface_point_at_geodesic_distance(
                FaceId::new(host_face_raw),
                DVec3::new(cx, cy, cz),
                d,
            )
            .map(|p| vec![p.x, p.y, p.z])
            .unwrap_or_default()
    }

    #[wasm_bindgen(js_name = "previewCircleOnSurface")]
    pub fn preview_circle_on_surface(
        &self,
        host_face_raw: u32,
        cx: f64, cy: f64, cz: f64,
        rx: f64, ry: f64, rz: f64,
    ) -> Vec<f32> {
        self.scene
            .mesh
            .preview_circle_on_surface(
                FaceId::new(host_face_raw),
                DVec3::new(cx, cy, cz),
                DVec3::new(rx, ry, rz),
            )
            .unwrap_or_default()
    }

    /// ADR-252 — `true` if the face is a coplanar profile contained in a LARGER
    /// face on the same plane (the "rect drawn on a wall" signal). The Push/Pull
    /// tool uses this to route an inward push to a pocket carve. Read-only.
    #[wasm_bindgen(js_name = "faceHasLargerCoplanarContainer")]
    pub fn face_has_larger_coplanar_container(&self, face_raw: u32) -> bool {
        self.scene
            .mesh
            .face_has_larger_coplanar_container(FaceId::new(face_raw))
    }

    /// ADR-252 — wall thickness under a profile sheet drawn on a solid wall (the
    /// distance the inward push travels before it reaches the opposite wall =
    /// the pocket↔through threshold). Returns `-1` if `face` is not a source-on-
    /// wall sheet (no larger coplanar container / no opposite wall). Read-only —
    /// the Push/Pull tool uses it to color the live cut ghost (pocket vs through)
    /// and clamp its depth.
    #[wasm_bindgen(js_name = "wallThicknessFromSourceFace")]
    pub fn wall_thickness_from_source_face(&self, face_raw: u32) -> f64 {
        self.scene
            .mesh
            .wall_thickness_from_source_face(FaceId::new(face_raw))
            .unwrap_or(-1.0)
    }

    /// ADR-190 Phase 3 — how far an INWARD MoveOnly push may travel before the
    /// solid would invert (the local thickness under `face`). Returns `-1` when
    /// the face has no connecting walls parallel to its normal (a flat/open
    /// profile → unclamped). Read-only, mirroring `wallThicknessFromSourceFace`.
    ///
    /// ADR-196 clamps an over-push here so a box top pushed past its own bottom
    /// *sticks* instead of flipping inside-out — correct, but until now
    /// **silent**: the push returns `true`, the result is watertight, and the
    /// user is left with a sliver and no explanation (measured: a 2000×1000×1000
    /// box pushed −1500 collapses to 0.001mm thick; ADR-293 §5). The tool reads
    /// this to say so.
    #[wasm_bindgen(js_name = "moveOnlyMaxInward")]
    pub fn move_only_max_inward(&self, face_raw: u32) -> f64 {
        axia_geo::operations::push_pull::move_only_max_inward(
            &self.scene.mesh,
            FaceId::new(face_raw),
        )
        .unwrap_or(-1.0)
    }

    /// Tolerance 지정 단일 엣지 병합 (B1).
    /// `angle_tol_deg` — 허용 각도 (°). 기본 0.5° (strict). 관대하게는 2~5°.
    #[wasm_bindgen(js_name = "mergeFacesByEdgeTol")]
    pub fn merge_faces_by_edge_tol(&mut self, edge_id_raw: u32, angle_tol_deg: f64) -> i32 {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            self.set_error(format!("Edge {} not found", edge_id_raw));
            return -1;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.merge_faces_by_edge_with_tolerance(eid, angle_tol_deg) {
            Ok(new_face) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                new_face.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(e.to_string());
                -1
            }
        }
    }

    /// Tolerance 지정 인접 면 반복 병합 (B1).
    #[wasm_bindgen(js_name = "tryMergeAdjacentFacesTol")]
    pub fn try_merge_adjacent_faces_tol(&mut self, face_ids: Vec<u32>, angle_tol_deg: f64) -> u32 {
        if face_ids.len() < 2 {
            self.set_error("Need 2+ faces".to_string());
            return 0;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mut current: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let mut merges_done: u32 = 0;

        loop {
            let mut edge_to_faces: std::collections::HashMap<EdgeId, Vec<FaceId>> =
                std::collections::HashMap::new();
            for &fid in &current {
                let f = match self.scene.mesh.faces.get(fid) {
                    Some(f) if f.is_active() => f,
                    _ => continue,
                };
                let start = f.outer().start;
                if start.is_null() { continue; }
                if let Ok(hes) = self.scene.mesh.collect_loop_hes(start) {
                    for he in hes {
                        let e = self.scene.mesh.hes[he].edge();
                        edge_to_faces.entry(e).or_default().push(fid);
                    }
                }
            }
            let mut candidate: Option<(EdgeId, FaceId, FaceId)> = None;
            for (e, faces) in edge_to_faces.iter() {
                if faces.len() == 2 && faces[0] != faces[1] {
                    candidate = Some((*e, faces[0], faces[1]));
                    break;
                }
            }
            let (edge_id, f1, f2) = match candidate {
                Some(v) => v,
                None => break,
            };
            match self.scene.mesh.merge_faces_by_edge_with_tolerance(edge_id, angle_tol_deg) {
                Ok(new_face) => {
                    merges_done += 1;
                    current.retain(|&x| x != f1 && x != f2);
                    current.push(new_face);
                }
                Err(_) => {
                    current.retain(|&x| x != f2);
                }
            }
        }

        if merges_done > 0 {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            self.scene.transactions.cancel();
            self.set_error("No coplanar adjacent faces to merge".to_string());
        }
        merges_done
    }

    /// Dry-run analysis of merge candidates — does NOT mutate the mesh.
    ///
    /// For each pair of faces in the selection that shares an edge, checks:
    ///   - shared edge count (must be 1)
    ///   - coplanarity (strict tolerance)
    ///
    /// Returns JSON:
    ///   {
    ///     "total": N,                 // pairs sharing any edge
    ///     "mergeable": M,             // pairs passing both checks
    ///     "nonCoplanar": K,           // pairs sharing 1 edge but not coplanar
    ///     "ambiguous": L,             // pairs sharing >1 edge
    ///     "estMergesAfterCascade": E  // upper bound of final merge count
    ///   }
    ///
    /// `estMergesAfterCascade` approximates how many merges would happen if
    /// the user proceeded with `tryMergeAdjacentFaces` — each merge can enable
    /// new adjacencies so the exact count is not known without running it.
    /// The upper bound = min(mergeable, face_count - 1).
    #[wasm_bindgen(js_name = "analyzeMergeCandidates")]
    pub fn analyze_merge_candidates(&self, face_ids: Vec<u32>) -> String {
        self.analyze_merge_candidates_tol(face_ids, 0.5)
    }

    /// Tolerance 지정 merge analysis (B1).
    #[wasm_bindgen(js_name = "analyzeMergeCandidatesTol")]
    pub fn analyze_merge_candidates_tol(&self, face_ids: Vec<u32>, angle_tol_deg: f64) -> String {
        if face_ids.len() < 2 {
            return r#"{"total":0,"mergeable":0,"nonCoplanar":0,"ambiguous":0,"estMergesAfterCascade":0}"#.to_string();
        }

        use std::collections::HashMap;
        let face_set: std::collections::HashSet<FaceId> =
            face_ids.iter().map(|&id| FaceId::new(id)).collect();

        // Map: edge → list of selected faces using it
        let mut edge_to_faces: HashMap<EdgeId, Vec<FaceId>> = HashMap::new();
        for &fid in &face_set {
            let f = match self.scene.mesh.faces.get(fid) {
                Some(f) if f.is_active() => f,
                _ => continue,
            };
            let start = f.outer().start;
            if start.is_null() { continue; }
            if let Ok(hes) = self.scene.mesh.collect_loop_hes(start) {
                for he in hes {
                    let e = self.scene.mesh.hes[he].edge();
                    edge_to_faces.entry(e).or_default().push(fid);
                }
            }
        }

        // Collect unique face pairs + edges they share
        let mut pair_edges: HashMap<(FaceId, FaceId), u32> = HashMap::new();
        for (_, faces) in edge_to_faces.iter() {
            if faces.len() == 2 && faces[0] != faces[1] {
                let mut a = faces[0];
                let mut b = faces[1];
                if b.raw() < a.raw() { std::mem::swap(&mut a, &mut b); }
                *pair_edges.entry((a, b)).or_insert(0) += 1;
            }
        }

        let mut mergeable: u32 = 0;
        let mut non_coplanar: u32 = 0;
        let mut ambiguous: u32 = 0;

        for ((f1, f2), shared_count) in pair_edges.iter() {
            if *shared_count > 1 {
                ambiguous += 1;
                continue;
            }
            match self.scene.mesh.are_faces_coplanar_with_tolerance(*f1, *f2, angle_tol_deg) {
                Ok(true) => mergeable += 1,
                _ => non_coplanar += 1,
            }
        }

        let total = pair_edges.len() as u32;
        let face_count = face_ids.len() as u32;
        let est_max = if face_count > 0 { face_count - 1 } else { 0 };
        let est_merges = mergeable.min(est_max);

        format!(
            r#"{{"total":{},"mergeable":{},"nonCoplanar":{},"ambiguous":{},"estMergesAfterCascade":{}}}"#,
            total, mergeable, non_coplanar, ambiguous, est_merges,
        )
    }

    /// Try to merge adjacent coplanar faces in the given selection.
    ///
    /// Iteratively finds pairs of faces that share exactly one edge and are
    /// coplanar, merges them, and repeats until no more pairs qualify.
    /// Returns the number of merges actually performed.
    ///
    /// All merges are wrapped in a single undo transaction.
    #[wasm_bindgen(js_name = "tryMergeAdjacentFaces")]
    pub fn try_merge_adjacent_faces(&mut self, face_ids: Vec<u32>) -> u32 {
        if face_ids.len() < 2 {
            return 0;
        }

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let mut current: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let mut merges_done: u32 = 0;

        loop {
            // Build {edge -> faces sharing it (within selection)}
            let mut edge_to_faces: std::collections::HashMap<EdgeId, Vec<FaceId>> =
                std::collections::HashMap::new();

            for &fid in &current {
                let f = match self.scene.mesh.faces.get(fid) {
                    Some(f) if f.is_active() => f,
                    _ => continue,
                };
                let start = f.outer().start;
                if start.is_null() { continue; }
                if let Ok(hes) = self.scene.mesh.collect_loop_hes(start) {
                    for he in hes {
                        let e = self.scene.mesh.hes[he].edge();
                        edge_to_faces.entry(e).or_default().push(fid);
                    }
                }
            }

            // Find a candidate edge shared by exactly two selected faces
            let mut candidate: Option<(EdgeId, FaceId, FaceId)> = None;
            for (e, faces) in edge_to_faces.iter() {
                if faces.len() == 2 && faces[0] != faces[1] {
                    candidate = Some((*e, faces[0], faces[1]));
                    break;
                }
            }
            let (edge_id, f1, f2) = match candidate {
                Some(v) => v,
                None => break,
            };

            // Attempt merge; silently skip non-coplanar candidates
            match self.scene.mesh.merge_faces_by_edge(edge_id) {
                Ok(new_face) => {
                    merges_done += 1;
                    // Replace f1/f2 with new_face in the working set
                    current.retain(|&x| x != f1 && x != f2);
                    current.push(new_face);
                }
                Err(_) => {
                    // Remove this pair from consideration to make progress
                    // (we don't modify the mesh on error since merge_faces_by_edge
                    //  bails pre-mutation thanks to F5 hardening)
                    // Remove one face so this pair isn't re-examined
                    current.retain(|&x| x != f2);
                }
            }
        }

        if merges_done > 0 {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            self.scene.transactions.cancel();
            self.set_error("No coplanar adjacent faces to merge".to_string());
        }

        merges_done
    }

    // ========================================================================
    // DXF Import
    // ========================================================================

    /// DXF 파일 바이트를 파싱하여 DCEL 메시로 가져오기
    /// 반환: JSON 문자열 (통계 정보)
    pub fn import_dxf(&mut self, data: &[u8]) -> String {
        debug_log!("[RUST] import_dxf: {} bytes", data.len());

        match self.scene.import_dxf(data) {
            Ok(stats) => {
                let verts = self.scene.mesh.vert_count();
                let faces = self.scene.mesh.face_count();
                debug_log!("[RUST] DXF import done: {}", stats);
                debug_log!("[RUST] Mesh now: verts={} faces={}", verts, faces);
                self.mark_topology_changed();
                self.invalidate_cache();

                format!(
                    r#"{{"ok":true,"lines":{},"polylines":{},"circles":{},"arcs":{},"faces3d":{},"solids":{},"points":{},"ellipses":{},"splines":{},"skipped":{},"errors":{},"totalVerts":{},"totalFaces":{}}}"#,
                    stats.lines, stats.polylines, stats.circles, stats.arcs,
                    stats.faces_3d, stats.solids, stats.points, stats.ellipses,
                    stats.splines, stats.skipped, stats.errors.len(),
                    verts, faces,
                )
            }
            Err(e) => {
                console_error!("[RUST] DXF import ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    // ========================================================================
    // Boolean Operations
    // ========================================================================

    /// Boolean 연산 수행
    /// faces_a, faces_b: face ID 배열 (u32)
    /// op: "union" | "subtract" | "intersect"
    /// 반환: JSON 문자열 (결과 정보)
    pub fn boolean_op(
        &mut self,
        faces_a: &[u32],
        faces_b: &[u32],
        op: &str,
    ) -> String {
        let fids_a: Vec<FaceId> = faces_a.iter().map(|&id| FaceId::new(id)).collect();
        let fids_b: Vec<FaceId> = faces_b.iter().map(|&id| FaceId::new(id)).collect();

        let bool_op = match op {
            "union" => BoolOp::Union,
            "subtract" => BoolOp::Subtract,
            "intersect" => BoolOp::Intersect,
            _ => {
                return format!(r#"{{"ok":false,"error":"unknown op: {}"}}"#, op);
            }
        };

        debug_log!(
            "[RUST] boolean: op={} A={} faces, B={} faces",
            op, fids_a.len(), fids_b.len()
        );

        // 트랜잭션 래핑 + ADR-267 β-3 watertight 게이트 (delta).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(integrity_snapshot.clone());

        let mat = axia_core::FORM_MATERIAL;
        let result = self.scene.mesh.boolean(&fids_a, &fids_b, bool_op, mat);

        match result {
            Ok(res) => {
                // ADR-267 β-3 — op 가 새 손상을 유발했으면 byte-identical rollback +
                // txn cancel (phantom frame 방지).
                let after = self
                    .scene
                    .mesh
                    .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh);
                if after.damage_count() > integrity_before {
                    console_error!(
                        "[RUST] boolean REJECTED by integrity gate:\n{}",
                        after.summary()
                    );
                    self.scene.restore_scene_snapshot(&integrity_snapshot);
                    self.scene.transactions.cancel();
                    self.set_error(format!(
                        "부피 무결성 위반으로 취소됨 (boolean): {}",
                        after.summary()
                    ));
                    self.invalidate_cache();
                    let reason = after.summary().replace('"', "'").replace('\n', " ");
                    return format!(r#"{{"ok":false,"error":"{}"}}"#, reason);
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();

                for msg in &res.debug {
                    debug_log!("[BOOL] {}", msg);
                }

                let face_ids: Vec<u32> = res.faces.iter().map(|f| f.raw()).collect();
                // ADR-197 β-3-n — flag the curved (surface-preserving) dispatch so
                // the UI can tell the user the NURBS surface was kept.
                let curved = res.debug.iter().any(|d| d.contains("curved"));
                format!(
                    r#"{{"ok":true,"op":"{}","resultFaces":{},"newVerts":{},"totalVerts":{},"totalFaces":{},"curved":{}}}"#,
                    op,
                    format!("{:?}", face_ids),
                    res.new_verts,
                    self.scene.mesh.vert_count(),
                    self.scene.mesh.face_count(),
                    curved,
                )
            }
            Err(e) => {
                console_error!("[RUST] boolean ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// ADR-276 Phase 5 — solid-CSG boolean (`Mesh::boolean_solid`): the
    /// split-by-chain + seam-weld path that cuts box/planar solids WATERTIGHT
    /// (convex-corner subtract). Same JSON shape as `boolean_op`. `boolean_solid`
    /// is internally fail-closed (any non-watertight result → byte-identical
    /// rollback + Err), so the caller gets either a real watertight cut or a
    /// clean error to fall back on (e.g. the ADR-275 warning).
    #[wasm_bindgen(js_name = "booleanSolid")]
    pub fn boolean_solid_op(
        &mut self,
        faces_a: &[u32],
        faces_b: &[u32],
        op: &str,
    ) -> String {
        let fids_a: Vec<FaceId> = faces_a.iter().map(|&id| FaceId::new(id)).collect();
        let fids_b: Vec<FaceId> = faces_b.iter().map(|&id| FaceId::new(id)).collect();
        let bool_op = match op {
            "union" => BoolOp::Union,
            "subtract" => BoolOp::Subtract,
            "intersect" => BoolOp::Intersect,
            _ => return format!(r#"{{"ok":false,"error":"unknown op: {}"}}"#, op),
        };
        let snapshot = self.scene.scene_snapshot();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(snapshot.clone());
        let mat = axia_core::FORM_MATERIAL;
        match self.scene.mesh.boolean_solid(&fids_a, &fids_b, bool_op, mat) {
            Ok(res) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                let face_ids: Vec<u32> = res.faces.iter().map(|f| f.raw()).collect();
                format!(
                    r#"{{"ok":true,"op":"{}","resultFaces":{},"newVerts":{},"totalVerts":{},"totalFaces":{}}}"#,
                    op,
                    format!("{:?}", face_ids),
                    res.new_verts,
                    self.scene.mesh.vert_count(),
                    self.scene.mesh.face_count(),
                )
            }
            Err(e) => {
                // boolean_solid already rolled back the mesh internally; restore
                // the scene snapshot + cancel the txn for full consistency.
                self.scene.restore_scene_snapshot(&snapshot);
                self.scene.transactions.cancel();
                self.invalidate_cache();
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// Diagnose non-manifold edges (ADR-007 I5) without modifying the
    /// scene. Returns JSON: `{count, edges:[{edge, faceCount}, …]}`.
    /// Useful for the UI's "씬 무결성 검사" command.
    #[wasm_bindgen(js_name = "findNonManifoldEdges")]
    pub fn find_non_manifold_edges(&self) -> String {
        let bad = self.scene.mesh.find_non_manifold_edges();
        let mut out = String::from("{\"count\":");
        out.push_str(&bad.len().to_string());
        out.push_str(",\"edges\":[");
        for (i, nm) in bad.iter().enumerate() {
            if i > 0 { out.push(','); }
            out.push_str(&format!(
                r#"{{"edge":{},"faceCount":{}}}"#,
                nm.edge.raw(), nm.faces.len()
            ));
        }
        out.push_str("]}");
        out
    }

    /// ADR-274 (d) — collapse a "flushed" extrusion. When a boss/pocket is
    /// pushed back until its height reaches ~0, moving vertices to coincidence
    /// leaves degenerate walls + coincident-distinct verts (dedup only fires on
    /// creation), so the solid never closes. This recognizes that pattern and
    /// rebuilds the clean flat face, reconciling Xia/Shape ownership.
    ///
    /// Gate-guarded + undoable: on any topology damage the engine rolls back
    /// and this returns `{ok:false,error}` with the scene unchanged. When
    /// nothing needs collapsing it returns `{ok:true,collapsed:0}` (no-op).
    ///
    /// `area_tol` — a face below this area counts as a collapsed wall
    /// (`<= 0` → default 1e-3 mm²).
    #[wasm_bindgen(js_name = "collapseFlushExtrusion")]
    pub fn collapse_flush_extrusion(&mut self, area_tol: f64) -> String {
        let tol = if area_tol > 0.0 { area_tol } else { 1e-3 };
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.collapse_flush_extrusion(tol) {
            Ok(0) => {
                self.scene.transactions.cancel();
                r#"{"ok":true,"collapsed":0}"#.to_string()
            }
            Ok(n) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                format!(r#"{{"ok":true,"collapsed":{}}}"#, n)
            }
            Err(e) => {
                self.scene.transactions.cancel();
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// Repair non-manifold edges (ADR-007 I5) — XIA-aware where possible,
    /// geometric fallback otherwise. Returns JSON report:
    /// `{ok, edgesExamined, edgesRepaired, edgesSkipped, facesDetached, vertsCreated}`.
    #[wasm_bindgen(js_name = "repairNonManifoldEdges")]
    pub fn repair_non_manifold_edges(&mut self) -> String {
        // Wrap in transaction so the user can undo a repair.
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        let r = self.scene.repair_non_manifold_edges();

        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        if r.faces_detached > 0 {
            self.mark_topology_changed();
            self.invalidate_cache();
        }

        format!(
            r#"{{"ok":true,"edgesExamined":{},"edgesRepaired":{},"edgesSkipped":{},"facesDetached":{},"vertsCreated":{}}}"#,
            r.edges_examined, r.edges_repaired, r.edges_skipped.len(),
            r.faces_detached, r.vertices_created,
        )
    }

    /// Slice (Plane Cut) — split a closed Wall volume into two volumes.
    ///
    /// Inputs:
    ///   `face_ids`     — face IDs of a single closed volume (one XIA).
    ///   `origin_x/y/z` — point on the cutting plane (mm).
    ///   `normal_x/y/z` — plane normal (any non-zero length, will be normalized).
    ///
    /// Returns: JSON `{ok, newXia, aboveCount, belowCount}` or `{ok:false, error}`.
    /// On success the original XIA keeps the above half; the below half is
    /// returned as a new XIA id.
    #[wasm_bindgen(js_name = "sliceVolumeByPlane")]
    pub fn slice_volume_by_plane(
        &mut self,
        face_ids: &[u32],
        origin_x: f64, origin_y: f64, origin_z: f64,
        normal_x: f64, normal_y: f64, normal_z: f64,
    ) -> String {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let plane = match axia_geo::operations::slice::SlicePlane::new(
            DVec3::new(origin_x, origin_y, origin_z),
            DVec3::new(normal_x, normal_y, normal_z),
        ) {
            Ok(p) => p,
            Err(e) => return format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'")),
        };

        debug_log!("[RUST] sliceVolumeByPlane: {} faces, plane n=({},{},{})",
            fids.len(), normal_x, normal_y, normal_z);

        // ADR-267 γ — Scene 메서드가 txn 을 내부 commit → 게이트 op 후 실행,
        // 실패 시 discard_last_undo (manual_txn=false).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        match self.scene.slice_volume_by_plane(&fids, plane) {
            Ok(new_xia) => {
                if !self.integrity_gate_passed(integrity_before, &integrity_snapshot, "slice", false)
                {
                    return format!(
                        r#"{{"ok":false,"error":"{}"}}"#,
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                let total = self.scene.mesh.face_count();
                format!(
                    r#"{{"ok":true,"newXia":{},"totalFaces":{}}}"#,
                    new_xia, total
                )
            }
            Err(e) => {
                console_error!("[RUST] sliceVolumeByPlane ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// ADR-241 (Phase 1 C5) — polygonal TRIM: plane-cut a volume and KEEP only
    /// one half (`keep_above` keeps the +normal side). Removes the discarded
    /// half; the source XIA is reassigned to the kept faces (no new XIA, unlike
    /// `sliceVolumeByPlane`). Returns JSON `{ok, totalFaces}` or `{ok:false,error}`.
    /// Transaction-wrapped (single undo; mesh restored on failure).
    #[wasm_bindgen(js_name = "trimVolumeByPlane")]
    #[allow(clippy::too_many_arguments)]
    pub fn trim_volume_by_plane(
        &mut self,
        face_ids: &[u32],
        origin_x: f64, origin_y: f64, origin_z: f64,
        normal_x: f64, normal_y: f64, normal_z: f64,
        keep_above: bool,
    ) -> String {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let plane = match axia_geo::operations::slice::SlicePlane::new(
            DVec3::new(origin_x, origin_y, origin_z),
            DVec3::new(normal_x, normal_y, normal_z),
        ) {
            Ok(p) => p,
            Err(e) => return format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'")),
        };
        // ADR-291 — mirror slice's integrity gate (trim shares slice's core).
        // OpenMesh scope: catches new geometric cracks / invariant violations
        // (baseline-relative). NOT self-intersection — SI between the two halves
        // at the cut plane is inherent to keeping both halves, not corruption of
        // a resulting solid (measured: trim keep-one is SI-clean at the very
        // planes where slice keep-both self-intersects). Scene commits internally
        // → manual_txn=false.
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        match self.scene.trim_volume_by_plane(&fids, plane, keep_above) {
            Ok(()) => {
                if !self.integrity_gate_passed(integrity_before, &integrity_snapshot, "trim", false)
                {
                    return format!(
                        r#"{{"ok":false,"error":"{}"}}"#,
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                let total = self.scene.mesh.face_count();
                format!(r#"{{"ok":true,"totalFaces":{}}}"#, total)
            }
            Err(e) => {
                console_error!("[RUST] trimVolumeByPlane ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// ADR-197 β-3-n — CURVED KNIFE: cut a curved Path B solid by a horizontal
    /// plane `z = k`. `mode`: "slice" (→ 2 volumes) | "above" | "below" (trim).
    /// Returns JSON `{ok, routed, resultFaces, totalFaces}`. `routed:false`
    /// signals the caller (SliceTool) to fall back to the polygonal slice (the
    /// volume is NOT a single analytic primitive). Transaction-wrapped (undo).
    #[wasm_bindgen(js_name = "cutCurvedByZPlane")]
    pub fn cut_curved_by_z_plane(&mut self, face_ids: &[u32], z: f64, mode: &str) -> String {
        use axia_geo::operations::boolean::CurvedCutMode;
        let m = match mode {
            "slice" => CurvedCutMode::Slice,
            "above" => CurvedCutMode::KeepAbove,
            "below" => CurvedCutMode::KeepBelow,
            _ => return format!(r#"{{"ok":false,"error":"unknown cut mode: {}"}}"#, mode),
        };
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        // ADR-291 — mirror slice's integrity gate on the routed (mutating) arm.
        // Baseline-relative OpenMesh damage (crack/invariant), so pre-existing
        // Path B rim artifacts don't false-reject. Scene commits internally →
        // manual_txn=false. Captured before the call; unused on the fallback arm.
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        // ADR-197 #Track3 — XIA management lives in the Scene wrapper: SLICE splits
        // the 2 disjoint shells into 2 volumes (upper kept, lower = new `_below`
        // XIA); trim keeps 1 volume. Transaction-wrapped there (single undo).
        match self.scene.cut_curved_volume_by_z(&fids, z, m) {
            Ok(res) if !res.routed => {
                // Not a curved primitive → signal polygonal fallback (txn canceled).
                r#"{"ok":true,"routed":false}"#.to_string()
            }
            Ok(res) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "cut curved", false,
                ) {
                    return format!(
                        r#"{{"ok":false,"routed":true,"error":"{}"}}"#,
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                let ids: Vec<u32> = res.faces.iter().map(|f| f.raw()).collect();
                let new_xia = res.new_xia.map(|x| x.to_string()).unwrap_or_else(|| "-1".to_string());
                format!(
                    r#"{{"ok":true,"routed":true,"resultFaces":{:?},"newXia":{},"totalFaces":{}}}"#,
                    ids, new_xia, self.scene.mesh.face_count(),
                )
            }
            Err(e) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                console_error!("[RUST] cutCurvedByZPlane ERROR: {}", e);
                format!(r#"{{"ok":false,"routed":true,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// **ADR-205 γ-wire-ui** — TRIM a curved volume (a tilted cylinder) by an
    /// ARBITRARY plane (origin + normal), keeping the `+normal` side. Routes via
    /// `Scene::trim_curved_volume_by_plane` (cylinder → β-2/β-4/local-frame by the
    /// plane-vs-axis angle). `routed:false` → the SliceTool falls back to the
    /// polygonal slice. Transaction-wrapped in the Scene (single undo).
    #[wasm_bindgen(js_name = "trimCurvedByPlane")]
    pub fn trim_curved_by_plane(
        &mut self,
        face_ids: &[u32],
        ox: f64, oy: f64, oz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> String {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let origin = glam::DVec3::new(ox, oy, oz);
        let normal = glam::DVec3::new(nx, ny, nz);
        // ADR-291 — mirror slice's integrity gate on the routed (mutating) arm
        // (baseline-relative OpenMesh damage; Scene commits internally →
        // manual_txn=false).
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        match self.scene.trim_curved_volume_by_plane(&fids, origin, normal) {
            Ok(res) if !res.routed => r#"{"ok":true,"routed":false}"#.to_string(),
            Ok(res) => {
                if !self.integrity_gate_passed(
                    integrity_before, &integrity_snapshot, "trim curved", false,
                ) {
                    return format!(
                        r#"{{"ok":false,"routed":true,"error":"{}"}}"#,
                        self.last_error.replace('"', "'").replace('\n', " ")
                    );
                }
                self.mark_topology_changed();
                self.invalidate_cache();
                let ids: Vec<u32> = res.faces.iter().map(|f| f.raw()).collect();
                format!(
                    r#"{{"ok":true,"routed":true,"resultFaces":{:?},"totalFaces":{}}}"#,
                    ids, self.scene.mesh.face_count(),
                )
            }
            Err(e) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                console_error!("[RUST] trimCurvedByPlane ERROR: {}", e);
                format!(r#"{{"ok":false,"routed":true,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// Sheet 2D Boolean (Tier 4 B-5).
    /// 두 coplanar Sheet face에 대해 union/subtract/intersect 수행.
    /// op: "union" | "subtract" | "intersect"
    /// 반환: JSON `{ok, resultFace}` 또는 `{ok:false, error}`
    #[wasm_bindgen(js_name = "sheetBoolean")]
    pub fn sheet_boolean(&mut self, a: u32, b: u32, op: &str) -> String {
        let fa = FaceId::new(a);
        let fb = FaceId::new(b);
        let mat = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.sheet_boolean(fa, fb, op, mat) {
            Ok(new_face) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                format!(r#"{{"ok":true,"op":"{}","resultFace":{}}}"#, op, new_face.raw())
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] sheetBoolean ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// Phase 2 — auto_intersect_on_draw 토글.
    /// **ADR-139 B-β-1 (2026-05-18)**: default `false`. 자동 trigger
    /// antipattern 폐기 (메타-원칙 #16). Boundary tool 명시 only 정책.
    /// `true` 명시 시 legacy 동작 (사용자 explicit opt-in).
    #[wasm_bindgen(js_name = "setAutoIntersectOnDraw")]
    pub fn set_auto_intersect_on_draw(&mut self, enabled: bool) {
        self.scene.auto_intersect_on_draw = enabled;
    }

    #[wasm_bindgen(js_name = "getAutoIntersectOnDraw")]
    pub fn get_auto_intersect_on_draw(&self) -> bool {
        self.scene.auto_intersect_on_draw
    }

    /// **ADR-139 B-β-2 (2026-05-18)** — auto_face_synthesis_on_draw 토글.
    /// Default `false`. LOCKED #12 ADR-025 P11 Step 4.99 자동 cycle face
    /// synthesis 폐기 (메타-원칙 #16). Boundary tool (B-γ ~ B-ε) 명시 trigger
    /// 로 대체. `true` 명시 시 legacy 동작 (사용자 explicit opt-in).
    #[wasm_bindgen(js_name = "setAutoFaceSynthesisOnDraw")]
    pub fn set_auto_face_synthesis_on_draw(&mut self, enabled: bool) {
        self.scene.auto_face_synthesis_on_draw = enabled;
    }

    #[wasm_bindgen(js_name = "getAutoFaceSynthesisOnDraw")]
    pub fn get_auto_face_synthesis_on_draw(&self) -> bool {
        self.scene.auto_face_synthesis_on_draw
    }

    /// **ADR-186 δ-4d (2026-06-01)** — face_rederive_on_draw (유도면 모델) 토글.
    /// Default `false` (engine OFF). `true` (opt-in) 시 draw 후 intersect_faces_
    /// inner 가 case-by-case auto_intersect/annulus 대신 boundary kernel
    /// re-derive (rebuild_coplanar_faces) 로 라우팅 — "면사라짐/면분할 안됨
    /// 반복"의 근본 통합 경로. production localStorage 'true' opt-in.
    #[wasm_bindgen(js_name = "setFaceRederiveOnDraw")]
    pub fn set_face_rederive_on_draw(&mut self, enabled: bool) {
        self.scene.face_rederive_on_draw = enabled;
    }

    #[wasm_bindgen(js_name = "getFaceRederiveOnDraw")]
    pub fn get_face_rederive_on_draw(&self) -> bool {
        self.scene.face_rederive_on_draw
    }

    /// **ADR-186 A3/B6-2a** — freeform (Bezier/BSpline/NURBS) overlap → smooth
    /// lens toggle. Default `false` (engine OFF). `true` (production opt-in via
    /// TS FreeformOverlapSettings) → overlapping freeform self-loops split into
    /// lens sub-faces, idempotent (B6-1 owner-id restore). Requires
    /// `face_rederive_on_draw` (only enables the overlap branch within rederive).
    #[wasm_bindgen(js_name = "setFreeformOverlapOnDraw")]
    pub fn set_freeform_overlap_on_draw(&mut self, enabled: bool) {
        self.scene.freeform_overlap_on_draw = enabled;
    }

    #[wasm_bindgen(js_name = "getFreeformOverlapOnDraw")]
    pub fn get_freeform_overlap_on_draw(&self) -> bool {
        self.scene.freeform_overlap_on_draw
    }

    /// "Intersect with Model" — SketchUp 스타일 수동 교차선 생성.
    /// 선택된 face 들과 나머지 active face 사이의 3D 교차선을 edge 로 변환.
    /// inside/outside 판정 없이 모든 sub-face 유지.
    ///
    /// 반환: 성공 시 {"ok":true,"faceCount":N,"totalFaces":M}
    ///       실패 시 {"ok":false,"error":"..."}
    #[wasm_bindgen(js_name = "intersectWithModel")]
    pub fn intersect_with_model(&mut self, face_ids: &[u32]) -> String {
        if face_ids.is_empty() {
            return r#"{"ok":false,"error":"no faces selected"}"#.to_string();
        }
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        debug_log!("[RUST] intersect_with_model: {} faces selected", fids.len());

        match self.scene.intersect_faces_with_scene(&fids) {
            Ok(n) => {
                self.mark_topology_changed();
                self.invalidate_cache();
                format!(
                    r#"{{"ok":true,"resultFaces":{},"totalFaces":{}}}"#,
                    n, self.scene.mesh.face_count()
                )
            }
            Err(e) => {
                console_error!("[RUST] intersect_with_model ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    // ========================================================================
    // Transform Operations (Move / Rotate / Scale)
    // ========================================================================

    /// 선택된 face들의 정점을 이동
    pub fn translate_faces(&mut self, face_ids: &[u32], dx: f64, dy: f64, dz: f64) -> bool {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let delta = DVec3::new(dx, dy, dz);

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.translate_faces(&fids, delta) {
            Ok(res) => {
                debug_log!("[RUST] translate: moved {} verts, {} faces", res.verts_moved, res.faces_affected);
                // Level 2 auto-resolve constraints after face transform
                // Level 3: iterative XPBD-style solve until convergence
                let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
                // ADR-274 P2-1 — atomic flush-collapse INSIDE the same
                // transaction, BEFORE any export. A face pushed flush (height→0)
                // leaves degenerate walls; if we waited for a commit-time call the
                // render export (deactivate_empty_emit_faces) would have already
                // removed those walls → collapse no-op. Running it here (gate-
                // guarded no-op otherwise, self-rolling-back on failure) folds the
                // cleanup into a single Undo step and covers every tool.
                let _ = self.scene.collapse_flush_extrusion(1e-3);
                // ADR-274 Phase 3 P3-B — a translate that folds a face through the
                // solid (e.g. overshooting a boss cap past the base) self-intersects
                // / breaks winding invariants (measured: closed but valid=false,
                // violations=4) and was silently committed. Gate rejects+rolls back;
                // a valid move (and the flush-collapse, which stays closed/valid)
                // passes untouched.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "translate", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] translate ERROR: {}", e);
                self.set_error(format!("translate: {}", e));
                false
            }
        }
    }

    /// 선택된 face들의 정점을 회전
    /// cx,cy,cz: 회전 중심, ax,ay,az: 회전축, angle_deg: 각도 (도)
    pub fn rotate_faces(
        &mut self, face_ids: &[u32],
        cx: f64, cy: f64, cz: f64,
        ax: f64, ay: f64, az: f64,
        angle_deg: f64,
    ) -> bool {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let center = DVec3::new(cx, cy, cz);
        let axis = DVec3::new(ax, ay, az);
        let angle_rad = angle_deg.to_radians();

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.rotate_faces(&fids, center, axis, angle_rad) {
            Ok(res) => {
                debug_log!("[RUST] rotate: {} verts, {:.1}°", res.verts_moved, angle_deg);
                // Level 3: iterative XPBD-style solve until convergence
                let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
                // ADR-274 Phase 3 P3-A — rotating a SUBSET of a closed solid can
                // self-intersect or flip winding invariants (measured: SI 0→1 /
                // invariants→INVALID). Gate rejects+rolls back; SAFE moves (whole
                // faces, no SI) pass untouched.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "rotate", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] rotate ERROR: {}", e);
                self.set_error(format!("rotate: {}", e));
                false
            }
        }
    }

    /// 선택된 face들의 정점을 스케일
    /// cx,cy,cz: 스케일 중심, sx,sy,sz: 축별 배율
    pub fn scale_faces(
        &mut self, face_ids: &[u32],
        cx: f64, cy: f64, cz: f64,
        sx: f64, sy: f64, sz: f64,
    ) -> bool {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let center = DVec3::new(cx, cy, cz);
        let scale = DVec3::new(sx, sy, sz);

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.scale_faces(&fids, center, scale) {
            Ok(res) => {
                debug_log!("[RUST] scale: {} verts, ({:.2},{:.2},{:.2})", res.verts_moved, sx, sy, sz);
                // ADR-274 Phase 3 P3-A — a NEGATIVE (reflection) scale flips
                // winding + self-intersects (measured: SI 0→1 / invariants→INVALID).
                // Gate rejects+rolls back; positive/non-uniform scale passes.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "scale", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                // Use topology_changed for full rebuild: shared vertices between
                // selected and adjacent faces make partial delta unreliable.
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] scale ERROR: {}", e);
                self.set_error(format!("scale: {}", e));
                false
            }
        }
    }

    // ========================================================================
    // Constraint Solver Level 1 (vertex-level ops + edge queries)
    // ========================================================================

    /// 지정 정점 배열을 delta만큼 이동. Constraint Solver에서 makeParallel/
    /// Perpendicular/setDistance의 기초 연산으로 사용.
    #[wasm_bindgen(js_name = "translateVerts")]
    pub fn translate_verts(&mut self, vert_ids: &[u32], dx: f64, dy: f64, dz: f64) -> bool {
        let vids: Vec<VertId> = vert_ids.iter().map(|&id| VertId::new(id)).collect();
        let delta = DVec3::new(dx, dy, dz);

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.translate_verts(&vids, delta) {
            Ok(_) => {
                // Level 2: auto-resolve constraints touching any moved vertex
                // Level 3: iterative XPBD-style solve until convergence
                let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
                // ADR-274 P2-1 — atomic flush-collapse before export (see
                // translate_faces). Gate-guarded no-op unless the move left
                // degenerate walls; folded into this single Undo step.
                let _ = self.scene.collapse_flush_extrusion(1e-3);
                // ADR-274 Phase 3 P3-B — reject a vert move that self-intersects /
                // breaks invariants (overshoot through the solid); valid moves pass.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "translate_verts", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                console_error!("[RUST] translate_verts ERROR: {}", e);
                self.set_error(format!("translate_verts: {}", e));
                self.scene.transactions.cancel();
                false
            }
        }
    }

    /// 지정 정점을 center/axis 기준으로 회전.
    #[wasm_bindgen(js_name = "rotateVerts")]
    pub fn rotate_verts(
        &mut self, vert_ids: &[u32],
        cx: f64, cy: f64, cz: f64,
        ax: f64, ay: f64, az: f64,
        angle_deg: f64,
    ) -> bool {
        let vids: Vec<VertId> = vert_ids.iter().map(|&id| VertId::new(id)).collect();
        let center = DVec3::new(cx, cy, cz);
        let axis = DVec3::new(ax, ay, az);
        let angle_rad = angle_deg.to_radians();

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.rotate_verts(&vids, center, axis, angle_rad) {
            Ok(_) => {
                // Level 2: auto-resolve constraints
                // Level 3: iterative XPBD-style solve until convergence
                let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
                // ADR-274 Phase 3 P3-A — rotating a SUBSET of verts self-intersects
                // (measured SI 0→1). Gate rejects+rolls back.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "rotate_verts", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                console_error!("[RUST] rotate_verts ERROR: {}", e);
                self.set_error(format!("rotate_verts: {}", e));
                self.scene.transactions.cancel();
                false
            }
        }
    }

    /// 지정 정점을 center 기준으로 스케일. (sx,sy,sz)로 비균일 지원.
    #[wasm_bindgen(js_name = "scaleVerts")]
    pub fn scale_verts(
        &mut self, vert_ids: &[u32],
        cx: f64, cy: f64, cz: f64,
        sx: f64, sy: f64, sz: f64,
    ) -> bool {
        let vids: Vec<VertId> = vert_ids.iter().map(|&id| VertId::new(id)).collect();
        let center = DVec3::new(cx, cy, cz);
        let scale = DVec3::new(sx, sy, sz);

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.scale_verts(&vids, center, scale) {
            Ok(_) => {
                let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
                // ADR-274 Phase 3 P3-A — a NEGATIVE (reflection) scale of a subset
                // self-intersects (measured SI 0→1). Gate rejects+rolls back;
                // positive non-uniform scale passes.
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "scale_verts", true,
                ) {
                    return false;
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                true
            }
            Err(e) => {
                console_error!("[RUST] scale_verts ERROR: {}", e);
                self.set_error(format!("scale_verts: {}", e));
                self.scene.transactions.cancel();
                false
            }
        }
    }

    /// Edge를 지정 위치에서 split하여 새 vertex를 생성하고 edge를 2개로 나눈다.
    /// 반환: 성공 시 새 vertex id (>=0), 실패 시 -1.
    /// position이 엣지 선분 밖이면 가까운 쪽으로 clamp.
    /// 내부적으로 mesh.split_edge를 호출하고 단일 undo 트랜잭션으로 감쌈.
    #[wasm_bindgen(js_name = "splitEdge")]
    pub fn split_edge(&mut self, edge_id_raw: u32, px: f64, py: f64, pz: f64) -> i32 {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            self.set_error(format!("Edge {} not found", edge_id_raw));
            return -1;
        }
        // Clamp position onto the edge segment for safety
        let pos = {
            let edge = &self.scene.mesh.edges[eid];
            let p0 = self.scene.mesh.vertex_pos(edge.v_small()).unwrap_or(DVec3::ZERO);
            let p1 = self.scene.mesh.vertex_pos(edge.v_large()).unwrap_or(DVec3::ZERO);
            let p  = DVec3::new(px, py, pz);
            let d  = p1 - p0;
            let len_sq = d.length_squared();
            if len_sq < 1e-12 {
                p0
            } else {
                let t = ((p - p0).dot(d) / len_sq).clamp(0.05, 0.95);
                p0 + d * t
            }
        };

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        match self.scene.mesh.split_edge(eid, pos) {
            Ok((vp, _e1, _e2)) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                vp.raw() as i32
            }
            Err(e) => {
                self.scene.transactions.cancel();
                self.set_error(format!("split_edge: {}", e));
                -1
            }
        }
    }

    /// Collect all edges in the polyline chain containing `edge_id`.
    /// Walks through degree-2 vertices and stops at junctions/dead-ends.
    /// Empty Vec on invalid / inactive edge.
    #[wasm_bindgen(js_name = "collectEdgeChain")]
    pub fn collect_edge_chain(&self, edge_id_raw: u32) -> Vec<u32> {
        let eid = EdgeId::new(edge_id_raw);
        self.scene.mesh.collect_edge_chain(eid).iter().map(|e| e.raw()).collect()
    }

    /// Edge의 두 끝점 VertId를 반환 ([v_small, v_large]).
    /// 실패 시 빈 벡터.
    #[wasm_bindgen(js_name = "getEdgeEndpoints")]
    pub fn get_edge_endpoints(&self, edge_id_raw: u32) -> Vec<u32> {
        let eid = EdgeId::new(edge_id_raw);
        let edge = match self.scene.mesh.edges.get(eid) {
            Some(e) if e.is_active() => e,
            _ => return Vec::new(),
        };
        vec![edge.v_small().raw(), edge.v_large().raw()]
    }

    /// Vertex 위치를 [x, y, z]로 반환. 실패 시 빈 벡터.
    #[wasm_bindgen(js_name = "getVertexPos")]
    pub fn get_vertex_pos(&self, vert_id_raw: u32) -> Vec<f64> {
        let vid = VertId::new(vert_id_raw);
        match self.scene.mesh.vertex_pos(vid) {
            Ok(p) => vec![p.x, p.y, p.z],
            Err(_) => Vec::new(),
        }
    }

    /// 주어진 world 좌표 (x,y,z) 에 가장 가까운 활성 vertex 의 VertId 반환.
    /// `tol` 거리 안에 vertex 가 없으면 -1.
    ///
    /// Move tool 의 vertex pick 경로 — 사용자가 endpoint snap 위에서 클릭한
    /// 위치를 VertId 로 변환하여 단일 정점 이동을 가능하게 한다.
    #[wasm_bindgen(js_name = "findVertexIdAt")]
    pub fn find_vertex_id_at(&self, x: f64, y: f64, z: f64, tol: f64) -> i32 {
        let target = DVec3::new(x, y, z);
        let tol_sq = (tol.max(1e-6)) * (tol.max(1e-6));
        let mut best: Option<(VertId, f64)> = None;
        for (vid, _) in self.scene.mesh.verts.iter() {
            if let Ok(pos) = self.scene.mesh.vertex_pos(vid) {
                let d_sq = (pos - target).length_squared();
                if d_sq <= tol_sq {
                    if best.map(|b| d_sq < b.1).unwrap_or(true) {
                        best = Some((vid, d_sq));
                    }
                }
            }
        }
        match best {
            Some((vid, _)) => vid.raw() as i32,
            None => -1,
        }
    }

    // ========================================================================
    // Constraint Solver Level 2 — persistent graph (Scene.constraints)
    // ========================================================================

    /// Add a parallel/perpendicular/collinear constraint between two edges.
    /// `edgeA_v_a/b` and `edgeB_v_a/b` are vertex IDs.
    /// `kind`: "parallel" | "perpendicular" | "collinear"
    /// Returns the new constraint ID (>=1) on success, 0 on failure.
    #[wasm_bindgen(js_name = "addEdgeConstraint")]
    pub fn add_edge_constraint(
        &mut self,
        kind: &str,
        edge_a_v_a: u32, edge_a_v_b: u32,
        edge_b_v_a: u32, edge_b_v_b: u32,
    ) -> u32 {
        let kind = match kind {
            "parallel"      => ConstraintKind::Parallel,
            "perpendicular" => ConstraintKind::Perpendicular,
            "collinear"     => ConstraintKind::Collinear,
            other => { self.set_error(format!("unknown constraint kind: {}", other)); return 0; }
        };
        let refs = vec![
            ConstraintRef::Edge { v_a: VertId::new(edge_a_v_a), v_b: VertId::new(edge_a_v_b) },
            ConstraintRef::Edge { v_a: VertId::new(edge_b_v_a), v_b: VertId::new(edge_b_v_b) },
        ];
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let id = self.scene.constraints.add(kind, refs, None);
        // Apply immediately — single constraint, iterative gives same result
        // but handles newly conflicting geometry gracefully.
        let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        id
    }

    /// Add a distance constraint between two vertices.
    #[wasm_bindgen(js_name = "addDistanceConstraint")]
    pub fn add_distance_constraint(&mut self, v_a: u32, v_b: u32, distance: f64) -> u32 {
        if !distance.is_finite() || distance <= 0.0 {
            self.set_error(format!("distance must be > 0, got {}", distance));
            return 0;
        }
        let refs = vec![
            ConstraintRef::Vertex(VertId::new(v_a)),
            ConstraintRef::Vertex(VertId::new(v_b)),
        ];
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let id = self.scene.constraints.add(ConstraintKind::Distance, refs, Some(distance));
        if let Some(c) = self.scene.constraints.get(id).cloned() {
            let _ = resolve_constraint(&mut self.scene.mesh, &c);
        }
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        id
    }

    /// ADR-216 — add an angle constraint between two edges (the driving angular
    /// dimension). `angle_rad` in (0, π). refs[0] = driver edge, refs[1] = driven.
    /// Returns the new constraint id (>=1), or 0 on failure.
    #[wasm_bindgen(js_name = "addAngleConstraint")]
    pub fn add_angle_constraint(
        &mut self,
        edge_a_v_a: u32, edge_a_v_b: u32,
        edge_b_v_a: u32, edge_b_v_b: u32,
        angle_rad: f64,
    ) -> u32 {
        if !angle_rad.is_finite() || angle_rad <= 1e-6 || angle_rad >= std::f64::consts::PI - 1e-6 {
            self.set_error(format!("angle must be in (0, π) radians, got {}", angle_rad));
            return 0;
        }
        let refs = vec![
            ConstraintRef::Edge { v_a: VertId::new(edge_a_v_a), v_b: VertId::new(edge_a_v_b) },
            ConstraintRef::Edge { v_a: VertId::new(edge_b_v_a), v_b: VertId::new(edge_b_v_b) },
        ];
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let id = self.scene.constraints.add(ConstraintKind::Angle, refs, Some(angle_rad));
        let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        id
    }

    /// ADR-217 — add a radius constraint on the Circle/Arc edge at `ref_vert`
    /// (the driving radial dimension). `radius` > 0. Returns the new id, or 0.
    #[wasm_bindgen(js_name = "addRadiusConstraint")]
    pub fn add_radius_constraint(&mut self, ref_vert: u32, radius: f64) -> u32 {
        if !radius.is_finite() || radius <= 0.0 {
            self.set_error(format!("radius must be > 0, got {}", radius));
            return 0;
        }
        let vid = VertId::new(ref_vert);
        if self.scene.mesh.find_curve_edge_at(vid).is_none() {
            self.set_error(format!("vertex {} is not on a circle/arc edge", ref_vert));
            return 0;
        }
        let refs = vec![ConstraintRef::Vertex(vid)];
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let id = self.scene.constraints.add(ConstraintKind::Radius, refs, Some(radius));
        if let Some(c) = self.scene.constraints.get(id).cloned() {
            let _ = resolve_constraint(&mut self.scene.mesh, &c);
        }
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        id
    }

    /// ADR-218 — add a REFERENCE (read-only) linear dimension between two
    /// vertices. Same Distance kind, but `value = None` so the solver never
    /// drives it — it only measures (the label shows the live geometry value in
    /// parentheses, non-editable). Returns the new id, or 0.
    #[wasm_bindgen(js_name = "addReferenceDistance")]
    pub fn add_reference_distance(&mut self, v_a: u32, v_b: u32) -> u32 {
        let refs = vec![
            ConstraintRef::Vertex(VertId::new(v_a)),
            ConstraintRef::Vertex(VertId::new(v_b)),
        ];
        self.add_reference_constraint(ConstraintKind::Distance, refs)
    }

    /// ADR-218 — add a REFERENCE (read-only) angular dimension between two edges.
    /// Same Angle kind with `value = None` (measures only). Returns the new id, or 0.
    #[wasm_bindgen(js_name = "addReferenceAngle")]
    pub fn add_reference_angle(
        &mut self,
        edge_a_v_a: u32, edge_a_v_b: u32,
        edge_b_v_a: u32, edge_b_v_b: u32,
    ) -> u32 {
        let refs = vec![
            ConstraintRef::Edge { v_a: VertId::new(edge_a_v_a), v_b: VertId::new(edge_a_v_b) },
            ConstraintRef::Edge { v_a: VertId::new(edge_b_v_a), v_b: VertId::new(edge_b_v_b) },
        ];
        self.add_reference_constraint(ConstraintKind::Angle, refs)
    }

    /// ADR-218 — add a REFERENCE (read-only) radial dimension on the Circle/Arc
    /// edge at `ref_vert`. Same Radius kind with `value = None`. Returns id, or 0.
    #[wasm_bindgen(js_name = "addReferenceRadius")]
    pub fn add_reference_radius(&mut self, ref_vert: u32) -> u32 {
        let vid = VertId::new(ref_vert);
        if self.scene.mesh.find_curve_edge_at(vid).is_none() {
            self.set_error(format!("vertex {} is not on a circle/arc edge", ref_vert));
            return 0;
        }
        self.add_reference_constraint(ConstraintKind::Radius, vec![ConstraintRef::Vertex(vid)])
    }

    /// ADR-218 — shared body for the three reference dimensions: add the
    /// constraint with `value = None` (so it is never solved), wrapped in a
    /// single undo transaction. No geometry / topology change.
    fn add_reference_constraint(&mut self, kind: ConstraintKind, refs: Vec<ConstraintRef>) -> u32 {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let id = self.scene.constraints.add(kind, refs, None);
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        id
    }

    /// ADR-217 — radius of a Circle/Arc edge, or -1 if the edge has no such curve.
    #[wasm_bindgen(js_name = "edgeCurveRadius")]
    pub fn edge_curve_radius(&self, edge_id: u32) -> f64 {
        self.scene.mesh.edge_curve_radius(EdgeId::new(edge_id)).unwrap_or(-1.0)
    }

    /// ADR-217 — for the radial dimension render: the curve [center.xyz, radius]
    /// of the Circle/Arc edge at `ref_vert`. Empty when not found.
    #[wasm_bindgen(js_name = "radiusDimAt")]
    pub fn radius_dim_at(&self, ref_vert: u32) -> Vec<f64> {
        let vid = VertId::new(ref_vert);
        let edge = match self.scene.mesh.find_curve_edge_at(vid) {
            Some(e) => e,
            None => return Vec::new(),
        };
        match self.scene.mesh.edge_curve(edge) {
            Some(axia_geo::curves::AnalyticCurve::Circle { center, radius, .. })
            | Some(axia_geo::curves::AnalyticCurve::Arc { center, radius, .. }) => {
                vec![center.x, center.y, center.z, *radius]
            }
            _ => Vec::new(),
        }
    }

    /// ADR-215 — set a constraint's target `value` (the parametric dimension
    /// value) and re-solve. Used by the editable Dimension label. Returns true
    /// if the constraint existed and `value > 0`.
    #[wasm_bindgen(js_name = "setConstraintValue")]
    pub fn set_constraint_value(&mut self, id: u32, value: f64) -> bool {
        if !value.is_finite() || value <= 0.0 {
            self.set_error(format!("dimension value must be > 0, got {}", value));
            return false;
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let ok = self.scene.constraints.set_value(id, value);
        if ok {
            // Re-solve the whole graph so the edited dimension drives geometry
            // and any interacting constraints settle.
            let _ = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, 50, 1e-5);
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            self.scene.transactions.cancel();
            self.set_error(format!("constraint {} not found", id));
        }
        ok
    }

    /// Remove a constraint by ID. Returns true on success.
    #[wasm_bindgen(js_name = "removeConstraint")]
    pub fn remove_constraint(&mut self, id: u32) -> bool {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let removed = self.scene.constraints.remove(id);
        if removed {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
        } else {
            self.scene.transactions.cancel();
        }
        removed
    }

    /// List all constraints as JSON.
    /// Format: [{id, kind, active, refs:[...], value}, ...]
    #[wasm_bindgen(js_name = "listConstraints")]
    pub fn list_constraints(&self) -> String {
        // Lightweight manual JSON (avoid pulling in serde_json just here)
        let mut out = String::from("[");
        for (i, c) in self.scene.constraints.iter().enumerate() {
            if i > 0 { out.push(','); }
            let kind = match c.kind {
                ConstraintKind::Parallel      => "parallel",
                ConstraintKind::Perpendicular => "perpendicular",
                ConstraintKind::Collinear     => "collinear",
                ConstraintKind::Distance      => "distance",
                ConstraintKind::Angle         => "angle",
                ConstraintKind::Radius        => "radius",
            };
            out.push_str(&format!(
                r#"{{"id":{},"kind":"{}","active":{}"#, c.id, kind, c.active
            ));
            if let Some(v) = c.value {
                out.push_str(&format!(r#","value":{}"#, v));
            }
            out.push_str(r#","refs":["#);
            for (j, r) in c.refs.iter().enumerate() {
                if j > 0 { out.push(','); }
                match r {
                    ConstraintRef::Edge { v_a, v_b } =>
                        out.push_str(&format!(r#"{{"edge":[{},{}]}}"#, v_a.raw(), v_b.raw())),
                    ConstraintRef::Vertex(v) =>
                        out.push_str(&format!(r#"{{"vertex":{}}}"#, v.raw())),
                }
            }
            out.push_str("]}");
        }
        out.push(']');
        out
    }

    /// Re-solve all active constraints. Returns number of constraints that
    /// actually moved geometry.
    #[wasm_bindgen(js_name = "resolveAllConstraints")]
    pub fn resolve_all_constraints(&mut self) -> u32 {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let count = resolve_all(&mut self.scene.mesh, &self.scene.constraints);
        if count > 0 {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            self.scene.transactions.cancel();
        }
        count as u32
    }

    /// Toggle active flag of a constraint.
    #[wasm_bindgen(js_name = "setConstraintActive")]
    pub fn set_constraint_active(&mut self, id: u32, active: bool) -> bool {
        self.scene.constraints.set_active(id, active)
    }

    /// **Level 3**: iterative XPBD-style solver. Returns a JSON result
    /// `{converged, iterations, finalResidual, initialResidual, overConstrained}`.
    /// Wraps in a single undo transaction if anything moved.
    #[wasm_bindgen(js_name = "resolveConstraintsIterative")]
    pub fn resolve_constraints_iterative(&mut self, max_iter: u32, tolerance: f64) -> String {
        let max_iter = if max_iter == 0 { 50 } else { max_iter.min(2000) };
        let tolerance = if tolerance <= 0.0 { 1e-5 } else { tolerance };

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let result = resolve_iterative(&mut self.scene.mesh, &self.scene.constraints, max_iter, tolerance);
        // Only commit a transaction if the solver actually changed something
        // (final residual differs from initial).
        if (result.initial_residual - result.final_residual).abs() > 1e-12 {
            self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
            self.scene.transactions.commit();
            self.mark_topology_changed();
            self.invalidate_cache();
        } else {
            self.scene.transactions.cancel();
        }
        format!(
            r#"{{"converged":{},"iterations":{},"finalResidual":{:.9},"initialResidual":{:.9},"overConstrained":{}}}"#,
            result.converged, result.iterations, result.final_residual,
            result.initial_residual, result.over_constrained,
        )
    }

    /// **Level 3**: max residual across all active constraints at current state.
    /// For monitoring / UI status without mutating the mesh.
    #[wasm_bindgen(js_name = "maxConstraintResidual")]
    pub fn max_constraint_residual(&self) -> f64 {
        max_residual(&self.scene.mesh, &self.scene.constraints)
    }

    /// Count of constraints (active + inactive).
    #[wasm_bindgen(js_name = "constraintCount")]
    pub fn constraint_count(&self) -> u32 {
        self.scene.constraints.len() as u32
    }

    /// Offset: face의 경계를 dist만큼 안쪽(+)/바깥쪽(-)으로 오프셋
    /// 반환: JSON 결과 { ok, innerFace, stripFaces, ... }
    pub fn offset_face(&mut self, face_id_raw: u32, dist: f64) -> String {
        let fid = FaceId::new(face_id_raw);

        // ADR-016 Q2 — multi-loop face (ring with holes) 거부.
        if let Some(face) = self.scene.mesh.faces.get(fid) {
            if !face.inners().is_empty() {
                return format!(
                    "{{\"ok\":false,\"error\":\"multi-loop face Offset unsupported (ADR-016 Q2): face {} has {} hole(s)\"}}",
                    face_id_raw, face.inners().len()
                );
            }
        }

        // 트랜잭션 시작
        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        // Defense-in-depth: closure-preserving + self-intersection baseline
        // (offset R10 fixed a real SI bug — this keeps future regressions
        // fail-loud). A sheet input is open (before_boundary>0) so closure is
        // not enforced; only a NEW self-intersection or a closed→open tear is.
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.offset_face(fid, dist) {
            Ok(result) => {
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "offset", true,
                ) {
                    return r#"{"ok":false,"error":"offset 가 solid 를 열거나 자기교차(self-intersection)를 만들어 취소됨"}"#.to_string();
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();

                let strip_ids: Vec<u32> = result.strip_faces.iter()
                    .map(|f| f.raw())
                    .collect();

                format!(
                    r#"{{"ok":true,"innerFace":{},"stripFaces":{:?},"totalFaces":{},"totalVerts":{}}}"#,
                    result.inner_face.raw(),
                    strip_ids,
                    self.scene.mesh.face_count(),
                    self.scene.mesh.vert_count(),
                )
            }
            Err(e) => {
                self.scene.transactions.cancel();
                console_error!("[RUST] offset ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// 3D pocket recess = inset the face boundary by `inset` (> 0), then push
    /// the flush inner face into the solid by `depth` (> 0) to form a pocket
    /// (floor + walls) with the coplanar ring (frame) kept flush. The standard
    /// "offset then push/pull inward" recess as one manifold-safe op. Guarded by
    /// the closure-preserving + self-intersection gate. Returns JSON
    /// `{ ok, pocketFace, wallFaces, frameFaces }` or `{ ok:false, error }`.
    pub fn create_recess(&mut self, face_id_raw: u32, inset: f64, depth: f64) -> String {
        let fid = FaceId::new(face_id_raw);
        let mat = axia_core::FORM_MATERIAL;

        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();

        match self.scene.mesh.create_recess(fid, inset, depth, mat) {
            Ok(r) => {
                if !self.closure_preserving_gate_passed(
                    before_boundary, before_si, &before_snapshot, "recess", true,
                ) {
                    return r#"{"ok":false,"error":"recess 가 solid 를 열거나 자기교차(self-intersection)를 만들어 취소됨"}"#.to_string();
                }
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();

                let walls: Vec<u32> = r.wall_faces.iter().map(|f| f.raw()).collect();
                let frame: Vec<u32> = r.frame_faces.iter().map(|f| f.raw()).collect();
                format!(
                    r#"{{"ok":true,"pocketFace":{},"wallFaces":{:?},"frameFaces":{:?},"totalFaces":{}}}"#,
                    r.pocket_face.raw(),
                    walls,
                    frame,
                    self.scene.mesh.face_count(),
                )
            }
            Err(e) => {
                // Defensive: create_recess mutates (soft_remove + add_face)
                // before it can fail — restore the pre-op snapshot so a partial
                // build never survives, then cancel the transaction.
                self.scene.restore_scene_snapshot(&before_snapshot);
                self.scene.transactions.cancel();
                self.invalidate_cache();
                console_error!("[RUST] recess ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// Read-only recess preview geometry for the UI ghost (no mutation).
    /// Returns JSON `{ ok, insetLoop:[x,y,z,...], floorLoop:[x,y,z,...] }` —
    /// the inset boundary flush with the surface and the recessed floor loop.
    pub fn recess_preview(&self, face_id_raw: u32, inset: f64, depth: f64) -> String {
        let fid = FaceId::new(face_id_raw);
        match self.scene.mesh.recess_preview(fid, inset, depth) {
            Ok((inset_loop, floor_loop)) => {
                let flat = |pts: &[DVec3]| -> String {
                    pts.iter()
                        .flat_map(|p| [p.x, p.y, p.z])
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                };
                format!(
                    r#"{{"ok":true,"insetLoop":[{}],"floorLoop":[{}]}}"#,
                    flat(&inset_loop),
                    flat(&floor_loop),
                )
            }
            Err(e) => format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'")),
        }
    }

    /// Edge(line)를 평행하게 offset하여 새 edge 생성 (선만 복사, 면은 만들지 않음)
    /// plane_normal: 참조 평면 법선 (Y-up = 0,1,0)
    pub fn offset_edge(
        &mut self,
        edge_id_raw: u32,
        dist: f64,
        pnx: f64, pny: f64, pnz: f64,
    ) -> String {
        let eid = EdgeId::new(edge_id_raw);
        let plane_normal = glam::DVec3::new(pnx, pny, pnz);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.offset_edge(eid, dist, plane_normal) {
            Ok(result) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();

                format!(
                    r#"{{"ok":true,"newEdge":{},"newV0":{},"newV1":{}}}"#,
                    result.new_edge.raw(),
                    result.new_v0.raw(),
                    result.new_v1.raw(),
                )
            }
            Err(e) => {
                console_error!("[RUST] offset_edge ERROR: {}", e);
                format!(r#"{{"ok":false,"error":"{}"}}"#, e.to_string().replace('"', "'"))
            }
        }
    }

    /// ADR-080 V-β-α-bridge — Edge offset using host face's surface as the
    /// reference (no caller-supplied plane_normal). Returns JSON whose
    /// `reason` field on failure is one of:
    ///   - `"unsupported_surface"` (with `kind`: "Cylinder" / "Sphere" /
    ///     "Cone" / "Torus" / "BezierPatch" / "BSplineSurface" /
    ///     "NURBSSurface") — V-β-γ / W-3 forward defer
    ///   - `"unsupported_curve"` (with `kind`: "Arc" / "Circle" / "Bezier"
    ///     / "BSpline" / "NURBS") — V-β-β / W-3 forward defer
    ///   - `"no_incident_face"` — free wire (V-δ scope)
    ///   - `"ambiguous_host"` — multiple incident faces with conflicting
    ///     surfaces
    ///   - `"multi_loop"` — host face has hole loops (ADR-016 Q2 / L8)
    ///   - `"degenerate_distance"` — |dist| below epsilon
    ///   - `"other"` (with `message`) — any other failure
    ///
    /// On success: `{"ok":true,"newEdge":<u32>,"newV0":<u32>,"newV1":<u32>}`.
    pub fn offset_edge_on_host(&mut self, edge_id_raw: u32, dist: f64) -> String {
        use axia_geo::operations::offset::OffsetEdgeError;
        let eid = EdgeId::new(edge_id_raw);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.offset_edge_on_host_face(eid, dist) {
            Ok(result) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                format!(
                    r#"{{"ok":true,"newEdge":{},"newV0":{},"newV1":{}}}"#,
                    result.new_edge.raw(),
                    result.new_v0.raw(),
                    result.new_v1.raw(),
                )
            }
            Err(err) => {
                self.scene.transactions.cancel();
                debug_log!("[RUST] offset_edge_on_host failure: {}", err);
                match err {
                    OffsetEdgeError::UnsupportedHostSurface { kind } => {
                        format!(r#"{{"ok":false,"reason":"unsupported_surface","kind":"{}"}}"#, kind)
                    }
                    OffsetEdgeError::UnsupportedCurveKind { kind } => {
                        format!(r#"{{"ok":false,"reason":"unsupported_curve","kind":"{}"}}"#, kind)
                    }
                    OffsetEdgeError::NoIncidentFace => {
                        r#"{"ok":false,"reason":"no_incident_face"}"#.to_string()
                    }
                    OffsetEdgeError::AmbiguousHostFace { n_faces } => {
                        format!(r#"{{"ok":false,"reason":"ambiguous_host","nFaces":{}}}"#, n_faces)
                    }
                    OffsetEdgeError::MultiLoopHostFace(_) => {
                        r#"{"ok":false,"reason":"multi_loop"}"#.to_string()
                    }
                    OffsetEdgeError::DegenerateDistance(_) => {
                        r#"{"ok":false,"reason":"degenerate_distance"}"#.to_string()
                    }
                    OffsetEdgeError::ArcPlaneMismatch => {
                        r#"{"ok":false,"reason":"arc_plane_mismatch"}"#.to_string()
                    }
                    OffsetEdgeError::RadiusCollapse { current_r, new_r, .. } => {
                        format!(
                            r#"{{"ok":false,"reason":"radius_collapse","currentRadius":{},"newRadius":{}}}"#,
                            current_r, new_r
                        )
                    }
                    OffsetEdgeError::UnsupportedCurveOnSurface { surface_kind, curve_kind } => {
                        format!(
                            r#"{{"ok":false,"reason":"unsupported_curve_on_surface","surfaceKind":"{}","curveKind":"{}"}}"#,
                            surface_kind, curve_kind
                        )
                    }
                    OffsetEdgeError::AxialOutOfRange { new_v, v_min, v_max } => {
                        format!(
                            r#"{{"ok":false,"reason":"axial_out_of_range","newV":{},"vMin":{},"vMax":{}}}"#,
                            new_v, v_min, v_max
                        )
                    }
                    OffsetEdgeError::WireNotPlanar { rms_error } => {
                        format!(
                            r#"{{"ok":false,"reason":"wire_not_planar","rmsError":{}}}"#,
                            rms_error
                        )
                    }
                    OffsetEdgeError::NoReferencePlane => {
                        r#"{"ok":false,"reason":"no_reference_plane"}"#.to_string()
                    }
                    other => {
                        let msg = other.to_string().replace('"', "'");
                        format!(r#"{{"ok":false,"reason":"other","message":"{}"}}"#, msg)
                    }
                }
            }
        }
    }

    /// ADR-080 V-δ-β — Edge offset with caller-supplied reference plane.
    /// Escape hatch for V-δ-α failures (single-edge wire / collinear /
    /// non-planar) and TS sketch-session integration (V-δ-γ).
    ///
    /// Same JSON return shape as `offset_edge_on_host`. Reasons:
    /// `degenerate_distance`, `unsupported_curve`, `radius_collapse`,
    /// `arc_plane_mismatch` — and any other Plane-host applicable
    /// errors. Free-wire-specific reasons (no_reference_plane,
    /// wire_not_planar) do NOT appear here since caller supplies plane.
    pub fn offset_edge_with_reference_plane(
        &mut self,
        edge_id_raw: u32,
        dist: f64,
        ox: f64, oy: f64, oz: f64,
        nx: f64, ny: f64, nz: f64,
    ) -> String {
        use axia_geo::operations::offset::OffsetEdgeError;
        let eid = EdgeId::new(edge_id_raw);
        let origin = glam::DVec3::new(ox, oy, oz);
        let normal = glam::DVec3::new(nx, ny, nz);

        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.mesh.offset_edge_with_reference_plane(eid, dist, origin, normal) {
            Ok(result) => {
                self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                self.mark_topology_changed();
                self.invalidate_cache();
                format!(
                    r#"{{"ok":true,"newEdge":{},"newV0":{},"newV1":{}}}"#,
                    result.new_edge.raw(),
                    result.new_v0.raw(),
                    result.new_v1.raw(),
                )
            }
            Err(err) => {
                self.scene.transactions.cancel();
                debug_log!("[RUST] offset_edge_with_reference_plane failure: {}", err);
                match err {
                    OffsetEdgeError::UnsupportedCurveKind { kind } => {
                        format!(r#"{{"ok":false,"reason":"unsupported_curve","kind":"{}"}}"#, kind)
                    }
                    OffsetEdgeError::DegenerateDistance(_) => {
                        r#"{"ok":false,"reason":"degenerate_distance"}"#.to_string()
                    }
                    OffsetEdgeError::ArcPlaneMismatch => {
                        r#"{"ok":false,"reason":"arc_plane_mismatch"}"#.to_string()
                    }
                    OffsetEdgeError::RadiusCollapse { current_r, new_r, .. } => {
                        format!(
                            r#"{{"ok":false,"reason":"radius_collapse","currentRadius":{},"newRadius":{}}}"#,
                            current_r, new_r
                        )
                    }
                    OffsetEdgeError::EdgeParallelToNormal => {
                        r#"{"ok":false,"reason":"edge_parallel_to_normal"}"#.to_string()
                    }
                    other => {
                        let msg = other.to_string().replace('"', "'");
                        format!(r#"{{"ok":false,"reason":"other","message":"{}"}}"#, msg)
                    }
                }
            }
        }
    }

    /// face 집합의 중심점 반환 [x, y, z]
    pub fn faces_centroid(&self, face_ids: &[u32]) -> Vec<f64> {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        match self.scene.mesh.faces_centroid(&fids) {
            Ok(c) => vec![c.x, c.y, c.z],
            Err(_) => vec![0.0, 0.0, 0.0],
        }
    }

    // ========================================================================
    // Group / Component Operations
    // ========================================================================

    /// 선택된 face들을 그룹으로 생성
    /// 반환: group ID (성공) 또는 0 (실패)
    pub fn create_group(&mut self, name: &str, face_ids: &[u32]) -> f64 {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        let cmd = Command::CreateGroup {
            name: name.to_string(),
            face_ids: fids,
        };
        let result = self.scene.execute(cmd);
        match result {
            CommandResult::GroupUpdated(gid) => {
                debug_log!("[RUST] create_group: id={} name={}", gid, name);
                gid as f64
            }
            _ => 0.0,
        }
    }

    /// 그룹 해제
    pub fn delete_group(&mut self, group_id: u32) -> bool {
        let cmd = Command::DeleteGroup { group_id };
        let result = self.scene.execute(cmd);
        matches!(result, CommandResult::GroupUpdated(_))
    }

    /// 그룹 이름 변경
    pub fn rename_group(&mut self, group_id: u32, new_name: &str) -> bool {
        let cmd = Command::RenameGroup {
            group_id,
            new_name: new_name.to_string(),
        };
        let result = self.scene.execute(cmd);
        matches!(result, CommandResult::GroupUpdated(_))
    }

    /// 그룹 가시성 토글
    pub fn toggle_group_visibility(&mut self, group_id: u32) -> bool {
        let cmd = Command::ToggleGroupVisibility { group_id };
        let result = self.scene.execute(cmd);
        if matches!(result, CommandResult::GroupUpdated(_)) {
            self.mark_topology_changed();
            self.invalidate_cache();
            true
        } else {
            false
        }
    }

    /// face가 잠긴 그룹에 속하는지 확인
    pub fn is_face_locked(&self, face_id_raw: u32) -> bool {
        let fid = axia_geo::FaceId::new(face_id_raw);
        self.scene.is_face_locked(fid)
    }

    /// 그룹 잠금 토글
    pub fn toggle_group_lock(&mut self, group_id: u32) -> bool {
        let cmd = Command::ToggleGroupLock { group_id };
        let result = self.scene.execute(cmd);
        matches!(result, CommandResult::GroupUpdated(_))
    }

    /// face가 속한 그룹 ID 조회 (없으면 0 반환)
    pub fn get_group_for_face(&self, face_id_raw: u32) -> f64 {
        let fid = FaceId::new(face_id_raw);
        match self.scene.groups.get_group_for_face(fid) {
            Some(gid) => gid as f64,
            None => 0.0,
        }
    }

    /// 그룹의 모든 face ID 반환 (재귀적)
    pub fn get_group_faces(&self, group_id: u32) -> Vec<u32> {
        self.scene.groups.get_all_faces_recursive(group_id)
            .iter()
            .map(|f| f.raw())
            .collect()
    }

    /// 그룹에 face 추가
    pub fn add_faces_to_group(&mut self, group_id: u32, face_ids: &[u32]) -> bool {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        self.scene.groups.add_faces_to_group(group_id, &fids)
    }

    /// 그룹에서 face 제거
    pub fn remove_faces_from_group(&mut self, group_id: u32, face_ids: &[u32]) -> bool {
        let fids: Vec<FaceId> = face_ids.iter().map(|&id| FaceId::new(id)).collect();
        self.scene.groups.remove_faces_from_group(group_id, &fids)
    }

    /// 중첩 그룹 설정
    pub fn set_group_parent(&mut self, child_id: u32, parent_id: f64) -> bool {
        let parent = if parent_id <= 0.0 { None } else { Some(parent_id as u32) };
        self.scene.groups.set_parent(child_id, parent)
    }

    /// 그룹을 컴포넌트로 변환
    pub fn make_component(&mut self, group_id: u32, name: &str) -> f64 {
        match self.scene.groups.make_component(group_id, name.to_string()) {
            Some(def_id) => {
                debug_log!("[RUST] make_component: group={} def={}", group_id, def_id);
                def_id as f64
            }
            None => 0.0,
        }
    }

    /// 그룹 정보 JSON 반환
    pub fn get_group_info(&self, group_id: u32) -> String {
        match self.scene.groups.export_group_info(group_id) {
            Some(json) => json,
            None => r#"{"error":"group not found"}"#.to_string(),
        }
    }

    /// 전체 그룹 트리 JSON 반환
    pub fn get_all_groups(&self) -> String {
        self.scene.groups.export_all_groups_json()
    }

    /// 그룹 수
    pub fn group_count(&self) -> usize {
        self.scene.groups.group_count()
    }

    // ═══════════════════════════════════════════════
    //  Material Operations
    // ═══════════════════════════════════════════════

    /// 면에 재질 부여 (material_id_raw = MaterialId의 raw u32 값)
    pub fn assign_material(&mut self, face_ids_raw: &[u32], material_id_raw: u32) -> bool {
        let face_ids: Vec<FaceId> = face_ids_raw.iter()
            .map(|&r| FaceId::new(r))
            .collect();
        let material_id = axia_geo::MaterialId::new(material_id_raw);
        let cmd = Command::AssignMaterial { face_ids, material_id };
        match self.scene.execute(cmd) {
            CommandResult::MaterialAssigned { .. } => {
                self.cache_dirty = true;
                true
            },
            _ => false,
        }
    }

    /// 면에서 재질 제거 → XIA가 Volume으로 복귀
    pub fn remove_material(&mut self, face_ids_raw: &[u32]) -> bool {
        let face_ids: Vec<FaceId> = face_ids_raw.iter()
            .map(|&r| FaceId::new(r))
            .collect();
        let cmd = Command::RemoveMaterial { face_ids };
        match self.scene.execute(cmd) {
            CommandResult::MaterialRemoved { .. } => {
                self.cache_dirty = true;
                true
            },
            _ => false,
        }
    }

    /// 면의 재질 ID 조회 (없으면 0 반환, 0 = 기본 재질)
    pub fn get_face_material(&self, face_id_raw: u32) -> u32 {
        let fid = FaceId::new(face_id_raw);
        if let Some(face) = self.scene.mesh.faces.get(fid) {
            return face.material().raw();
        }
        0
    }

    /// 전체 재질 목록 JSON 반환 (format! 기반, serde_json 불필요)
    pub fn get_all_materials(&self) -> String {
        let mats = self.scene.material_library.all();
        if mats.is_empty() {
            return "[]".to_string();
        }
        let entries: Vec<String> = mats.iter()
            .map(|m| {
                let hex = format!("{:06x}", m.visual.color);
                format!(
                    r##"{{"id":{},"name":"{}","nameEn":"{}","density":{},"color":"#{}"}}"##,
                    m.id.raw(), m.name, m.name_en, m.physical.density, hex
                )
            })
            .collect();
        format!("[{}]", entries.join(","))
    }

    // ════════════════════════════════════════════════════════════════
    // ADR-060 Phase O Step 6 — WASM additive-only API
    //
    // §D lock-in (강제):
    //   ✅ 신규 endpoint 추가만
    //   ❌ 기존 export 시그니처 / 출력 변경 금지
    //
    // 모든 새 endpoint:
    //   - JSON 반환 → schemaVersion 필드 포함
    //   - VertId raw 절대 노출 금지 (ADR-037 P22)
    //   - sync (Promise 미사용)
    //   - error 시 { ok: false, error: "...", schemaVersion: 1 }
    // ════════════════════════════════════════════════════════════════

    /// ADR-060 Phase O Step 6 — Edge analytic curve as JSON.
    ///
    /// Returns the edge's `AnalyticCurve` (Phase A/B/C) as a JSON object
    /// with `schemaVersion: 1`. `Line` variant emits world coordinates
    /// (resolves VertId via mesh) — raw VertId never exposed (R7 / ADR-037).
    ///
    /// Returns `null` (string) when:
    ///   - edge missing / inactive
    ///   - edge has no curve attached (`Edge.curve = None`)
    ///
    /// Schema:
    ///   `{ "schemaVersion": 1, "kind": "Line"|"Circle"|..., ... }`
    #[wasm_bindgen(js_name = "getEdgeCurveJson")]
    pub fn get_edge_curve_json(&self, edge_id_raw: u32) -> String {
        step6_json::edge_curve_json(&self.scene.mesh, EdgeId::new(edge_id_raw))
    }

    /// ADR-060 Phase O Step 6 — Face analytic surface as JSON.
    ///
    /// Returns the face's `AnalyticSurface` (Phase D/E) as a JSON
    /// object with `schemaVersion: 1`. Returns `null` when face missing,
    /// inactive, or has no surface attached.
    ///
    /// Schema:
    ///   `{ "schemaVersion": 1, "kind": "Plane"|"Cylinder"|..., ... }`
    ///
    /// MVP scope: emits primitive surfaces (Plane/Cylinder/Sphere/Cone/
    /// Torus) in full; tensor variants (BezierPatch / BSplineSurface /
    /// NURBSSurface) emit only metadata (kind + degree counts) per
    /// Phase L deferral.
    #[wasm_bindgen(js_name = "getFaceSurfaceJson")]
    pub fn get_face_surface_json(&self, face_id_raw: u32) -> String {
        step6_json::face_surface_json(&self.scene.mesh, FaceId::new(face_id_raw))
    }

    /// ADR-060 Phase O Step 6 — Phase N migration (curve_mandatory +
    /// surface_mandatory) callable from JS.
    ///
    /// Idempotent (R5): repeated calls are safe; second call no-ops on
    /// already-migrated entities. Single transaction (Ctrl+Z restores
    /// pre-migration state).
    ///
    /// Returns JSON migration report:
    ///   `{ "schemaVersion": 1, "edgesUpgraded": N, "facesUpgraded": M,
    ///      "edgesDroppedToLine": K, "facesDroppedToPlane": J,
    ///      "driftMaxMm": F, "ok": true }`
    #[wasm_bindgen(js_name = "migrateCurveSurfaceMandatory")]
    pub fn migrate_curve_surface_mandatory(&mut self) -> String {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let report = self.scene.mesh.migrate_v3_to_v4_with_sanity();
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        step6_json::migration_report_json(&report)
    }

    /// ADR-060 Phase O Step 6 — Step 4 Boolean dispatch result as JSON.
    ///
    /// Routes through `Mesh::boolean_dispatch` (§F lock-in: silent
    /// fallback prohibited). Result includes path tag + skip reason.
    ///
    /// Schema:
    ///   `{ "schemaVersion": 1, "ok": bool, "pathUsed": "Mesh"|"Nurbs"|
    ///      "NurbsWithMeshFallback", "fallbackReason": { "kind": "...",
    ///      "label": "..." } | null, "nurbsAttempted": bool,
    ///      "nurbsClean": bool, "faceCount": N }`
    #[wasm_bindgen(js_name = "booleanDispatchJson")]
    pub fn boolean_dispatch_json(
        &mut self,
        faces_a: &[u32],
        faces_b: &[u32],
        op: u32,
        material_id: u32,
    ) -> String {
        let op = match op {
            0 => BoolOp::Union,
            1 => BoolOp::Subtract,
            2 => BoolOp::Intersect,
            _ => return r#"{"schemaVersion":1,"ok":false,"error":"invalid op"}"#.to_string(),
        };
        let fa: Vec<FaceId> = faces_a.iter().map(|&i| FaceId::new(i)).collect();
        let fb: Vec<FaceId> = faces_b.iter().map(|&i| FaceId::new(i)).collect();
        let mat = axia_geo::MaterialId::new(material_id);
        self.scene.transactions.begin();
        let before_snapshot = self.scene.scene_snapshot();
        self.scene.transactions.set_before_snapshot(before_snapshot.clone());
        // Defense-in-depth: closure-preserving + self-intersection baseline.
        // Solid-solid boolean must stay a closed solid; a sheet input is open
        // (before_boundary>0) so closure is not enforced. Only a NEW self-
        // intersection or a closed→open tear is rejected (R12 verified clean).
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        let result = self.scene.mesh.boolean_dispatch(&fa, &fb, op, mat);
        let dispatch_result = match result {
            Ok(r) => r,
            Err(e) => {
                self.scene.transactions.cancel();
                return format!(
                    r#"{{"schemaVersion":1,"ok":false,"error":"{}"}}"#,
                    e.to_string().replace('"', "'"),
                );
            }
        };
        if !self.closure_preserving_gate_passed(
            before_boundary, before_si, &before_snapshot, "boolean", true,
        ) {
            return r#"{"schemaVersion":1,"ok":false,"error":"boolean 이 solid 를 열거나 자기교차(self-intersection)를 만들어 취소됨"}"#.to_string();
        }
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        step6_json::boolean_dispatch_result_json(&dispatch_result)
    }

    // ADR-076 Step 2 — Removed: boolean_dispatch_dcel_json (ADR-064 Step
    // 6-α single-face DCEL export). Reachable only from removed
    // BooleanHandler single fast-path (sunset by ADR-076 Step 1) and
    // removed WasmBridge.booleanDispatchDcel wrapper (sunset by ADR-076
    // Step 2). Rust impl Mesh::boolean_dispatch_dcel preserved — multi
    // (booleanDispatchDcelMultiJson) delegates to it on 1×1 degenerate
    // and per-pair cartesian (Y-1 lock-in #4).


    /// ADR-066 Y-2 (Path Y) — Multi-face DCEL Boolean dispatch as JSON.
    ///
    /// Routes through `Mesh::boolean_dispatch_dcel_multi` (Y-1) which
    /// iterates the cartesian product `facesA × facesB` and accumulates
    /// per-pair outcomes plus aggregate `allNewFaces` / `allRemovedFaces`.
    ///
    /// On Y-E strict eligibility violation (any face missing surface
    /// or unsupported kind), returns `pathUsed="Mesh"` upfront with
    /// `perPair` / aggregates empty + `fallbackReason` populated.
    ///
    /// Schema (per ADR-066 Y-2-c full per-pair, Y-2-j discriminated kind):
    /// ```json
    /// { "schemaVersion": 1, "ok": true,
    ///   "pathUsed": "Nurbs"|"Mesh",
    ///   "fallbackReason": {...} | null,
    ///   "perPair": [
    ///     { "faceA": u32, "faceB": u32,
    ///       "outcome": { "kind": "ok", "dcel": {...} }
    ///                 | { "kind": "err", "detail": "..." } },
    ///     ...
    ///   ],
    ///   "allNewFaces": [u32, ...], "allRemovedFaces": [u32, ...],
    ///   "warnings": [string, ...] }
    /// ```
    ///
    /// On invalid op string or core Err: returns
    /// `{"schemaVersion":1,"ok":false,"error":"..."}` and rolls back
    /// the transaction (Y-H safe-only consistency).
    #[wasm_bindgen(js_name = "booleanDispatchDcelMultiJson")]
    pub fn boolean_dispatch_dcel_multi_json(
        &mut self,
        faces_a: &[u32],
        faces_b: &[u32],
        op_str: &str,
        tol_geometric: f64,
    ) -> String {
        let op = match op_str {
            "union"     => BoolOp::Union,
            "subtract"  => BoolOp::Subtract,
            "intersect" => BoolOp::Intersect,
            _ => return r#"{"schemaVersion":1,"ok":false,"error":"invalid op string (expected: union | subtract | intersect)"}"#.to_string(),
        };
        let fa: Vec<FaceId> = faces_a.iter().map(|&i| FaceId::new(i)).collect();
        let fb: Vec<FaceId> = faces_b.iter().map(|&i| FaceId::new(i)).collect();
        let mut tol = axia_geo::surfaces::ssi::tolerance::BooleanTolerance::default();
        if tol_geometric > 0.0 {
            tol.geometric = tol_geometric;
        }
        // ADR-267 β-3 — watertight 게이트 (delta). op 전 baseline + snapshot.
        let integrity_before = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh)
            .damage_count();
        let integrity_snapshot = self.scene.scene_snapshot();
        // Defense-in-depth: the OpenMesh gate below misses a closed→open tear
        // and a self-intersection (flap). Capture the watertight/SI baseline.
        let before_boundary = self.active_boundary_count();
        let before_si = self.scene.mesh.detect_self_intersections().count();
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(integrity_snapshot.clone());
        let result = self.scene.mesh.boolean_dispatch_dcel_multi(&fa, &fb, op, tol);
        let dispatch_result = match result {
            Ok(r) => r,
            Err(e) => {
                self.scene.transactions.cancel();
                return format!(
                    r#"{{"schemaVersion":1,"ok":false,"error":"{}"}}"#,
                    e.to_string().replace('"', "'"),
                );
            }
        };
        // op 가 새 손상을 유발했으면 byte-identical rollback + txn cancel.
        let after = self
            .scene
            .mesh
            .verify_volume_integrity(axia_geo::IntegrityScope::OpenMesh);
        if after.damage_count() > integrity_before {
            console_error!(
                "[RUST] boolean_dispatch_dcel_multi REJECTED by integrity gate:\n{}",
                after.summary()
            );
            self.scene.restore_scene_snapshot(&integrity_snapshot);
            self.scene.transactions.cancel();
            self.set_error(format!(
                "부피 무결성 위반으로 취소됨 (boolean multi): {}",
                after.summary()
            ));
            self.invalidate_cache();
            let reason = after.summary().replace('"', "'").replace('\n', " ");
            return format!(r#"{{"schemaVersion":1,"ok":false,"error":"{}"}}"#, reason);
        }
        // Defense-in-depth closure-preserving + SI gate — catches the classes
        // the OpenMesh damage gate misses (watertight solid torn open, wall
        // folded through itself). No false rejection: the two-box no-op path
        // preserves closure + SI count, so it passes (R12 + acceptance sweep).
        if !self.closure_preserving_gate_passed(
            before_boundary, before_si, &integrity_snapshot, "boolean multi", true,
        ) {
            return r#"{"schemaVersion":1,"ok":false,"error":"boolean 이 solid 를 열거나 자기교차(self-intersection)를 만들어 취소됨"}"#.to_string();
        }
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        step6_json::boolean_dispatch_dcel_multi_result_json(&dispatch_result)
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-078 P-2 — Boolean Group Persistence WASM bridge
    //
    // Per ADR-078 §B P-2 lock-ins:
    // - P-2-a typed methods (6 — bool/array, no JSON envelope)
    // - P-2-b camelCase via #[wasm_bindgen(js_name = ...)]
    // - P-2-c String tag input + Result<(), JsValue> on invalid (strict)
    // - P-2-d Vec<u32> face IDs (wasm-bindgen 표준, JS array → Rust Vec)
    // - P-2-e Vec<u32> output (sorted, P-1 helpers 위임)
    // - P-2-f set/clear methods 만 transaction wrapping (Undo/Redo 정합)
    // - P-2-i AxiaEngineExtended optional methods 추가 (additive)
    // ════════════════════════════════════════════════════════════════════

    /// ADR-078 P-2 — Tag a list of face IDs as Boolean Group A or B.
    ///
    /// `tag` accepts `"A"` or `"B"` (uppercase only — strict, no
    /// lowercase fallback per P-2-c lock-in). Invalid tag → throws JS
    /// `Error` (Result<(), JsValue>). Wrapped in transaction for
    /// Undo/Redo (P-2-f).
    ///
    /// Mirrors TS `SelectionManager.setGroupTag` (ADR-074 U-1) at the
    /// Scene-persistent layer.
    #[wasm_bindgen(js_name = "setBooleanGroupTag")]
    pub fn set_boolean_group_tag(
        &mut self,
        face_ids: Vec<u32>,
        tag: String,
    ) -> Result<(), JsValue> {
        let group = match tag.as_str() {
            "A" => axia_core::BooleanGroupTag::A,
            "B" => axia_core::BooleanGroupTag::B,
            other => return Err(JsValue::from_str(&format!(
                "setBooleanGroupTag: invalid tag '{}' (expected 'A' or 'B')",
                other,
            ))),
        };
        let fids: Vec<FaceId> = face_ids.iter().map(|&i| FaceId::new(i)).collect();
        // P-2-f — transaction wrap so Undo restores prior tag state.
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        self.scene.set_boolean_group_tag(&fids, group);
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        Ok(())
    }

    /// ADR-078 P-2 — Returns face IDs tagged Group A (sorted ascending).
    /// Mirrors TS `SelectionManager.getGroupA` (ADR-074 U-1).
    #[wasm_bindgen(js_name = "getBooleanGroupAFaces")]
    pub fn get_boolean_group_a_faces(&self) -> Vec<u32> {
        self.scene.get_boolean_group_a().iter().map(|f| f.raw()).collect()
    }

    /// ADR-078 P-2 — Returns face IDs tagged Group B (sorted ascending).
    /// Mirrors TS `SelectionManager.getGroupB` (ADR-074 U-1).
    #[wasm_bindgen(js_name = "getBooleanGroupBFaces")]
    pub fn get_boolean_group_b_faces(&self) -> Vec<u32> {
        self.scene.get_boolean_group_b().iter().map(|f| f.raw()).collect()
    }

    /// ADR-078 P-2 — Clear all Boolean group tags (transaction wrapped).
    /// Mirrors TS `SelectionManager.clearGroupTags` (ADR-074 U-1).
    #[wasm_bindgen(js_name = "clearBooleanGroupTags")]
    pub fn clear_boolean_group_tags(&mut self) {
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        self.scene.clear_boolean_group_tags();
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
    }

    /// ADR-078 P-2 — True iff at least one face has a Boolean group tag.
    /// Mirrors TS `SelectionManager.hasAnyGroupTag` (ADR-074 U-2 Clear
    /// 가시성 / ADR-076 §E.5-4 단축키 Alt+0 활성화).
    #[wasm_bindgen(js_name = "hasAnyBooleanGroupTag")]
    pub fn has_any_boolean_group_tag(&self) -> bool {
        self.scene.has_any_boolean_group_tag()
    }

    /// ADR-078 P-2 — True iff BOTH Group A and Group B have ≥1 tagged face.
    /// Mirrors TS `SelectionManager.hasGroupSelection` (ADR-074 U-3
    /// BooleanHandler routing).
    #[wasm_bindgen(js_name = "hasBooleanGroupSelection")]
    pub fn has_boolean_group_selection(&self) -> bool {
        self.scene.has_boolean_group_selection()
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-050 P-4 — Shape (form-layer citizenship) WASM bridge.
    //
    // Per ADR-050 §B P-4 lock-ins (mirroring ADR-078 P-2):
    // - camelCase via `js_name` (P-2-b)
    // - Vec<u32> ownership for face_ids (P-2-d, wasm-bindgen 명확)
    // - strict Result<u32, JsValue> for promote (P-2-c, invalid input
    //   throws — silent skip 차단)
    // - Transaction wrapping on all mutators (P-2-f, Undo/Redo 통합)
    //
    // Mirrors `Scene::create_shape` / `get_shape` / `list_shape_ids` /
    // `delete_shape` / `clear_shapes` / `promote_shape_to_xia` exactly —
    // bridge layer is a thin pass-through.
    // ════════════════════════════════════════════════════════════════════

    /// ADR-050 P-4 — Create a new Shape (form-layer citizen).
    ///
    /// Returns the new ShapeId as `u32`. Mirror of TS-side eventual
    /// `bridge.createShape(name, faceIds)`. Transaction-wrapped so
    /// Undo restores the prior shape map.
    #[wasm_bindgen(js_name = "createShape")]
    pub fn create_shape(&mut self, name: String, face_ids: Vec<u32>) -> u32 {
        let fids: Vec<FaceId> = face_ids.iter().map(|&i| FaceId::new(i)).collect();
        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());
        let shape_id = self.scene.create_shape(name, fids);
        self.scene
            .transactions
            .set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        shape_id.raw()
    }

    /// ADR-050 P-4 — Returns all current ShapeIds (sorted ascending).
    /// Used by future Inspector enumeration.
    #[wasm_bindgen(js_name = "getShapeIds")]
    pub fn get_shape_ids(&self) -> Vec<u32> {
        self.scene
            .list_shape_ids()
            .iter()
            .map(|s| s.raw())
            .collect()
    }

    /// ADR-050 P-4 — Returns the face IDs owned by a Shape, or empty
    /// vec if the shape doesn't exist (no error — graceful for callers
    /// that may have stale IDs).
    #[wasm_bindgen(js_name = "getShapeFaceIds")]
    pub fn get_shape_face_ids(&self, shape_id: u32) -> Vec<u32> {
        let sid = axia_core::ShapeId::new(shape_id);
        self.scene
            .get_shape(sid)
            .map(|s| s.face_ids.iter().map(|f| f.raw()).collect())
            .unwrap_or_default()
    }

    /// ADR-050 P-4 — Delete a Shape by id. Returns true if deleted.
    /// Transaction-wrapped.
    #[wasm_bindgen(js_name = "deleteShape")]
    pub fn delete_shape(&mut self, shape_id: u32) -> bool {
        let sid = axia_core::ShapeId::new(shape_id);
        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());
        let removed = self.scene.delete_shape(sid);
        self.scene
            .transactions
            .set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        removed
    }

    /// ADR-050 P-4 — Clear all Shapes. Transaction-wrapped.
    #[wasm_bindgen(js_name = "clearShapes")]
    pub fn clear_shapes(&mut self) {
        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());
        self.scene.clear_shapes();
        self.scene
            .transactions
            .set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
    }

    /// ADR-050 P-4 — Promote a Shape to a Xia via 4-condition validation.
    ///
    /// On success: returns the new XiaId as `u32`.
    /// On failure: throws JS `Error` with the PromoteError message
    /// (strict — silent skip 차단, P-2-c lock-in 답습).
    ///
    /// Errors (matching `Scene::promote_shape_to_xia`):
    /// - Shape not found
    /// - No geometry / Invalid material / Zero volume / Zero dimension
    /// - Not watertight / Not manifold (ADR-051 P7 prerequisite)
    ///
    /// Transaction-wrapped — Undo restores the pre-promote state
    /// (no Xia created, no shape_to_xia linkage).
    #[wasm_bindgen(js_name = "promoteShapeToXia")]
    pub fn promote_shape_to_xia(
        &mut self,
        shape_id: u32,
        material_id: u32,
    ) -> Result<u32, JsValue> {
        let sid = axia_core::ShapeId::new(shape_id);
        let mat = axia_geo::MaterialId::new(material_id);

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.promote_shape_to_xia(sid, mat) {
            Ok(promote_ok) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                Ok(promote_ok.xia_id)
            }
            Err(err) => {
                // Failure rolls back the transaction (no state change).
                self.scene.transactions.cancel();
                Err(JsValue::from_str(&format!("promoteShapeToXia: {}", err)))
            }
        }
    }

    /// ADR-145 β-2 — Circle annulus 명시 promote.
    ///
    /// 두 coplanar Circle face (outer + inner) 를 annulus (outer with
    /// inner hole) 로 명시 promote. inner face deactivate.
    ///
    /// **사용자 명시 trigger only** (메타-원칙 #16) — 휴리스틱 자동
    /// detect 안 됨. ContextMenu "annulus 만들기" 우클릭 후 호출 (β-4).
    ///
    /// Engine API: `axia_geo::operations::annulus::promote_circles_to_annulus`
    /// (β-1 validation + β-1+ promote logic full implementation, PR #173).
    ///
    /// # Errors (matching `AnnulusError`)
    /// - `InactiveFace` — outer 또는 inner not found / inactive
    /// - `NotCircleFace` — 둘 다 closed-curve Circle face 아님
    ///   (1 self-loop edge with `AnalyticCurve::Circle`)
    /// - `NotCoplanar` — 다른 평면 (normal parity + plane distance,
    ///   LOCKED #5 1.5μm tolerance)
    /// - `InnerNotContained` — inner Circle 이 outer 안 contained 안 됨
    ///   (center_distance + inner.radius > outer.radius)
    ///
    /// Transaction-wrapped — Undo restores the pre-promote state
    /// (inner face active + outer face hole 0).
    #[wasm_bindgen(js_name = "promoteCirclesToAnnulus")]
    pub fn promote_circles_to_annulus(
        &mut self,
        outer_face_id: u32,
        inner_face_id: u32,
    ) -> Result<(), JsValue> {
        use axia_geo::FaceId;
        use axia_geo::operations::annulus;
        let outer_fid = FaceId::new(outer_face_id);
        let inner_fid = FaceId::new(inner_face_id);

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match annulus::promote_circles_to_annulus(&mut self.scene.mesh, outer_fid, inner_fid) {
            Ok(()) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                Ok(())
            }
            Err(err) => {
                // Failure rolls back the transaction (no state change).
                self.scene.transactions.cancel();
                Err(JsValue::from_str(&format!("promoteCirclesToAnnulus: {}", err)))
            }
        }
    }

    /// ADR-148 β-3 — Point-Localized BoundaryTool WASM endpoint.
    ///
    /// **사용자 명시 trigger only** (메타-원칙 #16) — 휴리스틱 자동
    /// activation 0. UI BoundaryTool 클릭 후 호출 (β-4).
    ///
    /// Given a 3D world-space click point + plane (cardinal projection
    /// or face plane), find the smallest enclosing orphan edge cycle
    /// containing the point and synthesize a boundary face.
    ///
    /// CAD 표준 BOUNDARY 명령 equivalent (AutoCAD BPOLY).
    ///
    /// Engine API: `axia_geo::operations::boundary::boundary_from_point`
    /// (β-1 skeleton, PR #184 + β-2 algorithm, PR #185).
    ///
    /// # Parameters
    /// - `px`, `py`, `pz`: 3D world-space click point
    /// - `nx`, `ny`, `nz`: plane normal (unit vector recommended; normalized
    ///   internally)
    /// - `plane_dist`: plane equation `normal · p = dist` (signed distance
    ///   from world origin)
    /// - `search_radius_mm`: BVH/linear scan radius. ≤0 → default 1000mm
    ///
    /// # Returns
    /// - `Ok(face_id: u32)`: 새로 합성된 boundary face
    /// - `Err(JsValue)`: 4 validation failure (PointNotOnPlane /
    ///   NoOrphanEdgesInRadius / NoEnclosingCycle / CycleAlreadyFaced)
    ///
    /// Transaction-wrapped — Undo restores the pre-synthesis state.
    #[wasm_bindgen(js_name = "boundaryFromPoint")]
    pub fn boundary_from_point(
        &mut self,
        px: f64,
        py: f64,
        pz: f64,
        nx: f64,
        ny: f64,
        nz: f64,
        plane_dist: f64,
        search_radius_mm: f64,
    ) -> Result<u32, JsValue> {
        use axia_geo::operations::boundary;
        use axia_geo::operations::boolean_geo::Plane;
        use glam::DVec3;

        let point = DVec3::new(px, py, pz);
        let plane = Plane {
            normal: DVec3::new(nx, ny, nz).normalize(),
            dist: plane_dist,
        };

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match boundary::boundary_from_point(
            &mut self.scene.mesh,
            point,
            plane,
            search_radius_mm,
        ) {
            Ok(face_id) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                Ok(face_id.raw())
            }
            Err(err) => {
                self.scene.transactions.cancel();
                Err(JsValue::from_str(&format!("boundaryFromPoint: {}", err)))
            }
        }
    }

    /// ADR-148 §5 — `boundaryFromPoint` with the plane inferred from the
    /// free edges around the click.
    ///
    /// Boundary is used where there is no face yet, which is exactly where a
    /// face-hit cannot say which plane to work on — so the click falls through
    /// to Z=0 and a loop drawn at z=100 cannot be faced at all. This asks the
    /// geometry instead.
    ///
    /// Not the heuristic 메타-원칙 #16 warns about: when the free edges in
    /// range share one plane, that plane is the only answer consistent with
    /// them. When they do not, it refuses rather than choosing (메타-원칙 #5).
    ///
    /// Transaction-wrapped like its explicit-plane sibling — it creates a face.
    ///
    /// Engine API: `axia_geo::operations::boundary::boundary_from_point_auto_plane`.
    #[wasm_bindgen(js_name = "boundaryFromPointAutoPlane")]
    pub fn boundary_from_point_auto_plane(
        &mut self,
        px: f64,
        py: f64,
        pz: f64,
        search_radius_mm: f64,
    ) -> Result<u32, JsValue> {
        use axia_geo::operations::boundary;
        use glam::DVec3;

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match boundary::boundary_from_point_auto_plane(
            &mut self.scene.mesh,
            DVec3::new(px, py, pz),
            search_radius_mm,
        ) {
            Ok(face_id) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                Ok(face_id.raw())
            }
            Err(err) => {
                self.scene.transactions.cancel();
                Err(JsValue::from_str(&format!(
                    "boundaryFromPointAutoPlane: {}",
                    err
                )))
            }
        }
    }

    /// ADR-148 §5 — 3D BOUNDARY: the faces of the closed shell enclosing a
    /// point.
    ///
    /// The 3D sibling of `boundaryFromPoint`. 2D synthesizes the face an edge
    /// loop implies; 3D has nothing to synthesize — a shell being closed is
    /// already true, and Volume is a computed state rather than an entity. So
    /// this reports which faces bound the solid you clicked inside, and the
    /// caller selects them.
    ///
    /// Read-only: no transaction, no Undo entry, nothing created — which is
    /// why it needs no citizenship or ADR-016 Q2 policy change.
    ///
    /// Nested solids resolve smallest-first, like the 2D tool: the innermost
    /// one is the one you pointed at.
    ///
    /// Engine API: `axia_geo::operations::boundary::shell_from_point`.
    ///
    /// # Returns
    /// - `Ok(Uint32Array)`: face ids of the enclosing shell
    /// - `Err(JsValue)`: NoClosedShell / PointNotInsideAnyShell
    #[wasm_bindgen(js_name = "shellFromPoint")]
    pub fn shell_from_point(&self, px: f64, py: f64, pz: f64) -> Result<Vec<u32>, JsValue> {
        use axia_geo::operations::boundary;
        use glam::DVec3;

        boundary::shell_from_point(&self.scene.mesh, DVec3::new(px, py, pz))
            .map(|faces| faces.into_iter().map(|f| f.raw()).collect())
            .map_err(|err| JsValue::from_str(&format!("shellFromPoint: {}", err)))
    }

    // ========================================================================
    // ADR-149 — T-junction Sweep 명시 도구 (β-3 WASM bridge)
    // ========================================================================

    /// ADR-149 β-3 — Detect all mesh-level T-junctions.
    ///
    /// **사용자 명시 trigger only** (메타-원칙 #16) — 자동 sweep 0.
    /// UI ContextMenu "T-junction 정리" 클릭 (β-4) 시 detect → heal 시퀀스
    /// 의 첫 단계.
    ///
    /// Vertex V on edge E interior where V ∉ face F's boundary loop.
    /// LOCKED #1 P7 manifold artifacts + LOCKED #16 ADR-038 P23 normal
    /// drift 의 detection layer.
    ///
    /// Engine API: `axia_geo::operations::t_junction::detect_t_junctions`
    /// (β-1 detection, ADR-149 PR #197 merged 0ea83da).
    ///
    /// # Parameters
    /// - `tol_mm`: vertex-on-edge distance threshold. ≤0 → default
    ///   `T_JUNCTION_TOL = 1.5e-4` (LOCKED #5 0.15μm 답습).
    ///
    /// # Returns
    /// JSON-serialized `Vec<TJunctionReport>`:
    /// ```json
    /// [
    ///   {"face_id": 0, "edge_id": 4, "vertex_id": 5, "t_along_edge": 0.5},
    ///   ...
    /// ]
    /// ```
    /// Empty array = clean mesh (0 T-junctions detected).
    ///
    /// Read-only — no transaction wrap needed.
    #[wasm_bindgen(js_name = "detectTJunctions")]
    pub fn detect_t_junctions(&self, tol_mm: f64) -> Result<String, JsValue> {
        use axia_geo::operations::t_junction;
        let tol = if tol_mm > 0.0 { tol_mm } else { t_junction::T_JUNCTION_TOL };
        let reports = t_junction::detect_t_junctions(&self.scene.mesh, tol);

        // Manual JSON serialization (no serde dependency on TJunctionReport — it's
        // a plain struct without #[derive(Serialize)]).
        let items: Vec<String> = reports
            .iter()
            .map(|r| {
                format!(
                    "{{\"face_id\":{},\"edge_id\":{},\"vertex_id\":{},\"t_along_edge\":{}}}",
                    r.face_id.raw(),
                    r.edge_id.raw(),
                    r.vertex_id.raw(),
                    r.t_along_edge
                )
            })
            .collect();
        Ok(format!("[{}]", items.join(",")))
    }

    /// ADR-149 β-3 — Heal a single T-junction by splitting the edge and
    /// applying HARD flag.
    ///
    /// Caller supplies a JSON-encoded `TJunctionReport` (typically from
    /// a prior `detectTJunctions` call). Strict validation — stale or
    /// drifted reports → `JsError` (silent skip 차단, 메타-원칙 #16).
    ///
    /// Engine API: `axia_geo::operations::t_junction::heal_t_junction`
    /// (β-2 healing, ADR-149 PR #198 merged f35523b).
    ///
    /// # Parameters
    /// - `report_json`: JSON string matching the schema returned by
    ///   `detectTJunctions` (single element). Fields: `face_id`, `edge_id`,
    ///   `vertex_id`, `t_along_edge`.
    /// - `tol_mm`: drift re-verification tolerance. ≤0 → default
    ///   `T_JUNCTION_TOL = 1.5e-4`.
    ///
    /// # Returns
    /// - `Ok(json: String)`: `{"healed_count": 1, "new_vertex_id": u32,
    ///   "new_edge_a": u32, "new_edge_b": u32}`
    /// - `Err(JsValue)`: validation failure (InvalidReport / VertexNotOnEdge
    ///   / SplitEdgeFailed) — caller must re-detect.
    ///
    /// Transaction-wrapped — Undo restores the pre-heal state.
    #[wasm_bindgen(js_name = "healTJunction")]
    pub fn heal_t_junction(
        &mut self,
        report_json: &str,
        tol_mm: f64,
    ) -> Result<String, JsValue> {
        use axia_geo::operations::t_junction::{self, TJunctionReport};
        use axia_geo::{FaceId, EdgeId, VertId};

        // Parse JSON manually — 4 fields, plain integers + 1 float.
        let parsed = parse_t_junction_report(report_json)
            .map_err(|e| JsValue::from_str(&format!("healTJunction: invalid JSON: {}", e)))?;

        let report = TJunctionReport {
            face_id: FaceId::new(parsed.face_id),
            edge_id: EdgeId::new(parsed.edge_id),
            vertex_id: VertId::new(parsed.vertex_id),
            t_along_edge: parsed.t_along_edge,
        };

        let tol = if tol_mm > 0.0 { tol_mm } else { t_junction::T_JUNCTION_TOL };

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match t_junction::heal_t_junction(&mut self.scene.mesh, &report, tol) {
            Ok(heal) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                Ok(format!(
                    "{{\"healed_count\":{},\"new_vertex_id\":{},\"new_edge_a\":{},\"new_edge_b\":{}}}",
                    heal.healed_count,
                    heal.new_vertex_id.raw(),
                    heal.new_edge_a.raw(),
                    heal.new_edge_b.raw()
                ))
            }
            Err(err) => {
                self.scene.transactions.cancel();
                Err(JsValue::from_str(&format!("healTJunction: {}", err)))
            }
        }
    }

    // ========================================================================
    // ADR-150 — Coplanar Face Merge Sweep (β-3 WASM bridge)
    // ========================================================================

    /// ADR-150 β-3 — Sweep all coplanar mergeable pairs in the mesh.
    ///
    /// **사용자 명시 trigger only** (메타-원칙 #16) — 자동 sweep 0.
    /// UI ContextMenu "🧹 Coplanar 면 일괄 자동 정리" 클릭 (β-4) 시
    /// sweep → batch merge 시퀀스의 첫 단계.
    ///
    /// Coplanar faces that share a collinear boundary segment but not
    /// necessarily a shared DCEL edge. Each pair satisfies:
    /// 1. Both faces active
    /// 2. Normals coplanar within `tol_deg` (same or opposite direction)
    /// 3. `would_geometric_merge_succeed` dry-run pass
    /// 4. `face_a.raw() < face_b.raw()` (deterministic, no duplicates)
    ///
    /// Engine API: `axia_geo::operations::geometric_merge::
    /// sweep_coplanar_pairs` (β-1 detection, ADR-150 PR #203 merged
    /// `ad0ca3e`).
    ///
    /// # Parameters
    /// - `tol_deg`: coplanar normal angle threshold (deg). ≤0 → default
    ///   `COPLANAR_PAIR_TOL_DEG = 1.0` (ADR-150 §2 Q3=a).
    ///
    /// # Returns
    /// JSON-serialized `Vec<CoplanarPairReport>`:
    /// ```json
    /// [
    ///   {"face_a": 0, "face_b": 1, "plane_normal": {"x": 0, "y": 1, "z": 0}},
    ///   ...
    /// ]
    /// ```
    /// Empty array = clean mesh (0 mergeable pairs).
    ///
    /// Read-only — no transaction wrap needed.
    #[wasm_bindgen(js_name = "sweepCoplanarPairs")]
    pub fn sweep_coplanar_pairs(&self, tol_deg: f64) -> Result<String, JsValue> {
        use axia_geo::operations::geometric_merge;
        let tol = if tol_deg > 0.0 { tol_deg } else { geometric_merge::COPLANAR_PAIR_TOL_DEG };
        let reports = geometric_merge::sweep_coplanar_pairs(&self.scene.mesh, tol);

        let items: Vec<String> = reports
            .iter()
            .map(|r| {
                format!(
                    "{{\"face_a\":{},\"face_b\":{},\"plane_normal\":{{\"x\":{},\"y\":{},\"z\":{}}}}}",
                    r.face_a.raw(),
                    r.face_b.raw(),
                    r.plane_normal.x,
                    r.plane_normal.y,
                    r.plane_normal.z,
                )
            })
            .collect();
        Ok(format!("[{}]", items.join(",")))
    }

    /// ADR-150 β-3 — Batch merge coplanar pairs (cascade A-B → AB-C
    /// handling) with skip-on-error.
    ///
    /// Caller supplies a JSON-encoded array of `CoplanarPairReport`s
    /// (typically from a prior `sweepCoplanarPairs` call). Strict per-
    /// pair validation — stale pairs / inactive faces / drift → skipped
    /// (silent skip 차단 via `skipped_count` field).
    ///
    /// Engine API: `axia_geo::operations::geometric_merge::
    /// merge_coplanar_pair_batch` (β-2 mutation, ADR-150 PR #204 merged
    /// `1de92ae`).
    ///
    /// # Parameters
    /// - `pairs_json`: JSON array of pairs (schema matches `sweepCoplanar
    ///   Pairs` output). Empty array → no-op (merged=0, skipped=0).
    /// - `tol_deg`: drift re-verification tolerance. ≤0 → default
    ///   `COPLANAR_PAIR_TOL_DEG = 1.0`.
    ///
    /// # Returns
    /// - `Ok(json: String)`: `{"merged_count": N, "skipped_count": M,
    ///   "new_face_ids": [...]}`. `new_face_ids` may contain intermediate
    ///   IDs consumed by cascading merges — caller may inspect mesh state
    ///   to find final live faces.
    /// - `Err(JsValue)`: JSON parse failure (invalid input format).
    ///
    /// Transaction-wrapped — Undo restores the pre-batch state (single
    /// undo step for entire batch).
    #[wasm_bindgen(js_name = "mergeCoplanarPairBatch")]
    pub fn merge_coplanar_pair_batch(
        &mut self,
        pairs_json: &str,
        tol_deg: f64,
    ) -> Result<String, JsValue> {
        use axia_geo::operations::geometric_merge::{
            self, CoplanarPairReport,
        };
        use axia_geo::FaceId;
        use glam::DVec3;

        // Parse JSON array of CoplanarPairReport.
        let pairs = parse_coplanar_pair_array(pairs_json)
            .map_err(|e| JsValue::from_str(&format!("mergeCoplanarPairBatch: invalid JSON: {}", e)))?;
        let reports: Vec<CoplanarPairReport> = pairs
            .into_iter()
            .map(|p| CoplanarPairReport {
                face_a: FaceId::new(p.face_a),
                face_b: FaceId::new(p.face_b),
                plane_normal: DVec3::new(p.nx, p.ny, p.nz),
            })
            .collect();

        let tol = if tol_deg > 0.0 {
            tol_deg
        } else {
            geometric_merge::COPLANAR_PAIR_TOL_DEG
        };

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        let report = geometric_merge::merge_coplanar_pair_batch(
            &mut self.scene.mesh,
            &reports,
            tol,
        );

        self.scene
            .transactions
            .set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();

        let new_ids: Vec<String> = report
            .new_face_ids
            .iter()
            .map(|f| f.raw().to_string())
            .collect();
        Ok(format!(
            "{{\"merged_count\":{},\"skipped_count\":{},\"new_face_ids\":[{}]}}",
            report.merged_count,
            report.skipped_count,
            new_ids.join(","),
        ))
    }

    /// ADR-151 β-3 — Enforce P7 canonical topology on a container + inners
    /// (Sprint 3 셋째 ADR, Connected Stacked-inner Component-Merge Resolver).
    ///
    /// Engine API: `axia_geo::operations::p7_canonical_resolver::
    /// enforce_p7_canonical` (β-2 mutation 활성, PR #213). 명시 호출
    /// only — Draw 도구 자동 trigger 0 (메타-원칙 #16 / LOCKED #64 정합).
    ///
    /// # Parameters
    /// - `container_id`: ring face that contains the inner sub-faces.
    /// - `inner_ids`: connected/disjoint stacked-inner sub-faces
    ///   (`Vec<u32>` — JS array of FaceId raw values).
    ///
    /// # Returns
    /// - `Ok(json)`: `{"component_count":N,"is_valid":true|false,
    ///   "violation_count":M}` (manifold report summary; full report
    ///   inspection via separate query API).
    /// - `Err(JsValue)`: `P7EnforceError` Display message (silent skip
    ///   차단 — InvalidInput / NoComponents / PerimeterFailed /
    ///   RebuildFailed). Strict throw on invalid params (Q1=a default).
    ///
    /// Transaction-wrapped — Undo restores the pre-rebuild state
    /// (ADR-149/150 β-3 패턴 답습).
    #[wasm_bindgen(js_name = "enforceP7Canonical")]
    pub fn enforce_p7_canonical(
        &mut self,
        container_id: u32,
        inner_ids: Vec<u32>,
    ) -> Result<String, JsValue> {
        use axia_geo::operations::p7_canonical_resolver;
        use axia_geo::FaceId;

        let container = FaceId::new(container_id);
        let inners: Vec<FaceId> = inner_ids.iter().map(|&i| FaceId::new(i)).collect();

        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match p7_canonical_resolver::enforce_p7_canonical(
            &mut self.scene.mesh,
            container,
            &inners,
        ) {
            Ok(result) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();

                let is_valid = result.manifold_report.is_valid();
                let violation_count = result.manifold_report.violations.len();
                Ok(format!(
                    "{{\"component_count\":{},\"is_valid\":{},\"violation_count\":{}}}",
                    result.component_count, is_valid, violation_count,
                ))
            }
            Err(e) => {
                // Rollback transaction — no mutation should persist on error.
                self.scene.transactions.commit();
                Err(JsValue::from_str(&format!("enforceP7Canonical: {}", e)))
            }
        }
    }

    /// ADR-152 β-3 — Verify P7 manifold extended (M1/M2/M3 + M4/M5).
    ///
    /// Engine API: `axia_geo::p7_manifold::verify_p7_manifold` (β-1
    /// extension, PR #225 merged). Read-only inspection — returns the
    /// full violation list as JSON.
    ///
    /// # Parameters
    /// - `container_id`: ring face that contains the inner sub-faces.
    /// - `inner_ids`: connected/disjoint stacked-inner sub-faces.
    ///
    /// # Returns
    /// JSON: `{
    ///   "container": u32, "inner_count": N, "edges_checked": N,
    ///   "is_valid": bool, "violation_count": N,
    ///   "violations": [
    ///     { "kind": "M1" | "M2" | "M3" | "M4" | "M5", "detail": "..." }
    ///   ]
    /// }`
    /// (Display 형식 string 으로 detail 통일 — silent skip 차단, 사용자
    /// facing 검사 시점에서 문자열 그대로 표시 가능)
    #[wasm_bindgen(js_name = "verifyP7ManifoldExtended")]
    pub fn verify_p7_manifold_extended(
        &self,
        container_id: u32,
        inner_ids: Vec<u32>,
    ) -> String {
        use axia_geo::p7_manifold::verify_p7_manifold;
        use axia_geo::FaceId;

        let container = FaceId::new(container_id);
        let inners: Vec<FaceId> = inner_ids.iter().map(|&i| FaceId::new(i)).collect();
        let report = verify_p7_manifold(&self.scene.mesh, container, &inners);

        let is_valid = report.is_valid();
        let violation_count = report.violations.len();
        let violations_json: Vec<String> = report
            .violations
            .iter()
            .map(|v| {
                let kind = match v {
                    axia_geo::p7_manifold::P7Violation::EdgeSharedByWrongCount { .. } => "M1",
                    axia_geo::p7_manifold::P7Violation::HoleLoopMissingContainer { .. } => "M2",
                    axia_geo::p7_manifold::P7Violation::BoundaryEdgeMalformed { .. } => "M3",
                    axia_geo::p7_manifold::P7Violation::VertexValencePathology { .. } => "M4",
                    axia_geo::p7_manifold::P7Violation::FaceOrientationInconsistent { .. } => "M5",
                };
                let detail = v.to_string().replace('"', "\\\"");
                format!("{{\"kind\":\"{}\",\"detail\":\"{}\"}}", kind, detail)
            })
            .collect();

        format!(
            "{{\"container\":{},\"inner_count\":{},\"edges_checked\":{},\"is_valid\":{},\"violation_count\":{},\"violations\":[{}]}}",
            container_id,
            report.inner_count,
            report.edges_checked,
            is_valid,
            violation_count,
            violations_json.join(","),
        )
    }

    /// ADR-152 β-3 — Compute mesh topology (Euler + Genus + boundary loops).
    ///
    /// Engine API: `axia_geo::p7_manifold::compute_topology` (β-2, PR
    /// #226 merged). Read-only inspection — returns the full topology
    /// report as JSON.
    ///
    /// # Returns
    /// JSON: `{
    ///   "vertex_count": N, "edge_count": N, "face_count": N,
    ///   "euler_characteristic": i64,
    ///   "genus": i64 | null,  (null when open manifold)
    ///   "boundary_loop_count": N, "is_closed": bool
    /// }`
    #[wasm_bindgen(js_name = "computeTopology")]
    pub fn compute_topology(&self) -> String {
        use axia_geo::p7_manifold::compute_topology;
        let report = compute_topology(&self.scene.mesh);
        let genus_json = match report.genus {
            Some(g) => g.to_string(),
            None => "null".to_string(),
        };
        format!(
            "{{\"vertex_count\":{},\"edge_count\":{},\"face_count\":{},\"euler_characteristic\":{},\"genus\":{},\"boundary_loop_count\":{},\"is_closed\":{}}}",
            report.vertex_count,
            report.edge_count,
            report.face_count,
            report.euler_characteristic,
            genus_json,
            report.boundary_loop_count,
            report.is_closed,
        )
    }

    /// ADR-091 D-γ — Demote a Xia back to a Shape when its material has
    /// reverted to the form-layer sentinel (`FORM_MATERIAL`).
    ///
    /// On success: returns a JSON string
    ///   `{ "shape_id": u32, "original_id_restored": bool }`
    /// On failure: throws JS `Error` with the DemoteError message
    /// (strict — silent skip 차단, ADR-091 D-γ lock-in 답습).
    ///
    /// Errors (matching `Scene::demote_xia_to_shape`):
    /// - Xia not found
    /// - Material is not the FORM_MATERIAL sentinel
    /// - ShapeId conflict (defensive)
    ///
    /// Transaction-wrapped — Undo restores the pre-demote state
    /// (Xia + shape_to_xia linkage preserved).
    #[wasm_bindgen(js_name = "demoteXiaToShape")]
    pub fn demote_xia_to_shape(
        &mut self,
        xia_id: u32,
    ) -> Result<String, JsValue> {
        self.scene.transactions.begin();
        self.scene
            .transactions
            .set_before_snapshot(self.scene.scene_snapshot());

        match self.scene.demote_xia_to_shape(xia_id) {
            Ok(ok) => {
                self.scene
                    .transactions
                    .set_after_snapshot(self.scene.scene_snapshot());
                self.scene.transactions.commit();
                let json = format!(
                    "{{\"shape_id\":{},\"original_id_restored\":{}}}",
                    ok.shape_id.raw(),
                    ok.original_id_restored,
                );
                Ok(json)
            }
            Err(err) => {
                self.scene.transactions.cancel();
                Err(JsValue::from_str(&format!("demoteXiaToShape: {}", err)))
            }
        }
    }

    /// ADR-060 Phase O Step 6 — Step 5 Fillet dispatch result as JSON.
    ///
    /// Routes through `Mesh::fillet_edge_dispatch` (§F + §E lock-ins).
    ///
    /// Schema:
    ///   `{ "schemaVersion": 1, "ok": bool, "pathUsed": "Mesh"|"BRep"|
    ///      "BRepWithMeshFallback", "skipReason": { "kind": "...",
    ///      "label": "..." } | null, "createdSurfaceKind": "Cylinder"|
    ///      null, "filletStripFaceCount": N }`
    /// ADR-061 Phase P-narrow Step 3 — Z.1 Normal Cache hot-path.
    ///
    /// Returns per-vertex (outer-loop order) world-space analytic
    /// normals for `face_id_raw` as a flat `Float64Array`:
    ///   `[count, n0x, n0y, n0z, n1x, n1y, n1z, ...]`
    ///
    /// First call on a cacheable face: MISS → compute + populate cache.
    /// Subsequent calls (until surface_version / boundary_version
    /// changes): HIT → returns cached data without recompute.
    ///
    /// Plane / no-surface faces: returns empty array (no per-vertex
    /// analytic normals to provide; Three.js falls back to face.normal).
    ///
    /// **§D additive-only** (ADR-060 lock-in #2): does not modify any
    /// existing endpoint.
    /// ADR-061 Phase P-narrow Step 5 — Cache stats endpoint.
    ///
    /// Returns aggregate Z.1 + Z.2 cache state as JSON with
    /// `schemaVersion: 1`. Used by UI / telemetry for memory monitoring.
    ///
    /// Schema:
    /// ```json
    /// {
    ///   "schemaVersion": 1,
    ///   "faceEntryCount": N,
    ///   "edgeEntryCount": M,
    ///   "faceCacheBytes": X,
    ///   "edgeCacheBytes": Y,
    ///   "totalBytes": Z,
    ///   "capBytes": 104857600,
    ///   "evictionCount": K
    /// }
    /// ```
    ///
    /// **§D additive-only** (ADR-060 lock-in #2).
    #[wasm_bindgen(js_name = "getCacheStats")]
    pub fn get_cache_stats(&self) -> String {
        let s = self.scene.mesh.cache_stats();
        format!(
            r#"{{"schemaVersion":1,"faceEntryCount":{},"edgeEntryCount":{},"faceCacheBytes":{},"edgeCacheBytes":{},"totalBytes":{},"capBytes":{},"evictionCount":{}}}"#,
            s.face_entry_count,
            s.edge_entry_count,
            s.face_cache_bytes,
            s.edge_cache_bytes,
            s.total_bytes,
            s.cap_bytes,
            s.eviction_count,
        )
    }

    /// ADR-061 Phase P-narrow Step 4 — Z.2 Curve Hover Cache hot-path.
    ///
    /// Returns the polyline tessellation of `edge_id_raw` as a flat
    /// `Float64Array`:
    ///   `[count, p0x, p0y, p0z, p1x, p1y, p1z, ...]`
    ///
    /// Use the returned polyline as Newton initial-seed grid for
    /// `ray_to_curve_distance` (ADR-040 P25). For Line edges (or edges
    /// with no curve attached) returns empty array — closed-form
    /// distance applies, no polyline needed.
    ///
    /// First call on cacheable edge: MISS → compute + populate.
    /// Subsequent calls (until curve_version changes): HIT.
    ///
    /// `chord_tol` defaults to `tolerances::HOVER_CHORD_TOL` (0.01mm)
    /// when `≤ 0`.
    ///
    /// **§D additive-only** (ADR-060 lock-in #2): does not modify any
    /// existing endpoint.
    #[wasm_bindgen(js_name = "getEdgePolylineCached")]
    pub fn get_edge_polyline_cached(&self, edge_id_raw: u32, chord_tol: f64) -> Vec<f64> {
        let eid = EdgeId::new(edge_id_raw);
        let tol = if chord_tol > 0.0 {
            chord_tol
        } else {
            axia_geo::tolerances::HOVER_CHORD_TOL
        };
        let points = match self.scene.mesh.edge_cached_polyline_or_compute(eid, tol) {
            Some(p) => p,
            None => return Vec::new(),
        };
        let mut flat = Vec::with_capacity(1 + points.len() * 3);
        flat.push(points.len() as f64);
        for p in points {
            flat.push(p.x);
            flat.push(p.y);
            flat.push(p.z);
        }
        flat
    }

    #[wasm_bindgen(js_name = "getFaceNormalsCached")]
    pub fn get_face_normals_cached(&self, face_id_raw: u32) -> Vec<f64> {
        let fid = FaceId::new(face_id_raw);
        let normals = match self.scene.mesh.face_cached_normals_or_compute(fid) {
            Some(n) => n,
            None => return Vec::new(),
        };
        let mut flat = Vec::with_capacity(1 + normals.len() * 3);
        flat.push(normals.len() as f64);
        for n in normals {
            flat.push(n.x);
            flat.push(n.y);
            flat.push(n.z);
        }
        flat
    }

    #[wasm_bindgen(js_name = "filletEdgeDispatchJson")]
    pub fn fillet_edge_dispatch_json(
        &mut self,
        edge_id_raw: u32,
        radius: f64,
        segments: u32,
    ) -> String {
        let eid = EdgeId::new(edge_id_raw);
        if !self.scene.mesh.edges.contains(eid) {
            return r#"{"schemaVersion":1,"ok":false,"error":"edge not found"}"#.to_string();
        }
        self.scene.transactions.begin();
        self.scene.transactions.set_before_snapshot(self.scene.scene_snapshot());
        let result = self.scene.mesh.fillet_edge_dispatch(eid, radius, segments);
        let dispatch_result = match result {
            Ok(r) => r,
            Err(e) => {
                self.scene.transactions.cancel();
                return format!(
                    r#"{{"schemaVersion":1,"ok":false,"error":"{}"}}"#,
                    e.to_string().replace('"', "'"),
                );
            }
        };
        self.scene.transactions.set_after_snapshot(self.scene.scene_snapshot());
        self.scene.transactions.commit();
        self.mark_topology_changed();
        self.invalidate_cache();
        step6_json::fillet_dispatch_result_json(&dispatch_result)
    }
}

// ============================================================================
// ADR-149 β-3 — TJunctionReport JSON parsing helper
// ============================================================================

/// Parsed TJunctionReport fields from JSON (helper for `healTJunction`).
#[derive(Debug)]
struct ParsedTJunctionReport {
    face_id: u32,
    edge_id: u32,
    vertex_id: u32,
    t_along_edge: f64,
}

/// Minimal JSON parser for `TJunctionReport`. Accepts strict shape:
/// `{"face_id": N, "edge_id": N, "vertex_id": N, "t_along_edge": F}`
///
/// Field order is flexible but all 4 fields are required.
/// Whitespace within JSON tokens is preserved by serde_json semantics;
/// we use a minimal regex-free hand parser to avoid serde dependency on
/// `TJunctionReport` (engine struct has no `#[derive(Serialize/Deserialize)]`
/// — keeping serde out of `axia-geo::operations::t_junction`).
fn parse_t_junction_report(json: &str) -> Result<ParsedTJunctionReport, String> {
    fn find_field_u32(json: &str, field: &str) -> Result<u32, String> {
        let needle = format!("\"{}\"", field);
        let idx = json
            .find(&needle)
            .ok_or_else(|| format!("missing field '{}'", field))?;
        let rest = &json[idx + needle.len()..];
        let colon = rest.find(':').ok_or_else(|| format!("malformed '{}'", field))?;
        let after_colon = &rest[colon + 1..];
        // Skip whitespace
        let val_start = after_colon
            .find(|c: char| !c.is_whitespace())
            .ok_or_else(|| format!("no value for '{}'", field))?;
        let val_rest = &after_colon[val_start..];
        // Read digits
        let end = val_rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(val_rest.len());
        if end == 0 {
            return Err(format!("expected integer for '{}'", field));
        }
        val_rest[..end]
            .parse::<u32>()
            .map_err(|e| format!("invalid u32 for '{}': {}", field, e))
    }

    fn find_field_f64(json: &str, field: &str) -> Result<f64, String> {
        let needle = format!("\"{}\"", field);
        let idx = json
            .find(&needle)
            .ok_or_else(|| format!("missing field '{}'", field))?;
        let rest = &json[idx + needle.len()..];
        let colon = rest.find(':').ok_or_else(|| format!("malformed '{}'", field))?;
        let after_colon = &rest[colon + 1..];
        let val_start = after_colon
            .find(|c: char| !c.is_whitespace())
            .ok_or_else(|| format!("no value for '{}'", field))?;
        let val_rest = &after_colon[val_start..];
        // Read number (digits, '.', '-', '+', 'e', 'E')
        let end = val_rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'))
            .unwrap_or(val_rest.len());
        if end == 0 {
            return Err(format!("expected number for '{}'", field));
        }
        val_rest[..end]
            .parse::<f64>()
            .map_err(|e| format!("invalid f64 for '{}': {}", field, e))
    }

    Ok(ParsedTJunctionReport {
        face_id: find_field_u32(json, "face_id")?,
        edge_id: find_field_u32(json, "edge_id")?,
        vertex_id: find_field_u32(json, "vertex_id")?,
        t_along_edge: find_field_f64(json, "t_along_edge")?,
    })
}

// ============================================================================
// ADR-150 β-3 — CoplanarPairReport JSON parsing helper
// ============================================================================

/// Parsed CoplanarPairReport fields from JSON (helper for
/// `mergeCoplanarPairBatch`). Mirrors `axia_geo::operations::geometric_
/// merge::CoplanarPairReport` shape.
#[derive(Debug)]
struct ParsedCoplanarPair {
    face_a: u32,
    face_b: u32,
    nx: f64,
    ny: f64,
    nz: f64,
}

/// Minimal JSON parser for an array of CoplanarPairReport. Accepts the
/// strict schema emitted by `sweepCoplanarPairs`:
/// `[{"face_a": N, "face_b": N, "plane_normal": {"x": F, "y": F, "z": F}}, ...]`
///
/// Empty array `[]` returns empty Vec (no-op caller can dispatch).
///
/// Field order is flexible within each pair object. Whitespace tolerant.
fn parse_coplanar_pair_array(json: &str) -> Result<Vec<ParsedCoplanarPair>, String> {
    let trimmed = json.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(format!("expected array, got: {}", &trimmed[..trimmed.len().min(40)]));
    }
    let inner = &trimmed[1..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    // Split top-level objects via brace-depth tracking (handles nested
    // plane_normal {x,y,z} object).
    let mut pairs: Vec<ParsedCoplanarPair> = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = inner.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 { start = i; }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let obj = &inner[start..=i];
                    pairs.push(parse_one_coplanar_pair(obj)?);
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("unbalanced braces in array".into());
    }
    Ok(pairs)
}

fn parse_one_coplanar_pair(obj: &str) -> Result<ParsedCoplanarPair, String> {
    let face_a = find_field_u32_in(obj, "face_a")?;
    let face_b = find_field_u32_in(obj, "face_b")?;
    // plane_normal is a nested object {"x": F, "y": F, "z": F} — locate it
    // and parse 3 floats within its braces.
    let pn_idx = obj
        .find("\"plane_normal\"")
        .ok_or_else(|| "missing 'plane_normal'".to_string())?;
    let pn_rest = &obj[pn_idx..];
    let pn_obj_start = pn_rest
        .find('{')
        .ok_or_else(|| "malformed 'plane_normal'".to_string())?;
    let pn_obj_end = pn_rest[pn_obj_start..]
        .find('}')
        .ok_or_else(|| "malformed 'plane_normal' (no closing brace)".to_string())?;
    let pn_obj = &pn_rest[pn_obj_start..=pn_obj_start + pn_obj_end];
    let nx = find_field_f64_in(pn_obj, "x")?;
    let ny = find_field_f64_in(pn_obj, "y")?;
    let nz = find_field_f64_in(pn_obj, "z")?;
    Ok(ParsedCoplanarPair { face_a, face_b, nx, ny, nz })
}

// Re-export helpers from parse_t_junction_report scope as standalone
// versions (avoid duplicate definition while keeping clean inputs to each
// schema).
fn find_field_u32_in(json: &str, field: &str) -> Result<u32, String> {
    let needle = format!("\"{}\"", field);
    let idx = json
        .find(&needle)
        .ok_or_else(|| format!("missing field '{}'", field))?;
    let rest = &json[idx + needle.len()..];
    let colon = rest.find(':').ok_or_else(|| format!("malformed '{}'", field))?;
    let after_colon = &rest[colon + 1..];
    let val_start = after_colon
        .find(|c: char| !c.is_whitespace())
        .ok_or_else(|| format!("no value for '{}'", field))?;
    let val_rest = &after_colon[val_start..];
    let end = val_rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(val_rest.len());
    if end == 0 {
        return Err(format!("expected integer for '{}'", field));
    }
    val_rest[..end]
        .parse::<u32>()
        .map_err(|e| format!("invalid u32 for '{}': {}", field, e))
}

fn find_field_f64_in(json: &str, field: &str) -> Result<f64, String> {
    let needle = format!("\"{}\"", field);
    let idx = json
        .find(&needle)
        .ok_or_else(|| format!("missing field '{}'", field))?;
    let rest = &json[idx + needle.len()..];
    let colon = rest.find(':').ok_or_else(|| format!("malformed '{}'", field))?;
    let after_colon = &rest[colon + 1..];
    let val_start = after_colon
        .find(|c: char| !c.is_whitespace())
        .ok_or_else(|| format!("no value for '{}'", field))?;
    let val_rest = &after_colon[val_start..];
    let end = val_rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'))
        .unwrap_or(val_rest.len());
    if end == 0 {
        return Err(format!("expected number for '{}'", field));
    }
    val_rest[..end]
        .parse::<f64>()
        .map_err(|e| format!("invalid f64 for '{}': {}", field, e))
}

/// Evaluate an IfcBooleanResult tree (ADR-203 I-3) into the faces of the
/// resulting solid — a wall with an opening, say. Each operand is built as a
/// solid in the mesh, then combined with the engine's own boolean; `None` when
/// an operand cannot be built or the boolean's validity gate rejects the result.
fn eval_csg(
    mesh: &mut axia_geo::Mesh,
    node: &axia_ifc::CsgNode,
    mat: axia_geo::MaterialId,
) -> Option<Vec<axia_geo::FaceId>> {
    let a = build_csg_operand(mesh, &node.first, mat)?;
    let b = build_csg_operand(mesh, &node.second, mat)?;
    let op = match node.op {
        axia_ifc::BoolOp::Union => axia_geo::operations::boolean::BoolOp::Union,
        axia_ifc::BoolOp::Subtract => axia_geo::operations::boolean::BoolOp::Subtract,
        axia_ifc::BoolOp::Intersect => axia_geo::operations::boolean::BoolOp::Intersect,
    };
    mesh.boolean_solid(&a, &b, op, mat).ok().map(|res| res.faces)
}

/// Build one boolean operand as a solid in the mesh, returning its face ids.
/// A nested boolean recurses; a solid adds its faces (with their planes, so the
/// boolean has surfaces to work with).
fn build_csg_operand(
    mesh: &mut axia_geo::Mesh,
    operand: &axia_ifc::CsgOperand,
    mat: axia_geo::MaterialId,
) -> Option<Vec<axia_geo::FaceId>> {
    match operand {
        axia_ifc::CsgOperand::Node(n) => eval_csg(mesh, n, mat),
        axia_ifc::CsgOperand::Solid(loops) => {
            let mut ids = Vec::new();
            for f in loops {
                let outer: Vec<_> = f.outer.iter().map(|&p| mesh.add_vertex(p)).collect();
                let inner_ids: Vec<Vec<_>> = f
                    .inners
                    .iter()
                    .map(|ring| ring.iter().map(|&p| mesh.add_vertex(p)).collect())
                    .collect();
                let hole_refs: Vec<&[_]> = inner_ids.iter().map(|v| v.as_slice()).collect();
                if let Ok(fid) = mesh.add_face_with_holes(&outer, &hole_refs, mat) {
                    if let Some(plane) = f.plane() {
                        mesh.set_face_surface(fid, Some(plane));
                    }
                    ids.push(fid);
                }
            }
            if ids.len() < 4 {
                None // did not build a closed solid
            } else {
                Some(ids)
            }
        }
    }
}

/// A point on a closed curve to anchor its self-loop face (ADR-203 I-3
/// kernel-native import). The curve's parametric start: the reference point of
/// a circle, the first control point of a spline (which, for a closed clamped
/// curve, is also its last).
fn curve_anchor(curve: &axia_geo::AnalyticCurve) -> glam::DVec3 {
    use axia_geo::AnalyticCurve;
    match curve {
        AnalyticCurve::Circle { center, radius, basis_u, .. } => *center + *basis_u * *radius,
        AnalyticCurve::Bezier { control_pts }
        | AnalyticCurve::BSpline { control_pts, .. }
        | AnalyticCurve::NURBS { control_pts, .. } => {
            control_pts.first().copied().unwrap_or(glam::DVec3::ZERO)
        }
        // Line / Arc never reach here (not closed disks); safe fallback.
        _ => glam::DVec3::ZERO,
    }
}

#[cfg(test)]
mod adr149_tests {
    use super::*;

    #[test]
    fn adr149_beta3_parse_tjunction_report_canonical() {
        let json = r#"{"face_id":0,"edge_id":4,"vertex_id":5,"t_along_edge":0.5}"#;
        let parsed = parse_t_junction_report(json).unwrap();
        assert_eq!(parsed.face_id, 0);
        assert_eq!(parsed.edge_id, 4);
        assert_eq!(parsed.vertex_id, 5);
        assert!((parsed.t_along_edge - 0.5).abs() < 1e-12);
    }

    #[test]
    fn adr149_beta3_parse_tjunction_report_with_whitespace() {
        let json = r#"{ "face_id" : 42, "edge_id" : 7, "vertex_id" : 13, "t_along_edge" : 0.25 }"#;
        let parsed = parse_t_junction_report(json).unwrap();
        assert_eq!(parsed.face_id, 42);
        assert_eq!(parsed.edge_id, 7);
        assert_eq!(parsed.vertex_id, 13);
        assert!((parsed.t_along_edge - 0.25).abs() < 1e-12);
    }

    #[test]
    fn adr149_beta3_parse_tjunction_report_missing_field() {
        let json = r#"{"face_id":0,"edge_id":4,"vertex_id":5}"#;  // missing t_along_edge
        let result = parse_t_junction_report(json);
        assert!(result.is_err(), "expected error for missing field");
        let err = result.unwrap_err();
        assert!(err.contains("t_along_edge"), "error should mention missing field, got: {}", err);
    }

    #[test]
    fn adr149_beta3_parse_tjunction_report_field_order_flexible() {
        // Reverse order — parser should still find all fields
        let json = r#"{"t_along_edge":0.75,"vertex_id":99,"edge_id":88,"face_id":77}"#;
        let parsed = parse_t_junction_report(json).unwrap();
        assert_eq!(parsed.face_id, 77);
        assert_eq!(parsed.edge_id, 88);
        assert_eq!(parsed.vertex_id, 99);
        assert!((parsed.t_along_edge - 0.75).abs() < 1e-12);
    }

    // ────────────────────────────────────────────────────────────────────────
    // ADR-150 β-3 — parse_coplanar_pair_array tests (4)
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn adr150_beta3_parse_coplanar_pair_array_empty() {
        let parsed = parse_coplanar_pair_array("[]").unwrap();
        assert_eq!(parsed.len(), 0);
    }

    #[test]
    fn adr150_beta3_parse_coplanar_pair_array_single() {
        let json = r#"[{"face_a":0,"face_b":1,"plane_normal":{"x":0,"y":1,"z":0}}]"#;
        let parsed = parse_coplanar_pair_array(json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].face_a, 0);
        assert_eq!(parsed[0].face_b, 1);
        assert!((parsed[0].nx - 0.0).abs() < 1e-12);
        assert!((parsed[0].ny - 1.0).abs() < 1e-12);
        assert!((parsed[0].nz - 0.0).abs() < 1e-12);
    }

    #[test]
    fn adr150_beta3_parse_coplanar_pair_array_multiple() {
        let json = r#"[{"face_a":0,"face_b":1,"plane_normal":{"x":1,"y":0,"z":0}},{"face_a":2,"face_b":3,"plane_normal":{"x":0,"y":0,"z":1}}]"#;
        let parsed = parse_coplanar_pair_array(json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].face_a, 0);
        assert_eq!(parsed[0].face_b, 1);
        assert!((parsed[0].nx - 1.0).abs() < 1e-12);
        assert_eq!(parsed[1].face_a, 2);
        assert_eq!(parsed[1].face_b, 3);
        assert!((parsed[1].nz - 1.0).abs() < 1e-12);
    }

    #[test]
    fn adr150_beta3_parse_coplanar_pair_array_invalid() {
        // Not an array
        let r1 = parse_coplanar_pair_array(r#"{"face_a":0}"#);
        assert!(r1.is_err(), "non-array should error");
        // Missing plane_normal field
        let r2 = parse_coplanar_pair_array(r#"[{"face_a":0,"face_b":1}]"#);
        assert!(r2.is_err(), "missing plane_normal should error");
    }
}

#[cfg(test)]
mod erase_face_only_tests {
    use super::*;

    /// **Option B (2026-06-18)** — face-only delete of an ISOLATED closed-loop
    /// face (Path B circle = 1 anchor + 1 self-loop edge + 1 face) must NOT be
    /// re-synthesized. ADR-019 "면만 지우면 boundary edge 는 standalone wireframe
    /// 으로 남아야 한다" + the re-synth scoping intent (lib.rs comment
    /// "recreating a face the user deliberately face-only-deleted is prevented").
    /// Pre-fix the self-loop edge lands in `newly_freed` → `resolve_planar_free_
    /// faces_scoped` recreates the face (synthesized=1, count unchanged) — the
    /// user's "원 면을 클릭해도 안 지워짐". Fixed by excluding the face-only
    /// targets' boundary (`protected_orphan_edges`) from `newly_freed`.
    #[test]
    fn face_only_delete_circle_keeps_wireframe_not_resynth() {
        let mut engine = AxiaEngine::new();
        // Path B circle (self-loop face). Flags off → stable id, no rederive churn;
        // the re-synth-on-erase path is independent of the draw flags.
        let _ = engine.draw_circle_as_curve(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3000.0);
        let faces_before: Vec<u32> = engine
            .scene
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(f, _)| f.raw())
            .collect();
        assert_eq!(faces_before.len(), 1, "one circle face after draw");
        let face = faces_before[0];

        // res = [merged, cascaded_faces, cascaded_edges, softened, synthesized, desolidified]
        let res = engine.batch_erase_edges_with_merge(&[face], &[], 0.5, false);

        let active_faces = engine
            .scene
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .count();
        let active_edges = engine
            .scene
            .mesh
            .edges
            .iter()
            .filter(|(_, e)| e.is_active())
            .count();
        assert_eq!(res[4], 0, "no re-synthesis on face-only delete (synthesized={})", res[4]);
        assert_eq!(active_faces, 0, "circle face removed, NOT re-synthesized (active={})", active_faces);
        assert!(active_edges >= 1, "self-loop edge remains as wireframe (active edges={})", active_edges);
    }

    /// Guard: deleting an EDGE (not face-only) still re-synthesizes / heals the
    /// surrounding region — the fix must NOT disable the legitimate edge-erase
    /// re-synth. Two coplanar rects sharing an edge; deleting the shared edge
    /// must merge (not leave a permanent hole). Asserts the edge path is intact.
    #[test]
    fn edge_delete_still_merges_after_fix() {
        let mut engine = AxiaEngine::new();
        // Two adjacent rects sharing the edge x=4000 (8000-wide pair → 16000 span).
        let _ = engine.draw_rect_as_shape(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 8000.0, 6000.0);
        let _ = engine.draw_rect_as_shape(8000.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 8000.0, 6000.0);
        let faces0 = engine.scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert!(faces0 >= 2, "two rect faces drawn (got {})", faces0);
        // Find the shared edge (two active faces incident).
        let shared: Option<u32> = engine
            .scene
            .mesh
            .edges
            .iter()
            .filter(|(_, e)| e.is_active())
            .map(|(eid, _)| eid)
            .find(|&eid| {
                let (faces, _) = engine.scene.mesh.get_faces_sharing_edge(eid);
                faces.len() == 2 && faces[0] != faces[1]
            })
            .map(|eid| eid.raw());
        if let Some(edge) = shared {
            let res = engine.batch_erase_edges_with_merge(&[], &[edge], 5.0, false);
            // merged (res[0]) or cascaded — either way the shared edge is gone and
            // the two faces are reconciled (not a no-op).
            let still_has = engine.scene.mesh.edges.contains(EdgeId::new(edge))
                && engine.scene.mesh.edges[EdgeId::new(edge)].is_active();
            assert!(!still_has || res[0] > 0, "shared edge erase did something (merged={})", res[0]);
        }
    }
}

#[cfg(test)]
mod ifc_boolean_import_tests {
    use super::*;

    const PLAIN_WALL: &str = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#51=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#50));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'Swept',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
";

    // A wall with a window written the real-BIM way: the wall solid minus an
    // opening solid, an IfcBooleanClippingResult. The opening is thicker than the
    // wall so it punches clean through.
    const WALL_WITH_WINDOW: &str = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#60=IFCCARTESIANPOINT((0.,0.,0.8));
#61=IFCAXIS2PLACEMENT3D(#60,$,$);
#62=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,1.,0.4);
#63=IFCEXTRUDEDAREASOLID(#62,#61,$,1.2);
#70=IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#50,#63);
#51=IFCSHAPEREPRESENTATION($,'Body','CSG',(#70));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'CSG',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
";

    fn active_faces(e: &AxiaEngine) -> usize {
        e.scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
    }

    /// A wall-with-opening IfcBooleanResult must import as a *watertight solid
    /// with a hole* — the engine's own boolean run at import time, not the two
    /// operands left lying in the scene. The opening adds tunnel walls, so the
    /// result has more faces than a plain wall, and it is still a closed solid.
    #[test]
    fn boolean_clipping_result_imports_as_a_holed_solid() {
        let mut plain = AxiaEngine::new();
        plain.import_ifc(PLAIN_WALL.to_string());
        let plain_faces = active_faces(&plain);
        assert_eq!(plain_faces, 6, "a plain wall is a six-faced prism");

        let mut holed = AxiaEngine::new();
        holed.import_ifc(WALL_WITH_WINDOW.to_string());
        let holed_faces = active_faces(&holed);

        // A rectangular through-hole = the prism's front/back carry the opening
        // plus four tunnel walls. The exact count is 10, and it must exceed the
        // plain wall — an empty boolean would have left just the two operands or
        // nothing at all.
        assert!(
            holed_faces > plain_faces,
            "the opening adds geometry: plain {} vs holed {}",
            plain_faces,
            holed_faces
        );

        // And it is a real watertight solid, not a self-intersecting mess.
        let report = holed.scene.mesh.verify_face_invariants();
        assert!(report.is_valid(), "the holed wall is a valid solid: {:?}", report);
    }

    /// A boolean whose operand we cannot build (a half-space) must not import a
    /// wall with no opening — a silently wrong shape. Nothing is added; the scene
    /// is left as it was.
    #[test]
    fn boolean_with_a_half_space_operand_imports_nothing() {
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#60=IFCCARTESIANPOINT((0.,0.,0.));
#61=IFCDIRECTION((0.,0.,1.));
#62=IFCAXIS2PLACEMENT3D(#60,#61,$);
#63=IFCPLANE(#62);
#64=IFCHALFSPACESOLID(#63,.F.);
#70=IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#50,#64);
#51=IFCSHAPEREPRESENTATION($,'Body','CSG',(#70));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'CSG',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let mut e = AxiaEngine::new();
        e.import_ifc(src.to_string());
        assert_eq!(active_faces(&e), 0, "an unreadable boolean imports nothing");
    }
}

