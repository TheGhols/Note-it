const MAX_POINTER_DELTA = 100_000;

type FrameCallback = () => void;
type ScheduleFrame = (callback: FrameCallback) => number;
type CancelFrame = (handle: number) => void;

export class PointerDeltaCoalescer {
  private pendingX = 0;
  private pendingY = 0;
  private frameHandle: number | null = null;

  public constructor(
    private readonly onDelta: (dx: number, dy: number) => void,
    private readonly scheduleFrame: ScheduleFrame = (callback) => requestAnimationFrame(callback),
    private readonly cancelFrame: CancelFrame = (handle) => cancelAnimationFrame(handle),
  ) {}

  public add(dx: number, dy: number): boolean {
    if (!isValidPointerDelta(dx) || !isValidPointerDelta(dy)) {
      return false;
    }

    this.pendingX += dx;
    this.pendingY += dy;
    if (this.frameHandle === null) {
      this.frameHandle = this.scheduleFrame(() => {
        this.frameHandle = null;
        this.emitPending();
      });
    }
    return true;
  }

  public flush(): void {
    if (this.frameHandle !== null) {
      this.cancelFrame(this.frameHandle);
      this.frameHandle = null;
    }
    this.emitPending();
  }

  public finish(dx: number, dy: number): boolean {
    const accepted = dx === 0 && dy === 0 ? true : this.add(dx, dy);
    this.flush();
    return accepted;
  }

  public reset(): void {
    if (this.frameHandle !== null) {
      this.cancelFrame(this.frameHandle);
      this.frameHandle = null;
    }
    this.pendingX = 0;
    this.pendingY = 0;
  }

  private emitPending(): void {
    const dx = this.pendingX;
    const dy = this.pendingY;
    this.pendingX = 0;
    this.pendingY = 0;
    if (dx !== 0 || dy !== 0) {
      this.onDelta(dx, dy);
    }
  }
}

export function isValidPointerDelta(value: number): boolean {
  return Number.isFinite(value) && Math.abs(value) <= MAX_POINTER_DELTA;
}
