//! A path that follows the solid instead of cutting through it.
//!
//! Drawing has always resolved onto one plane. Two points on two different
//! faces of a box therefore get joined by a straight chord, which leaves the
//! surface and passes through the interior — it touches neither face, so
//! neither face is divided. Along the surface, the same two points are joined
//! by a line that bends where it crosses the edge between them.
//!
//! Finding that bend is an unfolding: rotate the second face about the shared
//! edge until it lies flat with the first, draw the straight line in that flat
//! plane, and read off where it meets the edge. Folded back, the crossing point
//! is where the drawn path turns the corner. On two flat faces this is exact,
//! and it is the shortest route across the skin.
//!
//! Adjacent faces only, for now. Two faces that do not touch would need a walk
//! across the intervening ones, and that is a different problem — this module
//! says so rather than guessing (`NoPath::NotAdjacent`).
use crate::{FaceId, MaterialId, Mesh, VertId};
use anyhow::{ensure, Result};
use glam::DVec3;

/// A polyline that stays on the surface.
///
/// `points[i] → points[i + 1]` lies in `faces[i]`, so `faces` is always one
/// shorter than `points`. Every interior point sits on an edge shared by the
/// faces on either side of it.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfacePath {
    pub points: Vec<DVec3>,
    pub faces: Vec<FaceId>,
}

impl SurfacePath {
    pub fn length(&self) -> f64 {
        self.points.windows(2).map(|w| (w[1] - w[0]).length()).sum()
    }
}

/// Why no path was produced. Never a silent `None` — the caller can say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoPath {
    /// A face id that is missing or inactive.
    NoSuchFace,
    /// An endpoint is not on the plane of the face it was given with.
    EndpointOffItsFace,
    /// The two faces do not share an edge. A surface route may still exist by
    /// walking across other faces; this module does not attempt it.
    NotAdjacent,
    /// Unfolded, the straight line misses the shared edge — it would leave the
    /// first face through a different edge, so the route crosses more than the
    /// two faces given.
    LeavesThroughAnotherEdge,
    /// Degenerate input: a zero-length edge, a face without a normal, or two
    /// coincident points.
    Degenerate,
}

const ON_PLANE_TOL: f64 = 1e-6;
/// A crossing is allowed to sit this far past an edge's end before it counts as
/// missing the edge. Slack for the arithmetic, not for reaching a neighbour.
const EDGE_SLACK: f64 = 1e-9;

impl Mesh {
    /// The face whose plane contains `p` and whose boundary encloses it.
    ///
    /// Returns the first match. On a closed solid a surface point normally
    /// belongs to exactly one face; on an edge or a corner it belongs to
    /// several, and which one comes back is then arbitrary — callers that care
    /// should pass the face they picked rather than asking here.
    pub fn find_surface_face(&self, p: DVec3, tol: f64) -> Option<FaceId> {
        self.faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(i, _)| i)
            .find(|&i| {
                crate::operations::face_split::point_on_face_plane(self, i, p)
                    .map(|d| d.abs() <= tol)
                    .unwrap_or(false)
                    && crate::operations::face_split::point_in_face(self, i, p).unwrap_or(false)
            })
    }

    /// The route from `from` to `to` that stays on the surface.
    ///
    /// Same face → the straight segment. Adjacent faces → two legs meeting on
    /// the shared edge. Anything else → [`NoPath`] naming what stopped it.
    pub fn path_along_surface(
        &self,
        from: DVec3,
        from_face: FaceId,
        to: DVec3,
        to_face: FaceId,
    ) -> Result<SurfacePath, NoPath> {
        for f in [from_face, to_face] {
            match self.faces.get(f) {
                Some(face) if face.is_active() => {}
                _ => return Err(NoPath::NoSuchFace),
            }
        }
        if (to - from).length() < ON_PLANE_TOL {
            return Err(NoPath::Degenerate);
        }
        let on_plane = |p: DVec3, f: FaceId| {
            crate::operations::face_split::point_on_face_plane(self, f, p)
                .map(|d| d.abs() <= 1e-3)
                .unwrap_or(false)
        };
        if !on_plane(from, from_face) || !on_plane(to, to_face) {
            return Err(NoPath::EndpointOffItsFace);
        }

        if from_face == to_face {
            return Ok(SurfacePath {
                points: vec![from, to],
                faces: vec![from_face],
            });
        }

        let shared = self.shared_edges(from_face, to_face);
        if shared.is_empty() {
            return Err(NoPath::NotAdjacent);
        }

        // Two faces can meet along more than one edge (a slot, a folded strip).
        // Try each; the one the unfolded line actually crosses is the route.
        let mut last = NoPath::LeavesThroughAnotherEdge;
        for (e0, e1) in shared {
            match self.bend_over_edge(from, from_face, to, e0, e1) {
                Ok(bend) => {
                    // A bend that lands on an endpoint collapses a leg; drop it
                    // rather than emitting a zero-length segment.
                    let mut points = vec![from];
                    let mut faces = vec![from_face];
                    if (bend - from).length() > ON_PLANE_TOL
                        && (to - bend).length() > ON_PLANE_TOL
                    {
                        points.push(bend);
                        faces.push(to_face);
                    }
                    points.push(to);
                    return Ok(SurfacePath { points, faces });
                }
                Err(why) => last = why,
            }
        }
        Err(last)
    }

    /// Edges that belong to both faces' outer loops, as endpoint positions.
    fn shared_edges(&self, a: FaceId, b: FaceId) -> Vec<(DVec3, DVec3)> {
        let ea = self.face_outer_edges(a).unwrap_or_default();
        let eb = self.face_outer_edges(b).unwrap_or_default();
        ea.into_iter()
            .filter(|e| eb.contains(e))
            .filter_map(|e| {
                let ed = self.edges.get(e)?;
                let p0 = self.verts.get(ed.v_small())?.pos();
                let p1 = self.verts.get(ed.v_large())?.pos();
                Some((p0, p1))
            })
            .collect()
    }

    /// Where the unfolded straight line meets the shared edge.
    fn bend_over_edge(
        &self,
        from: DVec3,
        from_face: FaceId,
        to: DVec3,
        e0: DVec3,
        e1: DVec3,
    ) -> Result<DVec3, NoPath> {
        let span = e1 - e0;
        let len = span.length();
        if len < ON_PLANE_TOL {
            return Err(NoPath::Degenerate);
        }
        let d = span / len;

        let n1 = self
            .faces
            .get(from_face)
            .map(|f| f.normal().normalize_or_zero())
            .unwrap_or(DVec3::ZERO);
        if n1.length_squared() < 0.5 {
            return Err(NoPath::Degenerate);
        }

        // In the first face's plane, the direction perpendicular to the edge.
        // Point it away from that face's interior, so crossing the edge means
        // going positive along it.
        let mut w = d.cross(n1).normalize_or_zero();
        if w.length_squared() < 0.5 {
            return Err(NoPath::Degenerate);
        }
        let interior = self.outer_loop_centroid(from_face).ok_or(NoPath::Degenerate)?;
        if (interior - e0).dot(w) > 0.0 {
            w = -w;
        }

        // Unfold `to` about the edge into the first face's plane: its distance
        // from the edge is preserved, only the direction changes.
        let rel = to - e0;
        let along = rel.dot(d);
        let depth = (rel - d * along).length();
        let to_flat = e0 + d * along + w * depth;

        // Meet the edge line, i.e. where the perpendicular component is zero.
        let a_perp = (from - e0).dot(w);
        let b_perp = depth;
        let denom = b_perp - a_perp;
        if denom.abs() < ON_PLANE_TOL {
            return Err(NoPath::Degenerate);
        }
        let s = -a_perp / denom;
        if !(-EDGE_SLACK..=1.0 + EDGE_SLACK).contains(&s) {
            return Err(NoPath::LeavesThroughAnotherEdge);
        }
        let crossing = from + (to_flat - from) * s;

        // It has to land on the edge itself, not on its extension.
        let t = (crossing - e0).dot(d);
        if !(-EDGE_SLACK..=len + EDGE_SLACK).contains(&t) {
            return Err(NoPath::LeavesThroughAnotherEdge);
        }
        Ok(e0 + d * t)
    }

    /// Average of a face's outer-loop vertices. Enough to tell which side of an
    /// edge the face lies on, which is all it is used for here.
    fn outer_loop_centroid(&self, f: FaceId) -> Option<DVec3> {
        let face = self.faces.get(f)?;
        let verts = self.collect_loop_verts(face.outer().start).ok()?;
        if verts.is_empty() {
            return None;
        }
        let sum: DVec3 = verts
            .iter()
            .filter_map(|&v| self.verts.get(v).map(|x| x.pos()))
            .sum();
        Some(sum / verts.len() as f64)
    }

    /// Cut a path that runs across several faces INTO them — the imprint.
    ///
    /// [`Self::path_along_surface`] works out where a route goes; this is the
    /// other half, which makes it geometry. The path arrives already resolved
    /// (`points[i] → points[i+1]` lies in `faces[i]`, every interior point on
    /// the edge between consecutive faces), so the work here is bookkeeping in
    /// the right order rather than any new geometry:
    ///
    /// 1. every point becomes a vertex FIRST, before anything is split. A
    ///    transition point sits on a shared edge and belongs to the chains on
    ///    BOTH sides of it; making it once means the two faces are cut at the
    ///    same vertex instead of at two that merely coincide — the same reason
    ///    a rim needs its pre-split before a shape is drawn over it.
    /// 2. then each face is split by its own run of the chain.
    ///
    /// `split_face_by_chain` already carries the HARD contract (메타-원칙 #15),
    /// so the seams show.
    ///
    /// **v1 limits, taken deliberately and reported rather than hidden:**
    ///
    /// - a run that does not cross its face from boundary to boundary cannot
    ///   divide it, so it is SKIPPED and counted in `skipped_runs`. That is the
    ///   open path's two end faces; a closed band has none, which is why a band
    ///   around a box is the case that works whole.
    /// - a path that visits the same face twice is REFUSED outright: the first
    ///   split replaces that face, and the second run would name an id that no
    ///   longer exists.
    ///
    /// Not self-transacted — the caller wraps it, so the mesh op and whatever
    /// it reconciles land as one undo step and roll back together.
    pub fn imprint_surface_path(
        &mut self,
        path: &SurfacePath,
        material: MaterialId,
    ) -> Result<ImprintReport> {
        ensure!(
            path.points.len() >= 2 && path.faces.len() + 1 == path.points.len(),
            "imprint_surface_path: {} points and {} faces do not describe a path",
            path.points.len(),
            path.faces.len()
        );
        for &f in &path.faces {
            ensure!(
                self.faces.get(f).is_some_and(|x| x.is_active()),
                "imprint_surface_path: face {f:?} is not active"
            );
        }
        // Runs of consecutive segments on one face, in order.
        let mut runs: Vec<(FaceId, usize, usize)> = Vec::new(); // (face, first pt, last pt)
        for (i, &f) in path.faces.iter().enumerate() {
            match runs.last_mut() {
                Some((prev, _, end)) if *prev == f => *end = i + 1,
                _ => runs.push((f, i, i + 1)),
            }
        }
        let mut seen = std::collections::HashSet::new();
        for (f, _, _) in &runs {
            ensure!(
                seen.insert(*f),
                "imprint_surface_path: the path returns to face {f:?}; the first \
                 split replaces it, so the second visit has nothing to cut"
            );
        }

        // Every point once, before any split — see (1) above.
        //
        // A transition point sits ON a shared edge, not at either of its ends,
        // and `split_face_by_chain` cuts between vertices the face's loop
        // ALREADY has. So the edge has to be broken there first, which also
        // gives the vertex to BOTH faces at once — the same pre-split the rim
        // needs before a shape is drawn across it. Measured without this: all
        // four runs of a band round a box were refused and nothing divided.
        let verts: Vec<VertId> = path
            .points
            .iter()
            .map(|&p| self.split_edge_at_or_add_vertex(p))
            .collect();

        // The chain's own edges, before the split. `split_face_by_chain` cuts
        // ALONG edges that already exist — it says so when they do not
        // ("edge between verts N and M missing — caller must draw it first").
        // Drawing them here is the caller doing that.
        for w in verts.windows(2) {
            if w[0] != w[1] {
                let _ = self.add_edge(w[0], w[1]);
            }
        }

        let mut report = ImprintReport::default();
        for (face, first, last) in runs {
            let chain = &verts[first..=last];
            match crate::operations::face_split::split_face_by_chain(self, face, chain, material) {
                Ok(res) => {
                    report.split_faces.push(face);
                    report.new_faces.extend(res.new_faces);
                }
                Err(e) => {
                    report.skipped_runs += 1;
                    if report.first_refusal.is_none() {
                        report.first_refusal = Some(e.to_string());
                    }
                }
            }
        }
        Ok(report)
    }
}

impl Mesh {
    /// The vertex at `p`, breaking an edge to make one if `p` lies along it.
    ///
    /// `add_vertex` alone would put a loose vertex at that position: the loops
    /// on either side of the edge would still run straight past it, and a chain
    /// ending there has nothing to attach to. Splitting the edge gives the
    /// vertex to both faces at once, which is what makes the next step possible.
    ///
    /// An existing vertex, or a point on no edge at all (the interior of a
    /// face), falls through to plain `add_vertex` and its own dedup.
    fn split_edge_at_or_add_vertex(&mut self, p: DVec3) -> VertId {
        const TOL: f64 = 1.5e-3; // the dedup floor (LOCKED #5)
        let target = self
            .edges
            .iter()
            .filter(|(_, e)| e.is_active() && e.curve().is_none())
            .find_map(|(eid, e)| {
                let a = self.verts.get(e.v_small())?.pos();
                let b = self.verts.get(e.v_large())?.pos();
                // An end already — nothing to split.
                if (p - a).length() <= TOL || (p - b).length() <= TOL {
                    return None;
                }
                let span = b - a;
                let len = span.length();
                if len <= TOL {
                    return None;
                }
                let t = (p - a).dot(span) / (len * len);
                if !(0.0..=1.0).contains(&t) {
                    return None;
                }
                ((a + span * t - p).length() <= TOL).then_some(eid)
            });
        match target {
            Some(eid) => match self.split_edge(eid, p) {
                Ok((v, _, _)) => v,
                Err(_) => self.add_vertex(p),
            },
            None => self.add_vertex(p),
        }
    }
}

/// What an imprint did, including what it could not do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImprintReport {
    /// Faces the path cut in two.
    pub split_faces: Vec<FaceId>,
    /// The piece each split left on the far side of the chain.
    pub new_faces: Vec<FaceId>,
    /// Runs that could not divide their face — an end of an open path, which
    /// enters but never leaves. Counted rather than silently dropped.
    pub skipped_runs: usize,
    /// Why the first skipped run was refused. A count alone says something was
    /// dropped without saying what to do about it.
    pub first_refusal: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MaterialId;

    /// Box spanning x 0..200, y 0..200, z 0..100.
    fn boxy() -> Mesh {
        let mut m = Mesh::new();
        m.create_box(
            DVec3::new(100.0, 100.0, 50.0),
            200.0,
            100.0,
            200.0,
            MaterialId::new(0),
        )
        .unwrap();
        m
    }

    fn face_at(m: &Mesh, n: DVec3, through: DVec3) -> FaceId {
        m.faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(i, _)| i)
            .find(|&i| {
                let f = m.faces.get(i).unwrap();
                f.normal().normalize_or_zero().dot(n) > 0.999
                    && crate::operations::face_split::point_on_face_plane(m, i, through)
                        .map(|d| d.abs() < 1e-6)
                        .unwrap_or(false)
            })
            .expect("face")
    }

    #[test]
    fn on_one_face_the_path_is_the_straight_segment() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        let p = m
            .path_along_surface(
                DVec3::new(50.0, 50.0, 100.0),
                top,
                DVec3::new(150.0, 150.0, 100.0),
                top,
            )
            .unwrap();
        assert_eq!(p.points.len(), 2);
        assert_eq!(p.faces, vec![top]);
    }

    #[test]
    fn over_an_edge_the_path_bends_on_the_edge() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        let east = face_at(&m, DVec3::X, DVec3::new(200.0, 0.0, 0.0));

        let a = DVec3::new(100.0, 100.0, 100.0);
        let b = DVec3::new(200.0, 100.0, 50.0);
        let p = m.path_along_surface(a, top, b, east).unwrap();

        assert_eq!(p.points.len(), 3, "{:?}", p.points);
        assert_eq!(p.faces, vec![top, east]);
        let bend = p.points[1];
        assert!(
            (bend.x - 200.0).abs() < 1e-9 && (bend.z - 100.0).abs() < 1e-9,
            "the bend must sit on the shared edge x=200,z=100: {bend:?}"
        );
        // Unfolded this is one straight 150 mm run, and the chord is shorter.
        assert!((p.length() - 150.0).abs() < 1e-6, "length {}", p.length());
        assert!(p.length() > (b - a).length());
    }

    /// The point of the whole thing: the path stays on the skin.
    #[test]
    fn every_point_of_the_path_lies_on_the_solid() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        let east = face_at(&m, DVec3::X, DVec3::new(200.0, 0.0, 0.0));
        let p = m
            .path_along_surface(
                DVec3::new(40.0, 70.0, 100.0),
                top,
                DVec3::new(200.0, 130.0, 30.0),
                east,
            )
            .unwrap();

        for w in p.points.windows(2) {
            for k in 0..=10 {
                let q = w[0] + (w[1] - w[0]) * (k as f64 / 10.0);
                let on_skin = (q.z - 100.0).abs() < 1e-6 || (q.x - 200.0).abs() < 1e-6;
                assert!(on_skin, "sample {q:?} left the surface");
                assert!(
                    q.x <= 200.0 + 1e-6 && q.z <= 100.0 + 1e-6,
                    "sample {q:?} is outside the box"
                );
            }
        }
    }

    #[test]
    fn the_bend_moves_with_the_endpoints() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        let east = face_at(&m, DVec3::X, DVec3::new(200.0, 0.0, 0.0));
        // Start at y=50 and end at y=150: unfolded the run is 100 across and
        // 100 along, so the bend sits halfway in y.
        let p = m
            .path_along_surface(
                DVec3::new(100.0, 50.0, 100.0),
                top,
                DVec3::new(200.0, 150.0, 50.0),
                east,
            )
            .unwrap();
        let bend = p.points[1];
        assert!(
            (bend.y - 116.666_666_666).abs() < 1e-6,
            "unfolded, the crossing is 100/150 of the way in y: {bend:?}"
        );
    }

    #[test]
    fn opposite_faces_are_not_adjacent_and_it_says_so() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        let bottom = face_at(&m, -DVec3::Z, DVec3::new(0.0, 0.0, 0.0));
        assert_eq!(
            m.path_along_surface(
                DVec3::new(100.0, 100.0, 100.0),
                top,
                DVec3::new(100.0, 100.0, 0.0),
                bottom
            ),
            Err(NoPath::NotAdjacent)
        );
    }

    #[test]
    fn an_endpoint_off_its_face_is_refused() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        let east = face_at(&m, DVec3::X, DVec3::new(200.0, 0.0, 0.0));
        assert_eq!(
            m.path_along_surface(
                DVec3::new(100.0, 100.0, 140.0), // floating above the top face
                top,
                DVec3::new(200.0, 100.0, 50.0),
                east
            ),
            Err(NoPath::EndpointOffItsFace)
        );
    }

    /// The shared edge is not always the whole side. Here the top face's east
    /// boundary is split at y=4, and the wall hangs from only the lower part, so
    /// the two faces meet along y∈[0,4]. A line aimed just past y=4 unfolds into
    /// a straight run that misses that short edge — it would leave the top face
    /// through the edge above it and cross a third face, which this module does
    /// not route.
    ///
    /// On a plain box this can never happen: two rectangles sharing a full edge
    /// always meet, because the unfolded crossing is a convex blend of the two
    /// endpoints. That is why the case has to be built rather than found.
    #[test]
    fn a_route_that_needs_a_third_face_says_so() {
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
        let a0 = m.add_vertex(p(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(p(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(p(10.0, 4.0, 0.0)); // the split point
        let a3 = m.add_vertex(p(10.0, 10.0, 0.0));
        let a4 = m.add_vertex(p(0.0, 10.0, 0.0));
        let top = m.add_face(&[a0, a1, a2, a3, a4], mat).unwrap();

        let b1 = m.add_vertex(p(10.0, 0.0, -6.0));
        let b2 = m.add_vertex(p(10.0, 4.0, -6.0));
        let wall = m.add_face(&[a1, b1, b2, a2], mat).unwrap();

        // Sanity: they meet along exactly the short edge.
        assert_eq!(m.shared_edges(top, wall).len(), 1);

        // Aimed at y=3.5 on the wall, but from far enough north that the
        // unfolded line crosses x=10 at y≈4.05 — just past the edge's end.
        assert_eq!(
            m.path_along_surface(p(1.0, 9.0, 0.0), top, p(10.0, 3.5, -1.0), wall),
            Err(NoPath::LeavesThroughAnotherEdge)
        );

        // Aimed a little lower it does meet the edge, so the refusal above is
        // about where the line goes and not about this fixture being unusable.
        let ok = m
            .path_along_surface(p(1.0, 9.0, 0.0), top, p(10.0, 1.0, -5.0), wall)
            .expect("this one crosses the short edge");
        let bend = ok.points[1];
        assert!(
            (bend.x - 10.0).abs() < 1e-9 && bend.y >= 0.0 && bend.y <= 4.0,
            "the bend must land on the short shared edge: {bend:?}"
        );
    }

    /// Which way is "across the edge" depends on which side the face is on, and
    /// the edge's own direction says nothing about that — it follows vertex ids.
    /// Here the face lies to the east of the shared edge, the mirror of the box
    /// fixture, so the perpendicular has to be flipped before unfolding. Without
    /// the flip the second point unfolds onto the wrong side and the line never
    /// reaches the edge.
    #[test]
    fn the_face_may_lie_on_either_side_of_the_shared_edge() {
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
        // Interior at x > 10, so the outward direction is -X.
        let a0 = m.add_vertex(p(10.0, 0.0, 0.0));
        let a1 = m.add_vertex(p(20.0, 0.0, 0.0));
        let a2 = m.add_vertex(p(20.0, 10.0, 0.0));
        let a3 = m.add_vertex(p(10.0, 10.0, 0.0));
        let top = m.add_face(&[a0, a1, a2, a3], mat).unwrap();

        let w1 = m.add_vertex(p(10.0, 0.0, -6.0));
        let w2 = m.add_vertex(p(10.0, 10.0, -6.0));
        let wall = m.add_face(&[a0, w1, w2, a3], mat).unwrap();

        let path = m
            .path_along_surface(p(15.0, 5.0, 0.0), top, p(10.0, 5.0, -3.0), wall)
            .expect("the wall is right there across the edge");
        assert_eq!(path.points.len(), 3, "{:?}", path.points);
        let bend = path.points[1];
        assert!(
            (bend - p(10.0, 5.0, 0.0)).length() < 1e-9,
            "straight across, so the bend is directly between the two: {bend:?}"
        );
        assert!((path.length() - 8.0).abs() < 1e-9, "5 across + 3 down");
    }

    #[test]
    fn a_surface_point_finds_its_face() {
        let m = boxy();
        let top = face_at(&m, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
        assert_eq!(
            m.find_surface_face(DVec3::new(100.0, 100.0, 100.0), 1e-6),
            Some(top)
        );
        assert_eq!(
            m.find_surface_face(DVec3::new(100.0, 100.0, 140.0), 1e-6),
            None
        );
    }
}
