/**
 * The cursor mini-grid's geometry, as pure functions.
 *
 * Two things live here and nothing else does: how many world units a pixel is
 * worth where the cursor is, and which segments a disc of grid lines is made of.
 * Kept out of `Viewport` so both can be checked without a renderer.
 *
 * Ported from Kayac's `upload_workplane_grid`
 * (native/rust/src/renderer/msaa/mod.rs) and `pixel_radius_at_depth`
 * (native/rust/src/geo/picking.rs).
 */

/**
 * World radius for an apparent `radiusPx`, under a PERSPECTIVE camera, at a
 * cursor `depth` units from the eye.
 *
 * ⚠ Linear in depth, and that linearity IS the zoom-invariance. A fixed world
 * radius was the bug Kayac's comment records: it vanished when zoomed out and
 * swallowed the model when zoomed in.
 *
 * `logicalH` is CSS pixels, not device pixels — the same space pointer events
 * arrive in, which is also the space `radiusPx` is quoted in.
 */
export function perspectiveRadiusWorld(
  radiusPx: number,
  depth: number,
  fovYRad: number,
  logicalH: number,
): number {
  if (logicalH <= 0) return radiusPx;
  return (radiusPx * 2 * Math.tan(fovYRad * 0.5) * Math.max(depth, 0.001)) / logicalH;
}

/**
 * World radius for an apparent `radiusPx` under an ORTHOGRAPHIC camera.
 *
 * ⚠ This is the branch Kayac does not have — its comment says "still perspective
 * projection — true ortho is a separate polish", so its formula is `fov_y`-only.
 * Our axis views (top / front / right / back / left / bottom) run
 * `OrthographicCamera`, where magnification does not depend on depth at all: the
 * frustum height alone sets it. Feeding depth in there would make the patch grow
 * as you fly the camera back while nothing on screen changed size.
 */
export function orthographicRadiusWorld(
  radiusPx: number,
  frustumHeightWorld: number,
  logicalH: number,
): number {
  if (logicalH <= 0) return radiusPx;
  return (radiusPx * frustumHeightWorld) / logicalH;
}

/** A 3D point, in the shape both `THREE.Vector3` and a plain object satisfy. */
export interface Vec3Like {
  x: number;
  y: number;
  z: number;
}

/**
 * The grid's line segments, as flat `[ax, ay, az, bx, by, bz, …]`.
 *
 * The disc has NO boundary ring and no axes — the cursor already marks the
 * centre. Each line is CLIPPED to the circle instead: the line at offset `off`
 * spans the chord `±√(R² − off²)`, so the field reads as a disc. `i == 0` draws
 * the centre cross.
 *
 * `u` and `v` must be perpendicular unit vectors in the plane. Returns an empty
 * array when the radius is not a usable number, so a caller can hand the result
 * straight to a geometry.
 */
export function miniGridSegments(
  center: Vec3Like,
  u: Vec3Like,
  v: Vec3Like,
  radius: number,
  cells: number,
): number[] {
  const out: number[] = [];
  if (!Number.isFinite(radius) || radius <= 1e-9) return out;
  const steps = Math.max(1, Math.round(cells));
  const step = radius / steps;

  const push = (
    au: number, av: number, bu: number, bv: number,
  ): void => {
    out.push(
      center.x + u.x * au + v.x * av,
      center.y + u.y * au + v.y * av,
      center.z + u.z * au + v.z * av,
      center.x + u.x * bu + v.x * bv,
      center.y + u.y * bu + v.y * bv,
      center.z + u.z * bu + v.z * bv,
    );
  };

  for (let i = -steps; i <= steps; i++) {
    const off = i * step;
    const l2 = radius * radius - off * off;
    if (l2 <= 1e-9) continue;
    const l = Math.sqrt(l2);
    push(-l, off, l, off); // along u, offset in v
    push(off, -l, off, l); // along v, offset in u
  }
  return out;
}
