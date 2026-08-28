# Markdown Format Specification

Each Note-it post-it is stored as a valid, human-readable Markdown (`.md`) file named using a UUID (e.g. `550e8400-e29b-41d4-a716-446655440000.md`).

## File Structure

```md
---
note_it:
  version: 1
  id: "550e8400-e29b-41d4-a716-446655440000"
  color: "yellow"
  paper_type: "lined"
  paper_intensity: "subtle"
  font_size: 16
  created_at: "2026-08-26T14:00:00Z"
  updated_at: "2026-08-26T14:05:00Z"
---

# Meeting Notes

- [ ] Complete project setup
- [x] Create documentation

Remember to check <u>underlined points</u> and <span data-note-it-color="#D32F2F" style="color:#D32F2F">urgent tasks</span>.

## Trailing Blank Lines Are Not Content

A stored note ends with a single newline, the way every other tool writes a
file. That terminator is not part of the note, and neither is any blank line
before it: Markdown gives trailing blank lines no meaning, and Note-it's own
editor terminates a document ending in a block — a list, a callout, a code
block — with one, while a document ending in a paragraph gets none.

So the same note has several equally valid spellings. Note-it compares and
stores one canonical form, with trailing newlines removed, and writes the
terminator back on save. This is what makes opening a note a read: a `.md`
written by another editor, or any note ending in a list, is not rewritten and
does not have its `updated_at` moved by being opened.

Trailing **spaces** are left alone — two of them are Markdown's hard line break
and are content.

## Block Syntax

Everything Note-it writes is ordinary Markdown. Nothing below is a private
extension of the file format: another editor opens a note and sees code fences,
blockquotes and HTML comments, and GitHub renders a callout as an alert.

### Fenced Code Blocks

````md
```python
def soma(a, b):
    return a + b
```
````

The language identifier is carried through in both directions **exactly as
written**. It is never rewritten, normalised or dropped:

- a fence with no language stays without one, and is not given a default;
- a language with no grammar available — `brainfuck`, a typo, something newer
  than this version — keeps its spelling and simply goes unhighlighted;
- an alias stays an alias. A note saying ` ```js ` is still ` ```js ` after a
  save, even though it is highlighted as JavaScript.

The content is literal. Nothing inside is interpreted: no inline formatting, no
typographic substitution, and no HTML — `<script>` inside a block is the five
characters `<`, `s`, `c`… and reaches the document as text.

The closing fence is always longer than the longest run of backticks inside the
block, so a note containing a Markdown example is written back whole.

Highlighting is **presentation only**. It is drawn as editor decorations over
the same characters; the stored file is a plain fence with no markup in it.
Sixteen grammars are loaded — `plaintext`, `bash`, `javascript`, `typescript`,
`json`, `html`/`xml`, `css`, `markdown`, `python`, `rust`, `c`, `cpp`, `java`,
`sql`, `yaml` and `toml` — with the aliases each of them already answers to.

### Callouts

The syntax is GitHub's alerts, which Obsidian reads as callouts:

```md
> [!NOTE]
> Um parágrafo.
>
> - e uma lista, se quiser
```

`NOTE`, `TIP`, `IMPORTANT`, `WARNING` and `CAUTION` are recognised, in any case;
the canonical uppercase form is what gets written back. A callout is a
blockquote carrying a kind, so it holds whatever a blockquote holds —
paragraphs, lists, nested blocks.

The marker must sit alone on the first line. Anything else is not a callout and
is **left as the blockquote it already is**, with its text intact:

| Written | Read as |
| --- | --- |
| `> [!NOTE]` + body | a NOTE callout |
| `> [!FOO]` + body | a blockquote whose first line is `[!FOO]` |
| `> [!NOTE] com texto` | a blockquote, marker and all |
| `> [!NOTE` | a blockquote, marker and all |

Degrading never costs content. A literal `[` is escaped as `\[` on the way back
out, which is how Markdown writes one, and the result is stable from then on.

### Blockquotes

An ordinary blockquote stays an ordinary blockquote:

```md
> uma citação
```

It is never promoted to a callout on its own, and it is written back without
decoration of any kind — no attributes, no classes, no HTML.

### Calculations

A line beginning with `=` is a calculation and a line of the form `nome := …` is
a declaration. Both are **ordinary Markdown text**, and that is the whole point:

```md
preco := 120
quantidade := 3
= preco * quantidade
```

Another editor opens this and sees three lines of prose, because that is what
they are. Note-it draws `360` beside the third one as an editor decoration —
the same mechanism syntax highlighting uses — and **writes nothing**. No result,
no marker, no attribute reaches the file, so a note is never rewritten by being
recalculated and its modification date never moves for one.

The full grammar is in `docs/features.md`. What matters to the file format:

- calculation is read from **plain paragraphs only**. A heading, a list, a task,
  a quote, a callout, a code block, a comment and an inline code span are all
  left as the text they are;
- `*` in a calculation is escaped as `\*` on the way out, which is how Markdown
  writes a literal asterisk in prose, and reads back as `*`. This is the
  serializer's existing rule for any prose, not something calculations
  introduced;
- results are recomputed on load. A note whose expressions have not changed is
  byte-identical after being opened, recalculated and closed.

A conversion is the same thing with a unit on each side:

```md
distancia := 10
= distancia km em m
```

`em` is the conversion keyword. The file holds those two lines and nothing else;
`10000 m` is drawn beside the second one and never written. The units are
ordinary words in ordinary prose, so another editor shows the note exactly as it
is stored, and a `.md` written elsewhere converts the moment Note-it opens it.

### Comments

```md
<!-- lembrete que não aparece na nota -->
```

A comment is stored in the file and shown in the editor as a small labelled
block, so it can be read, edited and removed — but it is not part of what the
note says, and it never renders as content.

It is data, never markup: what it holds is text, and a `<script>` inside one is
five characters. A `-->` typed into a comment is written as `--&gt;`, because
the literal sequence would close the comment early and spill the rest of the
note out; it reads back as what was typed.

An unterminated `<!--` is not a comment at all. It is escaped to `&lt;!--` so
that everything after it survives, rather than being swallowed to the end of the
file.

Note-it's own task metadata (`<!-- note-it:completed_at=… -->`) stays what it
always was: an inline comment absorbed by the task on its line.
