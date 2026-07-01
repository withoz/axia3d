import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { DrawText3DTool } from './DrawText3DTool';
import * as Text3DBuilder from './Text3DBuilder';
import { setText3DMode } from './Text3DSettings';

// Lazy builder is dynamic-imported inside _place; mock it (real one pulls in
// three/examples TextGeometry/FontLoader which aren't in the headless mock).
vi.mock('./Text3DBuilder', () => ({
  buildExtrudedText: vi.fn(),
  buildSpriteText: vi.fn(),
}));

// Toast → no-op (avoids DOM side effects in headless).
vi.mock('../ui/Toast', () => ({
  Toast: {
    info: vi.fn(),
    warning: vi.fn(),
    fromBridgeError: vi.fn(),
  },
}));

const flush = async () => {
  // let the dynamic import('./Text3DBuilder') + sync builder calls settle.
  // Dynamic import resolves on a macrotask, so a few setTimeout(0) ticks.
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

function makeCtx() {
  const addTextObject = vi.fn();
  const pt = { x: 1, y: 2, z: 3, clone() { return this; } };
  const ctx = {
    viewport: { addTextObject },
    bridge: {},
    syncMesh: vi.fn(),
    get3DPoint: vi.fn(() => pt),
    getSnappedPoint: vi.fn(() => null), // fall through to raw
    getDrawPlane: vi.fn(() => null),    // skip orient branch
  } as unknown as ConstructorParameters<typeof DrawText3DTool>[0];
  return { ctx, addTextObject };
}

describe('DrawText3DTool (ADR-228)', () => {
  let promptSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    setText3DMode('extruded');
    vi.mocked(Text3DBuilder.buildExtrudedText).mockReset();
    vi.mocked(Text3DBuilder.buildSpriteText).mockReset();
    promptSpy = vi.spyOn(window, 'prompt');
    try { localStorage.removeItem('axia:text3d:last'); } catch { /* ignore */ }
  });

  afterEach(() => {
    promptSpy.mockRestore();
  });

  it('name + wantsSnap', () => {
    const { ctx } = makeCtx();
    const t = new DrawText3DTool(ctx);
    expect(t.name).toBe('text3d');
    expect(t.wantsSnap).toBe(true);
    expect(t.isBusy()).toBe(false);
  });

  it('onActivate prompts + stores the string (localStorage)', () => {
    promptSpy.mockReturnValue('Hello');
    const { ctx } = makeCtx();
    const t = new DrawText3DTool(ctx);
    t.onActivate();
    expect(promptSpy).toHaveBeenCalled();
    expect(localStorage.getItem('axia:text3d:last')).toBe('Hello');
  });

  it('onActivate cancel (null) → no text; click warns, no placement', async () => {
    promptSpy.mockReturnValue(null);
    const { ctx, addTextObject } = makeCtx();
    const t = new DrawText3DTool(ctx);
    t.onActivate();
    t.onMouseDown({} as MouseEvent, null);
    await flush();
    expect(addTextObject).not.toHaveBeenCalled();
  });

  it('extruded mode: click builds extruded mesh + adds to overlay', async () => {
    promptSpy.mockReturnValue('Hi');
    setText3DMode('extruded');
    vi.mocked(Text3DBuilder.buildExtrudedText).mockReturnValue({
      name: 'text3d-extruded',
      position: { copy: vi.fn() },
      quaternion: { setFromRotationMatrix: vi.fn() },
    } as unknown as ReturnType<typeof Text3DBuilder.buildExtrudedText>);
    const { ctx, addTextObject } = makeCtx();
    const t = new DrawText3DTool(ctx);
    t.onActivate();
    t.onMouseDown({} as MouseEvent, null);
    await flush();
    expect(Text3DBuilder.buildExtrudedText).toHaveBeenCalledWith('Hi');
    expect(addTextObject).toHaveBeenCalledTimes(1);
  });

  it('extruded mode falls back to sprite when font lacks glyphs (Korean → null)', async () => {
    promptSpy.mockReturnValue('한글');
    setText3DMode('extruded');
    vi.mocked(Text3DBuilder.buildExtrudedText).mockReturnValue(null); // unsupported glyphs
    vi.mocked(Text3DBuilder.buildSpriteText).mockReturnValue({
      name: 'text3d-sprite',
      position: { copy: vi.fn() },
    } as unknown as ReturnType<typeof Text3DBuilder.buildSpriteText>);
    const { ctx, addTextObject } = makeCtx();
    const t = new DrawText3DTool(ctx);
    t.onActivate();
    t.onMouseDown({} as MouseEvent, null);
    await flush();
    expect(Text3DBuilder.buildExtrudedText).toHaveBeenCalled();
    expect(Text3DBuilder.buildSpriteText).toHaveBeenCalledWith('한글');
    expect(addTextObject).toHaveBeenCalledTimes(1);
  });

  it('sprite mode: click builds sprite directly (no extruded call)', async () => {
    promptSpy.mockReturnValue('Label');
    setText3DMode('sprite');
    vi.mocked(Text3DBuilder.buildSpriteText).mockReturnValue({
      name: 'text3d-sprite',
      position: { copy: vi.fn() },
    } as unknown as ReturnType<typeof Text3DBuilder.buildSpriteText>);
    const { ctx, addTextObject } = makeCtx();
    const t = new DrawText3DTool(ctx);
    t.onActivate();
    t.onMouseDown({} as MouseEvent, null);
    await flush();
    expect(Text3DBuilder.buildExtrudedText).not.toHaveBeenCalled();
    expect(Text3DBuilder.buildSpriteText).toHaveBeenCalledWith('Label');
    expect(addTextObject).toHaveBeenCalledTimes(1);
  });

  it('onMouseDown with no 3D point → no placement', async () => {
    promptSpy.mockReturnValue('Hi');
    const { ctx, addTextObject } = makeCtx();
    (ctx.get3DPoint as ReturnType<typeof vi.fn>).mockReturnValue(null);
    const t = new DrawText3DTool(ctx);
    t.onActivate();
    t.onMouseDown({} as MouseEvent, null);
    await flush();
    expect(addTextObject).not.toHaveBeenCalled();
  });
});
