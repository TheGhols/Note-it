/**
 * Every shortcut Note-it actually has, in one place.
 *
 * Before this module the same chords were written out in `keyboard.ts`, in
 * `index.html` titles, in `docs/niri.md` and in the README, and they had begun
 * to disagree — the collapse chord in particular, which is `Ctrl+Shift+M` for
 * *this* note locally and `Mod+Shift+M` for *every* note through the
 * compositor. A panel built from four sources would have shipped the
 * disagreement; this is the one it is built from.
 *
 * The distinction the list exists to make is scope, because it is the one a
 * reader cannot see and the one that explains why a shortcut "does not work":
 *
 * - **local** — a key event in this note's WebView. It needs the note to have
 *   keyboard focus, and a note on the `bottom` layer is behind every window,
 *   so once focus has gone elsewhere there is nothing left to press.
 * - **global** — a compositor binding. It works from any application, and it
 *   exists only if the reader has put it in their Niri configuration, which is
 *   why each one carries the command it runs.
 * - **command** — a shell command, and what a global binding actually spawns.
 *
 * Nothing here may be aspirational. Every local entry is handled in
 * `editor/keyboard.ts`, every command is a subcommand of `note-it`, and every
 * global entry is the configuration `docs/niri.md` recommends rather than a
 * promise the application can keep by itself.
 */

export type ShortcutScope = 'local' | 'global' | 'command';

export interface Shortcut {
  /** The chord, or the command, as a reader would type it. */
  readonly keys: string;
  /** What it does, in one line. */
  readonly description: string;
  /** Only for a global binding: what the compositor is asked to run. */
  readonly runs?: string;
}

export interface ShortcutGroup {
  readonly scope: ShortcutScope;
  readonly title: string;
  /** Why this group behaves the way it does. Shown above the rows. */
  readonly note: string;
  readonly shortcuts: readonly Shortcut[];
}

/**
 * The local chords, in the order `editor/keyboard.ts` handles them.
 *
 * `Ctrl+Shift+Space` is here as well as in the global group on purpose: the
 * local one is a fallback that works only with focus, and saying so is the
 * whole reason the panel exists.
 */
const LOCAL: readonly Shortcut[] = [
  { keys: 'Ctrl+N', description: 'Nova nota' },
  { keys: 'Ctrl+W', description: 'Fechar esta nota' },
  { keys: 'Ctrl+K', description: 'Buscar em todas as notas' },
  { keys: 'Ctrl+F', description: 'Localizar nesta nota' },
  { keys: 'Ctrl+H', description: 'Localizar e substituir' },
  { keys: 'Ctrl+R', description: 'Riscado' },
  { keys: 'Ctrl+=', description: 'Aumentar o zoom' },
  { keys: 'Ctrl+-', description: 'Diminuir o zoom' },
  { keys: 'Ctrl+0', description: 'Zoom padrão' },
  {
    keys: 'Ctrl+Shift+M',
    description: 'Recolher ou expandir esta nota',
  },
  {
    keys: 'Ctrl+Shift+Space',
    description:
      'Alternar Área de trabalho ↔ Sobreposição. Substituto local: só funciona com a nota em foco',
  },
  { keys: 'Ctrl+Shift+>', description: 'Aumentar o tamanho do texto' },
  { keys: 'Ctrl+Shift+<', description: 'Diminuir o tamanho do texto' },
  { keys: 'Esc', description: 'Fechar o painel aberto' },
];

/**
 * The compositor bindings `docs/niri.md` recommends.
 *
 * These are configuration, not behaviour Note-it can provide, so the panel says
 * so in the group's note rather than presenting them as features.
 */
const GLOBAL: readonly Shortcut[] = [
  {
    keys: 'Ctrl+Shift+Space',
    description: 'Alternar Área de trabalho ↔ Sobreposição, de qualquer aplicativo',
    runs: 'gapplication action io.github.theghols.NoteIt toggle-layer',
  },
  {
    keys: 'Mod+Shift+N',
    description: 'Convocar as notas: restaura e traz para a frente',
    runs: 'note-it',
  },
  {
    keys: 'Mod+Shift+M',
    description: 'Recolher ou expandir TODAS as notas',
    runs: 'note-it toggle-collapse-all',
  },
  {
    keys: 'Mod+Alt+N',
    description: 'Criar uma nota rapidamente',
    runs: 'note-it new',
  },
];

/** The subcommands `note-it --help` lists. */
const COMMANDS: readonly Shortcut[] = [
  { keys: 'note-it', description: 'Convoca as notas já abertas' },
  { keys: 'note-it new', description: 'Cria uma nota' },
  { keys: 'note-it toggle', description: 'Alterna Área de trabalho ↔ Sobreposição' },
  { keys: 'note-it show', description: 'Traz todas as notas para a sobreposição' },
  { keys: 'note-it hide', description: 'Esconde todas as notas' },
  { keys: 'note-it toggle-collapse-all', description: 'Recolhe ou expande todas as notas' },
  { keys: 'note-it quit', description: 'Salva tudo e encerra o Note-it' },
];

export const SHORTCUT_GROUPS: readonly ShortcutGroup[] = [
  {
    scope: 'local',
    title: 'Atalhos locais',
    note: 'Funcionam com esta nota em foco.',
    shortcuts: LOCAL,
  },
  {
    scope: 'global',
    title: 'Atalhos globais (Niri)',
    note:
      'Funcionam a partir de qualquer aplicativo e dependem da sua configuração do ' +
      'compositor. Se um deles não responder, ele ainda não está no seu ~/.config/niri.',
    shortcuts: GLOBAL,
  },
  {
    scope: 'command',
    title: 'Comandos',
    note: 'O que os atalhos globais executam. Também servem no terminal.',
    shortcuts: COMMANDS,
  },
];
