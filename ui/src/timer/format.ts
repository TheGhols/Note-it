import {
  FOCUS_SESSIONS_PER_CYCLE,
  PomodoroPhase,
  TimerFinishKind,
  TimerRunState,
  TimerView,
} from './engine.ts';

/**
 * A remainder, written as a clock.
 *
 * Rounded **up**, which is what a countdown means: a display reading `00:01`
 * says there is up to one second left, not that one second has already gone.
 * Rounding down would show `24:59` half a second into a twenty-five minute
 * timer and would reach `00:00` a whole second before the run actually ended.
 *
 * `MM:SS` below an hour and `H:MM:SS` at or above one, with the hours not
 * padded — `05:00`, `59:59`, `1:00:00`. Never a negative, because the engine
 * clamps at zero and this clamps again rather than trusting a caller.
 */
export function formatRemaining(ms: number): string {
  const totalSeconds = Math.max(0, Math.ceil((Number.isFinite(ms) ? ms : 0) / 1000));
  const seconds = totalSeconds % 60;
  const minutes = Math.floor(totalSeconds / 60) % 60;
  const hours = Math.floor(totalSeconds / 3600);
  const pad = (value: number): string => String(value).padStart(2, '0');
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${pad(minutes)}:${pad(seconds)}`;
}

const PHASE_LABELS: Record<PomodoroPhase, string> = {
  focus: 'Foco',
  shortBreak: 'Pausa curta',
  longBreak: 'Pausa longa',
};

export function phaseLabel(phase: PomodoroPhase): string {
  return PHASE_LABELS[phase];
}

const STATE_LABELS: Record<TimerRunState, string> = {
  idle: 'Pronto',
  running: 'Em andamento',
  paused: 'Pausado',
  finished: 'Concluído',
};

export function stateLabel(state: TimerRunState): string {
  return STATE_LABELS[state];
}

/**
 * Where the reader is in the cycle, in words.
 *
 * Words rather than four dots alone, because the position is a fact the reader
 * needs and colour is not a way to say it — see the marker row, which carries
 * both.
 */
export function cycleLabel(phase: PomodoroPhase, focusCompleted: number): string {
  if (phase === 'longBreak') return 'Ciclo concluído';
  const session = Math.min(focusCompleted + (phase === 'focus' ? 1 : 0), FOCUS_SESSIONS_PER_CYCLE);
  return `Sessão ${Math.max(session, 1)} de ${FOCUS_SESSIONS_PER_CYCLE}`;
}

/**
 * The whole state of the timer as one sentence.
 *
 * This is the accessible name of the header readout and of the panel's live
 * region, so a reader who cannot see the digits — or cannot tell the running
 * colour from the paused one — is told the same three things a sighted reader
 * gets from the panel: which tool, what state, how long is left.
 */
export function announcement(view: TimerView): string {
  const clock = formatRemaining(view.remainingMs);
  if (view.mode === 'pomodoro') {
    return `Pomodoro, ${phaseLabel(view.phase).toLowerCase()}, ${stateLabel(
      view.state,
    ).toLowerCase()}, ${clock}`;
  }
  return `Timer, ${stateLabel(view.state).toLowerCase()}, ${clock}`;
}

/**
 * What the note says at its foot when a run ends.
 *
 * The same three sentences the host posts as a notification, because they are
 * the same event: a reader watching the note and a reader who had it behind
 * another window should be told the same thing. Neither version carries the
 * note's title or a word of its text.
 */
export function finishMessage(kind: TimerFinishKind): string {
  switch (kind) {
    case 'timer':
      return 'Timer concluído.';
    case 'focus':
      return 'Sessão de foco concluída.';
    case 'shortBreak':
    case 'longBreak':
      return 'Pausa concluída.';
  }
}
