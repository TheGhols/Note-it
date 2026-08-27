import { describe, expect, it } from 'vitest';
import { PointerDeltaCoalescer } from '../src/geometry/pointerDelta.ts';

describe('PointerDeltaCoalescer', () => {
  it('coalesces fractional pointer moves once per animation frame without losing subpixels', () => {
    const frames: Array<() => void> = [];
    const emitted: Array<[number, number]> = [];
    const coalescer = new PointerDeltaCoalescer(
      (dx, dy) => emitted.push([dx, dy]),
      (callback) => {
        frames.push(callback);
        return frames.length;
      },
      () => {},
    );

    expect(coalescer.add(0.25, -0.4)).toBe(true);
    expect(coalescer.add(0.35, -0.3)).toBe(true);
    expect(coalescer.add(0.4, -0.3)).toBe(true);
    expect(frames).toHaveLength(1);
    expect(emitted).toEqual([]);

    frames[0]();
    expect(emitted).toEqual([[1, -1]]);
  });

  it('flushes the pending resize delta before the gesture ends', () => {
    const emitted: Array<[number, number]> = [];
    let cancelledHandle: number | null = null;
    const coalescer = new PointerDeltaCoalescer(
      (dx, dy) => emitted.push([dx, dy]),
      () => 42,
      (handle) => {
        cancelledHandle = handle;
      },
    );

    coalescer.add(9.9140625, 0.87109375);
    coalescer.flush();

    expect(cancelledHandle).toBe(42);
    expect(emitted).toEqual([[9.9140625, 0.87109375]]);
  });

  it('rejects non-finite and absurd deltas without scheduling IPC', () => {
    let scheduled = 0;
    const coalescer = new PointerDeltaCoalescer(
      () => {
        throw new Error('invalid delta must not be emitted');
      },
      () => {
        scheduled += 1;
        return scheduled;
      },
      () => {},
    );

    expect(coalescer.add(Number.NaN, 0)).toBe(false);
    expect(coalescer.add(Number.POSITIVE_INFINITY, 0)).toBe(false);
    expect(coalescer.add(100_000.1, 0)).toBe(false);
    expect(scheduled).toBe(0);
  });
});
