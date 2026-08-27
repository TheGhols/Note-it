import { CodeBlockLowlight } from '@tiptap/extension-code-block-lowlight';
import { createLowlight } from 'lowlight';
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';
import css from 'highlight.js/lib/languages/css';
import ini from 'highlight.js/lib/languages/ini';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';
import plaintext from 'highlight.js/lib/languages/plaintext';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import shell from 'highlight.js/lib/languages/shell';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';

/**
 * The languages a note can highlight, in the order the menu offers them.
 *
 * `id` is what gets written into the Markdown fence when the language is
 * chosen from the menu. Each is a name or an alias `highlight.js` already
 * knows, so `toml` reaches the `ini` grammar and `html` reaches `xml` without
 * a translation table of our own.
 */
export const CODE_LANGUAGES = [
  { id: 'plaintext', label: 'Texto simples' },
  { id: 'bash', label: 'Bash / Shell' },
  { id: 'javascript', label: 'JavaScript' },
  { id: 'typescript', label: 'TypeScript' },
  { id: 'json', label: 'JSON' },
  { id: 'html', label: 'HTML / XML' },
  { id: 'css', label: 'CSS' },
  { id: 'markdown', label: 'Markdown' },
  { id: 'python', label: 'Python' },
  { id: 'rust', label: 'Rust' },
  { id: 'c', label: 'C' },
  { id: 'cpp', label: 'C++' },
  { id: 'java', label: 'Java' },
  { id: 'sql', label: 'SQL' },
  { id: 'yaml', label: 'YAML' },
  { id: 'toml', label: 'TOML' },
] as const;

export type CodeLanguage = (typeof CODE_LANGUAGES)[number]['id'];

/**
 * Only the grammars above are loaded. `highlight.js` ships well over a hundred
 * and importing the bundle would cost more than the whole editor; each module
 * here is a few kilobytes, and registering one brings its aliases with it, so
 * `js`, `py`, `sh` and `cpp` resolve without being listed separately.
 */
const GRAMMARS = {
  bash,
  c,
  cpp,
  css,
  ini,
  java,
  javascript,
  json,
  markdown,
  plaintext,
  python,
  rust,
  shell,
  sql,
  typescript,
  xml,
  yaml,
};

const lowlight = createLowlight(GRAMMARS);

/**
 * Every name and alias each grammar answers to, read from the grammar itself.
 *
 * `highlight.js` knows that `js` is JavaScript and `toml` is the `ini` parser,
 * but neither it nor `lowlight` exposes a lookup for it. Asking each grammar
 * for its own alias list is the one source that cannot drift: a table written
 * here by hand would go stale the first time upstream added an alias.
 */
const ALIASES: ReadonlyArray<ReadonlySet<string>> = Object.entries(GRAMMARS).map(
  ([name, grammar]) => {
    const definition = grammar(hljs as never) as { aliases?: string[] };
    return new Set<string>([name, ...(definition.aliases ?? [])]);
  },
);

/** Whether a fence's language — name or alias — has a grammar behind it. */
export function canHighlight(language: unknown): boolean {
  return typeof language === 'string' && language !== '' && lowlight.registered(language);
}

/**
 * The menu label for a stored language identifier.
 *
 * A note may carry any identifier at all: one this version does not offer, an
 * alias, or something written by hand. An alias is shown under the language it
 * belongs to; anything else is shown as it was written, because the note keeps
 * it either way and pretending otherwise would misreport the file.
 */
export function codeLanguageLabel(language: unknown): string {
  if (typeof language !== 'string' || language === '') return 'Sem linguagem';

  const exact = CODE_LANGUAGES.find((entry) => entry.id === language);
  if (exact) return exact.label;

  const grammar = ALIASES.find((names) => names.has(language));
  const aliased = grammar && CODE_LANGUAGES.find((entry) => grammar.has(entry.id));
  return aliased ? aliased.label : language;
}

/**
 * A fence long enough to hold the code inside it.
 *
 * Three backticks are the usual opening, but a block whose own content holds a
 * run of three would be closed by it — a note containing a Markdown example
 * would lose everything after that line on the next save. The fence is one
 * backtick longer than the longest run it has to contain.
 */
export function fenceFor(code: string): string {
  const longest = (code.match(/`+/g) ?? []).reduce((most, run) => Math.max(most, run.length), 0);
  return '`'.repeat(Math.max(3, longest + 1));
}

/**
 * A `lowlight` that never guesses.
 *
 * The upstream plugin falls back to `highlightAuto` whenever a block has no
 * language or carries one it cannot resolve. Both are cases where the honest
 * answer is "this is not highlighted": a fence written without a language is
 * plain text on purpose, and colouring an unrecognised one as whatever it most
 * resembles would tell the reader something the note does not say. Returning
 * nothing leaves the code exactly as typed.
 */
const neverGuess = {
  highlight: (language: string, value: string) => lowlight.highlight(language, value),
  highlightAuto: () => ({ type: 'root' as const, children: [] }),
  listLanguages: () => lowlight.listLanguages(),
  registered: (aliasOrName: string) => lowlight.registered(aliasOrName),
};

/**
 * Fenced code blocks, highlighted for presentation only.
 *
 * The stored Markdown is a plain fence — the highlighting is ProseMirror
 * decorations over the same text, so nothing about it reaches the file. The
 * language identifier is carried through untouched in both directions: an
 * unknown one keeps its spelling and simply goes unhighlighted, and a fence
 * with no language stays without one rather than being given a default.
 *
 * Everything inside is literal. The node declares itself as code, so the
 * arrow substitution stands aside, inline marks do not apply, and `<script>`
 * or `&` typed in a block are text like any other character.
 */
export const NoteItCodeBlock = CodeBlockLowlight.extend({
  renderMarkdown(node: any, helpers: any) {
    const language = node.attrs?.language || '';
    if (!node.content) return '```' + language + '\n\n```';
    const code = helpers.renderChildren(node.content);
    const fence = fenceFor(code);
    return [`${fence}${language}`, code, fence].join('\n');
  },
}).configure({
  lowlight: neverGuess,
  defaultLanguage: null,
});
