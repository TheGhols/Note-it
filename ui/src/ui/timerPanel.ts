import { controlsFor } from '../timer/controls.ts';
import {
  delayToNextDisplayChange,
  MAX_TIMER_MINUTES,
  MIN_TIMER_MINUTES,
  TIMER_PRESET_MINUTES,
  TimerEngine,
  TimerFinishKind,
  TimerMode,
  TimerSnapshot,
  TimerView,
} from '../timer/engine.ts';
import { announcement, cycleLabel, formatRemaining, phaseLabel, stateLabel } from '../timer/format.ts';
import { FOCUS_SESSIONS_PER_CYCLE } from '../timer/engine.ts';

export interface TimerPanelHandlers {
  /**
   * A change worth keeping. Called for starts, pauses, resumes, cancels,
   * resets, phase changes and completions — and never for a tick, so a running
   * countdown costs no message and no write.
   */
  onPersist(snapshot: TimerSnapshot | null): void;
  /** A run reached zero, exactly once. */
  onFinished(kind: TimerFinishKind): void;
  /** Reported so the chrome can be held out while the panel is open. */
  onOpen?(): void;
  onClose?(): void;
}

export interface TimerPanelOptions {
  /** The header button that opens the panel and carries the readout. */
  trigger: HTMLElement;
  /** The span inside the button that shows the remaining time. */
  readout: HTMLElement;
  /** Element the popover is appended to, outside the drag region. */
  mount: HTMLElement;
  handlers: TimerPanelHandlers;
  /** Epoch milliseconds. `Date.now` in the application, a fake in the tests. */
  now?: () => number;
  document?: Document;
}

/**
 * The note's timer, as an interface.
 *
 * Three surfaces over one engine, which is why they are one object: the button
 * in the header, the readout on it, and the popover. They cannot disagree
 * because there is nothing for them to disagree about — each render asks the
 * same engine the same question at the same instant.
 *
 * It wears the menu's own popover chrome (`.note-menu`) rather than a second
 * look of its own, so it follows the interface theme, the paper and the
 * rounding without a second copy of any of that, and it closes the way every
 * other popover in the note closes: Escape, or a pointer somewhere else.
 *
 * **The countdown does not live in the panel.** The engine ticks whether or
 * not anything is open, because closing the popover must not stop the timer
 * and a collapsed note must still be able to finish one. What closing costs is
 * a redraw of the panel body, nothing more.
 */
export class TimerPanel {
  private readonly doc: Document;
  private readonly view: Window;
  private readonly root: HTMLElement;
  private readonly engine: TimerEngine;
  private readonly handlers: TimerPanelHandlers;
  private readonly trigger: HTMLElement;
  private readonly readout: HTMLElement;

  private readonly modeButtons = new Map<TimerMode, HTMLButtonElement>();
  private readonly presetButtons: HTMLButtonElement[] = [];
  private readonly clock: HTMLElement;
  private readonly phaseLine: HTMLElement;
  private readonly cycleRow: HTMLElement;
  private readonly cycleMarks: HTMLElement[] = [];
  private readonly stateLine: HTMLElement;
  private readonly durationBlock: HTMLElement;
  private readonly minutesInput: HTMLInputElement;
  private readonly hint: HTMLElement;
  private readonly primaryButton: HTMLButtonElement;
  private readonly secondaryButton: HTMLButtonElement;
  private readonly skipButton: HTMLButtonElement;

  private open = false;
  /** The single pending redraw. Cleared before every new one is set, so no
   *  sequence of starts, pauses and restores can leave two running. */
  private tickHandle: number | null = null;

  public constructor(options: TimerPanelOptions) {
    this.doc = options.document ?? options.trigger.ownerDocument;
    this.view = this.doc.defaultView!;
    this.handlers = options.handlers;
    this.trigger = options.trigger;
    this.readout = options.readout;

    this.engine = new TimerEngine({
      now: options.now ?? (() => Date.now()),
      onChange: (snapshot) => this.handlers.onPersist(snapshot),
      onFinish: (kind) => this.handlers.onFinished(kind),
    });

    this.root = this.doc.createElement('div');
    // The menu's popover chrome, worn rather than reimplemented.
    this.root.className = 'note-menu note-timer';
    this.root.id = 'note-timer';
    this.root.setAttribute('role', 'group');
    this.root.setAttribute('aria-label', 'Timer e Pomodoro');
    this.root.hidden = true;

    const modes = this.doc.createElement('div');
    modes.className = 'note-timer-modes';
    modes.setAttribute('role', 'group');
    modes.setAttribute('aria-label', 'Modo');
    for (const [mode, label] of [
      ['timer', 'Timer'],
      ['pomodoro', 'Pomodoro'],
    ] as const) {
      const button = this.doc.createElement('button');
      button.type = 'button';
      button.className = 'note-timer-mode';
      button.dataset.mode = mode;
      button.textContent = label;
      button.setAttribute('role', 'radio');
      button.setAttribute('aria-checked', 'false');
      button.addEventListener('click', () => this.run(() => this.engine.setMode(mode)));
      modes.append(button);
      this.modeButtons.set(mode, button);
    }

    this.phaseLine = this.doc.createElement('p');
    this.phaseLine.className = 'note-timer-phase';

    this.cycleRow = this.doc.createElement('div');
    this.cycleRow.className = 'note-timer-cycle';
    // The count is already spelled out in `phaseLine`; these are the same fact
    // drawn, so they are hidden from the accessibility tree rather than read
    // out as four bullets.
    this.cycleRow.setAttribute('aria-hidden', 'true');
    for (let index = 0; index < FOCUS_SESSIONS_PER_CYCLE; index += 1) {
      const mark = this.doc.createElement('span');
      mark.className = 'note-timer-mark';
      this.cycleRow.append(mark);
      this.cycleMarks.push(mark);
    }

    this.clock = this.doc.createElement('p');
    this.clock.className = 'note-timer-clock';

    this.stateLine = this.doc.createElement('p');
    this.stateLine.className = 'note-timer-state';
    // The state changes a handful of times a run; the digits change every
    // second. Announcing the state is useful, announcing the digits would be
    // a screen reader counting out loud, so only this line is live.
    this.stateLine.setAttribute('role', 'status');
    this.stateLine.setAttribute('aria-live', 'polite');

    this.durationBlock = this.doc.createElement('div');
    this.durationBlock.className = 'note-timer-duration';

    const presets = this.doc.createElement('div');
    presets.className = 'note-timer-presets';
    presets.setAttribute('role', 'group');
    presets.setAttribute('aria-label', 'Durações');
    for (const minutes of TIMER_PRESET_MINUTES) {
      const button = this.doc.createElement('button');
      button.type = 'button';
      button.className = 'note-timer-preset';
      button.dataset.minutes = String(minutes);
      button.textContent = String(minutes);
      button.setAttribute('role', 'radio');
      button.setAttribute('aria-checked', 'false');
      button.setAttribute('aria-label', `${minutes} minutos`);
      button.title = `${minutes} minutos`;
      button.addEventListener('click', () => this.chooseMinutes(minutes));
      presets.append(button);
      this.presetButtons.push(button);
    }

    const customRow = this.doc.createElement('div');
    customRow.className = 'note-timer-custom';
    const label = this.doc.createElement('label');
    label.className = 'note-timer-label';
    label.htmlFor = 'note-timer-minutes';
    label.textContent = 'Minutos';
    this.minutesInput = this.doc.createElement('input');
    this.minutesInput.id = 'note-timer-minutes';
    this.minutesInput.className = 'note-timer-input';
    this.minutesInput.type = 'number';
    this.minutesInput.inputMode = 'numeric';
    this.minutesInput.min = String(MIN_TIMER_MINUTES);
    this.minutesInput.max = String(MAX_TIMER_MINUTES);
    this.minutesInput.step = '1';
    this.minutesInput.setAttribute('aria-label', 'Duração em minutos');
    customRow.append(label, this.minutesInput);

    this.hint = this.doc.createElement('p');
    this.hint.className = 'note-timer-hint';
    this.hint.textContent = `Informe um número inteiro de ${MIN_TIMER_MINUTES} a ${MAX_TIMER_MINUTES} minutos.`;
    this.hint.hidden = true;

    this.durationBlock.append(presets, customRow, this.hint);

    const actions = this.doc.createElement('div');
    actions.className = 'note-timer-actions';
    this.primaryButton = this.action('note-timer-primary');
    this.secondaryButton = this.action('note-timer-secondary');
    this.skipButton = this.action('note-timer-skip');
    this.skipButton.textContent = 'Pular etapa';
    this.skipButton.setAttribute('aria-label', 'Pular esta etapa do Pomodoro');
    this.skipButton.addEventListener('click', () => this.run(() => this.engine.skip()));
    actions.append(this.primaryButton, this.secondaryButton, this.skipButton);

    this.root.append(
      modes,
      this.phaseLine,
      this.cycleRow,
      this.clock,
      this.stateLine,
      this.durationBlock,
      actions,
    );
    options.mount.append(this.root);

    this.primaryButton.addEventListener('click', () => this.activatePrimary());
    this.secondaryButton.addEventListener('click', () => this.activateSecondary());
    this.minutesInput.addEventListener('input', this.handleMinutesInput);
    this.minutesInput.addEventListener('keydown', this.handleMinutesKeyDown);

    // A pointerdown here must never be read as the start of a window drag, for
    // the same reason the menu stops one.
    this.root.addEventListener('pointerdown', (event) => event.stopPropagation());
    this.trigger.addEventListener('pointerdown', (event) => event.stopPropagation());
    this.trigger.setAttribute('aria-haspopup', 'true');
    this.trigger.setAttribute('aria-expanded', 'false');
    this.trigger.setAttribute('aria-controls', this.root.id);
    this.trigger.addEventListener('click', this.handleTriggerClick);

    this.doc.addEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.addEventListener('keydown', this.handleKeyDown);

    this.render();
  }

  public element(): HTMLElement {
    return this.root;
  }

  public isOpen(): boolean {
    return this.open;
  }

  /** The engine's state resolved against the clock. For tests and the header. */
  public currentView(): TimerView {
    return this.engine.view();
  }

  public snapshot(): TimerSnapshot | null {
    return this.engine.persisted();
  }

  /**
   * Puts a stored timer back when the note loads.
   *
   * Nothing is persisted from here: this is the note being told what it
   * already had, so writing it back would be a write for every note that
   * opens.
   */
  public restore(value: unknown): void {
    this.engine.restore(value);
    this.render();
  }

  public openPanel(): void {
    if (this.open) {
      this.render();
      return;
    }
    this.open = true;
    this.root.hidden = false;
    this.trigger.setAttribute('aria-expanded', 'true');
    this.handlers.onOpen?.();
    this.render();
    this.focusPrimary();
  }

  public close(): void {
    if (!this.open) return;
    this.open = false;
    this.root.hidden = true;
    this.trigger.setAttribute('aria-expanded', 'false');
    this.hint.hidden = true;
    this.handlers.onClose?.();
    // The countdown carries on; only the panel went away.
    this.render();
  }

  public toggle(): void {
    if (this.open) {
      this.close();
    } else {
      this.openPanel();
    }
  }

  /**
   * Takes down every listener and the pending redraw.
   *
   * The redraw matters most: a timeout still holding this object after the
   * page has moved on is the orphan that keeps a destroyed panel ticking.
   */
  public destroy(): void {
    this.clearTick();
    this.doc.removeEventListener('pointerdown', this.handleDocumentPointerDown, true);
    this.doc.removeEventListener('keydown', this.handleKeyDown);
    this.trigger.removeEventListener('click', this.handleTriggerClick);
    this.root.remove();
  }

  /** Re-reads the clock and redraws. Called by the tick and by every action. */
  public refresh(): void {
    this.engine.tick();
    this.render();
  }

  private action(className: string): HTMLButtonElement {
    const button = this.doc.createElement('button');
    button.type = 'button';
    button.className = `note-timer-action ${className}`;
    return button;
  }

  private run(mutate: () => void): void {
    mutate();
    this.render();
  }

  private chooseMinutes(minutes: number): void {
    if (this.engine.setMinutes(minutes)) {
      this.minutesInput.value = String(minutes);
      this.hint.hidden = true;
    }
    this.render();
  }

  private activatePrimary(): void {
    const controls = controlsFor(this.engine.view());
    switch (controls.primary.action) {
      case 'start':
        // Whatever is in the field is what starts, so a reader who typed a
        // duration and pressed the button never gets the previous one.
        if (controls.duration && !this.commitMinutes()) return;
        this.engine.start();
        break;
      case 'pause':
        this.engine.pause();
        break;
      case 'resume':
        this.engine.resume();
        break;
      case 'advance':
        this.engine.advance();
        break;
    }
    this.render();
  }

  private activateSecondary(): void {
    const controls = controlsFor(this.engine.view());
    if (!controls.secondary) return;
    if (controls.secondary.action === 'cancel') {
      this.engine.cancel();
    } else {
      this.engine.reset();
    }
    this.render();
  }

  /**
   * Reads the minutes field, and refuses rather than guesses.
   *
   * An empty field, a fraction, a negative or something past the ceiling stops
   * the start and says so. Nothing is rounded into range: a timer that quietly
   * ran for a duration nobody chose is worse than one that declined to start.
   */
  private commitMinutes(): boolean {
    const raw = this.minutesInput.value;
    // A `type="number"` field holds nothing at all when what was typed is not
    // a number, so an empty value means one of two different things. The
    // browser knows which, and says so: `badInput` separates "nothing typed"
    // from "letters typed", and only the first is an instruction to use what
    // is already chosen.
    if (raw.trim() === '' && !this.minutesInput.validity?.badInput) {
      this.minutesInput.value = String(this.engine.view().timerMinutes);
      this.hint.hidden = true;
      this.minutesInput.removeAttribute('aria-invalid');
      return true;
    }
    if (!this.engine.setMinutes(raw)) {
      this.hint.hidden = false;
      this.minutesInput.setAttribute('aria-invalid', 'true');
      this.minutesInput.focus();
      return false;
    }
    this.hint.hidden = true;
    this.minutesInput.removeAttribute('aria-invalid');
    return true;
  }

  private render(): void {
    const view = this.engine.view();
    const controls = controlsFor(view);
    const clock = formatRemaining(view.remainingMs);

    // The header, which is drawn whether or not the panel is open.
    this.readout.textContent = view.active ? clock : '';
    this.readout.hidden = !view.active;
    this.trigger.dataset.timerState = view.state;
    this.trigger.setAttribute(
      'aria-label',
      view.active ? announcement(view) : 'Timer e Pomodoro',
    );
    this.trigger.title = view.active ? announcement(view) : 'Timer e Pomodoro';
    // Read by the stylesheet, which is what decides whether a collapsed note
    // keeps the control on show.
    this.doc.body.setAttribute('data-timer', view.state);

    for (const [mode, button] of this.modeButtons) {
      button.setAttribute('aria-checked', String(view.mode === mode));
      button.disabled = !controls.modes;
    }

    const pomodoro = view.mode === 'pomodoro';
    this.phaseLine.hidden = !pomodoro;
    this.cycleRow.hidden = !pomodoro;
    if (pomodoro) {
      this.phaseLine.textContent = `${phaseLabel(view.phase)} · ${cycleLabel(
        view.phase,
        view.focusCompleted,
      )}`;
      this.cycleMarks.forEach((mark, index) => {
        const done = index < view.focusCompleted;
        const current = !done && index === view.focusCompleted && view.phase === 'focus';
        mark.dataset.state = done ? 'done' : current ? 'current' : 'pending';
      });
    }

    this.clock.textContent = clock;
    this.stateLine.textContent = stateLabel(view.state);
    this.root.dataset.state = view.state;
    this.root.dataset.mode = view.mode;

    this.durationBlock.hidden = !controls.duration;
    if (controls.duration && this.doc.activeElement !== this.minutesInput) {
      this.minutesInput.value = String(view.timerMinutes);
      this.minutesInput.removeAttribute('aria-invalid');
    }
    for (const button of this.presetButtons) {
      button.setAttribute(
        'aria-checked',
        String(Number(button.dataset.minutes) === view.timerMinutes),
      );
    }

    this.primaryButton.textContent = controls.primary.label;
    this.primaryButton.setAttribute('aria-label', controls.primary.label);
    this.primaryButton.dataset.action = controls.primary.action;
    this.secondaryButton.hidden = controls.secondary === null;
    if (controls.secondary) {
      this.secondaryButton.textContent = controls.secondary.label;
      this.secondaryButton.setAttribute('aria-label', controls.secondary.label);
      this.secondaryButton.dataset.action = controls.secondary.action;
    }
    this.skipButton.hidden = !controls.skip;

    this.schedule();
  }

  /**
   * Books the next redraw, and only the next one.
   *
   * The delay is the time until the *displayed* second changes, not a flat
   * second from now, so the clock never sits on a stale number and never
   * redraws twice for the same one. At most one redraw a second, and none at
   * all unless something is running — a paused or finished timer costs
   * nothing, and neither does a note that never opened this.
   *
   * Accuracy does not come from here. It comes from the deadline: a redraw
   * that arrives late, or not at all because the WebView was throttled, still
   * reads the correct remainder when it happens.
   */
  private schedule(): void {
    this.clearTick();
    const view = this.engine.view();
    if (view.state !== 'running') return;
    this.tickHandle = this.view.setTimeout(
      this.handleTick,
      delayToNextDisplayChange(view.remainingMs),
    ) as unknown as number;
  }

  private clearTick(): void {
    if (this.tickHandle !== null) {
      this.view.clearTimeout(this.tickHandle);
      this.tickHandle = null;
    }
  }

  private focusPrimary(): void {
    this.primaryButton.focus();
  }

  private readonly handleTick = (): void => {
    this.tickHandle = null;
    this.engine.tick();
    this.render();
  };

  private readonly handleTriggerClick = (event: Event): void => {
    event.preventDefault();
    event.stopPropagation();
    this.toggle();
  };

  private readonly handleMinutesInput = (): void => {
    this.hint.hidden = true;
    this.minutesInput.removeAttribute('aria-invalid');
  };

  private readonly handleMinutesKeyDown = (event: KeyboardEvent): void => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    this.activatePrimary();
  };

  private readonly handleDocumentPointerDown = (event: Event): void => {
    if (!this.open) return;
    const target = event.target as Node | null;
    if (target && (this.root.contains(target) || this.trigger.contains(target))) return;
    this.close();
  };

  private readonly handleKeyDown = (event: KeyboardEvent): void => {
    if (!this.open || event.key !== 'Escape') return;
    event.preventDefault();
    this.close();
    this.trigger.focus?.();
  };
}
