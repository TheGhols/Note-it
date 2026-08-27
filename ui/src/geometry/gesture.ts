import { PointerDeltaCoalescer } from './pointerDelta.ts';

export interface PointerGestureHandlers {
  onStart(): void;
  onDelta(dx: number, dy: number): void;
  onEnd(): void;
}

export interface PointerGestureOptions {
  /** Returning false refuses a new gesture, e.g. resizing a collapsed note. */
  canStart?: () => boolean;
  /** Consume the pointerdown so it cannot reach an ancestor gesture region. */
  claimPointerDown?: boolean;
  scheduleFrame?: (callback: () => void) => number;
  cancelFrame?: (handle: number) => void;
}

/**
 * Drives one drag or resize gesture on a single element.
 *
 * The controller owns a strict invariant: geometry deltas are emitted only
 * while exactly one pointer is captured. Anything that ends that capture -
 * `pointerup`, `pointercancel`, `lostpointercapture`, or a move reporting that
 * no button is held any more - terminates the gesture, and a frame left over
 * from before the end can no longer move the window.
 */
export class PointerGestureController {
  private activePointerId: number | null = null;
  private lastX = 0;
  private lastY = 0;
  private readonly deltas: PointerDeltaCoalescer;

  public constructor(
    private readonly target: HTMLElement,
    private readonly handlers: PointerGestureHandlers,
    private readonly options: PointerGestureOptions = {},
  ) {
    this.deltas = new PointerDeltaCoalescer(
      (dx, dy) => {
        // A frame that survived the end of a gesture must never move anything.
        if (this.activePointerId === null) return;
        this.handlers.onDelta(dx, dy);
      },
      options.scheduleFrame,
      options.cancelFrame,
    );

    target.addEventListener('pointerdown', this.handlePointerDown);
    target.addEventListener('pointermove', this.handlePointerMove);
    target.addEventListener('pointerup', this.handlePointerUp);
    target.addEventListener('pointercancel', this.handlePointerCancel);
    target.addEventListener('lostpointercapture', this.handleLostPointerCapture);
  }

  public destroy(): void {
    this.abort();
    this.target.removeEventListener('pointerdown', this.handlePointerDown);
    this.target.removeEventListener('pointermove', this.handlePointerMove);
    this.target.removeEventListener('pointerup', this.handlePointerUp);
    this.target.removeEventListener('pointercancel', this.handlePointerCancel);
    this.target.removeEventListener('lostpointercapture', this.handleLostPointerCapture);
  }

  public isActive(): boolean {
    return this.activePointerId !== null;
  }

  /** Drops the gesture without reporting an end, used when tearing down. */
  private abort(): void {
    if (this.activePointerId === null) return;
    this.activePointerId = null;
    this.deltas.reset();
  }

  private readonly handlePointerDown = (event: PointerEvent): void => {
    if (event.button !== 0) return;
    if (!Number.isFinite(event.screenX) || !Number.isFinite(event.screenY)) return;
    // A second pointer must not hijack the gesture already in flight.
    if (this.activePointerId !== null) return;
    if (this.options.canStart && !this.options.canStart()) return;

    if (this.options.claimPointerDown) {
      event.preventDefault();
      event.stopPropagation();
    }

    this.activePointerId = event.pointerId;
    this.lastX = event.screenX;
    this.lastY = event.screenY;
    this.deltas.reset();
    try {
      this.target.setPointerCapture(event.pointerId);
    } catch {
      // Capture is an optimisation; the gesture still tracks its pointer id.
    }
    this.handlers.onStart();
  };

  private readonly handlePointerMove = (event: PointerEvent): void => {
    if (!this.isOwnedBy(event)) return;

    // No button held means no valid gesture, whatever the browser still
    // reports: end it instead of letting a hover move the window.
    if (event.buttons === 0) {
      this.finishGesture(event, false);
      return;
    }

    const dx = event.screenX - this.lastX;
    const dy = event.screenY - this.lastY;
    this.lastX = event.screenX;
    this.lastY = event.screenY;
    this.deltas.add(dx, dy);
  };

  private readonly handlePointerUp = (event: PointerEvent): void => {
    if (!this.isOwnedBy(event)) return;
    this.finishGesture(event, true);
  };

  private readonly handlePointerCancel = (event: PointerEvent): void => {
    if (!this.isOwnedBy(event)) return;
    this.finishGesture(event, false);
  };

  private readonly handleLostPointerCapture = (event: PointerEvent): void => {
    // Reaching here with the gesture still active means the capture was taken
    // away from us; a normal end has already cleared the pointer id.
    if (!this.isOwnedBy(event)) return;
    this.finishGesture(event, false);
  };

  private isOwnedBy(event: PointerEvent): boolean {
    return this.activePointerId !== null && event.pointerId === this.activePointerId;
  }

  private finishGesture(event: PointerEvent, includeFinalDelta: boolean): void {
    // The pointer id stays set across the flush so the last delta of a fast
    // drag is still emitted, then it is cleared before anything else can run.
    if (includeFinalDelta) {
      this.deltas.finish(event.screenX - this.lastX, event.screenY - this.lastY);
    } else {
      this.deltas.flush();
    }

    const pointerId = this.activePointerId;
    this.activePointerId = null;
    if (pointerId !== null) {
      try {
        this.target.releasePointerCapture(pointerId);
      } catch {
        // Capture may already be gone; releasing it is best effort.
      }
    }
    this.handlers.onEnd();
  }
}
