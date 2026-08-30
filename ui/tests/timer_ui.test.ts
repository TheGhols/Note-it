import { afterEach, describe, expect, inject, it, vi } from 'vitest';
import { TimerPanel } from '../src/ui/timerPanel.ts';
import {
  MAX_TIMER_MINUTES,
  MIN_TIMER_MINUTES,
  TIMER_PRESET_MINUTES,
  TimerFinishKind,
  TimerSnapshot,
} from '../src/timer/engine.ts';
import { declarationIn, ruleFor, rulesFor } from './support/stylesheet.ts';

const MINUTE = 60_000;
const THEME_CSS = inject('themeCss');

/** The page exactly as the application loads it, icons and all. */
function renderedPage(): Document {
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

interface Mounted {
  panel: TimerPanel;
  trigger: HTMLElement;
  readout: HTMLElement;
  persisted: Array<TimerSnapshot | null>;
  finished: TimerFinishKind[];
  opens: number;
  closes: number;
  advance(ms: number): void;
  now(): number;
}

let mounted: TimerPanel | null = null;

afterEach(() => {
  mounted?.destroy();
  mounted = null;
  document.body.innerHTML = '';
  document.body.removeAttribute('data-timer');
  document.body.removeAttribute('data-collapsed');
});

/**
 * A timer panel wired to the real header markup, the way `main.ts` wires it.
 *
 * Built from the shipped page rather than a fixture, so a button that is
 * renamed or dropped fails here rather than in front of a reader.
 */
function mount(): Mounted {
  document.body.innerHTML = renderedPage().body.innerHTML;
  const trigger = document.getElementById('btn-timer')!;
  const readout = document.getElementById('note-timer-readout')!;
  const mountPoint = document.getElementById('note-controls-left')!;

  let clock = 1_800_000_000_000;
  const persisted: Array<TimerSnapshot | null> = [];
  const finished: TimerFinishKind[] = [];
  const counters = { opens: 0, closes: 0 };

  const panel = new TimerPanel({
    trigger,
    readout,
    mount: mountPoint,
    now: () => clock,
    handlers: {
      onPersist: (snapshot) => persisted.push(snapshot),
      onFinished: (kind) => finished.push(kind),
      onOpen: () => {
        counters.opens += 1;
      },
      onClose: () => {
        counters.closes += 1;
      },
    },
  });
  mounted = panel;

  return {
    panel,
    trigger,
    readout,
    persisted,
    finished,
    get opens() {
      return counters.opens;
    },
    get closes() {
      return counters.closes;
    },
    advance: (ms: number) => {
      clock += ms;
    },
    now: () => clock,
  };
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

function panelBody(): HTMLElement {
  return document.getElementById('note-timer')!;
}

function clockText(): string {
  return panelBody().querySelector('.note-timer-clock')!.textContent ?? '';
}

function stateText(): string {
  return panelBody().querySelector('.note-timer-state')!.textContent ?? '';
}

function primary(): HTMLButtonElement {
  return panelBody().querySelector<HTMLButtonElement>('.note-timer-primary')!;
}

function secondary(): HTMLButtonElement {
  return panelBody().querySelector<HTMLButtonElement>('.note-timer-secondary')!;
}

function skip(): HTMLButtonElement {
  return panelBody().querySelector<HTMLButtonElement>('.note-timer-skip')!;
}

function modeButton(mode: 'timer' | 'pomodoro'): HTMLButtonElement {
  return panelBody().querySelector<HTMLButtonElement>(`.note-timer-mode[data-mode="${mode}"]`)!;
}

function minutesField(): HTMLInputElement {
  return panelBody().querySelector<HTMLInputElement>('.note-timer-input')!;
}

describe('the timer control in the header', () => {
  it('ships in the bar, named for a reader who cannot see the icon', () => {
    const page = renderedPage();
    const button = page.getElementById('btn-timer');

    expect(button).not.toBeNull();
    expect(button!.getAttribute('aria-label')).toBe('Timer e Pomodoro');
    expect(button!.getAttribute('title')).toBe('Timer e Pomodoro');
    expect(button!.closest('.note-controls-left')).not.toBeNull();
    // Outside the drag region, so pressing it can never move the window.
    expect(button!.closest('.drag-region')).toBeNull();
    // It is not a seventh quick action: those open panels of the note menu,
    // and this opens its own.
    expect(button!.classList.contains('header-quick-action')).toBe(false);
    expect(page.querySelectorAll('.header-quick-action')).toHaveLength(6);
  });

  it('shows only the icon until the note has a timer', () => {
    const note = mount();
    expect(note.readout.hidden).toBe(true);
    expect(note.readout.textContent).toBe('');
    expect(note.trigger.dataset.timerState).toBe('idle');
    expect(document.body.getAttribute('data-timer')).toBe('idle');
    expect(note.trigger.getAttribute('aria-label')).toBe('Timer e Pomodoro');
  });

  it('carries the remaining time once there is one, and its state in words', () => {
    const note = mount();
    note.panel.openPanel();
    click(primary());

    expect(note.readout.hidden).toBe(false);
    expect(note.readout.textContent).toBe('25:00');
    expect(note.trigger.dataset.timerState).toBe('running');
    expect(document.body.getAttribute('data-timer')).toBe('running');
    // Not a colour: the state is in the accessible name and the title.
    expect(note.trigger.getAttribute('aria-label')).toBe('Timer, em andamento, 25:00');
    expect(note.trigger.title).toBe('Timer, em andamento, 25:00');
  });

  it('keeps the readout in step without the panel being open', () => {
    const note = mount();
    note.panel.openPanel();
    click(primary());
    note.panel.close();

    note.advance(90_000);
    note.panel.refresh();

    expect(note.panel.isOpen()).toBe(false);
    expect(note.readout.textContent).toBe('23:30');
    expect(note.panel.currentView().state).toBe('running');
  });

  it('reads an hour-long run as H:MM:SS in both places', () => {
    const note = mount();
    note.panel.openPanel();
    minutesField().value = '60';
    click(primary());

    expect(note.readout.textContent).toBe('1:00:00');
    expect(clockText()).toBe('1:00:00');

    // Past the hour it becomes MM:SS again, at the crossing rather than a
    // second either side of it.
    note.advance(MINUTE);
    note.panel.refresh();
    expect(clockText()).toBe('59:00');
    note.advance(1);
    note.panel.refresh();
    expect(clockText()).toBe('59:00');
    note.advance(999);
    note.panel.refresh();
    expect(clockText()).toBe('58:59');
  });
});

describe('opening and closing the panel', () => {
  it('opens on the button and closes on the button again', () => {
    const note = mount();
    expect(panelBody().hidden).toBe(true);

    click(note.trigger);
    expect(note.panel.isOpen()).toBe(true);
    expect(panelBody().hidden).toBe(false);
    expect(note.trigger.getAttribute('aria-expanded')).toBe('true');
    expect(note.opens).toBe(1);

    click(note.trigger);
    expect(note.panel.isOpen()).toBe(false);
    expect(panelBody().hidden).toBe(true);
    expect(note.trigger.getAttribute('aria-expanded')).toBe('false');
    expect(note.closes).toBe(1);
  });

  it('closes on Escape and on a pointer landing somewhere else', () => {
    const note = mount();

    note.panel.openPanel();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(note.panel.isOpen()).toBe(false);

    note.panel.openPanel();
    document
      .getElementById('editor-container')!
      .dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(note.panel.isOpen()).toBe(false);
  });

  it('stays open for a pointer inside itself', () => {
    const note = mount();
    note.panel.openPanel();
    panelBody().dispatchEvent(new Event('pointerdown', { bubbles: true }));
    expect(note.panel.isOpen()).toBe(true);
  });

  it('is one popover, opened many times, never stacked', () => {
    const note = mount();
    for (let attempt = 0; attempt < 5; attempt += 1) {
      note.panel.openPanel();
    }
    expect(document.querySelectorAll('#note-timer')).toHaveLength(1);
    expect(document.querySelectorAll('.note-timer')).toHaveLength(1);
  });

  it('does not stop the countdown when it closes', () => {
    const note = mount();
    note.panel.openPanel();
    click(primary());
    note.panel.close();

    note.advance(25 * MINUTE);
    note.panel.refresh();

    expect(note.panel.currentView().state).toBe('finished');
    expect(note.finished).toEqual(['timer']);
  });
});

describe('the two modes', () => {
  it('marks the mode it is in and switches between them', () => {
    const note = mount();
    note.panel.openPanel();

    expect(modeButton('timer').getAttribute('aria-checked')).toBe('true');
    expect(modeButton('pomodoro').getAttribute('aria-checked')).toBe('false');

    click(modeButton('pomodoro'));
    expect(modeButton('pomodoro').getAttribute('aria-checked')).toBe('true');
    expect(panelBody().dataset.mode).toBe('pomodoro');
    expect(clockText()).toBe('25:00');
    expect(panelBody().querySelector('.note-timer-phase')!.textContent).toBe(
      'Foco · Sessão 1 de 4',
    );
  });

  it('makes the other mode unavailable while a run is live, and says so', () => {
    const note = mount();
    note.panel.openPanel();
    click(primary());

    // Visible, so the reader can see the choice exists — and disabled, so it
    // is clear the note already has a countdown.
    expect(modeButton('pomodoro').disabled).toBe(true);
    click(modeButton('pomodoro'));
    expect(note.panel.currentView().mode).toBe('timer');

    click(secondary());
    expect(modeButton('pomodoro').disabled).toBe(false);
  });

  it('hides the durations outside the Timer mode', () => {
    const note = mount();
    note.panel.openPanel();
    expect(panelBody().querySelector<HTMLElement>('.note-timer-duration')!.hidden).toBe(false);

    click(modeButton('pomodoro'));
    expect(panelBody().querySelector<HTMLElement>('.note-timer-duration')!.hidden).toBe(true);
    expect(panelBody().querySelector<HTMLElement>('.note-timer-phase')!.hidden).toBe(false);
  });
});

describe('choosing a duration', () => {
  it('offers the seven presets and marks the chosen one', () => {
    const note = mount();
    note.panel.openPanel();
    const presets = panelBody().querySelectorAll<HTMLButtonElement>('.note-timer-preset');

    expect(presets).toHaveLength(TIMER_PRESET_MINUTES.length);
    expect([...presets].map((button) => button.textContent)).toEqual(
      TIMER_PRESET_MINUTES.map(String),
    );
    for (const preset of presets) {
      expect(preset.getAttribute('aria-label')).toBe(`${preset.dataset.minutes} minutos`);
    }

    click(presets[0]);
    expect(clockText()).toBe('05:00');
    expect(presets[0].getAttribute('aria-checked')).toBe('true');
    expect(minutesField().value).toBe('5');
    // Choosing is not starting: nothing is written for it.
    expect(note.persisted).toEqual([]);
  });

  it('accepts a typed duration and starts it', () => {
    const note = mount();
    note.panel.openPanel();
    minutesField().value = '90';
    click(primary());

    expect(note.panel.currentView().state).toBe('running');
    expect(clockText()).toBe('1:30:00');
  });

  it('starts on Enter in the field', () => {
    const note = mount();
    note.panel.openPanel();
    minutesField().value = '7';
    minutesField().dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    );

    expect(note.panel.currentView().state).toBe('running');
    expect(clockText()).toBe('07:00');
  });

  it('refuses a duration it was not given, and says so instead of guessing', () => {
    const note = mount();
    note.panel.openPanel();
    const hint = panelBody().querySelector<HTMLElement>('.note-timer-hint')!;

    // A `type="number"` field will not hold letters at all, so what can reach
    // this code is a number the range refuses, a fraction, or a decimal comma
    // that is not a number in any locale JavaScript parses.
    for (const bad of ['0', '-5', '2,5', '2.5', String(MAX_TIMER_MINUTES + 1)]) {
      minutesField().value = bad;
      click(primary());

      expect(note.panel.currentView().state).toBe('idle');
      expect(hint.hidden).toBe(false);
      expect(minutesField().getAttribute('aria-invalid')).toBe('true');
      // Nothing was rounded into range and nothing was started.
      expect(note.persisted).toEqual([]);
    }

    expect(hint.textContent).toContain(String(MIN_TIMER_MINUTES));
    expect(hint.textContent).toContain(String(MAX_TIMER_MINUTES));
  });

  it('clears the refusal as soon as the reader types again', () => {
    const note = mount();
    note.panel.openPanel();
    minutesField().value = '0';
    click(primary());
    expect(minutesField().getAttribute('aria-invalid')).toBe('true');

    minutesField().value = '30';
    minutesField().dispatchEvent(new Event('input', { bubbles: true }));
    expect(minutesField().getAttribute('aria-invalid')).toBeNull();
    expect(panelBody().querySelector<HTMLElement>('.note-timer-hint')!.hidden).toBe(true);

    click(primary());
    expect(clockText()).toBe('30:00');
  });

  it('uses what is already chosen when the field is left empty', () => {
    const note = mount();
    note.panel.openPanel();
    click(panelBody().querySelectorAll<HTMLButtonElement>('.note-timer-preset')[4]);
    minutesField().value = '';
    click(primary());

    expect(note.panel.currentView().state).toBe('running');
    expect(clockText()).toBe('30:00');
  });
});

describe('the four states, and only the controls each one has', () => {
  it('walks idle, running, paused and finished with the right buttons', () => {
    const note = mount();
    note.panel.openPanel();

    expect(primary().textContent).toBe('Iniciar');
    expect(secondary().hidden).toBe(true);
    expect(stateText()).toBe('Pronto');

    click(primary());
    expect(primary().textContent).toBe('Pausar');
    expect(secondary().hidden).toBe(false);
    expect(secondary().textContent).toBe('Cancelar');
    expect(stateText()).toBe('Em andamento');

    note.advance(6 * MINUTE + 18_000);
    click(primary());
    expect(primary().textContent).toBe('Continuar');
    expect(stateText()).toBe('Pausado');
    expect(clockText()).toBe('18:42');

    // Paused time is not the run's time.
    note.advance(40 * MINUTE);
    note.panel.refresh();
    expect(clockText()).toBe('18:42');
    expect(stateText()).toBe('Pausado');

    click(primary());
    expect(stateText()).toBe('Em andamento');
    note.advance(18 * MINUTE + 42_000);
    note.panel.refresh();

    expect(stateText()).toBe('Concluído');
    expect(clockText()).toBe('00:00');
    expect(primary().textContent).toBe('Reiniciar');
    expect(secondary().textContent).toBe('Limpar');
    expect(panelBody().dataset.state).toBe('finished');
  });

  it('cancels back to a note that is ready rather than one that is running', () => {
    const note = mount();
    note.panel.openPanel();
    click(primary());
    note.advance(3 * MINUTE);
    click(secondary());

    expect(note.panel.currentView().state).toBe('idle');
    expect(clockText()).toBe('25:00');
    expect(note.readout.hidden).toBe(true);
    expect(document.body.getAttribute('data-timer')).toBe('idle');
    // Back to a note with no timer, so the window state is told to drop it.
    expect(note.persisted.at(-1)).toBeNull();
  });

  it('names every control it shows, in every state', () => {
    const note = mount();
    note.panel.openPanel();

    const named = (): void => {
      for (const button of panelBody().querySelectorAll<HTMLElement>('button')) {
        if (button.hidden) continue;
        const name = button.getAttribute('aria-label') ?? button.textContent ?? '';
        expect(name.trim().length).toBeGreaterThan(0);
      }
    };

    named();
    click(primary());
    named();
    click(primary());
    named();
    click(modeButton('pomodoro'));
    named();
  });
});

describe('the Pomodoro in the panel', () => {
  it('shows the phase, the session and the cycle it is on', () => {
    const note = mount();
    note.panel.openPanel();
    click(modeButton('pomodoro'));

    const marks = panelBody().querySelectorAll<HTMLElement>('.note-timer-mark');
    expect(marks).toHaveLength(4);
    expect(marks[0].dataset.state).toBe('current');
    expect(marks[1].dataset.state).toBe('pending');
    // The dots are the phase line drawn, so they are not read out twice.
    expect(panelBody().querySelector('.note-timer-cycle')!.getAttribute('aria-hidden')).toBe(
      'true',
    );

    click(primary());
    note.advance(25 * MINUTE);
    note.panel.refresh();
    click(primary());

    expect(panelBody().querySelector('.note-timer-phase')!.textContent).toBe(
      'Pausa curta · Sessão 1 de 4',
    );
    expect(
      panelBody().querySelectorAll<HTMLElement>('.note-timer-mark')[0].dataset.state,
    ).toBe('done');
  });

  it('offers the next step rather than starting it', () => {
    const note = mount();
    note.panel.openPanel();
    click(modeButton('pomodoro'));
    click(primary());
    note.advance(25 * MINUTE);
    note.panel.refresh();

    expect(stateText()).toBe('Concluído');
    expect(primary().textContent).toBe('Iniciar pausa curta');
    expect(secondary().textContent).toBe('Reiniciar ciclo');
    expect(note.panel.currentView().phase).toBe('focus');

    click(primary());
    expect(note.panel.currentView().phase).toBe('shortBreak');
    expect(note.panel.currentView().state).toBe('running');
    expect(clockText()).toBe('05:00');
  });

  it('reaches the long break after the fourth focus session', () => {
    const note = mount();
    note.panel.openPanel();
    click(modeButton('pomodoro'));

    // Focus, break, focus, break, focus, break, focus — seven phases, each one
    // started by the reader from the button the finished one offers.
    const cycle = [25, 5, 25, 5, 25, 5, 25];
    click(primary());
    for (const [step, minutes] of cycle.entries()) {
      note.advance(minutes * MINUTE);
      note.panel.refresh();
      expect(stateText()).toBe('Concluído');
      if (step < cycle.length - 1) click(primary());
    }

    expect(primary().textContent).toBe('Iniciar pausa longa');
    click(primary());
    expect(note.panel.currentView().phase).toBe('longBreak');
    expect(clockText()).toBe('15:00');
    expect(note.finished).toEqual([
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
      'shortBreak',
      'focus',
    ]);
  });

  it('skips a step, and the skip is gone once a phase has finished', () => {
    const note = mount();
    note.panel.openPanel();
    click(modeButton('pomodoro'));
    expect(skip().hidden).toBe(false);

    click(skip());
    expect(note.panel.currentView().phase).toBe('shortBreak');
    expect(note.panel.currentView().state).toBe('idle');
    expect(note.finished).toEqual([]);

    click(primary());
    note.advance(5 * MINUTE);
    note.panel.refresh();
    // The way forward is the primary button; a skip would say it twice.
    expect(skip().hidden).toBe(true);
  });

  it('goes back to the start of a cycle on Reiniciar ciclo', () => {
    const note = mount();
    note.panel.openPanel();
    click(modeButton('pomodoro'));
    click(primary());
    note.advance(25 * MINUTE);
    note.panel.refresh();
    click(secondary());

    expect(note.panel.currentView().phase).toBe('focus');
    expect(note.panel.currentView().focusCompleted).toBe(0);
    expect(note.panel.currentView().state).toBe('idle');
  });
});

describe('finishing', () => {
  it('reports the completion exactly once, however often it is redrawn', () => {
    const note = mount();
    note.panel.openPanel();
    click(panelBody().querySelectorAll<HTMLButtonElement>('.note-timer-preset')[0]);
    click(primary());

    note.advance(5 * MINUTE);
    for (let redraw = 0; redraw < 40; redraw += 1) {
      note.panel.refresh();
      note.advance(1_000);
    }

    expect(note.finished).toEqual(['timer']);
    expect(note.persisted.filter((snapshot) => snapshot?.state === 'finished')).toHaveLength(1);
  });

  it('does not report one for a run restored past its deadline', () => {
    const note = mount();
    note.panel.restore({
      mode: 'timer',
      state: 'running',
      timerMinutes: 25,
      deadlineMs: note.now() - 3 * MINUTE,
      remainingMs: null,
      phase: 'focus',
      focusCompleted: 0,
    });

    expect(note.panel.currentView().state).toBe('finished');
    expect(note.readout.textContent).toBe('00:00');
    // Restoring is reading, not changing: no alarm and no write.
    expect(note.finished).toEqual([]);
    expect(note.persisted).toEqual([]);
  });

  it('gives a restored running timer the time that really passed', () => {
    const note = mount();
    note.panel.restore({
      mode: 'timer',
      state: 'running',
      timerMinutes: 25,
      deadlineMs: note.now() + 15 * MINUTE,
      remainingMs: null,
      phase: 'focus',
      focusCompleted: 0,
    });

    expect(note.panel.currentView().state).toBe('running');
    expect(note.readout.textContent).toBe('15:00');
  });
});

describe('a collapsed note', () => {
  it('keeps the timer on the bar and gives up the quick actions instead', () => {
    // The six actions go, because a collapsed note is only its bar. The timer
    // stays whenever there is one: a countdown you have to expand a note to
    // see is a countdown you cannot trust.
    expect(declarationIn('body[data-collapsed="true"] .header-quick-action', 'display')).toBe(
      'none',
    );
    expect(declarationIn('body[data-collapsed="true"] .header-timer-action', 'display')).toBe(
      'none',
    );
    for (const state of ['running', 'paused', 'finished']) {
      expect(
        declarationIn(
          `body[data-collapsed="true"][data-timer="${state}"] .header-timer-action`,
          'display',
        ),
      ).toBe('inline-flex');
    }
    // An idle note gets no control it cannot use on a bar that small.
    expect(
      rulesFor('body[data-collapsed="true"][data-timer="idle"] .header-timer-action'),
    ).toHaveLength(0);
  });

  it('publishes the state the stylesheet reads', () => {
    const note = mount();
    note.panel.openPanel();
    expect(document.body.getAttribute('data-timer')).toBe('idle');

    click(primary());
    expect(document.body.getAttribute('data-timer')).toBe('running');

    click(primary());
    expect(document.body.getAttribute('data-timer')).toBe('paused');

    click(primary());
    expect(document.body.getAttribute('data-timer')).toBe('running');

    note.advance(30 * MINUTE);
    note.panel.refresh();
    expect(document.body.getAttribute('data-timer')).toBe('finished');
  });

  it('shows the digits on a collapsed note at any width', () => {
    // The narrow-note rule is scoped to expanded notes on purpose: with the
    // six quick actions gone there is always room for the clock.
    const narrow = /@media \(max-width: (\d+)px\) \{\s*([^}]*)\}/.exec(THEME_CSS);
    expect(narrow).not.toBeNull();
    expect(narrow![2]).toContain('body:not([data-collapsed="true"]) .header-timer-readout');
    expect(narrow![2]).toContain('display: none');
  });
});

describe('the header bar still fits', () => {
  it('leaves room for every control at the narrowest a note can be', () => {
    // 3.9UX.R.2 is not to be reopened, and neither is the close cross being
    // pushed off the edge. The budget is measured rather than assumed.
    const page = renderedPage();
    const floor = inject('minNoteWidth');
    const iconPadding = Number.parseFloat(declarationIn('.icon-btn', 'padding'));
    const quickIcon = Number.parseFloat(
      ruleFor(':root').body.match(/--header-action-size:\s*([\d.]+)px/)![1],
    );
    const headerPadding = Number.parseFloat(
      declarationIn('.note-header', 'padding').split(/\s+/)[1],
    );

    let width = headerPadding * 2;
    for (const button of page.querySelectorAll('.note-header .icon-btn')) {
      const drawn = button.querySelector('svg');
      const intrinsic = drawn?.getAttribute('width');
      // A hand-drawn mark states its own size; a quick action is sized by the
      // stylesheet token.
      width += (intrinsic ? Number.parseFloat(intrinsic) : quickIcon) + iconPadding * 2;
    }

    // Every control, on the narrowest note the host will make, with the name
    // still able to take what is left.
    expect(width).toBeLessThan(floor);
  });

  it('holds the whole control row inside the strip the bar paints its paper over', () => {
    // The gutter fill is what keeps scrolled text from appearing under the
    // icons. A control taller than the gutter would reopen exactly that.
    const gutter = Number.parseFloat(ruleFor(':root').body.match(/--note-chrome-gutter:\s*([\d.]+)px/)![1]);
    const iconPadding = Number.parseFloat(declarationIn('.icon-btn', 'padding'));
    const offset = Number.parseFloat(declarationIn('.note-header .icon-btn', 'margin-top'));
    const readoutLine = Number.parseFloat(
      declarationIn('.header-timer-readout', 'line-height'),
    );

    expect(offset + iconPadding + readoutLine + iconPadding).toBeLessThanOrEqual(gutter);
  });

  it('never lets the digits change the width of the row as they count down', () => {
    expect(declarationIn('.header-timer-readout', 'font-variant-numeric')).toBe('tabular-nums');
    expect(declarationIn('.header-timer-readout', 'white-space')).toBe('nowrap');
    expect(declarationIn('.header-timer-action', 'flex')).toBe('0 0 auto');
  });
});

describe('the panel looks like the rest of the note', () => {
  it('wears the menu popover rather than a second surface of its own', () => {
    const note = mount();
    // One chrome: the theme, the border, the shadow and the rounding are the
    // menu's, so there is no second copy to keep in step.
    expect(note.panel.element().classList.contains('note-menu')).toBe(true);
    expect(declarationIn('.note-menu', 'background-color')).toBe('var(--ui-surface)');
    expect(declarationIn('.note-menu', 'color')).toBe('var(--ui-text)');
  });

  it('takes its colours from the interface theme and never from the paper', () => {
    // A popover floats over a note of any colour, so borrowing the paper's own
    // foreground would make a dark popover unreadable on yellow paper.
    for (const selector of [
      '.note-timer-mode',
      '.note-timer-clock',
      '.note-timer-state',
      '.note-timer-preset',
      '.note-timer-input',
      '.note-timer-action',
    ]) {
      expect(ruleFor(selector).body).not.toMatch(/var\(--paper-/);
    }
    expect(declarationIn('.note-timer-clock', 'color')).toBe('var(--ui-text)');
  });

  it('is a compact panel rather than something that fills the note', () => {
    const width = Number.parseFloat(declarationIn('.note-timer', 'width'));
    expect(width).toBeLessThanOrEqual(inject('minNoteWidth'));
  });

  it('tells the states apart by more than colour', () => {
    // Running, paused and finished each change weight, slant or opacity as
    // well as ink, and the accessible name says the state in words.
    expect(ruleFor('#btn-timer[data-timer-state="paused"] .header-timer-readout').body).toContain(
      'font-style: italic',
    );
    expect(
      ruleFor('#btn-timer[data-timer-state="finished"] .header-timer-readout').body,
    ).toContain('font-weight: 700');
    expect(ruleFor('.note-timer[data-state="paused"] .note-timer-clock').body).toContain(
      'opacity',
    );
    expect(ruleFor('.note-timer-mark[data-state="current"]').body).toContain('border-width');
  });
});

describe('what the timer is not', () => {
  it('never asks the host to save the note', () => {
    // Every message this panel can produce is about operational state. It has
    // no route to `content_changed`, which is the only thing that moves a
    // note's Markdown or its modification date.
    const note = mount();
    note.panel.openPanel();
    click(primary());
    note.advance(MINUTE);
    click(primary());
    click(primary());
    note.advance(30 * MINUTE);
    note.panel.refresh();
    click(secondary());

    for (const snapshot of note.persisted) {
      if (snapshot === null) continue;
      expect(Object.keys(snapshot).sort()).toEqual([
        'deadlineMs',
        'focusCompleted',
        'mode',
        'phase',
        'remainingMs',
        'state',
        'timerMinutes',
      ]);
    }
  });

  it('puts nothing into the note, its title or its text', () => {
    const note = mount();
    note.panel.openPanel();
    click(primary());

    // The countdown lives in the header and in the popover, both of which sit
    // outside the editor. Nothing about it is in the document.
    const editor = document.getElementById('editor-container')!;
    expect(editor.textContent).toBe('');
    expect(editor.querySelector('#note-timer')).toBeNull();
    expect(document.getElementById('note-title')!.textContent).toBe('Nota sem título');
    expect(panelBody().closest('#editor-container')).toBeNull();
  });
});

describe('the redraw', () => {
  it('books exactly one at a time and none at all when nothing is running', () => {
    vi.useFakeTimers();
    try {
      const note = mount();
      const idle = vi.getTimerCount();

      note.panel.openPanel();
      click(primary());
      // One running timer, one pending redraw.
      expect(vi.getTimerCount()).toBe(idle + 1);

      // Fifty pauses and resumes: still one, never fifty-one.
      for (let round = 0; round < 50; round += 1) {
        click(primary());
        click(primary());
      }
      expect(vi.getTimerCount()).toBe(idle + 1);

      click(primary());
      expect(vi.getTimerCount()).toBe(idle);
      click(secondary());
      expect(vi.getTimerCount()).toBe(idle);
    } finally {
      vi.useRealTimers();
    }
  });

  it('leaves nothing behind when the panel is taken down', () => {
    vi.useFakeTimers();
    try {
      const note = mount();
      const idle = vi.getTimerCount();
      note.panel.openPanel();
      click(primary());
      expect(vi.getTimerCount()).toBe(idle + 1);

      note.panel.destroy();
      mounted = null;
      expect(vi.getTimerCount()).toBe(idle);
      // And the popover is gone with it, listeners and all.
      expect(document.getElementById('note-timer')).toBeNull();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    } finally {
      vi.useRealTimers();
    }
  });

  it('redraws about once a second and finishes on its own', () => {
    vi.useFakeTimers();
    try {
      // The panel is driven by real timers here, but the engine's clock is the
      // fake one, so both have to be advanced together — which is what the
      // application does with one clock.
      const note = mount();
      note.panel.openPanel();
      click(panelBody().querySelectorAll<HTMLButtonElement>('.note-timer-preset')[0]);
      click(primary());

      let redraws = 0;
      for (let second = 0; second < 5 * 60 + 5; second += 1) {
        note.advance(1_000);
        vi.advanceTimersByTime(1_000);
        redraws += 1;
      }

      expect(note.panel.currentView().state).toBe('finished');
      expect(note.finished).toEqual(['timer']);
      // Five minutes of running is around three hundred redraws, not thousands.
      expect(redraws).toBeLessThanOrEqual(310);
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
});
