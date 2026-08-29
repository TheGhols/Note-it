# Features

## Window & Layer Modes

Note-it leverages Wayland Layer Shell to provide three distinct surface modes:

1. **Desktop Mode (`bottom` layer):**
   - Post-it surfaces remain pinned above the desktop wallpaper but behind application windows.
   - Non-intrusive keyboard mode to avoid stealing focus during normal window navigation.

2. **Overlay Mode (`overlay` layer):**
   - Post-it surfaces surface above all active applications, including maximized and fullscreen windows.
   - Interactive focus is enabled for swift editing.

3. **Hidden Mode:**
   - Surfaces are detached/hidden while the background daemon remains ready for instant activation.

## Note Header

- **Settings Menu (`☰`):**
  - A three-line button on the left of the header opens a small popover anchored to the bar.
  - Entries: **Cor da nota**, **Tipo de papel**, **Intensidade**, **Tamanho do texto**,
    **Cor do texto**, **Marca-texto**, **Zoom**, **Tema**, **Camada**, and **Recolher nota** /
    **Expandir nota**.
  - The menu shows the current paper, intensity, zoom, theme and layer on their own rows, so none
    of them depends on opening a submenu or knowing a shortcut.
  - Closes on outside click, `Escape`, or selecting an entry; only one popover exists per note.
  - The button and the popover sit outside the drag region, so using them never moves the note.
- **Note Information Tooltip:**
  - Resting the cursor on the free area of the header for ~450 ms shows the note's creation and
    modification dates in pt-BR `dd/MM/aaaa HH:mm`.
  - The tooltip never takes the pointer (`pointer-events: none`) and is dismissed by leaving the
    bar, clicking, starting a drag, or opening the menu.
- **Collapse / Expand:**
  - Collapsing reduces the note to its header bar; the editor is hidden, never unmounted, so the
    content, formatting and the Tiptap instance are preserved.
  - The expanded width and height are recorded before collapsing and restored on expand, at
    whatever position the collapsed bar was left.
  - A collapsed note can still be dragged; resizing is unavailable until it is expanded again.
  - The collapsed state is persisted, so a note left collapsed reopens collapsed.

## Paper

Each note carries its own paper, independently of every other note.

- **Cor da nota:** the seven colours — Amarelo, Azul, Verde, Rosa, Roxo, Cinza, Preto.
- **Tipo de papel:** **Liso**, **Pautado**, **Pontilhado**, **Quadriculado pequeno**,
  **Quadriculado grande**. Plain paper is the original look and draws no pattern at all.
- **Intensidade:** **Suave**, **Normal**, **Forte** — the opacity the pattern is drawn with, and
  nothing else. It never changes the paper colour, the text, or the note's geometry. Plain paper
  keeps whatever intensity it was given; it simply has no pattern to act on.
- The pattern is pure CSS: one parameterised system where the type picks a pattern and its
  spacing, the intensity picks the opacity, and the paper colour picks the ink — dark ink on the
  pale papers, light ink on the dark one, so it stays visible on all seven.
- Spacing is in pixels, so zoom scales the text while the pattern stays put. Ruled paper is spaced
  to the note's default line box, but it is a background, not a layout grid: lines are not pinned
  to individual lines of text.
- The pattern is painted on the scrolling surface, so it travels with the text, and the note's own
  colour still fills the window underneath — a fast resize exposes paper, never an unpainted strip.
- A collapsed note's bar shows its colour without the pattern; expanding brings the pattern back.
- Paper type and intensity are properties of the note, stored in its front matter beside the
  colour. Changing either saves the note without touching its content or its modification date.

## Theme

The theme is the appearance of the **application**, not of a note.

- **Sistema**, **Claro**, **Escuro**, chosen from any note's menu and shared by every note. The
  preference is global and lives in `config.toml`.
- **Sistema** follows the desktop's colour scheme and keeps following it, so switching the desktop
  to dark reaches open notes without a restart.
- It dresses only the chrome: menus, popovers, borders, shadows, hover and focus states, and
  auxiliary text. Everything drawn on the paper — the note's text, checkboxes, highlights, the
  header buttons — keeps taking its colour from the paper.
- A note keeps the colour it was given: a yellow note stays yellow under the dark theme, and a
  black one stays black under the light theme.

## Window Positioning & Interactions

- **Drag & Resize:**
  - Header drag region (`.drag-region`) for moving post-its freely across the workspace.
  - Discrete bottom-right resize handle (`.resize-handle`) with min-dimension limits (`220x160` px).
  - A gesture emits geometry deltas only while exactly one pointer is captured; `pointerup`,
    `pointercancel`, a lost pointer capture, or a move reporting no button held all end it
    completely, and a frame left over from before the end cannot move the window.
  - Geometry persisted to `$XDG_STATE_HOME/note-it/state.json` exclusively on gesture end (zero disk I/O during active dragging/resizing).
- **Safe Geometry Clamping & Monitor Fallback:**
  - Clamping guarantees notes stay visible on-screen even after monitor resolution changes.
  - Multi-monitor connector detection with graceful fallback if a display is disconnected.
- **Smart Cascade Placement:**
  - New notes cascade incrementally across the screen grid.

## Note Lifecycle

- **Closing keeps the note:** the `×` button saves the note, records it as closed, and destroys only
  the window. The Markdown file, its geometry, colour, zoom and collapsed state all stay on disk.
- **Summoning brings it back:** running `note-it` restores the notes and makes them visible. With
  every note closed, the one used last is reopened instead of a blank note being created.
- **One instance:** a second invocation reaches the running instance through the single-instance
  dispatcher and exits; it never starts a second application.
- **`note-it new`** is the explicit way to create an additional note.

## Tasks

- **Markdown Task Lists:**
  - Typing `- [ ] ` creates a task; `- [x] ` or `- [X] ` creates a completed one.
  - Real editor nodes with square checkboxes, not text characters, nested up to any depth with
    `Tab` / `Shift+Tab`.
- **Completion:**
  - Completing a task ticks the box, strikes the text through, and records the moment, shown
    discreetly as `Concluído dd/MM/aaaa HH:mm`.
  - Reopening a task clears the date; completing it again records a new one.
  - A task written elsewhere as `- [x]` loads as completed with no date invented for it.

## Smart Blocks

Four block kinds, all stored as ordinary Markdown and all reachable from the
**Blocos** section of the note's own menu — no second toolbar was introduced.

- **Bloco de código:** a fenced block whose language survives every round trip
  untouched, including one nothing here can highlight. Sixteen grammars are
  loaded: `plaintext`, `bash`, `javascript`, `typescript`, `json`, `html`/`xml`,
  `css`, `markdown`, `python`, `rust`, `c`, `cpp`, `java`, `sql`, `yaml` and
  `toml`, plus the aliases each already answers to (`js`, `ts`, `py`, `sh`,
  `cpp`…). The language is chosen from **Blocos → Linguagem**, which shows the
  current one and is offered only where it means something.
- **Callout:** `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]` and
  `> [!CAUTION]` — GitHub's alert syntax, which Obsidian reads too. A callout is
  a blockquote carrying a kind, so it holds several paragraphs, lists and nested
  blocks without a content model of its own. An unrecognised kind is left as the
  blockquote it already is, with its text untouched.
- **Citação:** the plain blockquote, which stays independent of callouts and is
  never promoted into one. Indented, ruled down the side, set in the note's own
  text colour rather than dimmed and italicised.
- **Comentário:** an `<!-- ... -->` kept in the file and shown as a small
  labelled block. It is editable — a comment the window never showed would be one
  nobody could remove — but it is not part of the note's text.

Markdown typed by hand still works: `` ``` `` opens a code block and `> ` opens a
quote, exactly as before.

Highlighting is **presentation only**: editor decorations over the same
characters, never markup in the file. It is not applied to a block with no
language, and never guessed for one whose language is unknown — an unhighlighted
block is the honest answer, not a colour scheme picked by resemblance. Typing
outside a code block does not re-run it, so a note full of code stays as light
to edit as any other.

Every colour a smart block paints — seven syntax tokens and five callout accents
— is defined once for the pale papers and once for the dark one, and each clears
4.5:1 against the paper it is actually drawn on. The grounds are tinted from the
paper rather than being surfaces of their own, so a note keeps its colour under
every block.

## Math Engine

A note calculates as it is written. Nothing is pressed, nothing is re-run, there
is no calculate button and no mode to enter: a line that looks like arithmetic
shows its answer beside it, and the answer follows the note as the note changes.

The result is a **decoration**, not text. It is not in the document, so it is
not saved, not selected, not copied, and not part of an undo step. The `.md` on
disk holds exactly what was typed, which is what makes it safe to open the same
note in another editor.

### Calculating a line

A calculation begins with `=`:

```text
= 2 + 2                            4
= (100 + 50) / 3                  50
= 10 * 8                          80
```

`+`, `-`, `*`, `/` and parentheses, with the usual precedence and
left-associativity. Numbers may be negative and may be written `10.5` or `10,5`
— both separators are read as decimal. A number with **two** separators
(`1.234.567`) is refused rather than guessed at: Note-it accepts no thousands
separator, in either direction, so a result can always be read back as itself.

There is no modulo operator. `%` means percent and only percent, because a
symbol that means two things is a symbol nobody can rely on.

### Variables

A declaration is `nome := expressão`:

```text
preco := 120
quantidade := 3
subtotal := preco * quantidade    360
= subtotal + 10%                 396
```

- Names are ASCII: a letter or `_`, then letters, digits and `_`. `preço` is not
  a name, and a line that says `:=` with an unusable name is reported as **nome
  inválido** rather than quietly read as prose.
- `sum`, `avg`, `count` and `de` belong to the grammar and cannot be names.
- Variables are **local to the note** and resolved **top-down**: a variable
  exists from its declaration downwards. `= preco * 2` written *above*
  `preco := 100` reports an unknown variable, which is also what makes cycles
  impossible — `a := b + 1` over `b := a + 1` simply fails on the first line.
- A later declaration replaces an earlier one from that line down. A declaration
  that fails un-declares the name, so everything below it says so.
- A declaration whose right-hand side is a bare number shows no result: the
  value is already on the line.

### Percentages

```text
= 10% de 200                      20
= 200 + 10%                      220
= 200 - 10%                      180
taxa := 10%
= taxa * 200                      20
```

`X%` is a hundredth. The contextual readings — an increase, a discount, and
`de` — apply to a `%` **written on the line**, never to a value that once came
from one: `taxa` holds `0.1`, so `= 200 + taxa` is `200,1`. What you can see is
what applies. `de` requires a percentage on its left; `200 de 10` is an invalid
expression rather than a number nobody meant.

### `sum`, `avg` and `count`

An aggregator is the **whole** expression of its line, and it reads the block of
consecutive calculation lines directly above it:

```text
= 10                              10
= 20                              20
= 30                              30
= sum                             60
= avg                             20
= count                            3
```

The block is exactly "the `=` lines immediately above that produced a value". A
line of prose, a heading, a declaration or a failed calculation ends it, so a
number sitting in a sentence is never added to anything and two lists separated
by a line of text stay two lists. The three aggregators read the block without
consuming it, so they stack; the first value under one starts a new block.

An empty block sums to `0` and counts `0`; its average is `0 / 0` and says so.

### When it will not calculate

Calculation is read from **plain paragraphs only**. Inside a fenced code block,
an inline code span, a comment, a heading, a list, a task, a quote or a callout,
`= 2 + 2` is the text it is. This is a deliberate first-version boundary: a line
that calculates in one place and not in another for invisible reasons is worse
than one that never calculates in either.

### When it cannot answer

A failure is four words beside the line, in italics, and nothing else — no
dialog, no popup, no stack trace, and nothing written to the file:

| | |
| --- | --- |
| `= 1 / 0` | divisão por zero |
| `= nao_existe * 2` | variável desconhecida |
| `= (2 + 3` | expressão inválida |
| `12preco := 1` | nome inválido |

### Reactivity, and what it costs

The whole note is re-evaluated on every document change. That is the entire
reactivity mechanism: change `preco` and every line under it moves in the same
pass, with no dependency graph to go stale and no timers. Measured on a note far
larger than a post-it — 100 paragraphs of prose, 20 variables, 50 expressions
and all three aggregators — one keystroke costs a fraction of a millisecond.

### There is no evaluator

Expressions are read by a small lexer and a recursive-descent parser written for
this and nothing else. There is no `eval`, no `Function`, no property access, no
call syntax and no host object anywhere in it, and no dependency was added. A
note writing `window.location`, `constructor.constructor(...)` or `fetch(...)`
is writing an invalid expression or naming a variable that does not exist —
variables live in a `Map`, which has no prototype chain to reach into.

### How it looks

Discreet: a small chip at the end of the line, in an ink mixed from the paper's
own two, over the same faint ground the code block, the callout and the comment
already use. It clears 4.5:1 on all seven papers, takes no part in selection or
pointer interaction, and needs no colour or theme override of its own.

## Conversions

A conversion is a calculation with a unit on each side, and it works the way
every other result does: written in the note, computed as you type, shown
beside the line, and never written into the file.

```text
= 10 km em m                      10000 m
= 1500 m em km                    1,5 km
= 0 C em F                        32 °F
```

### The syntax

```text
= <expressão> <unidade> em <unidade>
```

`em` is the conversion keyword and the only one — there is no second spelling
for the same thing. It is a reserved word, so no variable may be called `em`.

The left-hand side is a full expression from the math engine, so all of these
read:

```text
= (10 + 5) km em m                15000 m
= 2 * 3 km em m                   6000 m

distancia := 12
= distancia km em m               12000 m

x := 5
= x * 2 km em m                   10000 m
```

The unit applies to **the whole expression on its left**, so `= 10 + 5 km em m`
is fifteen kilometres. There is no unit algebra here to give the other reading
a meaning, and one rule you can hold in your head beats two you have to guess
between. Use parentheses when the grouping matters to you.

A declaration may hold a conversion — `metros := 10 km em m` — and the variable
then holds `10000`. It holds a **number**, not a quantity: a unit in a variable
is not part of this version, so `distancia := 10 km` is an invalid expression
rather than a distance. See the limitation at the end of this section.

### The units

Every spelling below is matched **exactly**. There is no case folding: `m` is a
metre and `M` is nothing at all, because a rule that folded them would fold `MB`
onto `mb` too. Where a lower-case convenience is safe it is simply listed as an
alias, which is why `ml` and `l` work and `mb` does not.

### Comprimento — base `m`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `mm` | `milimetro`, `milimetros` | mm | 0.001 |
| `cm` | `centimetro`, `centimetros` | cm | 0.01 |
| `m` | `metro`, `metros` | m | 1 |
| `km` | `quilometro`, `quilometros` | km | 1000 |
| `in` | `polegada`, `polegadas` | in | 0.0254 |
| `ft` | `pe`, `pes` | ft | 0.3048 |
| `yd` | `jarda`, `jardas` | yd | 0.9144 |
| `mi` | `milha`, `milhas` | mi | 1609.344 |

### Massa — base `g`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `mg` | `miligrama`, `miligramas` | mg | 0.001 |
| `g` | `grama`, `gramas` | g | 1 |
| `kg` | `quilograma`, `quilogramas`, `quilo`, `quilos` | kg | 1000 |
| `t` | `tonelada`, `toneladas` | t | 1000000 |
| `oz` | `onca`, `oncas` | oz | 28.349523125 |
| `lb` | `libra`, `libras` | lb | 453.59237 |

### Volume — base `mL`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `mL` | `ml`, `mililitro`, `mililitros` | mL | 1 |
| `cL` | `cl`, `centilitro`, `centilitros` | cL | 10 |
| `dL` | `dl`, `decilitro`, `decilitros` | dL | 100 |
| `L` | `l`, `litro`, `litros` | L | 1000 |
| `cm³` | `cm3`, `cc` | cm³ | 1 |
| `m³` | `m3` | m³ | 1000000 |

### Temperatura — base `K`

| unidade | aliases | exibida | conversão |
| --- | --- | --- | --- |
| `°C` | `C`, `c`, `celsius` | °C | `K = °C + 273,15` |
| `°F` | `F`, `f`, `fahrenheit` | °F | `K = (°F + 459,67) × 5/9` |
| `K` | `kelvin` | K | — |

### Tempo — base `s`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `ms` | `milissegundo`, `milissegundos` | ms | 0.001 |
| `s` | `seg`, `segundo`, `segundos` | s | 1 |
| `min` | `minuto`, `minutos` | min | 60 |
| `h` | `hora`, `horas` | h | 3600 |
| `dia` | `dias`, `d` | dia / dias | 86400 |
| `semana` | `semanas` | semana / semanas | 604800 |

### Área — base `m²`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `mm²` | `mm2` | mm² | 0.000001 |
| `cm²` | `cm2` | cm² | 0.0001 |
| `m²` | `m2` | m² | 1 |
| `km²` | `km2` | km² | 1000000 |
| `ha` | `hectare`, `hectares` | ha | 10000 |

An area unit is its own unit with its own factor, not a length with an
exponent: `= 1 m2 em cm2` is `10000 cm²`.

### Dados digitais — base `B`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `B` | `byte`, `bytes` | B | 1 |
| `KB` | — | KB | 1000 |
| `MB` | — | MB | 1000000 |
| `GB` | — | GB | 1000000000 |
| `TB` | — | TB | 1000000000000 |
| `KiB` | — | KiB | 1024 |
| `MiB` | — | MiB | 1048576 |
| `GiB` | — | GiB | 1073741824 |
| `TiB` | — | TiB | 1099511627776 |

The SI prefixes are **decimal** and the IEC prefixes are **binary**, which is
what the two sets of names exist to distinguish: `= 1 GB em MB` is `1000 MB`
and `= 1 GiB em MiB` is `1024 MiB`. Note-it never blurs them.

### Velocidade — base `m/s`

| unidade | aliases | exibida | fator |
| --- | --- | --- | --- |
| `m/s` | — | m/s | 1 |
| `km/h` | — | km/h | 1/3,6 |
| `mph` | — | mph | 0.44704 |

Three named rows, not a length divided by a time. There is no derived-unit
algebra behind them, so `kg/L` and `m/s²` are unknown units rather than
quantities Note-it works out.

### `m` is a metre, `min` is a minute

`m` is never a minute, in any context. If minutes ever gained a one-letter
abbreviation the two would collide, so they do not have one.

### What a conversion refuses

| | |
| --- | --- |
| `= 10 banana em m` | unidade desconhecida |
| `= 10 km em foo` | unidade desconhecida |
| `= 10 kg em km` | unidades incompatíveis |
| `= 1 m2 em m` | unidades incompatíveis |
| `= -300 C em K` | conversão inválida — nothing is colder than absolute zero |
| `= 10 km` | expressão inválida — a conversion has no target |
| `= banana km em m` | variável desconhecida |

An incompatible pair is refused before the expression is even evaluated: a
dimension is a property of the spelling, so `= 10 kg em km` cannot become valid
for some value of the left-hand side.

### Where a conversion is read

Exactly where a calculation is: **plain paragraphs only**. Inside a fenced code
block, an inline code span, a comment, a heading, a list, a task, a quote or a
callout, `= 10 km em m` is the text it is.

### Aggregators and converted quantities

`sum`, `avg` and `count` add up plain numbers and know nothing about units, so a
converted line **ends** the block they read rather than being totalled into it.
Aggregating over units is a real feature; aggregating silently across them is a
bug.

### Precision, and how a result is written

The factors are the defined ones and nothing was rounded to tidy a table: an
inch is exactly 0.0254 m, a pound exactly 453.59237 g, a mile exactly 1609.344 m.
Temperature carries its own converters rather than a factor, because no
multiplication takes 0 to 32 and 100 to 212 at the same time.

Results are written by the same formatter the math engine has always used:
comma for the decimal separator, no thousands separator, twelve significant
digits. The missing grouping is deliberate — `.` and `,` are both read as
decimal separators, so a grouped result would be one this same engine reads back
as a different number.

`dia` and `semana` are the only units whose displayed name changes with the
value, because `1 dia` and `7 dias` both have to read as Portuguese.

### Currencies are not here

`USD em BRL` has no answer without a rate, the rate changes every minute, and a
rate written into a table is wrong before it is committed. Note-it converts only
quantities that are constants, offline, and identical when the note is reopened
in ten years. Currencies are a later phase with a source of its own — see
`docs/decisions.md`, ADR-025.

### Known limitation: a unit cannot live in a variable

```text
distancia := 10 km     ← expressão inválida
```

A variable holds a number, so the unit goes on the line that uses it:

```text
distancia := 10
= distancia km em m    10000 m
```

Carrying units through variables would mean every value in the engine becoming
a quantity, and with it percentages, aggregation and every rule already
established. It is a deliberate boundary for this version rather than a
half-built one.

## Search

Opened with `Ctrl+K` from inside any note. The palette is a panel in the page, not a second window,
and not part of the document — nothing typed into it can reach the Markdown.

### What is searched

The note's **body**: everything below the front matter. Headings, lists, tasks, quotes, callouts,
code blocks and comments are all note content and are all searchable.

The front matter itself is not. `note_it:`, `created_at:`, `updated_at:` and `paper:` are how the
file is written, not what the reader wrote, and a search for `paper` must not return every note in
the store.

Neither is anything the editor merely draws. A `4` shown beside `= 2 + 2`, a `10000 m` shown beside
`= 10 km em m` and every other decoration are not in the file, so no search can find them.

### How a query is matched

| Property | Behaviour |
| --- | --- |
| Case | Insensitive — `BIÓPSIA`, `Biópsia` and `biópsia` are one word |
| Accents | Insensitive — `biopsia` finds `Biópsia`, `coracao` finds `Coração` |
| Matching | Literal substring. `.*`, `[a-z]` and `(foo\|bar)` are those characters, not a pattern |
| Query limit | 512 characters; longer is refused rather than truncated silently |
| Results | 100 notes at most |
| Snippet | About 240 characters, cut at a character boundary |
| Order | Most recently written in first |
| Notes scanned | **All of them.** There is no scan ceiling — the cap is on results, not on how far the search looks |

There is no stemming, no fuzzy matching and no semantic search. `biopsia` finds `biópsia`; it does
not find `punção`. The rule is one a reader can predict, which is the point.

**What the limits do not limit is the note.** They bound the query and the answer; the file is
read to its end, because a word at the end of a long note has to be findable. A store of a thousand
notes is searched in about 40 ms and a single 2 MB note is searched correctly and without writing
anything — both measured by tests — but there is no formal guarantee about an arbitrarily large
individual file, and none is claimed. See ADR-027.1.

### What a result looks like

One note is one result, however many times the word appears in it.

```text
Biópsia hepática                                    4
…a biópsia transjugular é utilizada quando…
```

- The **label** is the note's first non-empty line, with the most obvious Markdown markers removed
  for display — `# Biópsia hepática` is shown as `Biópsia hepática`. Nothing is written to the file
  to create a title. A note with no text is listed as `Nota vazia`.
- The **snippet** is the text around the first match, rendered as text. A note containing
  `<script>alert(1)</script>` shows those characters; it does not become an element.
- The **count** appears when a note holds more than one occurrence.

### An empty query lists recent notes

Opening the palette without typing shows the notes most recently **written in**, so the same
control is also how you move between them. Appearing in that list is not editing: `updated_at` does
not move.

"Most recently written in" is the note's own `updated_at`, not the date on the file. Changing a
note's colour, paper, pattern intensity or font size rewrites the file without being an edit, and
does not move the note up this list — repainting a note is not writing in it. A note with no
`updated_at` — written before the field existed, or with front matter that cannot be read — falls
back to the file's own timestamp. The same rule decides which note a summon brings back, so there
is one idea of "most recent" in the application rather than two that disagree.

### Opening a result

`Enter`, or a click:

- a note **already open** is activated;
- a note that is **closed** is opened;
- a note that is **collapsed** is expanded;
- the note scrolls to the first occurrence and highlights it, with the find bar open so the
  highlight has a visible cause and an obvious way out.

None of that changes the note's text, and none of it moves `updated_at`. The Desktop/Overlay layer
is not touched either: opening a note from a search never switches the layer for everything else.

A result the store no longer has — deleted from outside between the search and the `Enter` — says
`nota não encontrada`, drops the row and searches again. Nothing is recreated.

### Keyboard

| Key | Action |
| --- | --- |
| `Ctrl+K` | Open |
| `Esc` | Close, returning the keyboard to the editor |
| `↓` / `↑` | Next / previous result, wrapping |
| `Enter` | Open the selected result |
| `Ctrl+Shift+Space` | Deliberately **not** claimed — the layer belongs to the application, and toggling it with the palette open neither closes it nor types a space |

Typing is debounced by 120 ms and every request is numbered. Only the answer to the request
currently outstanding can change the list, so an answer to `bio` is discarded once `biopsia` has
been asked — whether it arrives before or after the newer one, and whether or not anything has
answered yet. An answer arriving after the palette has closed changes nothing.

### Searching writes nothing

No save, no flush, no `.md` touched, no `updated_at` moved, no index file, and nothing recorded in
`state.json` — not the query, not the selection, not the palette. Opening a closed note from a
result does change that note's `is_open`, because the reader really did open it.

### No index

There is none, on purpose. A thousand notes are listed, read, folded, matched and turned into
snippets in about 40 ms, so an index would buy nothing a person could perceive and would cost
invalidation, rebuilding, a file format to migrate and a second implementation to keep honest. The
measurement is a test, so the day it stops being true is a day something fails. See ADR-027.

## Find & Replace

Inside the note you are looking at, over the live document — including text typed a second ago and
not yet saved.

### Find

| Key | Action |
| --- | --- |
| `Ctrl+F` | Open, seeded from the selection when it is short and on one line |
| `Enter` | Next occurrence |
| `Shift+Enter` | Previous occurrence |
| `Esc` | Close |
| `Aa` | Match case |

The counter reads `2 de 7`, or `nenhuma`. Navigation wraps in both directions. Every occurrence is
highlighted, the current one more strongly, using theme tokens so the highlight is visible on light
paper and on black paper alike.

Finding changes nothing: the highlights are decorations, so there is no transaction, no undo step,
no rewritten Markdown and no change to `updated_at`.

Find searches the document, so it cannot find a calculated or converted result — searching a note
containing `= 2 + 2` for `4` reports `nenhuma`.

### Replace

`Ctrl+H` adds a second row: **Substituir por…**, **Substituir**, **Todas**.

- **Substituir** replaces the current occurrence and moves to the next. Each is its own undo step.
- **Todas** replaces every occurrence in **one** transaction, applied last-to-first so earlier
  positions stay valid. Twenty replacements come back with a single `Ctrl+Z`.
- Replacement is literal. There is no regex, no `$1`, no `\1` and no capture groups.
- Marks, lists, headings, tasks, quotes and code blocks survive, because the document is edited
  rather than serialised, string-replaced and reloaded.
- Replace is **accent-sensitive**, unlike global search: `saude` does not overwrite `saúde`.
  Because of that, a note opened from the palette is told the spelling that actually matched in it,
  so searching `biopsia` still highlights `Biópsia`.
- Replacing is a real edit, so `updated_at` moves — once, for the edit, and not again for the
  decorations that follow it.

Replace acts on the current note only. There is no cross-note replace.

## Pasting a URL over selected text

Select `site oficial`, paste `https://example.com`, and the note holds:

```markdown
[site oficial](https://example.com)
```

The words you chose are kept and become the link, instead of being replaced by the URL.

- The URL is judged by `safeLinkUrl`, the same allowlist the rest of the application uses. `http`,
  `https` and `mailto` become links; `javascript:`, `data:`, `file:`, `ftp:` and anything else are
  pasted as ordinary text.
- Nothing is fetched. No title, no favicon, no OpenGraph, no preview — and therefore no network, no
  tracking and no waiting.
- Inside inline code or a code block, or with a selection spanning two blocks, it is an ordinary
  paste: a URL in source is characters, and a link cannot wrap a structure.
- It is one undo step.

**Compact link rendering is deliberately not implemented.** Shortening a URL hides part of where it
leads, and the reader who most needs to see `https://evil.example.com/path` in full is exactly the
one an abbreviation would fool. See ADR-027.

## View Controls

- **Zoom (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`):**
  - Scales the note's content between 75% and 200% in 10% steps, without changing the window size,
    the Markdown, or the note's modification date. The header bar keeps its size.
  - Persisted per note in `state.json`; notes without a stored zoom open at 100%.
- **Tema (menu):**
  - Sistema / Claro / Escuro, applied at once to every open note and persisted globally.
- **Layer (`Ctrl+Shift+Space`):**
  - Switches between **Sempre no topo** (above other windows) and **Área de trabalho** (behind
    them, still open). This is the same application-wide switch as `note-it toggle`.
- **Collapse (`Ctrl+Shift+M`):**
  - The same action as the menu entry, reducing the note to its header bar and back. It applies to
    the focused note alone.
- **Collapse everything (`note-it toggle-collapse-all`):**
  - Collapses every note still expanded, and expands them all once they are all collapsed. Each
    note keeps its own collapsed flag and expanded size.
- **A collapsed note expands when clicked:**
  - Clicking anywhere on the bar restores the previous size in place. The close button still
    closes, dragging the bar still moves it, and the `☰` button expands the note and opens its menu
    in a single click.

## Editing Experience

- **Rich WYSIWYG Formatting:**
  - Paragraphs and Headings (H1, H2, H3)
  - Bold, Italic, Underline (`<u>`)
  - Semantic text color (`<span data-note-it-color="...">`) from a compact palette
  - Highlight marker (`<mark data-note-it-highlight="...">`) from a compact palette, always drawn
    with a dark foreground so highlighted text stays readable on every paper colour
  - Discrete text sizes (12–32 px) applied to a selection, independent of headings and of the zoom
  - Bullet lists and numbered lists
  - Interactive checklists (`- [ ]` / `- [x]`)
  - Typing `->` becomes a real `➜`, stored as the character itself rather than relying on a font
    with ligatures, and left untouched inside inline code and code blocks
  - Blockquotes and inline code / code blocks
- **Font Scaling:**
  - The note's base font size is stored in its front matter and applied when the note loads.
    `Ctrl+=` / `Ctrl+-` drive the view zoom rather than this base size.
- **Paper Themes:**
  - 7 curated soft pastel paper colors: Yellow, Blue, Green, Pink, Purple, Gray, Black (with high-contrast light text).
- **Keyboard Shortcuts:**
  - `Ctrl+N` to create a new note in cascade.
  - `Ctrl+W` to save and dismiss current note.
  - `Ctrl+K` to search every note, `Ctrl+F` to find in this one, `Ctrl+H` to find and replace.
    All three were free before Phase 3.8 and collide with nothing above.

## Storage & Reliability

- **Atomic Autosave:**
  - Debounced write (300 ms) via temporary file replacement and directory sync to prevent data corruption.
  - Close and `Ctrl+W` send the latest editor content in one save-and-close request; the window closes only after persistence succeeds.
- **Transactional Flush on Hide and Quit:**
  - `note-it hide` and `note-it quit` explicitly request latest buffer content from all active WebViews, cancel debounces, and await atomic write confirmation for every note before destroying surfaces or exiting.
  - A missing, expired, or invalid WebView response is a flush failure; the host never substitutes its potentially stale in-memory document as a successful confirmation.
  - If any note fails to confirm or save, the operation aborts: hide keeps every surface open in the previous mode, and quit keeps the daemon running. Without confirmation of current WebView content, neither operation destroys surfaces or exits.
- **Standard YAML Front Matter:**
  - Note ID, paper colour, paper type, pattern intensity, font size, and timestamps stored cleanly
    in note headers.
  - `created_at` is fixed at creation; `updated_at` follows content edits only, not appearance or
    window changes. A note without timestamps still opens and reports them as unknown.
  - Visiting a note is not editing it: opening and closing, summoning, hiding, showing or quitting
    without changing the text leaves `updated_at` alone, and the file is not rewritten at all.
