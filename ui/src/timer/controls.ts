import { advancePomodoro, TimerView } from './engine.ts';
import { phaseLabel } from './format.ts';

/**
 * Which button does what, worked out from the state alone.
 *
 * A model rather than four branches inside a click handler: "what can be done
 * from here" is the part of a state machine that is easiest to get subtly
 * wrong — a Pause offered on a paused timer, a Continue offered on one that
 * never started — and it is testable on its own only if it is a value.
 *
 * Nothing impossible is ever offered. A control that does not apply is absent,
 * not present and greyed out, except where being greyed out is itself the
 * information: the two mode tabs stay visible while a run is live so it is
 * clear the note already has one, and say why by being unavailable.
 */
export type PrimaryAction = 'start' | 'pause' | 'resume' | 'advance';
export type SecondaryAction = 'cancel' | 'reset';

export interface TimerControls {
  primary: { label: string; action: PrimaryAction };
  secondary: { label: string; action: SecondaryAction } | null;
  /** Whether "skip this step" applies. Pomodoro only, and never once a phase
   *  has already finished — there the primary button is the way forward. */
  skip: boolean;
  /** Whether the presets and the minutes field are usable. */
  duration: boolean;
  /** Whether the Timer/Pomodoro tabs may be used. */
  modes: boolean;
}

export function controlsFor(view: TimerView): TimerControls {
  const modes = view.state === 'idle' || view.state === 'finished';

  if (view.mode === 'timer') {
    switch (view.state) {
      case 'idle':
        return {
          primary: { label: 'Iniciar', action: 'start' },
          secondary: null,
          skip: false,
          duration: true,
          modes,
        };
      case 'running':
        return {
          primary: { label: 'Pausar', action: 'pause' },
          secondary: { label: 'Cancelar', action: 'cancel' },
          skip: false,
          duration: false,
          modes,
        };
      case 'paused':
        return {
          primary: { label: 'Continuar', action: 'resume' },
          secondary: { label: 'Cancelar', action: 'cancel' },
          skip: false,
          duration: false,
          modes,
        };
      case 'finished':
        return {
          primary: { label: 'Reiniciar', action: 'start' },
          secondary: { label: 'Limpar', action: 'cancel' },
          skip: false,
          duration: false,
          modes,
        };
    }
  }

  switch (view.state) {
    case 'idle':
      return {
        primary: {
          label: `Iniciar ${phaseLabel(view.phase).toLowerCase()}`,
          action: 'start',
        },
        secondary: null,
        skip: true,
        duration: false,
        modes,
      };
    case 'running':
      return {
        primary: { label: 'Pausar', action: 'pause' },
        secondary: { label: 'Cancelar', action: 'cancel' },
        skip: true,
        duration: false,
        modes,
      };
    case 'paused':
      return {
        primary: { label: 'Continuar', action: 'resume' },
        secondary: { label: 'Cancelar', action: 'cancel' },
        skip: true,
        duration: false,
        modes,
      };
    case 'finished': {
      // The phase is over and the next one is *offered*, never begun: a break
      // that started on its own while the reader was still writing would be a
      // Pomodoro nobody agreed to.
      const next = advancePomodoro(view.phase, view.focusCompleted);
      return {
        primary: {
          label: `Iniciar ${phaseLabel(next.phase).toLowerCase()}`,
          action: 'advance',
        },
        secondary: { label: 'Reiniciar ciclo', action: 'reset' },
        skip: false,
        duration: false,
        modes,
      };
    }
  }
}
