//! TWO OVERLAPPING CIRCLES DIVIDE INTO THREE REGIONS.
//!
//! Measured 2026-08-05. On a SHEET, two circles that overlap already resolve
//! into a lens and two crescents with their rims kept as arcs — the coplanar
//! re-derive feeds both curves to `boundary_kernel::analytic_arrange`, which
//! handles circle × circle exactly. On a SOLID's face the same draw is REFUSED:
//! that re-derive is deliberately skipped wherever the affected region touches
//! a solid, because re-arranging the region there re-cuts edges the solid's
//! walls are standing on.
//!
//! So both circles instead become holes of the cap, by the containment path,
//! and nothing downstream can tell that the two hole loops overlap each other.
//! Earcut cannot represent overlapping holes, so the cap's triangles land inside
//! the disks, the overlap detector reports three pairs, and the draw guard rolls
//! the whole thing back. The repair could not help either: a closed curve is one
//! anchor vertex, so neither the containment punch nor the polygon difference
//! walk has a polygon to work with.
//!
//! What this does instead is build the four arcs directly. Two circles that
//! cross meet at exactly two points, and those points cut each rim in two:
//!
//! ```text
//!   A_in   the part of A's rim inside B      A_out  the rest of A's rim
//!   B_in   the part of B's rim inside A      B_out  the rest of B's rim
//!
//!   lens        = A_in  + B_in           the region inside both
//!   crescent A  = A_out + B_in reversed  A without B
//!   crescent B  = B_out + A_in reversed  B without A
//!   union       = A_out + B_out          what the container's hole becomes
//! ```
//!
//! Why each piece is traversed the way it is, rather than by trial: on the lens
//! boundary the lens lies to the left along both arcs, and a circle traversed
//! counter-clockwise has its disk on the left — so both halves are taken in
//! their own counter-clockwise direction, and being one closed curve they must
//! meet head to tail. On a crescent the region is OUTSIDE the other circle, so
//! that circle's arc is walked the other way. The chaining is checked at runtime
//! all the same; a mismatch means the reasoning above did not hold for this
//! input and nothing is built.
//!
//! Each arc is split at its midpoint before becoming edges. A two-arc region
//! would otherwise be a two-edge bigon between the same pair of vertices, which
//! the DCEL cannot hold — the same split the re-derive materializer makes, for
//! the same reason.
use crate::mesh::Mesh;
use crate::{FaceId, MaterialId, VertId};
use anyhow::Result;
use glam::DVec3;

const TAU: f64 = std::f64::consts::TAU;

/// Below this the two circles are treated as touching rather than crossing, and
/// the region between them is too thin to build a face from. In mm, on the same
/// order as the vertex dedup floor (LOCKED #5, 1.5 μm).
const CROSS_EPS: f64 = 1.0e-3;

/// What the split produced.
#[derive(Debug, Clone)]
pub struct CircleOverlapSplit {
    /// The region inside both circles.
    pub lens: FaceId,
    /// Circle A without circle B.
    pub crescent_a: FaceId,
    /// Circle B without circle A.
    pub crescent_b: FaceId,
    /// The container both circles were holes of, if there was one: the face
    /// that was taken out, and the one holding the single union hole that
    /// replaced them.
    pub parent: Option<(FaceId, FaceId)>,
}

/// One arc, as a span of angles in a frame shared by both circles.
#[derive(Clone, Copy, Debug)]
struct Span {
    center: DVec3,
    radius: f64,
    from: f64,
    to: f64,
    /// Shared by an arc's two halves so the rim reads as one thing to pick
    /// (ADR-088), and shared by the same arc wherever it is reused — the union
    /// hole and the crescent are two faces along ONE arc.
    owner: u32,
}

impl Span {
    fn reversed(self) -> Self {
        Span { from: self.to, to: self.from, ..self }
    }
    fn at(&self, u: DVec3, v: DVec3, a: f64) -> DVec3 {
        self.center + u * (self.radius * a.cos()) + v * (self.radius * a.sin())
    }
    fn start_pt(&self, u: DVec3, v: DVec3) -> DVec3 {
        self.at(u, v, self.from)
    }
    fn end_pt(&self, u: DVec3, v: DVec3) -> DVec3 {
        self.at(u, v, self.to)
    }
}

/// The counter-clockwise span from `from` to `to`.
fn ccw(from: f64, to: f64) -> (f64, f64) {
    let mut t = to;
    while t <= from + 1e-12 {
        t += TAU;
    }
    (from, t)
}

/// Does this chain of arcs close, each one starting where the last ended?
fn chains(spans: &[Span], u: DVec3, v: DVec3) -> bool {
    let n = spans.len();
    (0..n).all(|i| {
        (spans[i].end_pt(u, v) - spans[(i + 1) % n].start_pt(u, v)).length() < CROSS_EPS
    })
}

/// Arcs → loop vertices, each arc split at its midpoint (see the module note).
fn lay_out(
    mesh: &mut Mesh,
    spans: &[Span],
    u: DVec3,
    v: DVec3,
) -> (Vec<VertId>, Vec<(DVec3, f64, f64, f64, Option<u32>)>) {
    let mut verts = Vec::with_capacity(spans.len() * 2);
    let mut arcs = Vec::with_capacity(spans.len() * 2);
    for s in spans {
        let mid = (s.from + s.to) / 2.0;
        verts.push(mesh.add_vertex(s.at(u, v, s.from)));
        arcs.push((s.center, s.radius, s.from, mid, Some(s.owner)));
        verts.push(mesh.add_vertex(s.at(u, v, mid)));
        arcs.push((s.center, s.radius, mid, s.to, Some(s.owner)));
    }
    (verts, arcs)
}

/// Build one region and hang its arcs on the edges it just made.
fn build_region(
    mesh: &mut Mesh,
    spans: &[Span],
    normal: DVec3,
    u: DVec3,
    v: DVec3,
    material: MaterialId,
    surface: Option<crate::surfaces::AnalyticSurface>,
) -> Option<FaceId> {
    let (verts, arcs) = lay_out(mesh, spans, u, v);
    if verts.len() < 3 {
        return None;
    }
    let fid = mesh.add_face_with_holes(&verts, &[], material).ok()?;
    if let Some(s) = surface {
        mesh.set_face_surface(fid, Some(s));
    }
    attach_arcs(mesh, &verts, &arcs, normal, u);
    let mut closed = verts.clone();
    closed.push(verts[0]);
    mesh.mark_chain_edges_hard(&closed);
    Some(fid)
}

/// Hang each arc on the edge between the two vertices it spans.
fn attach_arcs(
    mesh: &mut Mesh,
    verts: &[VertId],
    arcs: &[(DVec3, f64, f64, f64, Option<u32>)],
    normal: DVec3,
    basis_u: DVec3,
) {
    let n = verts.len();
    for i in 0..n {
        let (c, r, a0, a1, owner) = arcs[i];
        crate::operations::face_rederive::set_arc_on_edge(
            mesh,
            verts[i],
            verts[(i + 1) % n],
            c,
            r,
            normal,
            basis_u,
            a0,
            a1,
            owner,
        );
    }
}

/// A circle face's centre, radius and plane — the closed-curve form only
/// (ADR-089 Phase 2: one anchor vertex carrying one self-loop `Circle`).
fn circle_of(mesh: &Mesh, fid: FaceId) -> Option<(DVec3, f64, DVec3, crate::EdgeId, VertId)> {
    let face = mesh.faces.get(fid).filter(|f| f.is_active())?;
    let start = face.outer().start;
    if start.is_null() {
        return None;
    }
    let hes = mesh.collect_loop_hes(start).ok()?;
    if hes.len() != 1 {
        return None;
    }
    let eid = mesh.hes.get(hes[0])?.edge();
    let edge = mesh.edges.get(eid).filter(|e| e.is_self_loop())?;
    match edge.curve()? {
        crate::curves::AnalyticCurve::Circle { center, radius, normal, .. } => {
            Some((*center, *radius, *normal, eid, edge.v_small()))
        }
        _ => None,
    }
}

/// The face that fills a hole loop, when that loop is a closed curve's rim.
fn face_filling(mesh: &Mesh, hole_start: crate::HeId) -> Option<FaceId> {
    let hes = mesh.collect_loop_hes(hole_start).ok()?;
    if hes.len() != 1 {
        return None;
    }
    let eid = mesh.hes.get(hes[0])?.edge();
    mesh.faces.iter().find_map(|(fid, f)| {
        if !f.is_active() || f.outer().start.is_null() {
            return None;
        }
        let inner = mesh.collect_loop_hes(f.outer().start).ok()?;
        (inner.len() == 1 && mesh.hes.get(inner[0])?.edge() == eid).then_some(fid)
    })
}

/// The face holding `fid` as a hole, if one does.
fn hole_parent(mesh: &Mesh, fid: FaceId) -> Option<FaceId> {
    let start = mesh.faces.get(fid).filter(|f| f.is_active())?.outer().start;
    let hes = mesh.collect_loop_hes(start).ok()?;
    if hes.len() != 1 {
        return None;
    }
    let eid = mesh.hes.get(hes[0])?.edge();
    mesh.faces.iter().find_map(|(pid, p)| {
        if pid == fid || !p.is_active() {
            return None;
        }
        p.inners()
            .iter()
            .any(|lr| {
                mesh.collect_loop_hes(lr.start)
                    .ok()
                    .map_or(false, |h| h.len() == 1 && mesh.hes.get(h[0]).map(|x| x.edge()) == Some(eid))
            })
            .then_some(pid)
    })
}

/// What the container looked like before, so it can be put back with one union
/// hole in place of the two overlapping ones.
struct ParentPlan {
    fid: FaceId,
    outer: Vec<VertId>,
    material: MaterialId,
    surface: Option<crate::surfaces::AnalyticSurface>,
    /// Holes that are ordinary loops, carried across as-is.
    polygon_holes: Vec<Vec<VertId>>,
    /// Holes that are some OTHER closed curve's rim. A one-vertex loop cannot be
    /// rebuilt from a vertex list, so the disk filling it is re-punched into the
    /// new container afterwards, by the same path that put it there.
    curve_holes: Vec<FaceId>,
}

fn plan_parent(mesh: &Mesh, pid: FaceId, a: FaceId, b: FaceId) -> Option<ParentPlan> {
    let face = mesh.faces.get(pid).filter(|f| f.is_active())?;
    let outer = mesh.collect_loop_verts(face.outer().start).ok()?;
    if outer.len() < 3 {
        return None;
    }
    let mut polygon_holes = Vec::new();
    let mut curve_holes = Vec::new();
    for lr in face.inners() {
        let verts = mesh.collect_loop_verts(lr.start).ok()?;
        if verts.len() >= 3 {
            polygon_holes.push(verts);
            continue;
        }
        // A one-vertex hole: whose rim?
        match face_filling(mesh, lr.start) {
            Some(f) if f == a || f == b => {} // the two being replaced
            Some(f) => curve_holes.push(f),
            None => return None, // a rim with nothing behind it — leave well alone
        }
    }
    Some(ParentPlan {
        fid: pid,
        outer,
        material: face.material(),
        surface: face.surface().cloned(),
        polygon_holes,
        curve_holes,
    })
}

/// Take out a closed-curve face along with the rim edge and anchor nothing else
/// is using (ADR-089 A-υ — a left-behind self-loop still draws as a chord).
fn drop_circle_face(mesh: &mut Mesh, fid: FaceId, eid: crate::EdgeId, anchor: VertId) -> Result<()> {
    mesh.remove_face(fid)?;
    if mesh.edges.contains(eid) && mesh.edges[eid].is_active() {
        let _ = mesh.remove_edge_and_halfedges(eid);
    }
    if mesh.verts.contains(anchor)
        && mesh.verts[anchor].is_active()
        && mesh.verts[anchor].outgoing().is_none()
    {
        mesh.verts[anchor].set_active(false);
    }
    Ok(())
}

/// Split two overlapping circle faces into the lens and the two crescents,
/// and give whatever held them as holes a single union hole instead.
///
/// `None`, having changed nothing, unless both faces really are closed-curve
/// circles on one plane whose rims cross at two points, and both are loose or
/// both are holes of the same container.
pub fn split_overlapping_circles(
    mesh: &mut Mesh,
    fa: FaceId,
    fb: FaceId,
) -> Option<CircleOverlapSplit> {
    if fa == fb {
        return None;
    }
    let (ca, ra, na, ea, anchor_a) = circle_of(mesh, fa)?;
    let (cb, rb, nb, eb, anchor_b) = circle_of(mesh, fb)?;

    // One plane: parallel rims, and B's centre on A's.
    let n = na.normalize_or_zero();
    if n.length_squared() < 0.5 || n.dot(nb.normalize_or_zero()).abs() < 0.999 {
        return None;
    }
    if (cb - ca).dot(n).abs() > CROSS_EPS {
        return None;
    }

    // Crossing at two points, and clear of both touching cases.
    let d_vec = cb - ca;
    let d = d_vec.length();
    if d <= (ra - rb).abs() + CROSS_EPS || d >= ra + rb - CROSS_EPS {
        return None;
    }
    let d_hat = d_vec / d;
    let along = (d * d + ra * ra - rb * rb) / (2.0 * d);
    let h2 = ra * ra - along * along;
    if h2 <= CROSS_EPS * CROSS_EPS {
        return None;
    }
    let h = h2.sqrt();
    let perp = n.cross(d_hat).normalize_or_zero();
    if perp.length_squared() < 0.5 {
        return None;
    }
    let base = ca + d_hat * along;
    let p = base + perp * h;
    let q = base - perp * h;

    // One frame for both circles, matching the arc curve's own convention
    // (`curves::basis_v` = normal × basis_u).
    let u = d_hat;
    let v = n.cross(u).normalize_or_zero();
    let angle = |c: DVec3, pt: DVec3| -> f64 {
        let r = pt - c;
        r.dot(v).atan2(r.dot(u))
    };

    let owner_a_in = mesh.next_curve_owner_id();
    let owner_a_out = mesh.next_curve_owner_id();
    let owner_b_in = mesh.next_curve_owner_id();
    let owner_b_out = mesh.next_curve_owner_id();

    let span = |c: DVec3, r: f64, from: f64, to: f64, owner: u32| {
        let (f, t) = ccw(from, to);
        Span { center: c, radius: r, from: f, to: t, owner }
    };
    let inside = |s: &Span, c: DVec3, r: f64| {
        (s.at(u, v, (s.from + s.to) / 2.0) - c).length() < r
    };

    // Of a rim's two arcs, exactly one runs inside the other circle.
    let (ap, aq) = (angle(ca, p), angle(ca, q));
    let a1 = span(ca, ra, ap, aq, owner_a_in);
    let a2 = span(ca, ra, aq, ap, owner_a_in);
    let (a_in, mut a_out) = if inside(&a1, cb, rb) { (a1, a2) } else { (a2, a1) };
    if inside(&a_out, cb, rb) || !inside(&a_in, cb, rb) {
        return None; // not the two-arc picture this is built on
    }
    a_out.owner = owner_a_out;

    let (bp, bq) = (angle(cb, p), angle(cb, q));
    let b1 = span(cb, rb, bp, bq, owner_b_in);
    let b2 = span(cb, rb, bq, bp, owner_b_in);
    let (b_in, mut b_out) = if inside(&b1, ca, ra) { (b1, b2) } else { (b2, b1) };
    if inside(&b_out, ca, ra) || !inside(&b_in, ca, ra) {
        return None;
    }
    b_out.owner = owner_b_out;

    let lens_spans = [a_in, b_in];
    let cres_a_spans = [a_out, b_in.reversed()];
    let cres_b_spans = [b_out, a_in.reversed()];
    let union_spans = [a_out, b_out];
    for chain in [&lens_spans, &cres_a_spans, &cres_b_spans, &union_spans] {
        if !chains(chain, u, v) {
            return None;
        }
    }

    // Both loose, or both holes of the same face. Anything else is not a picture
    // this knows how to put back together.
    let (pa, pb) = (hole_parent(mesh, fa), hole_parent(mesh, fb));
    if pa != pb {
        return None;
    }
    let plan = match pa {
        Some(pid) => Some(plan_parent(mesh, pid, fa, fb)?),
        None => None,
    };

    let material = mesh.faces.get(fa).map(|f| f.material())?;
    let surface = mesh
        .faces
        .get(fa)
        .and_then(|f| f.surface().cloned())
        .or_else(|| plan.as_ref().and_then(|p| p.surface.clone()));

    // From here the mesh is changed. Every failure below leaves it part-built,
    // which is what the caller's rollback is for.
    if let Some(p) = &plan {
        mesh.remove_face(p.fid).ok()?;
    }
    drop_circle_face(mesh, fa, ea, anchor_a).ok()?;
    drop_circle_face(mesh, fb, eb, anchor_b).ok()?;

    let parent = match &plan {
        Some(p) => {
            let (union_verts, union_arcs) = lay_out(mesh, &union_spans, u, v);
            let mut holes: Vec<&[VertId]> =
                p.polygon_holes.iter().map(|h| h.as_slice()).collect();
            holes.push(&union_verts);
            let new_pid = mesh.add_face_with_holes(&p.outer, &holes, p.material).ok()?;
            if let Some(s) = &p.surface {
                mesh.set_face_surface(new_pid, Some(s.clone()));
            }
            attach_arcs(mesh, &union_verts, &union_arcs, n, u);
            let mut closed = union_verts.clone();
            closed.push(union_verts[0]);
            mesh.mark_chain_edges_hard(&closed);
            Some((p.fid, new_pid))
        }
        None => None,
    };

    let lens = build_region(mesh, &lens_spans, n, u, v, material, surface.clone())?;
    let crescent_a = build_region(mesh, &cres_a_spans, n, u, v, material, surface.clone())?;
    let crescent_b = build_region(mesh, &cres_b_spans, n, u, v, material, surface)?;

    // Any other closed-curve hole the container had is put back the way it was
    // put there in the first place.
    if let (Some(p), Some((_, new_pid))) = (&plan, &parent) {
        for &disk in &p.curve_holes {
            let _ = crate::operations::annulus::split_face_by_inner_circle_generic(
                mesh, *new_pid, disk,
            );
        }
    }

    Some(CircleOverlapSplit { lens, crescent_a, crescent_b, parent })
}
