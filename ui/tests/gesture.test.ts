import { afterEach, describe, expect, it } from 'vitest';
import { PointerGestureController } from '../src/geometry/gesture.ts';

interface PointerInit {
  pointerId?: number;
  button?: number;
  buttons?: number;
  screenX?: number;
  screenY?: number;
}

function pointerEvent(type: string, init: PointerInit = {}): PointerEvent {
  const {
    pointerId = 1,
    button = 0,
    buttons = type === 'pointerup' || type === 'pointercancel' ? 0 : 1,
    screenX = 0,
    screenY = 0,
  } = init;
  const event = new PointerEvent(type, { pointerId, button, buttons, bubbles: true, cancelable: true });
  // happy-dom ignores screen coordinates in the event init.
  Object.defineProperty(event, 'screenX', { value: screenX, configurable: true });
  Object.defineProperty(event, 'screenY', { value: screenY, configurable: true });
  return event;
}

interface Recorder {
  events: string[];
  deltas: Array<[number, number]>;
  frames: Array<() => void>;
  cancelled: number[];
}

function mountGesture(options: { canStart?: () => boolean } = {}) {
  const target = document.createElement('div');
  document.body.append(target);

  const recorder: Recorder = { events: [], deltas: [], frames: [], cancelled: [] };
  const controller = new PointerGestureController(
    target,
    {
      onStart: () => recorder.events.push('start'),
      onDelta: (dx, dy) => {
        recorder.events.push('delta');
        recorder.deltas.push([dx, dy]);
      },
      onEnd: () => recorder.events.push('end'),
    },
    {
      canStart: options.canStart,
      scheduleFrame: (callback) => {
        recorder.frames.push(callback);
        return recorder.frames.length;
      },
      cancelFrame: (handle) => recorder.cancelled.push(handle),
    },
  );

  return { target, controller, recorder };
}

describe('PointerGestureController', () => {
  let cleanup: Array<() => void> = [];

  afterEach(() => {
    for (const fn of cleanup) fn();
    cleanup = [];
    document.body.innerHTML = '';
  });

  function track<T extends { controller: PointerGestureController; target: HTMLElement }>(mounted: T): T {
    cleanup.push(() => mounted.controller.destroy());
    return mounted;
  }

  it('emits no geometry change without an active gesture', () => {
    const { target, recorder } = track(mountGesture());

    // Plain hover over the drag region, no button ever pressed.
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 0, screenX: 40, screenY: 40 }));
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 90, screenY: 90 }));
    target.dispatchEvent(pointerEvent('pointerup', { screenX: 120, screenY: 120 }));

    expect(recorder.events).toEqual([]);
    expect(recorder.frames).toHaveLength(0);
  });

  it('ends the gesture as soon as a move reports no button held', () => {
    const { target, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 100, screenY: 100 }));
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 110, screenY: 100 }));
    recorder.frames.pop()!();
    expect(recorder.deltas).toEqual([[10, 0]]);

    // The pointerup never reached us; the next hover must not drag the window.
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 0, screenX: 400, screenY: 400 }));
    expect(recorder.events).toEqual(['start', 'delta', 'end']);

    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 900, screenY: 900 }));
    expect(recorder.deltas).toEqual([[10, 0]]);
    expect(recorder.events).toEqual(['start', 'delta', 'end']);
  });

  it('pointerup fully terminates the gesture and includes the final delta', () => {
    const { target, controller, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 0, screenY: 0 }));
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 30, screenY: 15 }));
    target.dispatchEvent(pointerEvent('pointerup', { screenX: 42, screenY: 20 }));

    expect(controller.isActive()).toBe(false);
    // The pending frame delta and the last pointerup delta arrive as one move.
    expect(recorder.deltas).toEqual([[42, 20]]);
    expect(recorder.events).toEqual(['start', 'delta', 'end']);

    // Post-gesture moves are inert.
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 800, screenY: 800 }));
    expect(recorder.deltas).toEqual([[42, 20]]);
  });

  it('pointercancel fully terminates the gesture', () => {
    const { target, controller, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 0, screenY: 0 }));
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 25, screenY: 25 }));
    target.dispatchEvent(pointerEvent('pointercancel', { screenX: 25, screenY: 25 }));

    expect(controller.isActive()).toBe(false);
    expect(recorder.events).toEqual(['start', 'delta', 'end']);

    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 600, screenY: 600 }));
    expect(recorder.events).toEqual(['start', 'delta', 'end']);
  });

  it('a losing pointer capture ends the gesture', () => {
    const { target, controller, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 10, screenY: 10 }));
    expect(controller.isActive()).toBe(true);

    target.dispatchEvent(pointerEvent('lostpointercapture', { buttons: 1, screenX: 10, screenY: 10 }));

    expect(controller.isActive()).toBe(false);
    expect(recorder.events).toEqual(['start', 'end']);
  });

  it('a frame left over from a finished gesture cannot move the window', () => {
    const { target, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 0, screenY: 0 }));
    target.dispatchEvent(pointerEvent('pointermove', { buttons: 1, screenX: 50, screenY: 50 }));
    const staleFrame = recorder.frames[recorder.frames.length - 1];

    target.dispatchEvent(pointerEvent('pointerup', { screenX: 50, screenY: 50 }));
    const deltasAfterEnd = recorder.deltas.length;

    // The compositor still runs the animation frame scheduled mid-gesture.
    staleFrame();

    expect(recorder.deltas).toHaveLength(deltasAfterEnd);
    expect(recorder.events.filter((name) => name === 'end')).toHaveLength(1);
  });

  it('a second pointer cannot hijack a gesture in flight', () => {
    const { target, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { pointerId: 1, screenX: 0, screenY: 0 }));
    target.dispatchEvent(pointerEvent('pointerdown', { pointerId: 2, screenX: 500, screenY: 500 }));
    expect(recorder.events).toEqual(['start']);

    // Moves and ends from the foreign pointer are ignored entirely.
    target.dispatchEvent(pointerEvent('pointermove', { pointerId: 2, buttons: 1, screenX: 700, screenY: 700 }));
    target.dispatchEvent(pointerEvent('pointerup', { pointerId: 2, screenX: 700, screenY: 700 }));
    expect(recorder.events).toEqual(['start']);

    target.dispatchEvent(pointerEvent('pointermove', { pointerId: 1, buttons: 1, screenX: 12, screenY: 8 }));
    recorder.frames.pop()!();
    expect(recorder.deltas).toEqual([[12, 8]]);
  });

  it('ignores non-primary buttons and non-finite coordinates', () => {
    const { target, recorder } = track(mountGesture());

    target.dispatchEvent(pointerEvent('pointerdown', { button: 2, screenX: 10, screenY: 10 }));
    target.dispatchEvent(pointerEvent('pointerdown', { screenX: Number.NaN, screenY: 10 }));

    expect(recorder.events).toEqual([]);
  });

  it('refuses to start when the gesture is unavailable', () => {
    let allowed = false;
    const { target, recorder } = track(mountGesture({ canStart: () => allowed }));

    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 0, screenY: 0 }));
    expect(recorder.events).toEqual([]);

    allowed = true;
    target.dispatchEvent(pointerEvent('pointerdown', { screenX: 0, screenY: 0 }));
    expect(recorder.events).toEqual(['start']);
  });

  it('a pointerdown outside the gesture region never starts a drag', () => {
    const dragRegion = document.createElement('div');
    dragRegion.className = 'drag-region';
    const header = document.createElement('div');
    const menu = document.createElement('div');
    header.append(dragRegion, menu);
    document.body.append(header);

    const events: string[] = [];
    const controller = new PointerGestureController(dragRegion, {
      onStart: () => events.push('start'),
      onDelta: () => events.push('delta'),
      onEnd: () => events.push('end'),
    });
    cleanup.push(() => controller.destroy());

    // The popover is a sibling of the drag region, so its events bubble to the
    // header and never reach the drag handler.
    menu.dispatchEvent(pointerEvent('pointerdown', { screenX: 5, screenY: 5 }));
    expect(events).toEqual([]);
  });
});
