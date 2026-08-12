/**
 * The choice offered when a hole would cross a hole that is already there.
 *
 * 사용자 (2026-08-10): *"우리 엔진은 모든 기능이 이미 있으니 사용자가 선택하도록"*
 * — and 메타-원칙 #16: automation cannot infer intent, so a crossing is asked
 * about rather than guessed at.
 *
 * What made this necessary: the ordinary drill refuses a crossing (a straight
 * tube cannot bridge the void the first bore left), and `DrawHoleTool` then fell
 * through to punching a 2D face hole. Measured in the real app — the fallback
 * SUCCEEDED, the solid went from closed to open, and the user was told
 * "면 구멍을 뚫었습니다". A refusal the engine made carefully was thrown away and
 * replaced with a worse result reported as a success.
 *
 * Thin on purpose, like `TopologyRecoveryDialog` (ADR-097): this renders and
 * resolves; the tool decides what to offer and what to do with the answer.
 * Pure DOM, so jsdom can test it.
 */

export type CrossingBoreChoice = 'kernel' | 'cancel';

export interface CrossingBoreChoiceOptions {
  /** Why the ordinary drill refused, in the user's words. */
  reason: string;
  /**
   * Whether the kernel can actually do THIS crossing (equal radii, right
   * angles, a segment count that lands on the crossing curve). When false only
   * cancel is offered — an option that would fail is not an option.
   */
  kernelAvailable: boolean;
  /** Override host document — used by tests. */
  doc?: Document;
}

const DIALOG_ID = 'axia-crossing-bore-choice';

/**
 * Render the modal and resolve with the user's choice.
 *
 * Backdrop click and ESC both mean `cancel`, which is the option that changes
 * nothing — the same "least destructive dismissal" the recovery dialog uses.
 * A second call while one is open resolves `cancel` rather than stacking.
 */
export function showCrossingBoreChoiceDialog(
  options: CrossingBoreChoiceOptions,
): Promise<CrossingBoreChoice> {
  const doc = options.doc ?? (typeof document !== 'undefined' ? document : null);
  if (!doc) return Promise.resolve('cancel');
  if (doc.getElementById(DIALOG_ID)) return Promise.resolve('cancel');

  return new Promise((resolve) => {
    let resolved = false;
    const finish = (choice: CrossingBoreChoice) => {
      if (resolved) return;
      resolved = true;
      cleanup();
      resolve(choice);
    };

    const backdrop = doc.createElement('div');
    backdrop.id = DIALOG_ID;
    backdrop.setAttribute('role', 'dialog');
    backdrop.setAttribute('aria-modal', 'true');
    backdrop.style.cssText = [
      'position:fixed', 'inset:0', 'background:rgba(0,0,0,0.45)',
      'display:flex', 'align-items:center', 'justify-content:center',
      'z-index:10000', 'font-family:system-ui,sans-serif',
    ].join(';');

    const panel = doc.createElement('div');
    panel.style.cssText = [
      'background:#1f2030', 'color:#e8e8ec', 'padding:24px 28px',
      'border-radius:8px', 'min-width:360px', 'max-width:480px',
      'box-shadow:0 12px 40px rgba(0,0,0,0.5)',
    ].join(';');

    const title = doc.createElement('h3');
    title.textContent = '기존 구멍과 교차합니다';
    title.style.cssText = 'margin:0 0 12px 0;font-size:16px;color:#ffd760';
    panel.appendChild(title);

    const reasonEl = doc.createElement('p');
    reasonEl.textContent = options.reason;
    reasonEl.setAttribute('data-role', 'reason');
    reasonEl.style.cssText = 'margin:0 0 18px 0;font-size:13px;line-height:1.5';
    panel.appendChild(reasonEl);

    const btnRow = doc.createElement('div');
    btnRow.style.cssText = 'display:flex;gap:10px;justify-content:flex-end';

    const mkBtn = (label: string, choice: CrossingBoreChoice, primary = false) => {
      const b = doc.createElement('button');
      b.type = 'button';
      b.textContent = label;
      b.setAttribute('data-choice', choice);
      b.style.cssText = [
        'padding:8px 14px', 'border:none', 'border-radius:4px',
        'cursor:pointer', 'font-size:13px',
        primary ? 'background:#5b9bd5;color:#fff' : 'background:#3a3b4a;color:#e8e8ec',
      ].join(';');
      b.addEventListener('click', () => finish(choice));
      return b;
    };

    btnRow.appendChild(mkBtn('취소', 'cancel'));
    if (options.kernelAvailable) {
      btnRow.appendChild(mkBtn('교차 관통', 'kernel', true));
    }
    panel.appendChild(btnRow);

    backdrop.appendChild(panel);
    backdrop.addEventListener('click', (ev) => {
      if (ev.target === backdrop) finish('cancel');
    });

    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') finish('cancel');
    };
    doc.addEventListener('keydown', onKey);

    const cleanup = () => {
      doc.removeEventListener('keydown', onKey);
      if (backdrop.parentNode) backdrop.parentNode.removeChild(backdrop);
    };

    doc.body.appendChild(backdrop);
  });
}
