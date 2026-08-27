import { formatNoteTimestamp } from '../format/datetime.ts';

export const TOOLTIP_DELAY_MS = 450;

export interface NoteInfoLabels {
  created: string;
  modified: string;
}

export const DEFAULT_INFO_LABELS: NoteInfoLabels = {
  created: 'Criado',
  modified: 'Modificado',
};

export interface NoteTimestamps {
  createdAt: string | null;
  updatedAt: string | null;
}

export interface NoteInfoTooltipOptions {
  /** Free area of the header bar the tooltip reacts to. */
  hoverTarget: HTMLElement;
  /** Element the tooltip is rendered into, outside the drag region. */
  mount: HTMLElement;
  labels?: NoteInfoLabels;
  delayMs?: number;
  setTimer?: (callback: () => void, delayMs: number) => number;
  clearTimer?: (handle: number) => void;
  document?: Document;
}

/**
 * Shows the note's creation and modification dates after the cursor rests on
 * the free part of the header bar.
 *
 * The tooltip is purely informational: it never takes the pointer, so hovering
 * it cannot interrupt a drag, and it is dismissed by anything that starts an
 * interaction.
 */
export class NoteInfoTooltip {
  private readonly doc: Document;
  private readonly element: HTMLElement;
  private readonly delayMs: number;
  private readonly setTimer: (callback: () => void, delayMs: number) => number;
  private readonly clearTimer: (handle: number) => void;
  private readonly labels: NoteInfoLabels;
  private timerHandle: number | null = null;
  private visible = false;
  private timestamps: NoteTimestamps = { createdAt: null, updatedAt: null };

  public constructor(private readonly options: NoteInfoTooltipOptions) {
    this.doc = options.document ?? options.hoverTarget.ownerDocument;
    this.labels = options.labels ?? DEFAULT_INFO_LABELS;
    this.delayMs = options.delayMs ?? TOOLTIP_DELAY_MS;
    this.setTimer = options.setTimer ?? ((callback, ms) => window.setTimeout(callback, ms));
    this.clearTimer = options.clearTimer ?? ((handle) => window.clearTimeout(handle));

    this.element = this.doc.createElement('div');
    this.element.className = 'note-tooltip';
    this.element.id = 'note-tooltip';
    this.element.setAttribute('role', 'tooltip');
    this.element.hidden = true;
    options.mount.append(this.element);

    options.hoverTarget.addEventListener('pointerenter', this.handlePointerEnter);
    options.hoverTarget.addEventListener('pointerleave', this.hide);
    options.hoverTarget.addEventListener('pointerdown', this.hide);
  }

  public destroy(): void {
    this.hide();
    this.options.hoverTarget.removeEventListener('pointerenter', this.handlePointerEnter);
    this.options.hoverTarget.removeEventListener('pointerleave', this.hide);
    this.options.hoverTarget.removeEventListener('pointerdown', this.hide);
    this.element.remove();
  }

  public setTimestamps(timestamps: NoteTimestamps): void {
    this.timestamps = timestamps;
    if (this.visible) this.render();
  }

  public isVisible(): boolean {
    return this.visible;
  }

  public isPending(): boolean {
    return this.timerHandle !== null;
  }

  public get node(): HTMLElement {
    return this.element;
  }

  /** Dismisses the tooltip and cancels a pending one. */
  public readonly hide = (): void => {
    this.cancelPending();
    if (!this.visible) return;
    this.visible = false;
    this.element.hidden = true;
  };

  private readonly handlePointerEnter = (): void => {
    this.cancelPending();
    this.timerHandle = this.setTimer(() => {
      this.timerHandle = null;
      this.show();
    }, this.delayMs);
  };

  private cancelPending(): void {
    if (this.timerHandle === null) return;
    this.clearTimer(this.timerHandle);
    this.timerHandle = null;
  }

  private show(): void {
    this.render();
    this.visible = true;
    this.element.hidden = false;
  }

  private render(): void {
    this.element.textContent = '';
    for (const [label, value] of [
      [this.labels.created, this.timestamps.createdAt],
      [this.labels.modified, this.timestamps.updatedAt],
    ] as const) {
      const row = this.doc.createElement('span');
      row.className = 'note-tooltip-row';
      row.textContent = `${label}: ${formatNoteTimestamp(value)}`;
      this.element.append(row);
    }
  }
}
