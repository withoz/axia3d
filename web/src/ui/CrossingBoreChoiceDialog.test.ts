/**
 * The dialog that stops the engine guessing about a crossing.
 *
 * Pure DOM, so jsdom can drive it. What matters here is that the kernel button
 * only exists when the kernel can actually do it, and that every way OUT of the
 * dialog that is not an explicit click means `cancel` — the answer that changes
 * nothing.
 */
import { describe, it, expect, afterEach } from 'vitest';
import { showCrossingBoreChoiceDialog } from './CrossingBoreChoiceDialog';

const DIALOG_ID = 'axia-crossing-bore-choice';

function dialog(): HTMLElement | null {
  return document.getElementById(DIALOG_ID);
}
function button(choice: string): HTMLButtonElement | null {
  return document.querySelector(`#${DIALOG_ID} button[data-choice="${choice}"]`);
}

afterEach(() => {
  dialog()?.remove();
});

describe('CrossingBoreChoiceDialog', () => {
  it('offers the kernel when the engine says it can do this crossing', async () => {
    const p = showCrossingBoreChoiceDialog({ reason: '교차합니다', kernelAvailable: true });
    expect(button('kernel')).not.toBeNull();
    expect(button('cancel')).not.toBeNull();
    button('kernel')!.click();
    await expect(p).resolves.toBe('kernel');
  });

  it('does NOT offer the kernel when it cannot — an option that would fail is not an option', async () => {
    const p = showCrossingBoreChoiceDialog({ reason: '반지름이 다릅니다', kernelAvailable: false });
    expect(button('kernel')).toBeNull();
    expect(document.querySelector(`#${DIALOG_ID} [data-role="reason"]`)?.textContent)
      .toBe('반지름이 다릅니다');
    button('cancel')!.click();
    await expect(p).resolves.toBe('cancel');
  });

  it('treats ESC as cancel', async () => {
    const p = showCrossingBoreChoiceDialog({ reason: 'x', kernelAvailable: true });
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await expect(p).resolves.toBe('cancel');
    expect(dialog()).toBeNull();
  });

  it('treats a backdrop click as cancel, but not a click inside the panel', async () => {
    const p = showCrossingBoreChoiceDialog({ reason: 'x', kernelAvailable: true });
    (dialog()!.firstElementChild as HTMLElement).click(); // the panel
    expect(dialog()).not.toBeNull();
    dialog()!.click(); // the backdrop
    await expect(p).resolves.toBe('cancel');
  });

  it('does not stack — a second call while one is open resolves cancel', async () => {
    const first = showCrossingBoreChoiceDialog({ reason: 'x', kernelAvailable: true });
    await expect(showCrossingBoreChoiceDialog({ reason: 'y', kernelAvailable: true }))
      .resolves.toBe('cancel');
    expect(document.querySelectorAll(`#${DIALOG_ID}`).length).toBe(1);
    button('cancel')!.click();
    await expect(first).resolves.toBe('cancel');
  });

  it('cleans the DOM up whichever way it closes', async () => {
    const p = showCrossingBoreChoiceDialog({ reason: 'x', kernelAvailable: true });
    button('kernel')!.click();
    await p;
    expect(dialog()).toBeNull();
  });
});
