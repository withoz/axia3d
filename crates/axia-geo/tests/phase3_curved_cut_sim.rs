//! Phase 3 measurement sim — do the two CURVED-CUT ops corrupt a closed
//! curved solid (cylinder / sphere)?
//!
//! Ops:
//!   - Mesh::cut_curved_by_z_plane(faces, z, CurvedCutMode, material)
//!   - Mesh::trim_curved_by_plane(faces, plane_origin, plane_normal, material)
//!
//! Read-only measurement — the test always passes; it prints a before->after
//! table. Run: cargo test -p axia-geo --test phase3_curved_cut_sim -- --nocapture

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use axia_geo::operations::boolean::CurvedCutMode;
use axia_geo::entities::id::FaceId;
use glam::DVec3;

fn active_faces(mesh: &Mesh) -> Vec<FaceId> {
    mesh.faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect()
}

#[derive(Clone, Copy)]
struct Snapshot {
    closed: bool,
    be: usize,
    nm: usize,
    si: usize,
    inv_valid: bool,
    inv_violations: usize,
    face_count: usize,
}

fn snapshot(mesh: &Mesh, faces: &[FaceId]) -> Snapshot {
    let mi = mesh.face_set_manifold_info(faces);
    let inv = mesh.verify_face_invariants();
    let si = mesh.detect_self_intersections();
    Snapshot {
        closed: mi.is_closed_solid,
        be: mi.boundary_edge_count,
        nm: mi.non_manifold_edge_count,
        si: si.count(),
        inv_valid: inv.is_valid(),
        inv_violations: inv.violations.len(),
        face_count: faces.len(),
    }
}

fn fmt(s: &Snapshot) -> String {
    format!(
        "closed={} bE={} nm={} SI={} inv={}({}) faces={}",
        s.closed,
        s.be,
        s.nm,
        s.si,
        if s.inv_valid { "valid" } else { "INVALID" },
        s.inv_violations,
        s.face_count,
    )
}

/// Classify a scenario outcome. `half_space` = the op is *supposed* to open a
/// face and re-cap it (still should be a closed solid). All our cut ops of a
/// closed solid should remain closed.
fn classify(before: &Snapshot, kind: &str, after: Option<&Snapshot>) -> String {
    match kind {
        "None" => "DECLINES".to_string(),
        "Err" => "SELF-REJECTS".to_string(),
        "Ok" => {
            let a = after.unwrap();
            let before_watertight = before.be == 0 && before.nm == 0;
            let mut bad = vec![];
            // A cut of a closed/watertight solid should re-cap and stay
            // watertight. bE>0 after starting from watertight = a leak.
            if before_watertight && a.be > 0 {
                bad.push(format!("bE 0->{} (leak/open)", a.be));
            }
            if a.nm > before.nm {
                bad.push(format!("nm {}->{} (non-manifold)", before.nm, a.nm));
            }
            if a.si > before.si {
                bad.push(format!("SI {}->{} (self-intersect)", before.si, a.si));
            }
            if before.inv_valid && !a.inv_valid {
                bad.push(format!("inv valid->INVALID ({})", a.inv_violations));
            }
            if bad.is_empty() {
                "SAFE".to_string()
            } else {
                format!("CORRUPTS [{}]", bad.join(", "))
            }
        }
        _ => "??".to_string(),
    }
}

fn fresh_cylinder(radius: f64, height: f64) -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    let mat = MaterialId::new(0);
    mesh.create_cylinder_kernel_native_clean(DVec3::new(0., 0., 0.), radius, height, mat)
        .expect("cylinder build");
    let faces = active_faces(&mesh);
    (mesh, faces)
}

fn fresh_sphere(radius: f64) -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    let mat = MaterialId::new(0);
    mesh.create_sphere_kernel_native(DVec3::new(0., 0., 0.), radius, mat)
        .expect("sphere build");
    let faces = active_faces(&mesh);
    (mesh, faces)
}

#[test]
fn phase3_curved_cut_simulation() {
    let mat = MaterialId::new(0);
    println!("\n================ PHASE 3 CURVED-CUT SIM ================\n");

    // Base solids. NOTE (finding): create_cylinder_kernel_native_clean /
    // create_sphere_kernel_native are Path B (ADR-094/104) kernel-native
    // solids built from self-loop closed curves. They are WATERTIGHT
    // (bE=0, nm=0, SI=0) but face_set_manifold_info reports is_closed_solid
    // = false because it requires active_faces >= 4 (mesh.rs:9994) and these
    // have only 3 (cylinder) / 2 (sphere) faces. So "closed T->F" cannot be a
    // corruption signal here; bE / nm / SI / invariants are the real signals
    // (a watertight solid staying watertight keeps bE=0, nm=0, SI=0).
    {
        let (m, f) = fresh_cylinder(200.0, 400.0);
        let s = snapshot(&m, &f);
        println!("BASE cylinder r=200 h=400 (z in [0,400]): {}", fmt(&s));
        println!("  (watertight = bE==0 && nm==0; is_closed_solid needs >=4 faces so it reads false)");
    }
    {
        let (m, f) = fresh_sphere(200.0);
        let s = snapshot(&m, &f);
        println!("BASE sphere   r=200 (z in [-200,200]):    {}", fmt(&s));
    }
    println!();

    let mid = 200.0;      // cylinder z-center
    let top = 400.0;      // cylinder top rim
    let way_above = 5000.0;

    // ---- Scenario table for cut_curved_by_z_plane on a fresh cylinder ----
    struct ZScen {
        name: &'static str,
        z: f64,
        mode: CurvedCutMode,
    }
    let z_scenarios = [
        ZScen { name: "1. cut z=mid  Slice",      z: mid,       mode: CurvedCutMode::Slice },
        ZScen { name: "2. cut z=mid  KeepAbove",  z: mid,       mode: CurvedCutMode::KeepAbove },
        ZScen { name: "3. cut z=WAY_ABOVE KeepAbove (grazing/miss)", z: way_above, mode: CurvedCutMode::KeepAbove },
        ZScen { name: "4. cut z=top  Slice (tangent to top rim)",    z: top,       mode: CurvedCutMode::Slice },
    ];

    println!("---- cut_curved_by_z_plane (fresh cylinder each) ----");
    for sc in &z_scenarios {
        let (mut mesh, faces) = fresh_cylinder(200.0, 400.0);
        let before = snapshot(&mesh, &faces);
        let res = mesh.cut_curved_by_z_plane(&faces, sc.z, sc.mode, mat);
        let (kind, after) = match res {
            None => ("None", None),
            Some(Err(ref e)) => {
                println!("  {}\n    result: Some(Err) : {}", sc.name, e);
                ("Err", None)
            }
            Some(Ok(_)) => {
                let cur = active_faces(&mesh);
                let a = snapshot(&mesh, &cur);
                ("Ok", Some(a))
            }
        };
        let verdict = classify(&before, kind, after.as_ref());
        println!("  {}", sc.name);
        println!("    before: {}", fmt(&before));
        match kind {
            "Ok"  => println!("    after : {}", fmt(after.as_ref().unwrap())),
            "None"=> println!("    after : (declined, no mutation)"),
            "Err" => println!("    after : (self-rejected, no mutation)"),
            _ => {}
        }
        println!("    => {}\n", verdict);
    }

    // ---- trim_curved_by_plane on a fresh cylinder ----
    struct TScen {
        name: &'static str,
        origin: DVec3,
        normal: DVec3,
    }
    let t_scenarios = [
        TScen {
            name: "5. trim origin=(0,0,mid) n=(0,0,1) axial",
            origin: DVec3::new(0., 0., mid),
            normal: DVec3::new(0., 0., 1.),
        },
        TScen {
            name: "6. trim origin=(0,0,mid) n=(1,0,0.3).norm OBLIQUE",
            origin: DVec3::new(0., 0., mid),
            normal: DVec3::new(1., 0., 0.3).normalize(),
        },
    ];

    println!("---- trim_curved_by_plane (fresh cylinder each) ----");
    for sc in &t_scenarios {
        let (mut mesh, faces) = fresh_cylinder(200.0, 400.0);
        let before = snapshot(&mesh, &faces);
        let res = mesh.trim_curved_by_plane(&faces, sc.origin, sc.normal, mat);
        let (kind, after) = match res {
            None => ("None", None),
            Some(Err(ref e)) => {
                println!("  {}\n    result: Some(Err) : {}", sc.name, e);
                ("Err", None)
            }
            Some(Ok(_)) => {
                let cur = active_faces(&mesh);
                let a = snapshot(&mesh, &cur);
                ("Ok", Some(a))
            }
        };
        let verdict = classify(&before, kind, after.as_ref());
        println!("  {}", sc.name);
        println!("    before: {}", fmt(&before));
        match kind {
            "Ok"  => println!("    after : {}", fmt(after.as_ref().unwrap())),
            "None"=> println!("    after : (declined, no mutation)"),
            "Err" => println!("    after : (self-rejected, no mutation)"),
            _ => {}
        }
        println!("    => {}\n", verdict);
    }

    // ---- Sphere scenario ----
    println!("---- cut_curved_by_z_plane on SPHERE (fresh each) ----");
    let sphere_scenarios = [
        ("7a. sphere cut z=50 Slice",     50.0, CurvedCutMode::Slice),
        ("7b. sphere cut z=50 KeepAbove", 50.0, CurvedCutMode::KeepAbove),
    ];
    for (name, z, mode) in sphere_scenarios {
        let (mut mesh, faces) = fresh_sphere(200.0);
        let before = snapshot(&mesh, &faces);
        let res = mesh.cut_curved_by_z_plane(&faces, z, mode, mat);
        let (kind, after) = match res {
            None => ("None", None),
            Some(Err(ref e)) => {
                println!("  {}\n    result: Some(Err) : {}", name, e);
                ("Err", None)
            }
            Some(Ok(_)) => {
                let cur = active_faces(&mesh);
                let a = snapshot(&mesh, &cur);
                ("Ok", Some(a))
            }
        };
        let verdict = classify(&before, kind, after.as_ref());
        println!("  {}", name);
        println!("    before: {}", fmt(&before));
        match kind {
            "Ok"  => println!("    after : {}", fmt(after.as_ref().unwrap())),
            "None"=> println!("    after : (declined, no mutation)"),
            "Err" => println!("    after : (self-rejected, no mutation)"),
            _ => {}
        }
        println!("    => {}\n", verdict);
    }

    println!("================ END SIM ================\n");
}
