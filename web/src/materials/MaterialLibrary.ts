/**
 * AXiA 3D — Material System
 *
 * 개념 체계:
 *   Shape(2D) → Form(3D) → Appearance(화면) → Material 부여 → XIA(물체)
 *
 * Material이 부여되면 Volume은 XIA가 됩니다.
 * Material이 없으면 Volume은 Appearance(기하)로만 존재합니다.
 */

// ═══════════════════════════════════════
//  기하 계층 상태 (Geometry Layer)
// ═══════════════════════════════════════

/**
 * Geometry state — computed from owned geometry, not stored.
 * Architecture Decision (2026-04-15):
 *   Geometry Layer: Point → Edge → Face → Volume
 *   Semantic Layer: Object (= XIA), Material, Group
 * Material is a property of Object, not a state transition trigger.
 */
export enum GeometryState {
  Point  = 'point',   // 0D — 위치만, 치수 없음
  Edge   = 'edge',    // 1D — 길이 L만, H=0
  Face   = 'face',    // 2D — L×W, H=0
  Volume = 'volume',  // 3D — L×W×H, 부피 발생
}

export interface GeometryStateInfo {
  state: GeometryState;
  label: string;
  labelEn: string;
  description: string;
  color: string;         // 상태 표시 색상
  icon: string;          // 상태 아이콘
}

export const GEOMETRY_STATES: Record<GeometryState, GeometryStateInfo> = {
  [GeometryState.Point]: {
    state: GeometryState.Point,
    label: '점',
    labelEn: 'Point',
    description: '위치만 존재 (L=0, W=0, H=0)',
    color: '#888888',
    icon: '·',
  },
  [GeometryState.Edge]: {
    state: GeometryState.Edge,
    label: '선',
    labelEn: 'Edge',
    description: '길이만 존재 (H=0)',
    color: '#ff9800',
    icon: '─',
  },
  [GeometryState.Face]: {
    state: GeometryState.Face,
    label: '면',
    labelEn: 'Face',
    description: 'L × W (H=0)',
    color: '#2196f3',
    icon: '▢',
  },
  [GeometryState.Volume]: {
    state: GeometryState.Volume,
    label: '체적',
    labelEn: 'Volume',
    description: 'L × W × H (3D solid)',
    color: '#9c27b0',
    icon: '⬡',
  },
};

// ═══════════════════════════════════════
//  재질 (Material)
// ═══════════════════════════════════════

/** 화재 등급 */
export type FireRating = 'incombustible' | 'semi' | 'retardant';

/** 재질 카테고리 */
export type MaterialCategory = 'concrete' | 'metal' | 'wood' | 'glass' | 'stone' | 'insulation' | 'composite' | 'custom';

/** 물리적 속성 */
export interface PhysicalProperties {
  density: number;             // kg/m³ — 밀도
  friction: number;            // 0.0 ~ 1.0 — 마찰계수
  restitution: number;         // 0.0 ~ 1.0 — 탄성계수 (복원력)
  specificGravity: number;     // 비중 (밀도 / 물의 밀도)
  thermalConductivity: number; // W/(m·K) — 열전도율
  /**
   * 화재 등급. Absent on a material the engine made (ADR-313 D4) or mirrored
   * from it (D3): the engine's rating is a different model (None / Minutes),
   * and a new or imported material claims none, so the app shows no rating
   * rather than inventing one.
   */
  fireRating?: FireRating;
  elasticModulus?: number;    // GPa — 탄성률 (향후 확장)
  compressiveStrength?: number; // MPa — 압축강도 (향후 확장)
}

/** 텍스처 정보 (옵션) */
export interface TextureInfo {
  /** 이미지 데이터 URL (base64 PNG/JPEG) — 저장/로드 시 AXIA 파일에 포함 */
  dataUrl: string;
  /** UV projection 모드 */
  projection: 'planar' | 'box' | 'cylindrical';
  /** 월드 단위 당 반복 횟수 (예: 0.001 = 1000mm당 1타일) */
  scale: number;
  /** 투영 축 회전 (라디안, planar/box 전용) */
  rotation?: number;
  /** 사용자 표시명 (파일명 등) */
  label?: string;
}

/**
 * ADR-099 L-δ — TS counterpart of Rust `LayeredChannels`.
 *
 * 4 PBR channels (Lock-in L-A) — fixed slots, each optional. Mirrors
 * Rust `crate::material::LayeredChannels` for snapshot round-trip
 * (L-η Real Chromium). Channel naming canonical: 'albedo' | 'normal'
 * | 'roughness' | 'metallic'.
 *
 * Coexists with legacy `AuxTextureInfo` (normal + roughness only).
 * L-D migrate helper bridges single-texture → albedo on Rust side
 * (`migrate_legacy_textures_to_layered`). UI (L-ε) and bridge wrappers
 * (L-ζ) will use this interface canonically.
 */
export interface LayeredChannels {
  albedo?: TextureInfo;
  normal?: TextureInfo;
  roughness?: TextureInfo;
  metallic?: TextureInfo;
}

/** 추가 채널 텍스처 (PBR 보조 — 모두 옵션) */
export interface AuxTextureInfo {
  /** Normal map (tangent-space). Three.js MeshStandardMaterial.normalMap. */
  normal?: TextureInfo;
  /** Normal map intensity (default 1.0). Three.js normalScale.x/y. */
  normalIntensity?: number;
  /** Roughness map (greyscale; brighter = rougher). Multiplied by `roughness` field. */
  roughness?: TextureInfo;
}

/** 시각적 속성 */
export interface VisualProperties {
  color: number;              // hex color (0xRRGGBB)
  roughness: number;          // 0.0 ~ 1.0
  metalness: number;          // 0.0 ~ 1.0
  opacity: number;            // 0.0 ~ 1.0
  texture?: TextureInfo;      // 옵션: 베이스 컬러 텍스처
  /** 옵션: 노멀/러프니스 등 보조 PBR 채널 (2026-04-26 추가) */
  aux?: AuxTextureInfo;
  /**
   * ADR-099 L-δ — Layered material 4 PBR channels (Phase 5-B).
   * Mirrors Rust `VisualProperties.layered`. When present, the
   * renderer's `applyLayeredChannels` path replaces the legacy
   * `texture` + `aux` paths.
   */
  layered?: LayeredChannels;
}

/** Material 전체 정의 */
export interface Material {
  id: string;                 // 고유 ID (TypeScript용)
  rustId: number;             // Rust MaterialId (u32) — WASM 통신용
  name: string;               // 표시 이름 (한글)
  nameEn: string;             // 영문명
  category: MaterialCategory;
  physical: PhysicalProperties;
  visual: VisualProperties;
  builtIn: boolean;           // 내장 재질 여부
}

// ═══════════════════════════════════════
//  내장 재질 라이브러리
// ═══════════════════════════════════════

const BUILTIN_MATERIALS: Material[] = [
  {
    id: 'concrete', rustId: 1,
    name: '콘크리트',
    nameEn: 'Concrete',
    category: 'concrete',
    physical: { density: 2400, friction: 0.6, restitution: 0.1, specificGravity: 2.4, thermalConductivity: 1.6, fireRating: 'incombustible' },
    visual: { color: 0xb0b0b0, roughness: 0.9, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'steel', rustId: 2,
    name: '철강',
    nameEn: 'Steel',
    category: 'metal',
    physical: { density: 7850, friction: 0.8, restitution: 0.3, specificGravity: 7.85, thermalConductivity: 50.0, fireRating: 'incombustible' },
    visual: { color: 0x8899aa, roughness: 0.3, metalness: 0.8, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'wood', rustId: 3,
    name: '목재',
    nameEn: 'Wood',
    category: 'wood',
    physical: { density: 600, friction: 0.5, restitution: 0.15, specificGravity: 0.6, thermalConductivity: 0.15, fireRating: 'retardant' },
    visual: { color: 0xc49a6c, roughness: 0.7, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'glass', rustId: 4,
    name: '유리',
    nameEn: 'Glass',
    category: 'glass',
    physical: { density: 2500, friction: 0.7, restitution: 0.8, specificGravity: 2.5, thermalConductivity: 1.0, fireRating: 'incombustible' },
    visual: { color: 0xaaddee, roughness: 0.1, metalness: 0.0, opacity: 0.4 },
    builtIn: true,
  },
  {
    id: 'brick', rustId: 5,
    name: '벽돌',
    nameEn: 'Brick',
    category: 'stone',
    physical: { density: 1800, friction: 0.9, restitution: 0.1, specificGravity: 1.8, thermalConductivity: 0.8, fireRating: 'incombustible' },
    visual: { color: 0xc45a3a, roughness: 0.85, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'aluminum', rustId: 6,
    name: '알루미늄',
    nameEn: 'Aluminum',
    category: 'metal',
    physical: { density: 2700, friction: 0.8, restitution: 0.4, specificGravity: 2.7, thermalConductivity: 160.0, fireRating: 'incombustible' },
    visual: { color: 0xccccdd, roughness: 0.2, metalness: 0.9, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'stone', rustId: 7,
    name: '석재',
    nameEn: 'Stone',
    category: 'stone',
    physical: { density: 2600, friction: 0.85, restitution: 0.15, specificGravity: 2.6, thermalConductivity: 2.3, fireRating: 'incombustible' },
    visual: { color: 0x999988, roughness: 0.8, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'gypsum', rustId: 8,
    name: '석고보드',
    nameEn: 'Gypsum Board',
    category: 'composite',
    physical: { density: 800, friction: 0.4, restitution: 0.1, specificGravity: 0.8, thermalConductivity: 0.16, fireRating: 'incombustible' },
    visual: { color: 0xeeeeee, roughness: 0.95, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'insulation', rustId: 9,
    name: '단열재',
    nameEn: 'Insulation',
    category: 'insulation',
    physical: { density: 30, friction: 0.3, restitution: 0.05, specificGravity: 0.03, thermalConductivity: 0.035, fireRating: 'retardant' },
    visual: { color: 0xffee88, roughness: 0.95, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'water', rustId: 10,
    name: '물',
    nameEn: 'Water',
    category: 'custom',
    physical: { density: 1000, friction: 0.1, restitution: 0.5, specificGravity: 1.0, thermalConductivity: 0.6, fireRating: 'incombustible' },
    visual: { color: 0x4488cc, roughness: 0.0, metalness: 0.0, opacity: 0.5 },
    builtIn: true,
  },
  {
    id: 'soil', rustId: 11,
    name: '토양',
    nameEn: 'Soil',
    category: 'custom',
    physical: { density: 1600, friction: 0.85, restitution: 0.05, specificGravity: 1.6, thermalConductivity: 1.5, fireRating: 'incombustible' },
    visual: { color: 0x8b6914, roughness: 0.95, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
  {
    id: 'tile', rustId: 12,
    name: '타일',
    nameEn: 'Tile',
    category: 'composite',
    physical: { density: 2000, friction: 0.8, restitution: 0.15, specificGravity: 2.0, thermalConductivity: 1.3, fireRating: 'incombustible' },
    visual: { color: 0xddd8cc, roughness: 0.4, metalness: 0.0, opacity: 1.0 },
    builtIn: true,
  },
];

/** One entry of the engine's `getAllMaterials()` list (ADR-313 D3). */
interface EngineMaterial {
  id: number;
  name: string;
  nameEn: string;
  density: number;
  color: string; // '#rrggbb'
  friction?: number;
  restitution?: number;
  specificGravity?: number;
  thermalConductivity?: number;
  roughness?: number;
  metalness?: number;
  opacity?: number;
}

/**
 * A material the engine holds and the app had no entry for, as the app's
 * `Material`. Every number is the engine's; where the engine list carries no
 * value the field keeps a neutral default, and the fire rating is left absent.
 */
function mirrorEngineMaterial(em: EngineMaterial): Material {
  const color = parseInt(String(em.color ?? '#888888').replace('#', ''), 16);
  return {
    id: `engine-${em.id}`,
    rustId: em.id,
    name: em.name,
    nameEn: em.nameEn,
    category: 'custom',
    physical: {
      density: em.density,
      friction: em.friction ?? 0.5,
      restitution: em.restitution ?? 0.5,
      specificGravity: em.specificGravity ?? em.density / 1000,
      thermalConductivity: em.thermalConductivity ?? 0,
    },
    visual: {
      color: Number.isFinite(color) ? color : 0x888888,
      roughness: em.roughness ?? 0.5,
      metalness: em.metalness ?? 0,
      opacity: em.opacity ?? 1,
    },
    builtIn: false,
  };
}

// ═══════════════════════════════════════
//  MaterialLibrary 클래스
// ═══════════════════════════════════════

export class MaterialLibrary {
  private materials: Map<string, Material> = new Map();
  private assignments: Map<number, string> = new Map(); // faceId → materialId
  private listeners: Array<() => void> = [];
  private bridge: any = null; // WasmBridge reference (set via setBridge)

  constructor() {
    // 내장 재질 등록
    for (const mat of BUILTIN_MATERIALS) {
      this.materials.set(mat.id, mat);
    }
  }

  /** WasmBridge 연결 — Rust 엔진과 동기화 활성화 */
  setBridge(bridge: any): void {
    this.bridge = bridge;
  }

  // --- 재질 조회 ---

  get(id: string): Material | undefined {
    return this.materials.get(id);
  }

  getAll(): Material[] {
    return Array.from(this.materials.values());
  }

  getBuiltIn(): Material[] {
    return this.getAll().filter(m => m.builtIn);
  }

  getCustom(): Material[] {
    return this.getAll().filter(m => !m.builtIn);
  }

  getByCategory(category: MaterialCategory): Material[] {
    return this.getAll().filter(m => m.category === category);
  }

  // --- 사용자 정의 재질 ---

  addCustom(material: Omit<Material, 'builtIn'>): Material {
    const mat: Material = { ...material, builtIn: false };
    this.materials.set(mat.id, mat);
    this.notifyListeners();
    return mat;
  }

  /**
   * ADR-313 D4 — a custom material the ENGINE holds too.
   *
   * `addCustom` alone makes a material only this table knows. The engine then
   * refuses to record it on a face (`AssignMaterial` rejects an id its library
   * does not hold), so it was never saved, never exported, and the next time
   * anything read the faces back from the engine it was gone — measured for
   * Quick Colour: an unrelated draw and undo turned a red face grey. Quick
   * Colour had invented ids ≥ 10001 on the belief that the engine stored ids as
   * opaque numbers; the texture dialog sent 0, which is FORM_MATERIAL — "no
   * material" — and which the engine refuses since ADR-313.
   *
   * The engine creates it in the Project tier (so it is saved with the file)
   * and the id it returns is the one this table uses. `null` when the engine
   * cannot create it — nothing is added then, so nothing is assigned that the
   * engine would not keep.
   */
  addEngineMaterial(material: Omit<Material, 'builtIn' | 'rustId'>): Material | null {
    const rustId: number | null | undefined = this.bridge?.addProjectMaterial?.(
      material.name,
      material.nameEn,
      material.visual.color,
    );
    if (typeof rustId !== 'number' || rustId <= 0) return null;
    return this.addCustom({ ...material, rustId });
  }

  removeCustom(id: string): boolean {
    const mat = this.materials.get(id);
    if (!mat || mat.builtIn) return false;
    this.materials.delete(id);
    // 해당 재질이 할당된 face들 해제
    for (const [faceId, matId] of this.assignments) {
      if (matId === id) this.assignments.delete(faceId);
    }
    this.notifyListeners();
    return true;
  }

  // --- 재질 할당 (Face → Material) ---

  /** 면에 재질 부여 → Volume이 XIA로 전환되는 트리거 */
  assignToFaces(faceIds: number[], materialId: string): boolean {
    const mat = this.materials.get(materialId);
    if (!mat) return false;

    // ADR-313 D5 — the engine records the pick as one undo step, so it goes
    // first and the table follows only what it accepted. (`assignMaterial` is
    // for a bridge without the entry — test fakes.)
    if (this.bridge?.assignMaterialToFaces) {
      if (!this.bridge.assignMaterialToFaces(faceIds, mat.rustId)) return false;
    } else if (this.bridge?.assignMaterial) {
      this.bridge.assignMaterial(new Uint32Array(faceIds), mat.rustId);
    }

    for (const fid of faceIds) {
      this.assignments.set(fid, materialId);
    }
    this.notifyListeners();
    return true;
  }

  /** 면에서 재질 제거 → XIA가 Volume으로 복귀 */
  unassignFromFaces(faceIds: number[]): void {
    for (const fid of faceIds) {
      this.assignments.delete(fid);
    }

    // ADR-313 D5 — one undo step, as for a pick.
    if (this.bridge?.removeMaterialFromFaces) {
      this.bridge.removeMaterialFromFaces(faceIds);
    } else if (this.bridge?.removeMaterial) {
      this.bridge.removeMaterial(new Uint32Array(faceIds));
    }

    this.notifyListeners();
  }

  /** 면의 재질 조회 */
  getMaterialForFace(faceId: number): Material | undefined {
    const matId = this.assignments.get(faceId);
    return matId ? this.materials.get(matId) : undefined;
  }

  /** 면 집합의 공통 재질 (모두 같으면 반환, 다르면 undefined) */
  getCommonMaterial(faceIds: number[]): Material | undefined {
    if (faceIds.length === 0) return undefined;
    const firstId = this.assignments.get(faceIds[0]);
    if (!firstId) return undefined;
    for (let i = 1; i < faceIds.length; i++) {
      if (this.assignments.get(faceIds[i]) !== firstId) return undefined;
    }
    return this.materials.get(firstId);
  }

  /** 면 집합에 재질이 하나라도 할당되어 있는지 */
  hasMaterial(faceIds: number[]): boolean {
    return faceIds.some(fid => this.assignments.has(fid));
  }

  // --- 물리 계산 ---

  /**
   * Volume(mm³) + Material → 물리 속성 계산
   *
   * Volume(부피) × Density(밀도) = Mass(질량)
   * Mass × g(9.81 m/s²) = Weight(무게/중력)
   */
  computePhysics(volumeMM3: number, materialId: string): {
    volumeM3: number;
    density: number;
    mass: number;
    weight: number;
  } | null {
    const mat = this.materials.get(materialId);
    if (!mat) return null;

    const volumeM3 = volumeMM3 / 1e9;         // mm³ → m³
    const density = mat.physical.density;       // kg/m³
    const mass = volumeM3 * density;            // kg
    const weight = mass * 9.81;                 // N (뉴턴)

    return { volumeM3, density, mass, weight };
  }

  // --- 기하 상태 판정 ---

  /**
   * 면 집합의 기하 상태를 판정
   *
   * - 0면 → Point (또는 Line)
   * - 1면(평면) → Face
   * - 여러 면(열린) → Face group
   * - 닫힌 면 집합 → Volume
   * Material is a property of Object, not a state trigger.
   */
  determineState(info: {
    faceCount: number;
    edgeCount: number;
    isSolid: boolean;
    height: number;
  }, _faceIds: number[]): GeometryState {
    if (info.faceCount === 0) {
      return info.edgeCount > 0 ? GeometryState.Edge : GeometryState.Point;
    }

    // 1-2 faces = Face
    if (info.faceCount <= 2) return GeometryState.Face;

    // 3+ faces = Volume (regardless of material)
    return GeometryState.Volume;
  }

  // --- 변경 감지 ---

  onChange(listener: () => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter(l => l !== listener);
    };
  }

  private notifyListeners(): void {
    for (const l of this.listeners) l();
  }

  // --- 엔진에서 되읽기 (ADR-313 D3) ---

  /**
   * Make the app's assignments exactly what the engine holds.
   *
   * The engine is the truth for which face carries which material; this table
   * is how the app draws it (메타-원칙 #13). Until ADR-313 nothing read it back:
   * the viewport colours faces from `assignments`, and after a file was opened,
   * an IFC imported or a face split, the engine held materials this table did
   * not (measured: a reopened brick box came back grey while the engine still
   * read 벽돌; an imported brick member never showed at all). The method this
   * replaces, `syncFromRust`, only re-read faces the app already knew.
   *
   * Every face is read in one call. A material the engine holds and the app has
   * no entry for — one an IFC import or the Asset Library created — is mirrored
   * in from the engine's own numbers, so it can be drawn and named; nothing is
   * invented for it except the absence of a fire rating (see PhysicalProperties).
   *
   * No-op when the engine cannot answer, which is not the same as the engine
   * saying "nothing has a material". Notifies only when something changed.
   */
  syncFromEngine(): void {
    const faces: Map<number, number> | null = this.bridge?.getFaceMaterials?.() ?? null;
    if (!faces) return;

    const byRustId = new Map<number, string>();
    for (const mat of this.materials.values()) byRustId.set(mat.rustId, mat.id);

    let engineList: EngineMaterial[] | null = null;
    for (const rustId of new Set(faces.values())) {
      if (byRustId.has(rustId)) continue;
      engineList ??= this.readEngineMaterials();
      const em = engineList.find((m) => m.id === rustId);
      if (!em) continue;
      const mirrored = mirrorEngineMaterial(em);
      this.materials.set(mirrored.id, mirrored);
      byRustId.set(rustId, mirrored.id);
    }

    const next = new Map<number, string>();
    for (const [faceId, rustId] of faces) {
      const id = byRustId.get(rustId);
      if (id) next.set(faceId, id);
    }

    let changed = next.size !== this.assignments.size;
    if (!changed) {
      for (const [faceId, id] of next) {
        if (this.assignments.get(faceId) !== id) { changed = true; break; }
      }
    }
    if (changed || (engineList !== null)) {
      this.assignments = next;
      this.notifyListeners();
    }
  }

  private readEngineMaterials(): EngineMaterial[] {
    try {
      const json = this.bridge?.getAllMaterials?.();
      return json ? (JSON.parse(json) as EngineMaterial[]) : [];
    } catch {
      return [];
    }
  }

  // --- 직렬화 (향후 파일 저장용) ---

  toJSON(): { custom: Material[]; assignments: [number, string][] } {
    return {
      custom: this.getCustom(),
      assignments: Array.from(this.assignments.entries()),
    };
  }

  fromJSON(data: { custom?: Material[]; assignments?: [number, string][] }): void {
    if (data.custom) {
      for (const mat of data.custom) {
        this.materials.set(mat.id, { ...mat, builtIn: false });
      }
    }
    if (data.assignments) {
      for (const [fid, matId] of data.assignments) {
        this.assignments.set(fid, matId);
      }
    }
    this.notifyListeners();
  }
}

/** 전역 싱글턴 */
let _instance: MaterialLibrary | null = null;

export function getMaterialLibrary(): MaterialLibrary {
  if (!_instance) {
    _instance = new MaterialLibrary();
  }
  return _instance;
}
