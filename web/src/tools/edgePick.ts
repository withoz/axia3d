import * as THREE from 'three';
import { ToolContext } from './ITool';

/**
 * Resolve the edge under the cursor + the 3D click point on it
 * (raycast → edgeMap, mirroring SelectTool's pick). Shared by Trim/Extend
 * (ADR-211) and any tool that picks a single edge by click.
 */
export function pickClickedEdge(
  ctx: ToolContext,
  e: MouseEvent,
): { edgeId: number; point: THREE.Vector3 } | null {
  const vp = ctx.viewport;
  const rect =
    vp.container?.getBoundingClientRect?.() ??
    vp.renderer?.domElement?.getBoundingClientRect?.();
  if (!rect) return null;
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;

  const picked = vp.pickEdgeOrFace?.(x, y);
  if (!picked || picked.type !== 'edge' || !ctx.edgeMap) return null;

  const hit = picked.hit as { index?: number; point?: THREE.Vector3 };
  if (hit.index == null) return null;
  const segIdx = Math.floor(hit.index / 2);
  if (segIdx < 0 || segIdx >= ctx.edgeMap.length) return null;
  const edgeId = ctx.edgeMap[segIdx];

  const point = hit.point ?? new THREE.Vector3();
  return { edgeId, point };
}
