/**
 * Shared NURBS patch re-create (ADR-237) — the single source of truth for
 * "edit a control point → rebuild the patch".
 *
 * Per ADR-232 de-risk, setFaceSurface* has no NURBS update path, so a CP edit
 * is committed by creating a fresh patch (createNurbsSurface) from the edited
 * control net and deleting the old face (create-then-delete: the new faceId is
 * already allocated before the old one is removed). Used by both NurbsEditTool
 * (ADR-233/234 prompt + ADR-236 drag) and NurbsPatchPanel (ADR-237 inline table).
 *
 * A future single-Undo wrap (ADR-235 full-2 / ADR-239) will live here.
 */

import { Toast } from '../ui/Toast';
import type { WasmBridge, NurbsSurfaceParams } from '../bridge/WasmBridge';

export interface NurbsRecreateHooks {
  syncMesh: () => void;
  selectFaces: (ids: number[]) => void;
  updateOverlay: (params: NurbsSurfaceParams | null) => void;
}

export interface NurbsRecreateResult {
  newFid: number;
  newParams: NurbsSurfaceParams | null;
}

/**
 * Re-create a NURBS patch with an edited control net.
 * @returns the new faceId + freshly read params, or null on failure.
 */
export function recreateNurbsPatch(
  bridge: WasmBridge,
  oldFid: number,
  params: NurbsSurfaceParams,
  editedCtrlPts: number[],
  editedWeights: number[],
  hooks: NurbsRecreateHooks,
): NurbsRecreateResult | null {
  // ADR-238 — single-transaction replace (create new + remove old = 1 Undo
  // frame). The bridge falls back to createNurbsSurface + deleteFace (2 frames)
  // on legacy engines without the endpoint.
  const newFaces = bridge.replaceNurbsSurface(
    oldFid,
    editedCtrlPts,
    params.nU,
    params.nV,
    editedWeights,
    params.knotsU,
    params.knotsV,
    params.degU,
    params.degV,
  );
  if (!newFaces.length) {
    Toast.fromBridgeError(bridge, 'NURBS 패치 재생성 실패');
    return null;
  }
  const newFid = newFaces[0];
  hooks.syncMesh();
  const newParams = bridge.getNurbsSurfaceParams(newFid);
  hooks.selectFaces([newFid]);
  hooks.updateOverlay(newParams);
  return { newFid, newParams };
}
