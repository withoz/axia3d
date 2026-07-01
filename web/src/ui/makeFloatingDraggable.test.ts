import { describe, it, expect, beforeEach, vi } from 'vitest';
import { makeFloatingDraggable } from './makeFloatingDraggable';

function mouse(type: string, x: number, y: number, button = 0): MouseEvent {
  return new MouseEvent(type, { clientX: x, clientY: y, button, bubbles: true, cancelable: true });
}

describe('makeFloatingDraggable', () => {
  let el: HTMLButtonElement;

  beforeEach(() => {
    localStorage.clear();
    document.body.innerHTML = '';
    el = document.createElement('button');
    document.body.appendChild(el);
  });

  it('a real drag moves the element to fixed left/top and persists', () => {
    makeFloatingDraggable(el, { storageKey: 'k' });
    el.dispatchEvent(mouse('mousedown', 100, 100));
    document.dispatchEvent(mouse('mousemove', 160, 140)); // dx=60 dy=40 > threshold
    document.dispatchEvent(mouse('mouseup', 160, 140));

    expect(el.style.position).toBe('fixed');
    expect(el.style.left).toBe('60px');
    expect(el.style.top).toBe('40px');
    expect(JSON.parse(localStorage.getItem('k')!)).toEqual({ left: 60, top: 40 });
  });

  it('a sub-threshold press is a click — no move, no persist', () => {
    makeFloatingDraggable(el, { storageKey: 'k', threshold: 4 });
    el.dispatchEvent(mouse('mousedown', 100, 100));
    document.dispatchEvent(mouse('mousemove', 102, 101)); // < 4px
    document.dispatchEvent(mouse('mouseup', 102, 101));

    expect(el.style.left).toBe('');
    expect(localStorage.getItem('k')).toBeNull();
  });

  it('suppresses the trailing click after a drag (action does NOT fire)', () => {
    const onClick = vi.fn();
    el.addEventListener('click', onClick);
    makeFloatingDraggable(el, { storageKey: 'k' });

    el.dispatchEvent(mouse('mousedown', 100, 100));
    document.dispatchEvent(mouse('mousemove', 160, 160));
    document.dispatchEvent(mouse('mouseup', 160, 160));
    el.dispatchEvent(mouse('click', 160, 160)); // the synthesized post-drag click

    expect(onClick).not.toHaveBeenCalled();
  });

  it('a plain click (no drag) still fires the action', () => {
    const onClick = vi.fn();
    el.addEventListener('click', onClick);
    makeFloatingDraggable(el, { storageKey: 'k' });

    el.dispatchEvent(mouse('mousedown', 100, 100));
    document.dispatchEvent(mouse('mouseup', 100, 100));
    el.dispatchEvent(mouse('click', 100, 100));

    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('restores a persisted position on init (fixed left/top)', () => {
    localStorage.setItem('k', JSON.stringify({ left: 200, top: 90 }));
    makeFloatingDraggable(el, { storageKey: 'k' });

    expect(el.style.position).toBe('fixed');
    expect(el.style.left).toBe('200px');
    expect(el.style.top).toBe('90px');
  });

  it('clamps a restored position to the viewport', () => {
    // window.innerWidth/innerHeight default to jsdom 1024x768.
    localStorage.setItem('k', JSON.stringify({ left: 99999, top: 99999 }));
    makeFloatingDraggable(el, { storageKey: 'k' });
    expect(parseInt(el.style.left, 10)).toBeLessThanOrEqual(window.innerWidth);
    expect(parseInt(el.style.top, 10)).toBeLessThanOrEqual(window.innerHeight);
  });

  it('ignores non-left mouse buttons', () => {
    makeFloatingDraggable(el, { storageKey: 'k' });
    el.dispatchEvent(mouse('mousedown', 100, 100, 2)); // right button
    document.dispatchEvent(mouse('mousemove', 200, 200));
    document.dispatchEvent(mouse('mouseup', 200, 200));
    expect(el.style.left).toBe('');
  });

  it('uses a separate handle when provided', () => {
    const handle = document.createElement('div');
    el.appendChild(handle);
    makeFloatingDraggable(el, { storageKey: 'k', handle });
    handle.dispatchEvent(mouse('mousedown', 50, 50));
    document.dispatchEvent(mouse('mousemove', 120, 120));
    document.dispatchEvent(mouse('mouseup', 120, 120));
    expect(el.style.left).toBe('70px'); // moves the root el, not the handle
  });
});
