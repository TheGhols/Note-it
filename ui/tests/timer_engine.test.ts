import { describe, expect, it, vi } from 'vitest';
import {
  advancePomodoro,
  defaultSnapshot,
  delayToNextDisplayChange,
  FOCUS_SESSIONS_PER_CYCLE,
  MAX_TIMER_MINUTES,
  MIN_TIMER_MINUTES,
  normalizeMinutes,
  phaseDurationMs,
  sanitizeSnapshot,
  TimerChangeReason,
  TimerEngine,
  TimerFinishKind,
  TIMER_PRESET_MINUTES,
  TimerSnapshot,
} from '../src/timer/engine.ts';
import { controlsFor } from '../src/timer/controls.ts';
import {
  announcement,
  cycleLabel,
  finishMessage,
  formatRemaining,
  phaseLabel,
  stateLabel,
} from '../src/timer/format.ts';

const MINUTE = 60_000;

/**
 * A clock a test owns.
 *
 * Nothing in these tests waits. A timer's whole job is to be right about time
 * that has passed, and a suite that proves that by sitting through twenty-five
 * minutes proves it once and is then never run again.
 */
class FakeClock {
  public constructor(private value = 1_800_000_000_000) {}
  public readonly now = (): number => this.value;
  public advance(ms: number): void {
    this.value += ms;
  }
  public set(ms: number): void {
    this.value = ms;
  }
}

interface Harness {
  clock: FakeClock;
  engine: TimerEngine;
  /** Every persisted change, in order. Ticks must never appear here. */
  writes: Array<{ snapshot: TimerSnapshot | null; reason: TimerChangeReason }>;
  finishes: TimerFinishKind[];
}

function harness(start?: number): Harness {
  const clock = new FakeClock(start);
  const writes: Harness['writes'] = [];
  const finishes: TimerFinishKind[] = [];
  const engine = new TimerEngine({
    now: clock.now,
    onChange: (snapshot, reason) => writes.push({ snapshot, reason }),
    onFinish: (kind) => finishes.push(kind),
  });
  return { clock, engine, writes, finishes };
}

describe('a new timer', () => {
  it('starts idle, at twenty-five minutes, with nothing to store', () => {
    const { engine } = harness();
    const view = engine.view();
    expect(view.mode).toBe('timer');
    expect(view.state).toBe('idle');
    expect(view.timerMinutes).toBe(25);
    expect(view.remainingMs).toBe(25 * MINUTE);
    expect(view.active).toBe(false);
    // A note nobody has set a timer on writes nothing into its window state.
    expect(engine.persisted()).toBeNull();
  });

  it('offers the seven durations worth a single click', () => {
    expect(TIMER_PRESET_MINUTES).toEqual([5, 10, 15, 25, 30, 45, 60]);
  });
});

describe('starting, pausing, resuming and cancelling', () => {
  it('mints a deadline from the clock at the moment it starts', () => {
    const { clock, engine } = harness();
    engine.setMinutes(5);
    engine.start();

    expect(engine.snapshot().deadlineMs).toBe(clock.now() + 5 * MINUTE);
    expect(engine.snapshot().remainingMs).toBeNull();
    expect(engine.view().state).toBe('running');
    expect(formatRemaining(engine.view().remainingMs)).toBe('05:00');
  });

  it('reads the remainder from the clock rather than from a counter', () => {
    const { clock, engine } = harness();
    engine.setMinutes(5);
    engine.start();

    // Nothing ticked. The engine was simply asked again at a later instant,
    // which is the whole model: no accumulation, nothing to fall behind.
    clock.advance(91_000);
    expect(engine.view().remainingMs).toBe(5 * MINUTE - 91_000);
    expect(formatRemaining(engine.view().remainingMs)).toBe('03:29');
  });

  it('freezes the remainder on pause and drops the deadline', () => {
    const { clock, engine } = harness();
    engine.setMinutes(25);
    engine.start();
    clock.advance(6 * MINUTE + 18_000);
    engine.pause();

    const snapshot = engine.snapshot();
    expect(snapshot.state).toBe('paused');
    expect(snapshot.remainingMs).toBe(25 * MINUTE - (6 * MINUTE + 18_000));
    // The instant is gone, so there is nothing left for later time to be
    // measured against.
    expect(snapshot.deadlineMs).toBeNull();
  });

  it('does not spend a single millisecond while it is paused', () => {
    const { clock, engine } = harness();
    engine.setMinutes(10);
    engine.start();
    clock.advance(60_000);
    engine.pause();
    const frozen = engine.view().remainingMs;

    clock.advance(47 * MINUTE);
    engine.tick();

    expect(engine.view().state).toBe('paused');
    expect(engine.view().remainingMs).toBe(frozen);
    expect(formatRemaining(engine.view().remainingMs)).toBe('09:00');
  });

  it('mints a fresh deadline when it resumes', () => {
    const { clock, engine } = harness();
    engine.setMinutes(10);
    engine.start();
    clock.advance(60_000);
    engine.pause();
    clock.advance(3 * MINUTE);
    engine.resume();

    expect(engine.snapshot().deadlineMs).toBe(clock.now() + 9 * MINUTE);
    expect(engine.snapshot().remainingMs).toBeNull();
  });

  it('never accumulates across however many pauses and resumes', () => {
    // The failure this rules out is the classic one: each resume adding to a
    // running total, so a timer paused ten times outlives its own duration.
    const { clock, engine } = harness();
    engine.setMinutes(10);
    engine.start();

    let spent = 0;
    for (let round = 0; round < 10; round += 1) {
      clock.advance(15_000);
      spent += 15_000;
      engine.pause();
      // Paused time is not the timer's time, however much of it there is.
      clock.advance(90_000);
      engine.resume();
    }

    expect(engine.view().remainingMs).toBe(10 * MINUTE - spent);
    expect(formatRemaining(engine.view().remainingMs)).toBe('07:30');
  });

  it('keeps the chosen duration when it is cancelled', () => {
    const { engine } = harness();
    engine.setMinutes(45);
    engine.start();
    engine.cancel();

    expect(engine.view().state).toBe('idle');
    expect(engine.view().timerMinutes).toBe(45);
    expect(engine.snapshot().deadlineMs).toBeNull();
    expect(engine.snapshot().remainingMs).toBeNull();
  });

  it('refuses to restart a run that is already going', () => {
    const { clock, engine } = harness();
    engine.setMinutes(25);
    engine.start();
    const deadline = engine.snapshot().deadlineMs;

    clock.advance(30_000);
    engine.start();
    engine.start();

    // A double click cannot push the end further out.
    expect(engine.snapshot().deadlineMs).toBe(deadline);
  });

  it('refuses to change the duration while a run is live', () => {
    const { engine } = harness();
    engine.setMinutes(25);
    engine.start();
    expect(engine.setMinutes(5)).toBe(false);
    engine.pause();
    expect(engine.setMinutes(5)).toBe(false);
    engine.cancel();
    expect(engine.setMinutes(5)).toBe(true);
  });
});

describe('reaching zero', () => {
  it('finishes exactly once, however many ticks see the deadline behind them', () => {
    const { clock, engine, finishes, writes } = harness();
    engine.setMinutes(5);
    engine.start();
    clock.advance(5 * MINUTE);

    for (let attempt = 0; attempt < 50; attempt += 1) {
      engine.tick();
      clock.advance(1_000);
    }

    expect(finishes).toEqual(['timer']);
    expect(writes.filter((write) => write.reason === 'finish')).toHaveLength(1);
    expect(engine.view().state).toBe('finished');
  });

  it('reads exactly zero and never a negative, however long ago it ended', () => {
    const { clock, engine } = harness();
    engine.setMinutes(5);
    engine.start();
    clock.advance(5 * MINUTE + 9 * 60 * 60 * 1_000);
    engine.tick();

    expect(engine.view().remainingMs).toBe(0);
    expect(formatRemaining(engine.view().remainingMs)).toBe('00:00');
    // And it stays zero when asked again much later.
    clock.advance(4 * 24 * 60 * 60 * 1_000);
    expect(engine.view().remainingMs).toBe(0);
  });

  it('does not finish one millisecond early', () => {
    const { clock, engine } = harness();
    engine.setMinutes(5);
    engine.start();
    clock.advance(5 * MINUTE - 1);
    engine.tick();
    expect(engine.view().state).toBe('running');

    clock.advance(1);
    engine.tick();
    expect(engine.view().state).toBe('finished');
  });

  it('finishes rather than restarting when a spent pause is resumed', () => {
    const { engine, finishes } = harness();
    engine.setMinutes(5);
    engine.start();
    engine.pause();
    // The shape a damaged record would take: paused with nothing left.
    engine.restore({ ...engine.snapshot(), state: 'paused', remainingMs: 0 });
    engine.resume();

    expect(engine.view().state).toBe('finished');
    expect(finishes).toEqual(['timer']);
  });

  it('starts again from a finished run with the same duration', () => {
    const { clock, engine } = harness();
    engine.setMinutes(5);
    engine.start();
    clock.advance(6 * MINUTE);
    engine.tick();

    engine.start();
    expect(engine.view().state).toBe('running');
    expect(engine.snapshot().deadlineMs).toBe(clock.now() + 5 * MINUTE);
  });
});

describe('the durations a reader can ask for', () => {
  it('accepts whole minutes inside the supported range', () => {
    for (const minutes of [...TIMER_PRESET_MINUTES, MIN_TIMER_MINUTES, MAX_TIMER_MINUTES]) {
      expect(normalizeMinutes(minutes)).toBe(minutes);
      expect(normalizeMinutes(String(minutes))).toBe(minutes);
    }
    expect(normalizeMinutes(' 42 ')).toBe(42);
  });

  it('refuses rather than rounds anything it was not given', () => {
    // Every one of these comes back null, not a nearby number: a run for a
    // duration nobody chose is worse than one that declined to start.
    for (const value of [
      0,
      -1,
      -600,
      2.5,
      Number.NaN,
      Number.POSITIVE_INFINITY,
      Number.NEGATIVE_INFINITY,
      MAX_TIMER_MINUTES + 1,
      1e9,
      '',
      '   ',
      'abc',
      '25min',
      '1e3',
      null,
      undefined,
      {},
      [],
    ]) {
      expect(normalizeMinutes(value)).toBeNull();
    }
    // `Number('1e3')` is 1000, which is past the ceiling; the point is that it
    // is refused rather than clamped down to 600.
    expect(normalizeMinutes('1e3')).toBeNull();
  });

  it('leaves the engine alone when the duration is refused', () => {
    const { engine } = harness();
    expect(engine.setMinutes(0)).toBe(false);
    expect(engine.setMinutes(-5)).toBe(false);
    expect(engine.setMinutes(Number.NaN)).toBe(false);
    expect(engine.setMinutes(MAX_TIMER_MINUTES + 1)).toBe(false);
    expect(engine.view().timerMinutes).toBe(25);
    expect(engine.persisted()).toBeNull();
  });

  it('runs a five minute and a sixty minute timer to the second', () => {
    for (const minutes of [5, 60]) {
      const { clock, engine, finishes } = harness();
      engine.setMinutes(minutes);
      engine.start();

      clock.advance(minutes * MINUTE - 1_000);
      engine.tick();
      expect(engine.view().state).toBe('running');
      expect(formatRemaining(engine.view().remainingMs)).toBe('00:01');

      clock.advance(1_000);
      engine.tick();
      expect(engine.view().state).toBe('finished');
      expect(finishes).toEqual(['timer']);
    }
  });
});

describe('coming back to a note that was not on screen', () => {
  it('takes off the time that really passed, not the time that was displayed', () => {
    const { clock, engine } = harness();
    engine.setMinutes(25);
    engine.start();
    const stored = engine.persisted();

    // The note was hidden, the WebView destroyed, the application closed. Ten
    // minutes of wall clock went by with nothing counting anything.
    clock.advance(10 * MINUTE);
    const reopened = harness(clock.now());
    reopened.engine.restore(stored);

    expect(reopened.engine.view().state).toBe('running');
    expect(reopened.engine.view().remainingMs).toBe(15 * MINUTE);
    expect(formatRemaining(reopened.engine.view().remainingMs)).toBe('15:00');
  });

  it('comes back finished rather than counting through zero', () => {
    const { clock, engine } = harness();
    engine.setMinutes(25);
    engine.start();
    const stored = engine.persisted();

    clock.advance(31 * MINUTE);
    const reopened = harness(clock.now());
    reopened.engine.restore(stored);

    expect(reopened.engine.view().state).toBe('finished');
    expect(reopened.engine.view().remainingMs).toBe(0);
  });

  it('does not ring for a run that ended while nothing was there to hear it', () => {
    // Replaying an alarm about the past would be an alarm about the past. The
    // finished state is what tells the reader, and it is on show.
    const { clock, engine } = harness();
    engine.setMinutes(5);
    engine.start();
    const stored = engine.persisted();

    clock.advance(3 * 60 * MINUTE);
    const reopened = harness(clock.now());
    reopened.engine.restore(stored);

    expect(reopened.engine.view().state).toBe('finished');
    expect(reopened.finishes).toEqual([]);
    // And restoring is reading, not changing: nothing is written back.
    expect(reopened.writes).toEqual([]);
  });

  it('gives a paused run back exactly where it was left', () => {
    const { clock, engine } = harness();
    engine.setMinutes(25);
    engine.start();
    clock.advance(4 * MINUTE);
    engine.pause();
    const stored = engine.persisted();

    clock.advance(6 * 60 * MINUTE);
    const reopened = harness(clock.now());
    reopened.engine.restore(stored);

    expect(reopened.engine.view().state).toBe('paused');
    expect(reopened.engine.view().remainingMs).toBe(21 * MINUTE);
    expect(formatRemaining(reopened.engine.view().remainingMs)).toBe('21:00');
  });

  it('refuses a stored record that does not say what it claims to be', () => {
    const clock = new FakeClock();
    for (const damaged of [
      { state: 'running', deadlineMs: null },
      { state: 'paused', remainingMs: null },
      { state: 'running' },
      'não é um objeto',
      42,
      null,
      undefined,
    ]) {
      const engine = new TimerEngine({ now: clock.now });
      engine.restore(damaged);
      expect(engine.view().state).toBe('idle');
    }
  });

  it('clamps a hand-edited record instead of believing it', () => {
    const clean = sanitizeSnapshot({
      mode: 'pomodoro',
      state: 'paused',
      timerMinutes: 99_999,
      remainingMs: Number.MAX_SAFE_INTEGER,
      deadlineMs: 1,
      phase: 'nonsense',
      focusCompleted: 87,
    });
    expect(clean).not.toBeNull();
    expect(clean!.timerMinutes).toBe(25);
    expect(clean!.remainingMs).toBe(MAX_TIMER_MINUTES * MINUTE);
    expect(clean!.phase).toBe('focus');
    expect(clean!.focusCompleted).toBe(FOCUS_SESSIONS_PER_CYCLE);
    // Paused, so the leftover instant does not survive to be counted against.
    expect(clean!.deadlineMs).toBeNull();
  });
});

describe('the Pomodoro cycle', () => {
  it('walks four focus sessions to the long break and then begins again', () => {
    // The rule stated as the sequence itself rather than as four branches.
    let phase = 'focus' as const satisfies string;
    let state = { phase, focusCompleted: 0 } as ReturnType<typeof advancePomodoro>;
    const walked: string[] = [state.phase];
    for (let step = 0; step < 8; step += 1) {
      state = advancePomodoro(state.phase, state.focusCompleted);
      walked.push(state.phase);
    }

    expect(walked).toEqual([
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'longBreak',
      'focus',
    ]);
  });

  it('counts the sessions of a cycle and resets after the long break', () => {
    expect(advancePomodoro('focus', 0)).toEqual({ phase: 'shortBreak', focusCompleted: 1 });
    expect(advancePomodoro('focus', 2)).toEqual({ phase: 'shortBreak', focusCompleted: 3 });
    expect(advancePomodoro('focus', 3)).toEqual({ phase: 'longBreak', focusCompleted: 4 });
    expect(advancePomodoro('shortBreak', 2)).toEqual({ phase: 'focus', focusCompleted: 2 });
    expect(advancePomodoro('longBreak', 4)).toEqual({ phase: 'focus', focusCompleted: 0 });
  });

  it('uses twenty-five, five and fifteen minutes', () => {
    expect(phaseDurationMs('focus')).toBe(25 * MINUTE);
    expect(phaseDurationMs('shortBreak')).toBe(5 * MINUTE);
    expect(phaseDurationMs('longBreak')).toBe(15 * MINUTE);
  });

  it('runs a whole cycle, one completion at a time, and rings for each', () => {
    const { clock, engine, finishes } = harness();
    engine.setMode('pomodoro');

    const seen: string[] = [];
    for (let step = 0; step < 8; step += 1) {
      const view = engine.view();
      seen.push(view.phase);
      engine.start();
      clock.advance(phaseDurationMs(view.phase));
      engine.tick();
      expect(engine.view().state).toBe('finished');
      // The next phase is offered, never begun on its own.
      engine.advance();
    }

    expect(seen).toEqual([
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'longBreak',
    ]);
    expect(finishes).toEqual([
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'longBreak',
    ]);
    // Past the long break the count is back to nought: a fresh cycle.
    expect(engine.view().phase).toBe('focus');
    expect(engine.view().focusCompleted).toBe(0);
  });

  it('never starts the next phase by itself', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    clock.advance(25 * MINUTE);
    engine.tick();

    // A break that began on its own would be a Pomodoro nobody agreed to.
    expect(engine.view().state).toBe('finished');
    expect(engine.view().phase).toBe('focus');

    clock.advance(30 * MINUTE);
    engine.tick();
    expect(engine.view().state).toBe('finished');
    expect(engine.view().phase).toBe('focus');
  });

  it('skips a step through the same transition, and lands ready rather than running', () => {
    const { engine, finishes } = harness();
    engine.setMode('pomodoro');
    engine.skip();

    expect(engine.view().phase).toBe('shortBreak');
    expect(engine.view().focusCompleted).toBe(1);
    expect(engine.view().state).toBe('idle');
    // A phase that was skipped was not completed, so nothing rings.
    expect(finishes).toEqual([]);
  });

  it('skips out of a running phase without leaving it running', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    clock.advance(3 * MINUTE);
    engine.skip();

    expect(engine.view().state).toBe('idle');
    expect(engine.snapshot().deadlineMs).toBeNull();
    expect(engine.view().phase).toBe('shortBreak');
    expect(engine.view().remainingMs).toBe(5 * MINUTE);
  });

  it('pauses and resumes a focus session like any other run', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    clock.advance(7 * MINUTE);
    engine.pause();
    clock.advance(40 * MINUTE);
    engine.resume();

    expect(engine.view().remainingMs).toBe(18 * MINUTE);
    expect(formatRemaining(engine.view().remainingMs)).toBe('18:00');
  });

  it('goes back to the first session of a fresh cycle when it is reset', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    clock.advance(25 * MINUTE);
    engine.tick();
    engine.advance();
    engine.skip();
    expect(engine.view().focusCompleted).toBeGreaterThan(0);

    engine.reset();
    expect(engine.view().phase).toBe('focus');
    expect(engine.view().focusCompleted).toBe(0);
    expect(engine.view().state).toBe('idle');
    expect(engine.view().mode).toBe('pomodoro');
  });

  it('keeps its place in the cycle across a reopening', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    clock.advance(25 * MINUTE);
    engine.tick();
    engine.advance();
    clock.advance(2 * MINUTE);
    const stored = engine.persisted();

    const reopened = harness(clock.now());
    reopened.engine.restore(stored);
    const view = reopened.engine.view();
    expect(view.mode).toBe('pomodoro');
    expect(view.phase).toBe('shortBreak');
    expect(view.focusCompleted).toBe(1);
    expect(view.state).toBe('running');
    expect(view.remainingMs).toBe(3 * MINUTE);
  });
});

describe('one countdown per note', () => {
  it('will not change mode while a run is live', () => {
    const { engine } = harness();
    engine.setMinutes(25);
    engine.start();

    expect(engine.canChangeMode()).toBe(false);
    engine.setMode('pomodoro');
    expect(engine.view().mode).toBe('timer');
    expect(engine.view().state).toBe('running');

    engine.pause();
    expect(engine.canChangeMode()).toBe(false);
    engine.setMode('pomodoro');
    expect(engine.view().mode).toBe('timer');

    engine.cancel();
    expect(engine.canChangeMode()).toBe(true);
    engine.setMode('pomodoro');
    expect(engine.view().mode).toBe('pomodoro');
  });

  it('keeps the Timer duration across a trip to the Pomodoro tab', () => {
    const { engine } = harness();
    engine.setMinutes(45);
    engine.setMode('pomodoro');
    expect(engine.view().remainingMs).toBe(25 * MINUTE);

    engine.setMode('timer');
    expect(engine.view().timerMinutes).toBe(45);
    expect(engine.view().remainingMs).toBe(45 * MINUTE);
  });

  it('leaves nothing running behind a mode change', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    clock.advance(25 * MINUTE);
    engine.tick();

    engine.setMode('timer');
    expect(engine.view().state).toBe('idle');
    expect(engine.snapshot().deadlineMs).toBeNull();
    expect(engine.snapshot().remainingMs).toBeNull();
  });
});

describe('what reaches the disk', () => {
  it('writes for a semantic change and never for a tick', () => {
    const { clock, engine, writes } = harness();
    engine.setMinutes(5);
    engine.start();

    // Four hundred readings of the clock across a five minute run, only one
    // of which is a change: the completion.
    for (let tick = 0; tick < 400; tick += 1) {
      clock.advance(1_000);
      if (engine.view().state === 'running') engine.tick();
    }

    expect(writes.map((write) => write.reason)).toEqual(['start', 'finish']);
  });

  it('does not write for choosing a duration, which is not starting one', () => {
    const { engine, writes } = harness();
    engine.setMinutes(5);
    engine.setMinutes(45);
    engine.setMinutes(10);
    expect(writes).toEqual([]);
  });

  it('writes once for each of start, pause, resume and cancel', () => {
    const { clock, engine, writes } = harness();
    engine.setMinutes(25);
    engine.start();
    clock.advance(MINUTE);
    engine.pause();
    engine.pause();
    clock.advance(MINUTE);
    engine.resume();
    engine.resume();
    engine.cancel();
    engine.cancel();

    expect(writes.map((write) => write.reason)).toEqual([
      'start',
      'pause',
      'resume',
      'cancel',
    ]);
  });

  it('clears the record when the note goes back to having no timer', () => {
    const { engine, writes } = harness();
    engine.setMinutes(25);
    engine.start();
    engine.cancel();

    // Started at twenty-five, cancelled at twenty-five: exactly the pristine
    // state, so `state.json` is told the note has no timer rather than being
    // given a record that says nothing.
    expect(writes.at(-1)!.snapshot).toBeNull();
    expect(engine.persisted()).toBeNull();
  });

  it('stores a deadline and never a remainder for a running run', () => {
    const { engine } = harness();
    engine.setMinutes(25);
    engine.start();
    const stored = engine.persisted()!;
    expect(stored.deadlineMs).not.toBeNull();
    expect(stored.remainingMs).toBeNull();
    expect(Object.keys(stored).sort()).toEqual([
      'deadlineMs',
      'focusCompleted',
      'mode',
      'phase',
      'remainingMs',
      'state',
      'timerMinutes',
    ]);
  });

  it('carries nothing from the note in what it stores', () => {
    // The record is seven scalars. There is no field a line of the note, a
    // title or a snippet could travel in, which is why the timer cannot reach
    // search, the trash or the note's own file.
    const { engine } = harness();
    engine.setMinutes(25);
    engine.start();
    const vocabulary = [
      'timer',
      'pomodoro',
      'idle',
      'running',
      'paused',
      'finished',
      'focus',
      'shortBreak',
      'longBreak',
    ];
    for (const value of Object.values(engine.persisted()!)) {
      if (typeof value === 'string') {
        // Every string in the record is a word from a closed vocabulary, so
        // there is nowhere for a line of the note to be.
        expect(vocabulary).toContain(value);
      } else {
        expect(value === null || typeof value === 'number').toBe(true);
      }
    }
  });
});

describe('the clock as it is written', () => {
  it('reads MM:SS below an hour and H:MM:SS at or above one', () => {
    expect(formatRemaining(5 * MINUTE)).toBe('05:00');
    expect(formatRemaining(25 * MINUTE)).toBe('25:00');
    expect(formatRemaining(59 * MINUTE + 59_000)).toBe('59:59');
    expect(formatRemaining(60 * MINUTE)).toBe('1:00:00');
    expect(formatRemaining(90 * MINUTE)).toBe('1:30:00');
    expect(formatRemaining(MAX_TIMER_MINUTES * MINUTE)).toBe('10:00:00');
    expect(formatRemaining(0)).toBe('00:00');
  });

  it('rounds up, so a run reads its full duration until a second has gone', () => {
    // Rounding down would show 24:59 half a second into a 25 minute timer and
    // would reach 00:00 a whole second before the run actually ended.
    expect(formatRemaining(25 * MINUTE)).toBe('25:00');
    expect(formatRemaining(25 * MINUTE - 1)).toBe('25:00');
    expect(formatRemaining(24 * MINUTE + 59_000)).toBe('24:59');
    expect(formatRemaining(1)).toBe('00:01');
    expect(formatRemaining(1_000)).toBe('00:01');
  });

  it('never writes a negative or a number of milliseconds', () => {
    for (const value of [-1, -60_000, Number.NaN, Number.NEGATIVE_INFINITY]) {
      expect(formatRemaining(value)).toBe('00:00');
    }
    expect(formatRemaining(1_500_000)).toBe('25:00');
    expect(formatRemaining(1_500_000)).not.toContain('1500000');
  });

  it('books the next redraw for when the digits change, not a flat second later', () => {
    expect(delayToNextDisplayChange(25 * MINUTE)).toBe(1_000);
    expect(delayToNextDisplayChange(1_499_500)).toBe(500);
    expect(delayToNextDisplayChange(1_499_000)).toBe(1_000);
    expect(delayToNextDisplayChange(1_240)).toBe(240);
    // Floored, so a run about to end is not a busy loop; capped at a second,
    // so nothing renders faster than the digits move.
    expect(delayToNextDisplayChange(3)).toBe(20);
    expect(delayToNextDisplayChange(0)).toBe(0);
    expect(delayToNextDisplayChange(-5)).toBe(0);
    for (let remaining = 1; remaining < 5_000; remaining += 7) {
      const delay = delayToNextDisplayChange(remaining);
      expect(delay).toBeGreaterThan(0);
      expect(delay).toBeLessThanOrEqual(1_000);
    }
  });
});

describe('what the interface may offer', () => {
  it('never offers a control that cannot be used', () => {
    const { clock, engine } = harness();

    expect(controlsFor(engine.view())).toMatchObject({
      primary: { action: 'start', label: 'Iniciar' },
      secondary: null,
      duration: true,
      skip: false,
      modes: true,
    });

    engine.start();
    expect(controlsFor(engine.view())).toMatchObject({
      primary: { action: 'pause', label: 'Pausar' },
      secondary: { action: 'cancel' },
      duration: false,
      modes: false,
    });

    engine.pause();
    expect(controlsFor(engine.view())).toMatchObject({
      primary: { action: 'resume', label: 'Continuar' },
      secondary: { action: 'cancel' },
      duration: false,
      modes: false,
    });

    engine.resume();
    clock.advance(30 * MINUTE);
    engine.tick();
    expect(controlsFor(engine.view())).toMatchObject({
      primary: { action: 'start', label: 'Reiniciar' },
      secondary: { action: 'cancel' },
      duration: false,
      modes: true,
    });
  });

  it('names the next Pomodoro step on the button that starts it', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    expect(controlsFor(engine.view()).primary.label).toBe('Iniciar foco');
    expect(controlsFor(engine.view()).skip).toBe(true);

    engine.start();
    clock.advance(25 * MINUTE);
    engine.tick();
    const finished = controlsFor(engine.view());
    expect(finished.primary).toEqual({ label: 'Iniciar pausa curta', action: 'advance' });
    expect(finished.secondary).toEqual({ label: 'Reiniciar ciclo', action: 'reset' });
    // The way forward is the primary button; a skip here would mean the same
    // thing twice.
    expect(finished.skip).toBe(false);
  });

  it('names the long break on the button after a fourth focus session', () => {
    const { clock, engine } = harness();
    engine.setMode('pomodoro');
    for (let session = 0; session < 3; session += 1) {
      engine.start();
      clock.advance(25 * MINUTE);
      engine.tick();
      engine.advance();
      clock.advance(5 * MINUTE);
      engine.tick();
      engine.advance();
    }
    engine.start();
    clock.advance(25 * MINUTE);
    engine.tick();

    expect(controlsFor(engine.view()).primary.label).toBe('Iniciar pausa longa');
  });
});

describe('what the timer says in words', () => {
  it('names every phase and every state without relying on colour', () => {
    expect(phaseLabel('focus')).toBe('Foco');
    expect(phaseLabel('shortBreak')).toBe('Pausa curta');
    expect(phaseLabel('longBreak')).toBe('Pausa longa');
    expect(stateLabel('idle')).toBe('Pronto');
    expect(stateLabel('running')).toBe('Em andamento');
    expect(stateLabel('paused')).toBe('Pausado');
    expect(stateLabel('finished')).toBe('Concluído');
  });

  it('says where in the cycle the reader is', () => {
    expect(cycleLabel('focus', 0)).toBe('Sessão 1 de 4');
    expect(cycleLabel('shortBreak', 1)).toBe('Sessão 1 de 4');
    expect(cycleLabel('focus', 3)).toBe('Sessão 4 de 4');
    expect(cycleLabel('longBreak', 4)).toBe('Ciclo concluído');
  });

  it('gives the header readout an accessible name carrying the whole state', () => {
    const { engine } = harness();
    engine.setMinutes(5);
    engine.start();
    expect(announcement(engine.view())).toBe('Timer, em andamento, 05:00');

    engine.pause();
    expect(announcement(engine.view())).toBe('Timer, pausado, 05:00');

    const pomodoro = harness();
    pomodoro.engine.setMode('pomodoro');
    pomodoro.engine.start();
    expect(announcement(pomodoro.engine.view())).toBe('Pomodoro, foco, em andamento, 25:00');
  });

  it('says the same thing at the foot of the note that the desktop is told', () => {
    expect(finishMessage('timer')).toBe('Timer concluído.');
    expect(finishMessage('focus')).toBe('Sessão de foco concluída.');
    expect(finishMessage('shortBreak')).toBe('Pausa concluída.');
    expect(finishMessage('longBreak')).toBe('Pausa concluída.');
  });
});

describe('the record the two sides share', () => {
  it('round-trips through JSON exactly, which is how it reaches the host', () => {
    const { engine } = harness();
    engine.setMode('pomodoro');
    engine.start();
    const stored = engine.persisted();
    const reloaded = sanitizeSnapshot(JSON.parse(JSON.stringify(stored)));
    expect(reloaded).toEqual(stored);
  });

  it('treats a note with the pristine record as a note with no timer', () => {
    expect(sanitizeSnapshot(defaultSnapshot())).toEqual(defaultSnapshot());
    const { engine } = harness();
    engine.restore(defaultSnapshot());
    expect(engine.persisted()).toBeNull();
  });
});

describe('the redraw that keeps a countdown on screen', () => {
  it('never leaves two of them running, however the state is driven', () => {
    // A leaked interval is the classic countdown defect: the engine stays
    // right and the screen ends up being redrawn by three timers at once.
    vi.useFakeTimers();
    try {
      const before = vi.getTimerCount();
      const clock = new FakeClock();
      const engine = new TimerEngine({ now: clock.now });
      const redraw = (): void => {
        engine.tick();
      };
      for (let round = 0; round < 30; round += 1) {
        engine.start();
        engine.pause();
        engine.resume();
        engine.cancel();
        redraw();
      }
      // The engine books nothing itself; the panel owns the single timeout,
      // which is what `timer_ui` measures. Nothing here may leak one either.
      expect(vi.getTimerCount()).toBe(before);
    } finally {
      vi.useRealTimers();
    }
  });
});
