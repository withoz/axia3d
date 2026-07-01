/**
 * DimensionManager — persistent, editable LINEAR dimensions (ADR-215).
 *
 * Each driving Distance constraint is shown as a dimension line + editable label
 * via a dedicated `DimensionLabel`. Clicking a label opens inline edit →
 * `setConstraintValue` → the solver moves geometry (parametric / driving).
 *
 * Mirrors `ConstraintVisual`'s "snapshot once, render forever until invalidated"
 * pattern: the constraint LIST is cached (refreshed only on bridge events, never
 * per-frame), while vertex positions are re-fetched each frame (cheap, read-only —
 * same as ConstraintVisual). Reuses Distance constraint + DimensionLabel +
 * getVertexPos + setConstraintValue — Pattern-12, no new geometry kernel.
 */

import * as THREE from 'three';
import { DimensionLabel, DimLine } from './DimensionLabel';
import type { WasmBridge } from '../bridge/WasmBridge';
import type { UnitSystem } from '../units/UnitSystem';

interface DistItem {
  id: number;
  kind: string;
  active: boolean;
  value?: number;
  refs: Array<{ vertex?: number; edge?: [number, number] }>;
}

export interface DimensionManagerOpts {
  container: HTMLElement;
  bridge: WasmBridge;
  units: UnitSystem;
  getCamera: () => THREE.Camera;
  /** Called after an edit drives geometry (so the host can syncMesh). */
  onGeometryEdited: () => void;
}

export class DimensionManager {
  private label: DimensionLabel;
  private bridge: WasmBridge;
  private units: UnitSystem;
  private getCamera: () => THREE.Camera;
  private onGeometryEdited: () => void;

  private cache: DistItem[] = [];
  private lineMeta: Array<{ id: number; isAngle: boolean }> = []; // DimLine index → constraint
  private unsub: (() => void) | null = null;
  private visible = true;

  constructor(opts: DimensionManagerOpts) {
    this.bridge = opts.bridge;
    this.units = opts.units;
    this.getCamera = opts.getCamera;
    this.onGeometryEdited = opts.onGeometryEdited;

    this.label = new DimensionLabel(opts.container);
    this.label.onEdit = (idx, newValue) => this.handleEdit(idx, newValue);

    this.unsub = this.bridge.onConstraintsChanged(() => this.refresh());
    this.refresh();
  }

  setVisible(v: boolean): void {
    this.visible = v;
    if (!v) this.label.clear();
  }

  isVisible(): boolean {
    return this.visible;
  }

  /** Refresh the cached dimension-constraint list. Event-driven, NOT per-frame. */
  refresh(): void {
    try {
      this.cache = (this.bridge.listConstraints() as DistItem[])
        .filter((c) => (c.kind === 'distance' || c.kind === 'angle' || c.kind === 'radius') && c.active);
    } catch {
      // keep last-known cache on failure
    }
  }

  /** Per-frame: re-project cached dimensions. NO listConstraints call here. */
  update(): void {
    if (!this.visible || this.label.isEditing) return;
    const lines: DimLine[] = [];
    const meta: Array<{ id: number; isAngle: boolean }> = [];
    for (const c of this.cache) {
      if (c.kind === 'distance') {
        const line = this.distanceLine(c);
        if (line) { lines.push(line); meta.push({ id: c.id, isAngle: false }); }
      } else if (c.kind === 'angle') {
        const line = this.angleLine(c);
        if (line) { lines.push(line); meta.push({ id: c.id, isAngle: true }); }
      } else if (c.kind === 'radius') {
        const line = this.radiusLine(c);
        if (line) { lines.push(line); meta.push({ id: c.id, isAngle: false }); }
      }
    }
    this.lineMeta = meta;
    this.label.update(this.getCamera(), lines);
  }

  private distanceLine(c: DistItem): DimLine | null {
    const vA = c.refs[0]?.vertex;
    const vB = c.refs[1]?.vertex;
    if (vA === undefined || vB === undefined) return null;
    const pa = this.bridge.getVertexPos(vA);
    const pb = this.bridge.getVertexPos(vB);
    if (!pa || !pb) return null;
    const from = new THREE.Vector3(pa[0], pa[1], pa[2]);
    const to = new THREE.Vector3(pb[0], pb[1], pb[2]);
    // ADR-218 — a reference dimension (value=None) is read-only: shown in
    // parentheses (CAD convention) and not editable.
    const ref = c.value === undefined || c.value === null;
    const txt = this.units.format(from.distanceTo(to));
    return { from, to, text: ref ? `(${txt})` : txt, color: '#7be288', editable: !ref };
  }

  private angleLine(c: DistItem): DimLine | null {
    const eA = c.refs[0]?.edge;
    const eB = c.refs[1]?.edge;
    if (!eA || !eB) return null;
    const a0 = this.bridge.getVertexPos(eA[0]); const a1 = this.bridge.getVertexPos(eA[1]);
    const b0 = this.bridge.getVertexPos(eB[0]); const b1 = this.bridge.getVertexPos(eB[1]);
    if (!a0 || !a1 || !b0 || !b1) return null;
    // Plain-array math (mock-safe); THREE.Vector3 only for the DimLine fields.
    const sub = (p: number[], q: number[]): [number, number, number] => [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    const len = (d: number[]) => Math.hypot(d[0], d[1], d[2]) || 1;
    const unit = (d: [number, number, number]): [number, number, number] => {
      const l = len(d); return [d[0] / l, d[1] / l, d[2] / l];
    };
    const dA = unit(sub(a1, a0));
    const dB = unit(sub(b1, b0));
    const dot = Math.max(-1, Math.min(1, dA[0] * dB[0] + dA[1] * dB[1] + dA[2] * dB[2]));
    const valueRad = c.value ?? Math.acos(dot);
    const valueDeg = (valueRad * 180) / Math.PI;
    // apex = the shared corner vertex if the two edges share one, else edge A start.
    const shared = [eA[0], eA[1]].find((v) => v === eB[0] || v === eB[1]);
    let apexArr = a0;
    if (shared !== undefined) {
      const sp = this.bridge.getVertexPos(shared);
      if (sp) apexArr = sp;
    }
    const radius = Math.max(1, Math.min(len(sub(a1, a0)), len(sub(b1, b0))) * 0.35);
    const apex = new THREE.Vector3(apexArr[0], apexArr[1], apexArr[2]);
    // ADR-218 — reference angle (value=None): parenthesised, read-only.
    const ref = c.value === undefined || c.value === null;
    return {
      from: apex, to: apex, // unused for angular render
      text: ref ? `(${valueDeg.toFixed(1)}°)` : `${valueDeg.toFixed(1)}°`,
      color: '#ffc48a',
      editable: !ref,
      angular: {
        apex,
        dirA: new THREE.Vector3(dA[0], dA[1], dA[2]),
        dirB: new THREE.Vector3(dB[0], dB[1], dB[2]),
        radius,
        valueDeg,
      },
    };
  }

  private radiusLine(c: DistItem): DimLine | null {
    const refVert = c.refs[0]?.vertex;
    if (refVert === undefined) return null;
    const anchor = this.bridge.getVertexPos(refVert);
    const dim = this.bridge.radiusDimAt(refVert); // [cx, cy, cz, radius]
    if (!anchor || !dim) return null;
    const center = new THREE.Vector3(dim[0], dim[1], dim[2]);
    const to = new THREE.Vector3(anchor[0], anchor[1], anchor[2]);
    // Straight dim line from center to the curve point; "R" prefix. The inline
    // edit placeholder (|center − point|) equals the radius — no special-casing.
    // ADR-218 — reference radius (value=None): parenthesised, read-only.
    const ref = c.value === undefined || c.value === null;
    const txt = `R${this.units.format(dim[3])}`;
    return { from: center, to, text: ref ? `(${txt})` : txt, color: '#9ecbff', editable: !ref };
  }

  private handleEdit(idx: number, newValue: number): void {
    const m = this.lineMeta[idx];
    if (!m) return;
    // Angular labels are edited in DEGREES; the constraint value is radians.
    const value = m.isAngle ? (newValue * Math.PI) / 180 : newValue;
    if (this.bridge.setConstraintValue(m.id, value)) {
      // Geometry moved (constraint solver) → host re-syncs the mesh.
      // The bridge also emits constraintsChanged → refresh() updates the cache.
      this.onGeometryEdited();
    }
  }

  dispose(): void {
    if (this.unsub) { this.unsub(); this.unsub = null; }
    this.label.clear();
  }
}
