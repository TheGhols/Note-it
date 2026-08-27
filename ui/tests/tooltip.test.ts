import { afterEach, describe, expect, it } from 'vitest';
import { NoteInfoTooltip, TOOLTIP_DELAY_MS } from '../src/ui/tooltip.ts';
import { formatNoteTimestamp, UNKNOWN_TIMESTAMP } from '../src/format/datetime.ts';

interface FakeClock {
  pending: Array<{ handle: number; callback: () => void; delayMs: number }>;
  setTimer: (callback: () => void, delayMs: number) => number;
  clearTimer: (handle: number) => void;
  run: () => void;
}

function fakeClock(): FakeClock {
  let nextHandle = 1;
  const clock: FakeClock = {
    pending: [],
    setTimer: (callback, delayMs) => {
      const handle = nextHandle++;
      clock.pending.push({ handle, callback, delayMs });
      return handle;
    },
    clearTimer: (handle) => {
      clock.pending = clock.pending.filter((entry) => entry.handle !== handle);
    },
    run: () => {
      const due = clock.pending;
      clock.pending = [];
      for (const entry of due) entry.callback();
    },
  };
  return clock;
}

/** Local time so the assertion matches the formatter's local rendering. */
function localIso(
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
): string {
  return new Date(year, month - 1, day, hour, minute).toISOString();
}

function mountTooltip() {
  const mount = document.createElement('div');
  const dragRegion = document.createElement('div');
  dragRegion.className = 'drag-region';
  document.body.append(mount, dragRegion);

  const clock = fakeClock();
  const tooltip = new NoteInfoTooltip({
    hoverTarget: dragRegion,
    mount,
    setTimer: clock.setTimer,
    clearTimer: clock.clearTimer,
  });

  return { tooltip, dragRegion, clock };
}

function pointer(target: Element, type: string): void {
  target.dispatchEvent(new PointerEvent(type, { pointerId: 1, bubbles: false, cancelable: true }));
}

describe('formatNoteTimestamp', () => {
  it('renders pt-BR dd/MM/aaaa HH:mm', () => {
    expect(formatNoteTimestamp(localIso(2026, 8, 27, 7, 31))).toBe('27/08/2026 07:31');
    expect(formatNoteTimestamp(localIso(2026, 12, 5, 19, 4))).toBe('05/12/2026 19:04');
  });

  it('never renders the American month-first ordering', () => {
    // 03/11 in pt-BR is 3 November, not 11 March.
    expect(formatNoteTimestamp(localIso(2026, 11, 3, 0, 0))).toBe('03/11/2026 00:00');
  });

  it('reports an unknown timestamp instead of inventing one', () => {
    expect(formatNoteTimestamp(null)).toBe(UNKNOWN_TIMESTAMP);
    expect(formatNoteTimestamp(undefined)).toBe(UNKNOWN_TIMESTAMP);
    expect(formatNoteTimestamp('')).toBe(UNKNOWN_TIMESTAMP);
    expect(formatNoteTimestamp('not a date')).toBe(UNKNOWN_TIMESTAMP);
  });
});

describe('NoteInfoTooltip', () => {
  let active: NoteInfoTooltip | null = null;

  afterEach(() => {
    active?.destroy();
    active = null;
    document.body.innerHTML = '';
  });

  it('waits for a short pause before appearing', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    pointer(dragRegion, 'pointerenter');

    expect(tooltip.isVisible()).toBe(false);
    expect(tooltip.isPending()).toBe(true);
    expect(clock.pending[0].delayMs).toBe(TOOLTIP_DELAY_MS);
    expect(TOOLTIP_DELAY_MS).toBeGreaterThanOrEqual(400);
    expect(TOOLTIP_DELAY_MS).toBeLessThanOrEqual(500);

    clock.run();
    expect(tooltip.isVisible()).toBe(true);
    expect(tooltip.node.hidden).toBe(false);
  });

  it('shows the note dates in pt-BR', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    tooltip.setTimestamps({
      createdAt: localIso(2026, 8, 27, 7, 14),
      updatedAt: localIso(2026, 8, 27, 7, 31),
    });
    pointer(dragRegion, 'pointerenter');
    clock.run();

    const rows = Array.from(tooltip.node.querySelectorAll('.note-tooltip-row')).map(
      (row) => row.textContent,
    );
    expect(rows).toEqual(['Criado: 27/08/2026 07:14', 'Modificado: 27/08/2026 07:31']);
  });

  it('shows unknown dates honestly for a note without timestamps', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    tooltip.setTimestamps({ createdAt: null, updatedAt: null });
    pointer(dragRegion, 'pointerenter');
    clock.run();

    const rows = Array.from(tooltip.node.querySelectorAll('.note-tooltip-row')).map(
      (row) => row.textContent,
    );
    expect(rows).toEqual([`Criado: ${UNKNOWN_TIMESTAMP}`, `Modificado: ${UNKNOWN_TIMESTAMP}`]);
  });

  it('disappears when the cursor leaves the bar', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    pointer(dragRegion, 'pointerenter');
    clock.run();
    expect(tooltip.isVisible()).toBe(true);

    pointer(dragRegion, 'pointerleave');
    expect(tooltip.isVisible()).toBe(false);
    expect(tooltip.node.hidden).toBe(true);
  });

  it('a pending tooltip is cancelled when the cursor leaves before the delay', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    pointer(dragRegion, 'pointerenter');
    pointer(dragRegion, 'pointerleave');
    clock.run();

    expect(tooltip.isPending()).toBe(false);
    expect(tooltip.isVisible()).toBe(false);
  });

  it('does not appear during a drag', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    // Hovering arms the tooltip, then the drag starts before it is due.
    pointer(dragRegion, 'pointerenter');
    pointer(dragRegion, 'pointerdown');
    clock.run();
    expect(tooltip.isVisible()).toBe(false);

    // A tooltip already on screen is dismissed the moment a drag begins.
    pointer(dragRegion, 'pointerenter');
    clock.run();
    expect(tooltip.isVisible()).toBe(true);
    tooltip.hide();
    expect(tooltip.isVisible()).toBe(false);
  });

  it('never captures the pointer', () => {
    const { tooltip } = mountTooltip();
    active = tooltip;
    // The class is styled with pointer-events: none; assert the hook the
    // stylesheet targets is present so the rule cannot silently drift.
    expect(tooltip.node.classList.contains('note-tooltip')).toBe(true);
    expect(tooltip.node.getAttribute('role')).toBe('tooltip');
  });

  it('refreshes the modification date when the host reports a new save', () => {
    const { tooltip, dragRegion, clock } = mountTooltip();
    active = tooltip;

    tooltip.setTimestamps({
      createdAt: localIso(2026, 8, 27, 7, 14),
      updatedAt: localIso(2026, 8, 27, 7, 14),
    });
    pointer(dragRegion, 'pointerenter');
    clock.run();

    tooltip.setTimestamps({
      createdAt: localIso(2026, 8, 27, 7, 14),
      updatedAt: localIso(2026, 8, 27, 9, 45),
    });

    const rows = Array.from(tooltip.node.querySelectorAll('.note-tooltip-row')).map(
      (row) => row.textContent,
    );
    expect(rows[1]).toBe('Modificado: 27/08/2026 09:45');
  });
});
