/**
 * The countdown itself: a state machine over a deadline, and nothing else.
 *
 * There is no DOM here, no bridge and no `window`. The clock is an argument,
 * so every rule below is testable at whatever instant a test names rather than
 * by waiting — a suite that has to sit through a Pomodoro is a suite nobody
 * runs.
 *
 * The one property everything else rests on: **while a run is going, the truth
 * is an instant, not a counter.** A running timer stores the wall-clock moment
 * it ends, and every reading is `deadline - now`. It is never `remaining - 1s`
 * on a tick, because that model is wrong in exactly the situations a timer is
 * for: a throttled WebView, a note that lost the CPU, a laptop closed for ten
 * minutes. Subtracting a second per tick silently pays out whatever the
 * scheduler happened to deliver; reading the clock cannot be wrong by more
 * than the age of the reading.
 *
 * Pausing is the mirror of that. The deadline is discarded and the *remaining*
 * duration is frozen, because a paused timer has no end instant — it has a
 * debt. Resuming mints a new deadline from the clock at that moment. Nothing
 * accumulates across a pause/resume cycle, however many there are, because
 * neither half ever adds to a running total.
 */

/** Which of the two tools the note is running. Never both at once. */
export type TimerMode = 'timer' | 'pomodoro';

/** Where a run is. The four states the interface has to be able to tell apart. */
export type TimerRunState = 'idle' | 'running' | 'paused' | 'finished';

/** Which step of the Pomodoro cycle the note is on. */
export type PomodoroPhase = 'focus' | 'shortBreak' | 'longBreak';

/**
 * What finished, as a value from a closed set.
 *
 * The host turns this into the notification's words. The page cannot supply
 * that text, which is the point: there is no message on the wire that a note's
 * content could be smuggled into.
 */
export type TimerFinishKind = 'timer' | 'focus' | 'shortBreak' | 'longBreak';

/** Why the state is being written out. A tick is never a reason. */
export type TimerChangeReason =
  | 'start'
  | 'pause'
  | 'resume'
  | 'cancel'
  | 'reset'
  | 'skip'
  | 'finish'
  | 'mode';

export const MIN_TIMER_MINUTES = 1;
/** Ten hours. Long enough for anything a note is used for, short enough that
 *  nothing here can leave the range whole milliseconds are exact in. */
export const MAX_TIMER_MINUTES = 600;
export const DEFAULT_TIMER_MINUTES = 25;

/** The durations worth one click. Everything else goes through the field. */
export const TIMER_PRESET_MINUTES: readonly number[] = [5, 10, 15, 25, 30, 45, 60];

export const FOCUS_MINUTES = 25;
export const SHORT_BREAK_MINUTES = 5;
export const LONG_BREAK_MINUTES = 15;
/** Four focus sessions make a cycle; the fourth is followed by the long break. */
export const FOCUS_SESSIONS_PER_CYCLE = 4;

export const MS_PER_MINUTE = 60_000;

/**
 * Everything worth surviving the note being collapsed, hidden or closed.
 *
 * Small on purpose. It is written to `state.json` beside the window geometry,
 * never into the note's Markdown, and only when one of the events in
 * [`TimerChangeReason`] happens — never on a tick, so a running timer does not
 * write to disk once a second.
 *
 * There is no stored "duration of the current run". A Pomodoro phase's length
 * is fixed by which phase it is, and a Timer's is the minutes the reader
 * chose, so the run's duration is derived from the two fields that are already
 * here rather than recorded a third time where it could come to disagree with
 * them.
 */
export interface TimerSnapshot {
  mode: TimerMode;
  state: TimerRunState;
  /** The Timer mode's chosen duration, in whole minutes. */
  timerMinutes: number;
  /** Wall-clock instant the run ends, in epoch milliseconds. Only while running. */
  deadlineMs: number | null;
  /** Frozen remainder, in milliseconds. Only while paused. */
  remainingMs: number | null;
  phase: PomodoroPhase;
  /** Focus sessions completed in this cycle, 0 to `FOCUS_SESSIONS_PER_CYCLE`. */
  focusCompleted: number;
}

/** The snapshot resolved against a particular instant, ready to be drawn. */
export interface TimerView {
  mode: TimerMode;
  state: TimerRunState;
  phase: PomodoroPhase;
  focusCompleted: number;
  /** Never negative. A finished run reads exactly zero. */
  remainingMs: number;
  /** How long the loaded run is, derived from the mode and the phase. */
  durationMs: number;
  timerMinutes: number;
  /** Whether the note has a timer worth showing in its header bar. */
  active: boolean;
}

export interface TimerEngineOptions {
  /** Epoch milliseconds. `Date.now` in the application, a fake in the tests. */
  now: () => number;
  /** A semantic change worth persisting. Never called for a tick. */
  onChange?: (snapshot: TimerSnapshot | null, reason: TimerChangeReason) => void;
  /** A run reached zero. Called at most once per run — see `finish`. */
  onFinish?: (kind: TimerFinishKind) => void;
}

/** The length of one Pomodoro phase. */
export function phaseDurationMs(phase: PomodoroPhase): number {
  switch (phase) {
    case 'focus':
      return FOCUS_MINUTES * MS_PER_MINUTE;
    case 'shortBreak':
      return SHORT_BREAK_MINUTES * MS_PER_MINUTE;
    case 'longBreak':
      return LONG_BREAK_MINUTES * MS_PER_MINUTE;
  }
}

/**
 * A duration typed by a reader, resolved to whole minutes or refused.
 *
 * Refusing is a real answer. Nothing here coerces: `''`, `0`, `-5`, `2.5`,
 * `NaN`, `Infinity` and `1e9` all come back as `null` rather than as some
 * nearby number the reader did not ask for, because a timer that quietly ran
 * for a duration nobody chose is worse than one that declined to start.
 */
export function normalizeMinutes(value: unknown): number | null {
  let minutes: unknown = value;
  if (typeof minutes === 'string') {
    const trimmed = minutes.trim();
    // `Number('')` is 0 and `Number(' ')` is 0; neither is a duration.
    if (trimmed === '') return null;
    minutes = Number(trimmed);
  }
  if (typeof minutes !== 'number') return null;
  if (!Number.isFinite(minutes) || !Number.isInteger(minutes)) return null;
  if (minutes < MIN_TIMER_MINUTES || minutes > MAX_TIMER_MINUTES) return null;
  return minutes;
}

/** The state a note with no timer at all is in. */
export function defaultSnapshot(): TimerSnapshot {
  return {
    mode: 'timer',
    state: 'idle',
    timerMinutes: DEFAULT_TIMER_MINUTES,
    deadlineMs: null,
    remainingMs: null,
    phase: 'focus',
    focusCompleted: 0,
  };
}

/**
 * The next step of the Pomodoro cycle.
 *
 * One function, used both when a phase runs out and when the reader skips it,
 * so "what comes next" has exactly one definition and the cycle cannot mean
 * two different things depending on how it was left.
 *
 * Focus, short break, focus, short break, focus, short break, focus, **long**
 * break — then the count resets and the cycle begins again.
 */
export function advancePomodoro(
  phase: PomodoroPhase,
  focusCompleted: number,
): { phase: PomodoroPhase; focusCompleted: number } {
  if (phase === 'focus') {
    const completed = focusCompleted + 1;
    return completed >= FOCUS_SESSIONS_PER_CYCLE
      ? { phase: 'longBreak', focusCompleted: FOCUS_SESSIONS_PER_CYCLE }
      : { phase: 'shortBreak', focusCompleted: completed };
  }
  if (phase === 'longBreak') {
    // The cycle is over; the next focus session is the first of a new one.
    return { phase: 'focus', focusCompleted: 0 };
  }
  return { phase: 'focus', focusCompleted };
}

/**
 * A snapshot from disk, or from a page, made safe to act on.
 *
 * `state.json` is an ordinary file a person can edit, and the WebView is not
 * trusted with the host's data structures anywhere else in Note-it either. The
 * rules are the ones the type only implies: the mode and the phase are members
 * of their sets, the counters are in range, and — the one that matters — a
 * state carries the field it is defined by or it is not that state. A
 * `running` with no deadline is not a running timer, it is a damaged record,
 * and it comes back `idle` rather than as a countdown against `null`.
 */
export function sanitizeSnapshot(value: unknown): TimerSnapshot | null {
  if (value === null || typeof value !== 'object') return null;
  const raw = value as Record<string, unknown>;

  const mode: TimerMode = raw.mode === 'pomodoro' ? 'pomodoro' : 'timer';
  const phase: PomodoroPhase =
    raw.phase === 'shortBreak' || raw.phase === 'longBreak' ? raw.phase : 'focus';

  const focusCompleted = Number.isInteger(raw.focusCompleted)
    ? Math.min(Math.max(raw.focusCompleted as number, 0), FOCUS_SESSIONS_PER_CYCLE)
    : 0;

  const timerMinutes = normalizeMinutes(raw.timerMinutes) ?? DEFAULT_TIMER_MINUTES;

  const deadlineMs =
    typeof raw.deadlineMs === 'number' && Number.isFinite(raw.deadlineMs)
      ? Math.round(raw.deadlineMs)
      : null;
  const remainingMs =
    typeof raw.remainingMs === 'number' &&
    Number.isFinite(raw.remainingMs) &&
    raw.remainingMs >= 0
      ? Math.min(Math.round(raw.remainingMs), MAX_TIMER_MINUTES * MS_PER_MINUTE)
      : null;

  let state: TimerRunState;
  switch (raw.state) {
    case 'running':
      state = deadlineMs === null ? 'idle' : 'running';
      break;
    case 'paused':
      state = remainingMs === null ? 'idle' : 'paused';
      break;
    case 'finished':
      state = 'finished';
      break;
    default:
      state = 'idle';
  }

  return {
    mode,
    state,
    timerMinutes,
    deadlineMs: state === 'running' ? deadlineMs : null,
    remainingMs: state === 'paused' ? remainingMs : null,
    phase,
    focusCompleted,
  };
}

function sameSnapshot(a: TimerSnapshot, b: TimerSnapshot): boolean {
  return (
    a.mode === b.mode &&
    a.state === b.state &&
    a.timerMinutes === b.timerMinutes &&
    a.deadlineMs === b.deadlineMs &&
    a.remainingMs === b.remainingMs &&
    a.phase === b.phase &&
    a.focusCompleted === b.focusCompleted
  );
}

/**
 * How long until the displayed number changes.
 *
 * The countdown reads `MM:SS` rounded up, so the digits change when the
 * remainder crosses a whole second — not every 1000 ms from whenever the run
 * happened to start. Scheduling the next redraw at the crossing means the
 * clock never sits on a stale second, never redraws twice for the same one,
 * and lands its last tick exactly on the deadline instead of somewhere in the
 * second after it.
 *
 * A floor keeps a run that is nearly over from becoming a busy loop.
 */
export function delayToNextDisplayChange(remainingMs: number): number {
  if (remainingMs <= 0) return 0;
  const untilCrossing = remainingMs - (Math.ceil(remainingMs / 1000) - 1) * 1000;
  return Math.min(1000, Math.max(20, untilCrossing));
}

/**
 * One note's Timer and Pomodoro, as one machine.
 *
 * One machine on purpose: a note has at most one countdown, so there is no
 * sequence of clicks that leaves two of them running. Changing mode is not a
 * way around that — the tabs are unavailable while a run is live, and the
 * engine refuses the change anyway.
 */
export class TimerEngine {
  private current: TimerSnapshot = defaultSnapshot();

  public constructor(private readonly options: TimerEngineOptions) {}

  public snapshot(): TimerSnapshot {
    return { ...this.current };
  }

  /**
   * What to persist: the snapshot, or `null` for a note that has no timer.
   *
   * A pristine note writes nothing into its window state, so opening the panel
   * and closing it again leaves `state.json` exactly as it was.
   */
  public persisted(): TimerSnapshot | null {
    return sameSnapshot(this.current, defaultSnapshot()) ? null : this.snapshot();
  }

  /**
   * Puts a stored snapshot back, resolved against the clock as it is now.
   *
   * This is where "the application was closed for ten minutes" is answered. A
   * run that was going is not resumed for its old remainder: its deadline is
   * compared with the present, so it comes back with the time that really
   * passed already taken off — and if the deadline is behind us it comes back
   * **finished**, never as a running timer counting through zero.
   *
   * Restoring never rings. A completion signal is an alarm, and replaying one
   * for a run that ended while the application was not there would be an alarm
   * about the past; the finished state is on show instead, which is what tells
   * the reader what happened. Nothing is persisted from here either: this is
   * reading the record, not changing it.
   */
  public restore(value: unknown): void {
    const restored = sanitizeSnapshot(value) ?? defaultSnapshot();
    if (
      restored.state === 'running' &&
      restored.deadlineMs !== null &&
      this.options.now() >= restored.deadlineMs
    ) {
      this.current = { ...restored, state: 'finished', deadlineMs: null, remainingMs: null };
      return;
    }
    this.current = restored;
  }

  /** How long the loaded run is, derived rather than stored. */
  public durationMs(): number {
    return this.current.mode === 'pomodoro'
      ? phaseDurationMs(this.current.phase)
      : this.current.timerMinutes * MS_PER_MINUTE;
  }

  /** The snapshot resolved against the clock, ready to be drawn. */
  public view(): TimerView {
    return {
      mode: this.current.mode,
      state: this.current.state,
      phase: this.current.phase,
      focusCompleted: this.current.focusCompleted,
      remainingMs: this.remainingMs(),
      durationMs: this.durationMs(),
      timerMinutes: this.current.timerMinutes,
      active: this.current.state !== 'idle',
    };
  }

  /**
   * How much is left, from the clock rather than from a counter.
   *
   * Clamped at zero, so nothing downstream ever has to cope with a negative
   * duration and no display can show one.
   */
  public remainingMs(): number {
    const { state, deadlineMs, remainingMs } = this.current;
    switch (state) {
      case 'running':
        return deadlineMs === null ? 0 : Math.max(0, deadlineMs - this.options.now());
      case 'paused':
        return remainingMs === null ? 0 : Math.max(0, remainingMs);
      case 'finished':
        return 0;
      case 'idle':
        return this.durationMs();
    }
  }

  /** Whether the two mode tabs may be used. A live run owns the note's timer. */
  public canChangeMode(): boolean {
    return this.current.state === 'idle' || this.current.state === 'finished';
  }

  public setMode(mode: TimerMode): void {
    if (mode === this.current.mode || !this.canChangeMode()) return;
    this.current = {
      ...this.current,
      mode,
      state: 'idle',
      deadlineMs: null,
      remainingMs: null,
    };
    this.emitChange('mode');
  }

  /**
   * The duration a Timer run will use.
   *
   * Not itself worth a write to disk — choosing a number is not starting one —
   * so it rides along in whatever the next real event persists. Refused while
   * a run is live, which is also why the field is disabled there.
   */
  public setMinutes(minutes: unknown): boolean {
    const normalized = normalizeMinutes(minutes);
    if (normalized === null || this.current.mode !== 'timer') return false;
    if (this.current.state === 'running' || this.current.state === 'paused') return false;
    this.current = {
      ...this.current,
      state: 'idle',
      deadlineMs: null,
      remainingMs: null,
      timerMinutes: normalized,
    };
    return true;
  }

  /**
   * Starts the current run: a Timer for the chosen duration, or the phase the
   * Pomodoro is on.
   *
   * The deadline is minted here and nowhere else, from the clock at this
   * instant. A run already going is left alone rather than restarted, so a
   * double click cannot push the end further out.
   */
  public start(): void {
    const { state } = this.current;
    if (state === 'running' || state === 'paused') return;
    const duration = this.durationMs();
    if (duration <= 0) return;
    this.current = {
      ...this.current,
      state: 'running',
      deadlineMs: this.options.now() + duration,
      remainingMs: null,
    };
    this.emitChange('start');
  }

  /**
   * Freezes the run.
   *
   * The remainder is worked out from the deadline before the deadline is
   * dropped, and the deadline really is dropped: an instant left lying around
   * on a paused timer is exactly what makes paused time get spent anyway when
   * something later reads it.
   */
  public pause(): void {
    if (this.current.state !== 'running') return;
    this.current = {
      ...this.current,
      state: 'paused',
      remainingMs: this.remainingMs(),
      deadlineMs: null,
    };
    this.emitChange('pause');
  }

  /** Mints a fresh deadline from the frozen remainder and the clock now. */
  public resume(): void {
    const { state, remainingMs } = this.current;
    if (state !== 'paused' || remainingMs === null) return;
    if (remainingMs <= 0) {
      // Paused with nothing left: it is over, and it goes through the one
      // completion path rather than starting a zero-length run.
      this.current = { ...this.current, state: 'running', deadlineMs: this.options.now() };
      this.finish();
      return;
    }
    this.current = {
      ...this.current,
      state: 'running',
      deadlineMs: this.options.now() + remainingMs,
      remainingMs: null,
    };
    this.emitChange('resume');
  }

  /**
   * Stops the run and leaves the note where it was.
   *
   * A cancelled Timer keeps the duration that was chosen, so starting again
   * does not mean picking it again. A cancelled Pomodoro keeps its place in
   * the cycle; `reset` is what goes back to the beginning.
   */
  public cancel(): void {
    if (this.current.state === 'idle') return;
    this.current = {
      ...this.current,
      state: 'idle',
      deadlineMs: null,
      remainingMs: null,
    };
    this.emitChange('cancel');
  }

  /** Back to the first focus session of a fresh cycle. */
  public reset(): void {
    const next: TimerSnapshot = {
      ...defaultSnapshot(),
      mode: this.current.mode,
      timerMinutes: this.current.timerMinutes,
    };
    if (sameSnapshot(next, this.current)) return;
    this.current = next;
    this.emitChange('reset');
  }

  /**
   * Moves to the next phase of the cycle without waiting for this one.
   *
   * The same transition a completed phase takes, because there is only one
   * idea of "next" — and it lands idle rather than running, so the reader
   * starts the next step deliberately and never finds a countdown they did not
   * ask for. Nothing rings: a phase that was skipped was not completed.
   */
  public skip(): void {
    if (this.current.mode !== 'pomodoro') return;
    const next = advancePomodoro(this.current.phase, this.current.focusCompleted);
    this.current = {
      ...this.current,
      state: 'idle',
      phase: next.phase,
      focusCompleted: next.focusCompleted,
      deadlineMs: null,
      remainingMs: null,
    };
    this.emitChange('skip');
  }

  /**
   * Moves a finished Pomodoro phase on to the next one and starts it.
   *
   * This is the reader pressing "start the break": the transition is theirs,
   * which is why nothing happens on its own when a phase runs out.
   */
  public advance(): void {
    if (this.current.mode !== 'pomodoro' || this.current.state !== 'finished') return;
    const next = advancePomodoro(this.current.phase, this.current.focusCompleted);
    this.current = {
      ...this.current,
      state: 'idle',
      phase: next.phase,
      focusCompleted: next.focusCompleted,
      deadlineMs: null,
      remainingMs: null,
    };
    this.start();
  }

  /**
   * Re-reads the clock, and finishes the run if it has arrived.
   *
   * Safe to call as often as anything likes: it asks a question about the
   * deadline rather than taking a step of a countdown, so calling it twice in
   * one millisecond and calling it once a minute give the same answer.
   */
  public tick(): void {
    if (this.current.state !== 'running') return;
    if (this.remainingMs() > 0) return;
    this.finish();
  }

  /**
   * The single completion.
   *
   * Guarded by the state itself rather than by a flag: only a `running` run can
   * finish, and finishing makes it `finished`, so however many ticks observe a
   * deadline in the past, exactly one of them is the one that transitions —
   * one notification, one write, one signal. That is the whole defence against
   * the race, and it is why the check and the assignment are the same step.
   */
  private finish(): void {
    if (this.current.state !== 'running') return;
    const kind: TimerFinishKind =
      this.current.mode === 'pomodoro' ? this.current.phase : 'timer';
    this.current = {
      ...this.current,
      state: 'finished',
      deadlineMs: null,
      remainingMs: null,
    };
    this.emitChange('finish');
    this.options.onFinish?.(kind);
  }

  private emitChange(reason: TimerChangeReason): void {
    this.options.onChange?.(this.persisted(), reason);
  }
}
