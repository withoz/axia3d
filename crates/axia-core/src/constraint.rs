//! Constraint Solver Level 2 — persistent constraint graph with local solver.
//!
//! Level 1 (`ConstraintCommands` in TS) applies geometric adjustments once.
//! Level 2 stores constraints in the scene, resolves them automatically after
//! vertex transforms, and persists through save/load + undo/redo.
//!
//! ## Design
//! - Constraints reference entities by **VertId pairs** (edges = 2 verts).
//!   This is more stable than `EdgeId` across edge splits/merges.
//! - Each constraint has a clear **driver / driven** role:
//!   - `refs[0]` = reference (driver)
//!   - `refs[1]` = adjusted (driven)
//!   When a driver vertex moves, the driven entity is re-solved.
//! - Solver is **local per-constraint**, not iterative global.
//!   Multiple interacting constraints may not converge; users get a one-shot
//!   re-application by transform.
//! - Topology changes (vert deletion etc.) detected by `is_ref_valid`:
//!   invalid references cause `active = false`, not outright removal.

use serde::{Deserialize, Serialize};
use glam::DVec3;
use axia_geo::{Mesh, VertId};

/// Stable identifier for a constraint (u32).
pub type ConstraintId = u32;

/// Constraint kind discriminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    /// Two edges parallel (same direction up to sign).
    Parallel,
    /// Two edges perpendicular in their common plane.
    Perpendicular,
    /// Two edges collinear (parallel + on same infinite line).
    Collinear,
    /// Two vertices at fixed 3D distance.
    Distance,
    /// Two edges at a fixed angle (radians, in their common plane). ADR-216 —
    /// the driving angular dimension. `value` holds the target angle.
    Angle,
    /// A Circle/Arc edge at a fixed radius. ADR-217 — the driving radial
    /// dimension. `refs[0]` = a vertex on the curve edge; `value` = the radius.
    Radius,
}

/// Reference to an entity participating in a constraint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConstraintRef {
    /// Edge identified by its two endpoint vertices.
    Edge { v_a: VertId, v_b: VertId },
    /// Single vertex.
    Vertex(VertId),
}

impl ConstraintRef {
    /// Return true if all referenced vertices exist in `mesh`.
    pub fn is_valid(&self, mesh: &Mesh) -> bool {
        match self {
            Self::Edge { v_a, v_b } =>
                mesh.verts.contains(*v_a) && mesh.verts.contains(*v_b),
            Self::Vertex(v) => mesh.verts.contains(*v),
        }
    }

    /// Collect the vertices involved (flattened).
    pub fn verts(&self) -> Vec<VertId> {
        match self {
            Self::Edge { v_a, v_b } => vec![*v_a, *v_b],
            Self::Vertex(v) => vec![*v],
        }
    }
}

/// A persistent geometric constraint between two entities.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Constraint {
    pub id: ConstraintId,
    pub kind: ConstraintKind,
    /// refs[0] = driver (reference), refs[1] = driven (adjusted)
    pub refs: Vec<ConstraintRef>,
    /// Target value — currently only used by `Distance`.
    pub value: Option<f64>,
    /// Deactivated constraints are kept in the graph but not solved.
    pub active: bool,
}

/// Container for all constraints in a scene.
/// Keeps ordered list + auto-increment id generator.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConstraintGraph {
    items: Vec<Constraint>,
    next_id: ConstraintId,
}

impl ConstraintGraph {
    pub fn new() -> Self {
        Self { items: Vec::new(), next_id: 1 }
    }

    pub fn add(&mut self, kind: ConstraintKind, refs: Vec<ConstraintRef>, value: Option<f64>) -> ConstraintId {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.items.push(Constraint { id, kind, refs, value, active: true });
        id
    }

    pub fn remove(&mut self, id: ConstraintId) -> bool {
        let pos = self.items.iter().position(|c| c.id == id);
        if let Some(i) = pos { self.items.remove(i); true } else { false }
    }

    pub fn set_active(&mut self, id: ConstraintId, active: bool) -> bool {
        if let Some(c) = self.items.iter_mut().find(|c| c.id == id) {
            c.active = active;
            true
        } else { false }
    }

    /// ADR-215 — update a constraint's target `value` (the parametric dimension
    /// value, used by `Distance`). Returns true if the id existed.
    pub fn set_value(&mut self, id: ConstraintId, value: f64) -> bool {
        if let Some(c) = self.items.iter_mut().find(|c| c.id == id) {
            c.value = Some(value);
            true
        } else { false }
    }

    pub fn clear(&mut self) { self.items.clear(); self.next_id = 1; }

    pub fn get(&self, id: ConstraintId) -> Option<&Constraint> {
        self.items.iter().find(|c| c.id == id)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Constraint> { self.items.iter() }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }

    /// Constraint references containing `vid` — used to find which constraints
    /// need re-solving when `vid` moves.
    pub fn constraints_touching(&self, vid: VertId) -> Vec<ConstraintId> {
        self.items.iter()
            .filter(|c| c.active && c.refs.iter().any(|r| r.verts().contains(&vid)))
            .map(|c| c.id)
            .collect()
    }

    /// Deactivate any constraint whose refs are invalid (referenced vert deleted).
    pub fn prune_invalid(&mut self, mesh: &Mesh) -> usize {
        let mut count = 0;
        for c in self.items.iter_mut() {
            if c.active && c.refs.iter().any(|r| !r.is_valid(mesh)) {
                c.active = false;
                count += 1;
            }
        }
        count
    }
}

/// Local per-constraint solver — applies geometric adjustment to satisfy the
/// constraint. Returns `true` if anything moved.
///
/// Uses the same math as Level 1 (`ConstraintCommands`) but operates on `Mesh`
/// directly. Driver/driven distinction: `refs[0]` is fixed, `refs[1]` is moved.
pub fn resolve_constraint(mesh: &mut Mesh, c: &Constraint) -> bool {
    if !c.active { return false; }
    if c.refs.iter().any(|r| !r.is_valid(mesh)) { return false; }

    match c.kind {
        ConstraintKind::Parallel
        | ConstraintKind::Perpendicular
        | ConstraintKind::Collinear
        | ConstraintKind::Angle => {
            if c.refs.len() != 2 { return false; }
            let (a_va, a_vb) = match &c.refs[0] {
                ConstraintRef::Edge { v_a, v_b } => (*v_a, *v_b),
                _ => return false,
            };
            let (b_va, b_vb) = match &c.refs[1] {
                ConstraintRef::Edge { v_a, v_b } => (*v_a, *v_b),
                _ => return false,
            };
            resolve_edge_pair(mesh, (a_va, a_vb), (b_va, b_vb), c.kind, c.value)
        }
        ConstraintKind::Distance => {
            if c.refs.len() != 2 { return false; }
            let (v_a, v_b) = match (&c.refs[0], &c.refs[1]) {
                (ConstraintRef::Vertex(a), ConstraintRef::Vertex(b)) => (*a, *b),
                _ => return false,
            };
            let target = match c.value {
                Some(d) if d.is_finite() && d > 0.0 => d,
                _ => return false,
            };
            resolve_distance(mesh, v_a, v_b, target)
        }
        ConstraintKind::Radius => {
            // ADR-217 — drive a Circle/Arc edge's radius. refs[0] = a vertex on
            // the curve edge; value = target radius (center fixed).
            let vert = match c.refs.first() {
                Some(ConstraintRef::Vertex(v)) => *v,
                _ => return false,
            };
            let target = match c.value {
                Some(r) if r.is_finite() && r > 0.0 => r,
                _ => return false,
            };
            let edge = match mesh.find_curve_edge_at(vert) {
                Some(e) => e,
                None => return false,
            };
            let current = mesh.edge_curve_radius(edge).unwrap_or(0.0);
            if (current - target).abs() < 1e-9 {
                return false;
            }
            mesh.set_curve_radius(edge, target).is_ok()
        }
    }
}

/// Resolve parallel/perpendicular/collinear between edges A (driver) and B (driven).
fn resolve_edge_pair(
    mesh: &mut Mesh,
    (a_va, a_vb): (VertId, VertId),
    (b_va, b_vb): (VertId, VertId),
    kind: ConstraintKind,
    target_angle: Option<f64>,
) -> bool {
    let pa0 = mesh.vertex_pos(a_va).ok();
    let pa1 = mesh.vertex_pos(a_vb).ok();
    let pb0 = mesh.vertex_pos(b_va).ok();
    let pb1 = mesh.vertex_pos(b_vb).ok();
    let (pa0, pa1, pb0, pb1) = match (pa0, pa1, pb0, pb1) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return false,
    };

    let dir_a = (pa1 - pa0).try_normalize().unwrap_or(DVec3::X);
    let dir_b_raw = pb1 - pb0;
    let dir_b = dir_b_raw.try_normalize().unwrap_or(DVec3::X);
    let b_mid = (pb0 + pb1) * 0.5;

    // Compute target direction for edge B
    let target_dir = match kind {
        ConstraintKind::Parallel | ConstraintKind::Collinear => dir_a,
        ConstraintKind::Perpendicular => {
            let plane_normal = dir_a.cross(dir_b);
            if plane_normal.length_squared() < 1e-12 { return false; }
            let plane_n = plane_normal.normalize();
            let mut t = plane_n.cross(dir_a).normalize();
            if t.dot(dir_b) < 0.0 { t = -t; }
            t
        }
        ConstraintKind::Angle => {
            // ADR-216 — rotate dir_a by the target angle θ around the common-plane
            // normal (toward dir_b's side) to get B's target direction.
            let plane_normal = dir_a.cross(dir_b);
            if plane_normal.length_squared() < 1e-12 { return false; } // (anti)parallel — no plane
            let plane_n = plane_normal.normalize();
            let theta = match target_angle {
                Some(t) if t.is_finite() && t > 1e-6 && t < std::f64::consts::PI - 1e-6 => t,
                _ => return false,
            };
            (dir_a * theta.cos() + plane_n.cross(dir_a) * theta.sin()).normalize()
        }
        ConstraintKind::Distance | ConstraintKind::Radius => return false,
    };

    // Pivot — Angle rotates around the shared corner vertex when the two edges
    // share one (keeps the corner intact, ADR-216 Q1=a); otherwise B's midpoint.
    let pivot = if matches!(kind, ConstraintKind::Angle) {
        match [b_va, b_vb].into_iter().find(|v| *v == a_va || *v == a_vb) {
            Some(s) => mesh.vertex_pos(s).unwrap_or(b_mid),
            None => b_mid,
        }
    } else {
        b_mid
    };

    // Rotation: dir_b → target_dir around `pivot`
    let dot = dir_b.dot(target_dir).clamp(-1.0, 1.0);
    let mut moved = false;
    if (dot - 1.0).abs() > 1e-9 {
        let (axis, angle) = if (dot + 1.0).abs() < 1e-9 {
            // antipodal: pick arbitrary perpendicular axis
            let arbitrary = if dir_b.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            (dir_b.cross(arbitrary).normalize(), std::f64::consts::PI)
        } else {
            let a = dir_b.cross(target_dir);
            if a.length_squared() < 1e-18 { return false; }
            (a.normalize(), dot.acos())
        };
        let _ = mesh.rotate_verts(&[b_va, b_vb], pivot, axis, angle);
        moved = true;
    }

    // Collinear: additionally translate B's midpoint onto line A
    if matches!(kind, ConstraintKind::Collinear) {
        let mid_a = (pa0 + pa1) * 0.5;
        // Re-fetch b_mid after potential rotation
        let pb0_new = mesh.vertex_pos(b_va).unwrap_or(pb0);
        let pb1_new = mesh.vertex_pos(b_vb).unwrap_or(pb1);
        let b_mid_new = (pb0_new + pb1_new) * 0.5;
        let delta = b_mid_new - mid_a;
        let proj = dir_a * delta.dot(dir_a);
        let target_mid = mid_a + proj;
        let shift = target_mid - b_mid_new;
        if shift.length_squared() > 1e-18 {
            let _ = mesh.translate_verts(&[b_va, b_vb], shift);
            moved = true;
        }
    }

    moved
}

/// Resolve distance: move v_b along (v_a → v_b) direction to achieve target distance from v_a.
fn resolve_distance(mesh: &mut Mesh, v_a: VertId, v_b: VertId, target: f64) -> bool {
    let pa = match mesh.vertex_pos(v_a) { Ok(p) => p, Err(_) => return false };
    let pb = match mesh.vertex_pos(v_b) { Ok(p) => p, Err(_) => return false };
    let d = pb - pa;
    let len = d.length();
    if len < 1e-9 { return false; } // can't determine direction
    let dir = d / len;
    let new_pb = pa + dir * target;
    let shift = new_pb - pb;
    if shift.length_squared() < 1e-18 { return false; }
    let _ = mesh.translate_verts(&[v_b], shift);
    true
}

/// Resolve every active constraint once.
/// Returns the number of constraints that actually moved anything.
pub fn resolve_all(mesh: &mut Mesh, graph: &ConstraintGraph) -> usize {
    let mut count = 0;
    for c in graph.iter() {
        if resolve_constraint(mesh, c) { count += 1; }
    }
    count
}

// ═══════════════════════════════════════════════════════════════════════════
// Level 3 — Iterative solver (XPBD-style projection loop)
// ═══════════════════════════════════════════════════════════════════════════

/// Outcome of an iterative solve.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SolverResult {
    /// Converged within tolerance before max_iter ran out.
    pub converged: bool,
    /// Number of outer iterations performed.
    pub iterations: u32,
    /// Max residual across all active constraints after the final iteration.
    pub final_residual: f64,
    /// Maximum residual at iteration 0 — useful for relative progress.
    pub initial_residual: f64,
    /// Heuristic over-constrained flag — residual plateaued without converging.
    pub over_constrained: bool,
}

impl Default for SolverResult {
    fn default() -> Self {
        Self {
            converged: true,
            iterations: 0,
            final_residual: 0.0,
            initial_residual: 0.0,
            over_constrained: false,
        }
    }
}

/// Compute the residual (a scalar ≥ 0) of a constraint. Zero = satisfied.
pub fn constraint_residual(mesh: &Mesh, c: &Constraint) -> f64 {
    if !c.active { return 0.0; }
    if c.refs.iter().any(|r| !r.is_valid(mesh)) { return 0.0; }

    match c.kind {
        ConstraintKind::Parallel
        | ConstraintKind::Perpendicular
        | ConstraintKind::Collinear
        | ConstraintKind::Angle => {
            if c.refs.len() != 2 { return 0.0; }
            let (a_va, a_vb) = match &c.refs[0] {
                ConstraintRef::Edge { v_a, v_b } => (*v_a, *v_b),
                _ => return 0.0,
            };
            let (b_va, b_vb) = match &c.refs[1] {
                ConstraintRef::Edge { v_a, v_b } => (*v_a, *v_b),
                _ => return 0.0,
            };
            let (pa0, pa1, pb0, pb1) = match (
                mesh.vertex_pos(a_va).ok(),
                mesh.vertex_pos(a_vb).ok(),
                mesh.vertex_pos(b_va).ok(),
                mesh.vertex_pos(b_vb).ok(),
            ) {
                (Some(a), Some(b), Some(c_), Some(d)) => (a, b, c_, d),
                _ => return 0.0,
            };
            let dir_a = (pa1 - pa0).try_normalize().unwrap_or(DVec3::X);
            let dir_b = (pb1 - pb0).try_normalize().unwrap_or(DVec3::X);
            let dot_abs = dir_a.dot(dir_b).abs().min(1.0);
            match c.kind {
                ConstraintKind::Parallel => 1.0 - dot_abs,     // 0 = parallel
                ConstraintKind::Perpendicular => dot_abs,      // 0 = perpendicular
                ConstraintKind::Angle => {
                    // ADR-216 — |current angle − target| (radians).
                    // ADR-218 — a reference (value=None) angle is read-only: it
                    // never drives, so it contributes 0 residual (otherwise an
                    // unmatched target=0 would report the whole current angle and
                    // keep the iterative solver from converging).
                    match c.value {
                        Some(target) => {
                            let current = dir_a.dot(dir_b).clamp(-1.0, 1.0).acos();
                            (current - target).abs()
                        }
                        None => 0.0,
                    }
                }
                ConstraintKind::Collinear => {
                    let para_resid = 1.0 - dot_abs;
                    // Additional: distance from B's midpoint to A's infinite line.
                    let mid_a = (pa0 + pa1) * 0.5;
                    let mid_b = (pb0 + pb1) * 0.5;
                    let delta = mid_b - mid_a;
                    let proj = dir_a * delta.dot(dir_a);
                    let perp = delta - proj;
                    // Scale distance residual so parallel and distance components are
                    // balanced-ish; use relative to edge A length.
                    let len_a = (pa1 - pa0).length().max(1.0);
                    para_resid + perp.length() / len_a
                }
                _ => 0.0,
            }
        }
        ConstraintKind::Distance => {
            if c.refs.len() != 2 { return 0.0; }
            let (v_a, v_b) = match (&c.refs[0], &c.refs[1]) {
                (ConstraintRef::Vertex(a), ConstraintRef::Vertex(b)) => (*a, *b),
                _ => return 0.0,
            };
            let target = c.value.unwrap_or(0.0);
            if target <= 0.0 { return 0.0; }
            let (pa, pb) = match (mesh.vertex_pos(v_a).ok(), mesh.vertex_pos(v_b).ok()) {
                (Some(a), Some(b)) => (a, b),
                _ => return 0.0,
            };
            let actual = (pb - pa).length();
            // Relative residual so distance magnitude doesn't dominate other types.
            (actual - target).abs() / target.max(1.0)
        }
        ConstraintKind::Radius => {
            // ADR-217 — |current radius − target| / target.
            let vert = match c.refs.first() {
                Some(ConstraintRef::Vertex(v)) => *v,
                _ => return 0.0,
            };
            let target = c.value.unwrap_or(0.0);
            if target <= 0.0 { return 0.0; }
            match mesh.find_curve_edge_at(vert).and_then(|e| mesh.edge_curve_radius(e)) {
                Some(r) => (r - target).abs() / target.max(1.0),
                None => 0.0,
            }
        }
    }
}

/// Max residual across all active constraints.
pub fn max_residual(mesh: &Mesh, graph: &ConstraintGraph) -> f64 {
    graph.iter()
        .filter(|c| c.active)
        .map(|c| constraint_residual(mesh, c))
        .fold(0.0, f64::max)
}

/// Iterate through all constraints (XPBD-style sequential projection) until
/// max residual drops below `tolerance` or `max_iter` is reached.
///
/// Over-constraint heuristic: if residual fails to decrease by >1% across 5
/// consecutive iterations, we flag `over_constrained = true` and stop early.
pub fn resolve_iterative(
    mesh: &mut Mesh,
    graph: &ConstraintGraph,
    max_iter: u32,
    tolerance: f64,
) -> SolverResult {
    let initial = max_residual(mesh, graph);
    if initial < tolerance {
        return SolverResult {
            converged: true,
            iterations: 0,
            final_residual: initial,
            initial_residual: initial,
            over_constrained: false,
        };
    }

    let mut prev_residual = initial;
    let mut stagnation = 0u32;
    let mut iter = 0u32;
    let mut current = initial;

    while iter < max_iter {
        // One pass: apply each constraint's projection in order.
        for c in graph.iter() {
            resolve_constraint(mesh, c);
        }
        iter += 1;
        current = max_residual(mesh, graph);
        if current < tolerance {
            return SolverResult {
                converged: true,
                iterations: iter,
                final_residual: current,
                initial_residual: initial,
                over_constrained: false,
            };
        }

        // Stagnation detection — residual not decreasing meaningfully
        let improvement = if prev_residual > 1e-12 {
            (prev_residual - current) / prev_residual
        } else { 0.0 };
        if improvement < 0.01 {
            stagnation += 1;
            if stagnation >= 5 {
                return SolverResult {
                    converged: false,
                    iterations: iter,
                    final_residual: current,
                    initial_residual: initial,
                    over_constrained: true,
                };
            }
        } else {
            stagnation = 0;
        }
        prev_residual = current;
    }

    SolverResult {
        converged: false,
        iterations: iter,
        final_residual: current,
        initial_residual: initial,
        over_constrained: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    /// ADR-215 de-risk: editing a Distance constraint's value drives geometry —
    /// the parametric dimension behaviour the Dimension tool relies on.
    #[test]
    fn adr215_set_value_drives_distance() {
        let mut mesh = Mesh::new();
        let va = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let vb = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0)); // initial distance 5
        mesh.add_edge(va, vb).expect("edge");

        let mut graph = ConstraintGraph::new();
        let id = graph.add(
            ConstraintKind::Distance,
            vec![ConstraintRef::Vertex(va), ConstraintRef::Vertex(vb)],
            Some(5.0),
        );

        // Edit the dimension value → 12, resolve → vb moves so |va vb| = 12.
        assert!(graph.set_value(id, 12.0));
        resolve_all(&mut mesh, &graph);

        let pa = mesh.vertex_pos(va).unwrap();
        let pb = mesh.vertex_pos(vb).unwrap();
        assert!((pb.distance(pa) - 12.0).abs() < 1e-6, "distance after edit = {}", pb.distance(pa));
        // va is the driver (fixed); vb moved along +x to (12,0,0).
        assert!(pa.distance(DVec3::ZERO) < 1e-9, "driver vertex fixed");
        assert!((pb.x - 12.0).abs() < 1e-6 && pb.y.abs() < 1e-9 && pb.z.abs() < 1e-9, "vb at (12,0,0), got {:?}", pb);

        // set_value on an unknown id is a no-op false.
        assert!(!graph.set_value(9999, 3.0));
        // the constraint's stored value reflects the edit.
        assert_eq!(graph.get(id).and_then(|c| c.value), Some(12.0));
    }

    /// ADR-216 de-risk: a driving Angle constraint rotates the driven edge to
    /// the target angle, pivoting on the shared corner vertex (kept fixed), and
    /// editing the value re-drives the angle.
    #[test]
    fn adr216_angle_constraint_drives_corner() {
        let mut mesh = Mesh::new();
        // Corner at origin. Edge A (driver) = v→a1 along +x; edge B (driven) =
        // v→b1 at a shallow angle. Both share the corner vertex v.
        let v = mesh.add_vertex(DVec3::ZERO);
        let a1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let b1 = mesh.add_vertex(DVec3::new(10.0, 2.0, 0.0));
        mesh.add_edge(v, a1).expect("edge A");
        mesh.add_edge(v, b1).expect("edge B");

        let mut graph = ConstraintGraph::new();
        let target = std::f64::consts::FRAC_PI_2; // 90°
        let id = graph.add(
            ConstraintKind::Angle,
            vec![
                ConstraintRef::Edge { v_a: v, v_b: a1 },
                ConstraintRef::Edge { v_a: v, v_b: b1 },
            ],
            Some(target),
        );
        resolve_all(&mut mesh, &graph);

        let angle_now = |m: &Mesh| -> f64 {
            let pv = m.vertex_pos(v).unwrap();
            let da = (m.vertex_pos(a1).unwrap() - pv).normalize();
            let db = (m.vertex_pos(b1).unwrap() - pv).normalize();
            da.dot(db).clamp(-1.0, 1.0).acos()
        };
        assert!((angle_now(&mesh) - target).abs() < 1e-6, "angle = {}", angle_now(&mesh));
        // shared corner stays fixed (pivot); driver edge A untouched.
        assert!(mesh.vertex_pos(v).unwrap().distance(DVec3::ZERO) < 1e-9, "corner fixed");
        assert!(mesh.vertex_pos(a1).unwrap().distance(DVec3::new(10.0, 0.0, 0.0)) < 1e-9, "driver fixed");

        // Edit the angle → 45°, re-resolve.
        graph.set_value(id, std::f64::consts::FRAC_PI_4);
        resolve_all(&mut mesh, &graph);
        assert!(
            (angle_now(&mesh) - std::f64::consts::FRAC_PI_4).abs() < 1e-6,
            "angle after edit = {}",
            angle_now(&mesh)
        );
    }

    /// ADR-217 de-risk: a driving Radius constraint resizes an Arc edge (center
    /// fixed, endpoints move), and editing the value re-drives the radius.
    #[test]
    fn adr217_radius_constraint_drives_arc() {
        use axia_geo::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        let center = DVec3::ZERO;
        let basis_u = DVec3::X;
        let normal = DVec3::Z;
        let w = normal.cross(basis_u); // +Y
        let r0 = 5.0;
        let start_angle = 0.0;
        let end_angle = std::f64::consts::FRAC_PI_2;
        let vs = mesh.add_vertex(center + basis_u * r0); // (5,0,0)
        let ve = mesh.add_vertex(center + w * r0); // (0,5,0)
        let arc = AnalyticCurve::Arc { center, radius: r0, normal, basis_u, start_angle, end_angle };
        let e = mesh.add_edge_with_curve(vs, ve, arc).expect("arc edge");

        // Direct set_curve_radius → endpoints move to radius 10, center fixed.
        mesh.set_curve_radius(e, 10.0).unwrap();
        assert_eq!(mesh.edge_curve_radius(e), Some(10.0));
        assert!(mesh.vertex_pos(vs).unwrap().distance(DVec3::new(10.0, 0.0, 0.0)) < 1e-6);
        assert!(mesh.vertex_pos(ve).unwrap().distance(DVec3::new(0.0, 10.0, 0.0)) < 1e-6);

        // Radius constraint (ref = a vertex on the curve) drives the radius.
        let mut graph = ConstraintGraph::new();
        let id = graph.add(ConstraintKind::Radius, vec![ConstraintRef::Vertex(vs)], Some(8.0));
        resolve_all(&mut mesh, &graph);
        assert!((mesh.edge_curve_radius(e).unwrap() - 8.0).abs() < 1e-6);
        assert!(mesh.vertex_pos(vs).unwrap().distance(DVec3::new(8.0, 0.0, 0.0)) < 1e-6);

        // Edit the radius → 3.
        graph.set_value(id, 3.0);
        resolve_all(&mut mesh, &graph);
        assert!((mesh.edge_curve_radius(e).unwrap() - 3.0).abs() < 1e-6);
    }

    /// ADR-217 de-risk: set_curve_radius on a full Circle (self-loop) moves the
    /// anchor to the new radius, center fixed.
    #[test]
    fn adr217_set_curve_radius_circle() {
        use axia_geo::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        let center = DVec3::ZERO;
        let basis_u = DVec3::X;
        let normal = DVec3::Z;
        let r0 = 5.0;
        let anchor = mesh.add_vertex(center + basis_u * r0); // (5,0,0)
        let circ = AnalyticCurve::Circle { center, radius: r0, normal, basis_u };
        let e = mesh.add_edge_with_curve(anchor, anchor, circ).expect("circle self-loop");
        mesh.set_curve_radius(e, 12.0).unwrap();
        assert_eq!(mesh.edge_curve_radius(e), Some(12.0));
        assert!(mesh.vertex_pos(anchor).unwrap().distance(DVec3::new(12.0, 0.0, 0.0)) < 1e-6);
    }

    /// ADR-218 de-risk: reference (read-only) dimensions are the SAME constraint
    /// kinds (Distance / Angle / Radius) carrying `value = None`. They MUST never
    /// move geometry — `resolve` is a no-op and `max_residual` stays 0 so the
    /// iterative solver still converges with reference dims present.
    #[test]
    fn adr218_reference_dimensions_never_drive_geometry() {
        use axia_geo::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        // Distance pair: (0,0,0)-(5,0,0).
        let da = mesh.add_vertex(DVec3::ZERO);
        let db = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0));
        mesh.add_edge(da, db).expect("dist edge");
        // Angle corner: shared v, edge A +x, edge B at 2/10 slope.
        let v = mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
        let a1 = mesh.add_vertex(DVec3::new(110.0, 0.0, 0.0));
        let b1 = mesh.add_vertex(DVec3::new(110.0, 2.0, 0.0));
        mesh.add_edge(v, a1).expect("angle edge A");
        mesh.add_edge(v, b1).expect("angle edge B");
        // Circle self-loop radius 5.
        let anchor = mesh.add_vertex(DVec3::new(205.0, 0.0, 0.0));
        let circ = AnalyticCurve::Circle {
            center: DVec3::new(200.0, 0.0, 0.0), radius: 5.0, normal: DVec3::Z, basis_u: DVec3::X,
        };
        let ce = mesh.add_edge_with_curve(anchor, anchor, circ).expect("circle");

        // Capture geometry before.
        let before = |m: &Mesh| (
            m.vertex_pos(db).unwrap(), m.vertex_pos(b1).unwrap(), m.edge_curve_radius(ce).unwrap(),
        );
        let g0 = before(&mesh);

        // Reference (value = None) constraints of all three kinds.
        let mut graph = ConstraintGraph::new();
        let cd = graph.add(ConstraintKind::Distance, vec![ConstraintRef::Vertex(da), ConstraintRef::Vertex(db)], None);
        let cang = graph.add(ConstraintKind::Angle, vec![
            ConstraintRef::Edge { v_a: v, v_b: a1 }, ConstraintRef::Edge { v_a: v, v_b: b1 },
        ], None);
        let cr = graph.add(ConstraintKind::Radius, vec![ConstraintRef::Vertex(anchor)], None);

        // resolve_all moves nothing; each resolve_constraint returns false.
        assert!(!resolve_constraint(&mut mesh, &graph.get(cd).cloned().unwrap()));
        assert!(!resolve_constraint(&mut mesh, &graph.get(cang).cloned().unwrap()));
        assert!(!resolve_constraint(&mut mesh, &graph.get(cr).cloned().unwrap()));
        assert_eq!(resolve_all(&mut mesh, &graph), 0, "reference dims drive nothing");

        // Geometry unchanged + max residual 0 (the Angle None-guard fix).
        let g1 = before(&mesh);
        assert!(g1.0.distance(g0.0) < 1e-12 && g1.1.distance(g0.1) < 1e-12 && (g1.2 - g0.2).abs() < 1e-12);
        assert_eq!(max_residual(&mesh, &graph), 0.0, "reference angle must report 0 residual");
    }
}
