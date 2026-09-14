import { afterEach, describe, expect, inject, it } from 'vitest';
import { SHORTCUT_GROUPS } from '../src/ui/shortcuts.ts';
import { ShortcutsPanel } from '../src/ui/shortcutsPanel.ts';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { declarationIn } from './support/stylesheet.ts';

/** A file of the repository, read the way the conformance tests read theirs. */
function repoFile(relative: string): string {
  return readFileSync(resolve(process.cwd(), relative), 'utf8');
}

afterEach(() => document.body.replaceChildren());

function renderedPage(): Document {
  return new DOMParser().parseFromString(
    inject('renderedHtml').replace(/<script[\s\S]*?<\/script>/g, ''),
    'text/html',
  );
}

/** A trigger and a mount, wired the way `main.ts` wires them. */
function mounted(): { panel: ShortcutsPanel; trigger: HTMLButtonElement } {
  const trigger = document.createElement('button');
  const mount = document.createElement('div');
  document.body.append(trigger, mount);
  const panel = new ShortcutsPanel({ trigger, mount });
  return { panel, trigger };
}

describe('the shortcut reference button', () => {
  it('ships in the header with an accessible name and an icon', () => {
    const page = renderedPage();
    const button = page.getElementById('btn-shortcuts');
    expect(button).not.toBeNull();
    expect(button?.getAttribute('aria-label')).toBe('Atalhos do Note-it');
    expect(button?.getAttribute('title')).toContain('Atalhos');
    expect(button?.querySelector('svg')).not.toBeNull();
    // Beside the other tools, not in the destructive corner: the reference is
    // not something a misfire should be able to turn into a deleted note.
    expect(button?.closest('.header-view-group')).not.toBeNull();
    expect(button?.closest('.note-controls-right')).toBeNull();
  });

  it('does not disturb the drag region or the trash and close pair', () => {
    const page = renderedPage();
    const right = page.querySelector('.note-controls-right');
    expect(Array.from(right!.querySelectorAll('button'), (button) => button.id)).toEqual([
      'btn-trash-note',
      'btn-close',
    ]);
    expect(page.getElementById('btn-shortcuts')?.closest('.drag-region')).toBeNull();
  });
});

describe('the shortcut reference panel', () => {
  it('opens on a click, closes on Escape and returns the keyboard to the trigger', () => {
    const { panel, trigger } = mounted();
    expect(panel.isOpen()).toBe(false);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');

    trigger.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(panel.isOpen()).toBe(true);
    expect(panel.element().hidden).toBe(false);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(panel.isOpen()).toBe(false);
    expect(panel.element().hidden).toBe(true);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(document.activeElement).toBe(trigger);
  });

  it('closes when the pointer goes down outside it, and not when it goes down inside', () => {
    const { panel, trigger } = mounted();
    panel.openPanel();

    panel
      .element()
      .dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }));
    expect(panel.isOpen()).toBe(true);

    document.body.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }));
    expect(panel.isOpen()).toBe(false);

    // And a pointerdown on it never reaches the header as the start of a drag.
    panel.openPanel();
    const event = new MouseEvent('pointerdown', { bubbles: true, cancelable: true });
    let reachedBody = false;
    document.body.addEventListener('pointerdown', () => (reachedBody = true));
    panel.element().dispatchEvent(event);
    expect(reachedBody).toBe(false);
    void trigger;
  });

  it('separates local shortcuts from the compositor ones, and says why', () => {
    const { panel } = mounted();
    const groups = panel.element().querySelectorAll('.note-shortcuts-group');
    expect(Array.from(groups, (group) => (group as HTMLElement).dataset.scope)).toEqual([
      'local',
      'global',
      'command',
    ]);

    const local = panel.element().querySelector('[data-scope="local"]')!;
    const global = panel.element().querySelector('[data-scope="global"]')!;
    expect(local.querySelector('.note-shortcuts-group-note')?.textContent).toContain('foco');
    expect(global.querySelector('.note-shortcuts-group-note')?.textContent).toContain(
      'qualquer aplicativo',
    );
    expect(global.querySelector('.note-shortcuts-group-note')?.textContent).toContain('niri');
  });

  it('explains Ctrl+Shift+Space in both scopes, because that is the confusing one', () => {
    const { panel } = mounted();
    const text = (scope: string): string =>
      panel.element().querySelector(`[data-scope="${scope}"]`)!.textContent ?? '';

    expect(text('local')).toContain('Ctrl+Shift+Space');
    expect(text('local')).toContain('Substituto local');
    expect(text('global')).toContain('Ctrl+Shift+Space');
    expect(text('global')).toContain(
      'gapplication action io.github.theghols.NoteIt toggle-layer',
    );
  });

  it('tells this note’s collapse chord apart from the one for every note', () => {
    const { panel } = mounted();
    const local = panel.element().querySelector('[data-scope="local"]')!.textContent ?? '';
    const global = panel.element().querySelector('[data-scope="global"]')!.textContent ?? '';

    expect(local).toContain('Ctrl+Shift+M');
    expect(local).toContain('esta nota');
    expect(global).toContain('Mod+Shift+M');
    expect(global).toContain('TODAS');
    expect(global).toContain('note-it toggle-collapse-all');
  });

  it('writes every label as text, so a note can never inject markup into it', () => {
    const { panel } = mounted();
    expect(panel.element().querySelector('script')).toBeNull();
    for (const keys of panel.element().querySelectorAll('.note-shortcuts-keys')) {
      expect(keys.children.length).toBe(0);
    }
  });

  it('is torn down completely, listeners included', () => {
    const { panel, trigger } = mounted();
    panel.openPanel();
    panel.destroy();
    expect(panel.element().isConnected).toBe(false);
    // A destroyed panel must not answer the document any more.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    trigger.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(panel.element().isConnected).toBe(false);
  });
});

describe('the shortcut list itself', () => {
  it('invents nothing: every local chord is one the keyboard controller handles', () => {
    const source = repoFile('src/editor/keyboard.ts');
    const handled = new Set<string>();
    // The plain `Ctrl+<key>` arm, written as `case 'n':`.
    for (const match of source.matchAll(/case '([a-z0-9+\-=_])':/g)) {
      handled.add(match[1]);
    }
    const local = SHORTCUT_GROUPS.find((group) => group.scope === 'local')!;
    for (const shortcut of local.shortcuts) {
      const plain = /^Ctrl\+([A-Za-z0-9=\-])$/.exec(shortcut.keys);
      if (!plain) continue;
      expect(
        handled.has(plain[1].toLowerCase()),
        `${shortcut.keys} is listed but not handled in keyboard.ts`,
      ).toBe(true);
    }
    // And the four `Ctrl+Shift` chords are the four the chord handler has.
    expect(source).toContain("key === 'M'");
    expect(source).toContain("code === 'Space'");
    expect(source).toContain("code === 'Period'");
    expect(source).toContain("code === 'Comma'");
  });

  it('lists only real note-it subcommands', () => {
    const source = repoFile('../src/cli.rs');
    const commands = SHORTCUT_GROUPS.find((group) => group.scope === 'command')!;
    for (const shortcut of commands.shortcuts) {
      const argument = shortcut.keys.replace(/^note-it\s*/, '');
      if (!argument) continue;
      // `toggle-collapse-all` is spelled `ToggleCollapseAll` in the enum.
      const variant = argument
        .split('-')
        .map((part) => part[0].toUpperCase() + part.slice(1))
        .join('');
      expect(source, `${shortcut.keys} is not a note-it subcommand`).toContain(variant);
    }
  });

  it('matches the compositor bindings the documentation recommends', () => {
    const source = repoFile('../docs/niri.md');
    const global = SHORTCUT_GROUPS.find((group) => group.scope === 'global')!;
    for (const shortcut of global.shortcuts) {
      expect(source, `${shortcut.keys} is not in docs/niri.md`).toContain(shortcut.keys);
    }
  });
});

describe('the panel stylesheet', () => {
  it('is the application’s own furniture, so a note’s zoom leaves it alone', () => {
    const size = declarationIn('.note-shortcuts', 'font-size');
    expect(size).toContain('--ui-scale');
    expect(size).not.toContain('em');
  });

  it('scrolls instead of growing past the note', () => {
    expect(declarationIn('.note-shortcuts', 'max-height')).toContain('100%');
    expect(declarationIn('.note-shortcuts', 'overflow-y')).toBe('auto');
  });

  // The rule above passed for the whole time the panel was unusable, because
  // `100%` is only a promise about the containing block and this file never
  // said which one. It was `#note-controls-left`: a 26px row of header
  // buttons, so `calc(100% - var(--note-header-height) - 16px)` computed
  // negative, clamped to zero, and 1700px of reference opened as a 20px
  // sliver. Measured in WebKitGTK at 220x300, 420x360 and 760x560, the panel
  // showed 0.9%, 1.2% and 1.1% of its content. The geometry is written
  // against the note, so the mount has to be the note.
  it('is mounted on the note, which is what its percentages measure', () => {
    const source = repoFile('src/main.ts');
    const construction = /shortcutsPanel = new ShortcutsPanel\(\{[\s\S]*?\n {6}\}\)/.exec(source);
    expect(construction).not.toBeNull();
    expect(construction![0]).toContain('mount: appRoot');
    expect(construction![0]).not.toContain('mount: menuMount');

    // Both sides pinned with no width of its own, so the width is the
    // containing block's; and a max-height in percent, so the height is too.
    // That is why a button-sized containing block broke both at once.
    expect(declarationIn('.note-shortcuts', 'position')).toBe('absolute');
    expect(declarationIn('.note-shortcuts', 'left')).toBeTruthy();
    expect(declarationIn('.note-shortcuts', 'right')).toBeTruthy();
    expect(() => declarationIn('.note-shortcuts', 'width')).toThrow();
    expect(declarationIn('.note-shortcuts', 'max-height')).toContain('100%');
  });

  it('shares the containing block of the panels it was shaped after', () => {
    const source = repoFile('src/main.ts');
    // `.note-shortcuts` is a copy of `.note-trash`'s geometry, so a change
    // that moves one and not the other is the defect coming back.
    for (const panel of ['TrashPanel', 'SearchPalette', 'ShortcutsPanel']) {
      const construction = new RegExp(`new ${panel}\\(\\{[\\s\\S]*?mount: (\\w+)`).exec(source);
      expect(construction, `${panel} is constructed`).not.toBeNull();
      expect(construction![1], `${panel} mounts on the note`).toBe('appRoot');
    }
  });

  it('hides when hidden, like every other panel', () => {
    expect(declarationIn('.note-shortcuts[hidden]', 'display')).toBe('none');
  });
});
