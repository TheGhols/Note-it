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

- An expanded note keeps one overlay header, and it paints nothing at all until it is asked for.
  Moving the pointer into the strip along the top of the note reveals the controls over about
  120 ms; leaving the bar lets them recede. Keyboard focus inside the header, an open quick-action
  panel and a collapsed note each hold the chrome out on their own.
- The editor reserves only that strip — `--note-chrome-gutter`, not the bar's full height — so the
  note starts near its own top edge. The strip is the one part of the surface that is always a
  pointer target, and it is exactly the editor's top padding, so no line of text ever sits under
  it: the first line stays clickable, selectable and caret-addressable everywhere. While the
  chrome is hidden the controls take no pointer event at all, so an invisible button can never
  claim a click meant for the text below it.
- **Quick actions:** six icon-only buttons, each opening a panel the menu already owns —
  **Cor da nota**, **Tamanho do texto**, **Cor do texto**, **Marca-texto**, **Blocos** and
  **Buscar**. None of them has logic of its own; they are a second way into the same panel and the
  same handler. They are hidden while the note is collapsed.
  - Their drawings are inline SVG written into `index.html` at build time from six files in the
    supplied icon collection — `bucket`, `larger-text`, `text`, `edite`, `Category` and `Search`.
    Those six are the only ones the build releases, and each is the single source for its icon.
    Nothing is fetched: the page's own `default-src 'self'` blocks an image request for a CSS mask
    or a `data:` URL, which is why the earlier masked icons came out blank on WebKitGTK.
  - Every shape inherits `currentColor` at full strength, so one file serves all seven papers and
    both interface themes and clears 3:1 against every one of them.
- **Settings Menu (`☰`):**
  - A three-line button on the left of the header opens a small popover anchored to the bar.
  - Entries: **Tipo de papel**, **Intensidade**, **Dados**, **Zoom**, **Tema**, **Camada**, and
    **Recolher nota** / **Expandir nota**. The six quick actions are not repeated here — one
    function, one place to reach it — but the panels they open are the menu's own.
  - The menu shows the current paper, intensity, zoom, theme and layer on their own rows, so none
    of them depends on opening a submenu or knowing a shortcut.
  - On a short note the popover is capped at the WebView's remaining height and scrolls vertically;
    a large note keeps the original natural-height menu.
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
  - Its header stays visible and names the note from the first useful content line. A heading marker
    is removed for presentation, an empty note says **Nota sem título**, and long names end in `…`.
    The label is never written into the Markdown or front matter.
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

## Trash

Deleting a note is an explicit action, and it is recoverable.

**Moving a note to the trash.** *☰ › Dados › Mover esta nota para a lixeira*. It asks first:

```text
Mover esta nota para a lixeira? Você poderá restaurá-la depois em Dados › Lixeira.
[Cancelar] [Mover]
```

Cancel is focused, so the key already under your finger is the one that does nothing. Escape and a
click outside are also "no".

- The `×` button and `Ctrl+W` still mean **close the window**. Closing a note has never deleted it
  and still does not.
- The note is saved first. If its latest text cannot be written, nothing is moved: the note stays
  open, the failure is reported, and you can try again.
- The file leaves `notes/` for `trash/`, byte for byte. Front matter, colour, paper, tasks, links,
  calculations and comments all travel with it.
- Moving a note to the trash is not an edit, so its modification date does not change.

**A note in the trash is not a note.** `Ctrl+K` does not find it, the empty-query list does not
offer it, a summon does not bring it back, and restarting does not reopen it.

**Getting one back.** *☰ › Dados › Lixeira* lists what can be recovered — each note's first line, a
preview of its opening, and when it was deleted — newest first. Arrow keys walk the list, `Enter`
restores the selected one, `Esc` closes the panel. Every row also has a named **Restaurar** button.

Restoring puts the file back in `notes/` with the same identifier and the same bytes, and the note
becomes findable again immediately. It keeps its original modification date: a recovered note goes
back to where it was in the quick switcher rather than jumping to the top as though it had just
been written in. It also comes back the size and place it was.

**Restoring never overwrites a live note.** If a note carrying the same identifier is already in
the store, the restore is refused, neither file is changed, and the panel says so.

**There is no permanent delete and no "empty the trash"** in this version. That is deliberate: this
is the phase that makes deletion recoverable, and an irreversible button beside a restore button is
one wrong click away from the thing it exists to prevent. The trash therefore grows until you clear
it yourself, which you can do with any file manager — a note in the trash is an ordinary `.md` in
`~/.local/share/note-it/trash/`.

## Backups

Note-it keeps local snapshots of everything that can be recovered.

**Where.** `~/.local/share/note-it/backups/<data-e-hora>/`, holding `notes/`, `trash/`,
`config.toml`, `state.json` and a `manifest.json` describing the snapshot. Ordinary directories and
ordinary files — no archive, no database, no format of Note-it's own.

**When.** At most one automatic snapshot per 24 hours, taken **before** the first change after that
window has passed. Taking it first is the point: the state worth being able to go back to is the one
before the edit. There is no timer — an idle daemon does no work at all, and one left open for days
takes its snapshot the moment you start typing again.

**Now, if you want one.** *☰ › Dados › Fazer backup agora* takes a snapshot immediately and says
whether it worked, in a line at the foot of the note rather than a dialog over it. Useful before
doing something you are not sure about.

**How many.** The seven most recent are kept. Old ones are removed only **after** a new one has been
completely written, so a backup that fails never costs you the protection you already had.

**What is never in a snapshot:** previous snapshots, temporary files, and anything reached through a
symbolic link. A backup copies regular files from the two directories it was asked to copy and
follows nothing out of them.

**If a backup fails,** the note is still saved. A snapshot is an extra layer of safety; its failure
is written to the diagnostic output and retried later, never turned into a refusal to write your
text.

**Getting a snapshot back** is `cp`, with the application closed — see
[docs/storage.md](storage.md#recovering-from-a-snapshot) for the exact procedure, including how to
recover a single note rather than the whole store. There is deliberately no one-click "restore
everything": that is a multi-file transaction, and it deserves its own design rather than a menu
entry.

> **A local backup is not disaster recovery.** These snapshots sit on the same disk as the notes.
> They protect against an accidental deletion, a logical corruption, an edit you want to undo or a
> version you want to go back to. They protect against **none** of a dead drive, a lost machine or a
> stolen one, and they are not encrypted. Protection from hardware failure needs a copy on other
> hardware, and Note-it does not make one.

## Images

A picture in a note, kept as a file rather than smuggled into the text.

**Putting one in.** Paste it, drop it on the note, or *☰ › Mídia › Inserir imagem…* for a file
chooser. All three end in the same place: the bytes are written into the store, and the note gains a
reference to them.

**PNG, JPEG, WebP and GIF.** What a file *is* decided by its first few bytes, never by its name — so
a PNG called `.txt` is a PNG, and something called `.png` that is not an image is refused. **SVG is
not accepted**: it is a document format that can carry script, and admitting it would open a whole
surface for the sake of a picture. A refusal says so in a line at the foot of the note and leaves
nothing behind — no directory, no half-written file, no change to the note.

**Where the bytes go.** `~/.local/share/note-it/assets/<note-id>/<asset-id>.<ext>`, beside `notes/`
and `trash/`. Ordinary files with ordinary names, copied out with `cp` like everything else here.
Nothing is ever inlined into the Markdown as base64: a screenshot would turn a note you can read
into a megabyte you cannot, and would do it to your backups and your diffs too.

**What the note stores.** A path relative to `notes/` — `../assets/<note-id>/<asset-id>.png` — and
never an absolute one, so a note you put in Git says nothing about your home directory. That
relative form is also why a note reaches the trash and comes back untouched: `notes/` and `trash/`
are siblings, so `..` climbs to the same place from either, and nothing has to be rewritten.

**Two stored forms, and a rule for which.** While there is nothing to say beyond where the picture
is, it is plain Markdown — `![](../assets/…)`. Once you choose a width or an alignment, which
Markdown's image syntax has nowhere to put, it becomes a canonical tag carrying exactly four things:

```html
<img src="../assets/…" alt="" data-note-it-width="320" data-note-it-align="left">
```

Always those attributes, always in that order, and only the ones actually set — so the same picture
always writes the same bytes and a save that changed nothing changes nothing on disk. Anything else
in such a tag is dropped rather than kept: an `onerror`, a `style`, a `srcset`, or a source that is
not one of this store's own assets.

**Size.** A new picture opens capped — wide enough to see in a wide note, small enough to fit a
narrow one — and never larger than its own natural size. Select it and drag either handle to resize:
proportions are kept because only the width is ever stored, height following from the picture
itself. A picture can be made as wide as the note and no wider, whatever the pointer does. The whole
drag is one entry in the history, so `Ctrl+Z` returns the width you started from.

**Alignment and wrapping.** Select the picture and choose *Esquerda*, *Centro* or *Direita*.
Left and right float it, and the text runs down the other side — around the picture, never under it.
Centre is a block of its own, with the text above and below. Quotes, comments and code blocks sit
beside a floated picture rather than under it.

**Removing one.** Take it out of the note like any other content. **The file is not deleted.** There
is no automatic collection of pictures no note points at any more, deliberately: deciding a file is
unused is a guess, and acting on that guess destroys something. If you want the space back, the
assets are ordinary files in an ordinary directory and `rm` still works.

**Nothing is fetched, ever.** There is no way to insert an image by URL, and a remote image somebody
typed by hand is drawn with no source at all — so opening a note reaches the network for nothing, and
a note cannot be used to tell anyone that you read it. The page cannot even name a file: it asks the
application for `note-it-asset:/<note>/<asset>.<ext>`, and the application resolves that inside the
note's own asset directory or not at all.

**A picture is not text.** Nothing about how one is stored reaches the collapsed title, a search
result, the trash, or what the note reads as: searching an identifier, a width, an alignment or
`assets` finds nothing, and a note holding one picture and no words is still *Nota sem título*. The
words around a picture stay as findable as they always were.

## Clipboard AutoPaste

Copy something anywhere on the machine, and it lands at the end of a note you chose. No window
appears, no key is pressed for you, and nothing takes your cursor.

> **This is not *Paste URL on Selection*.** That one — select some words, paste a URL, get a link —
> is a different feature and is still where it was. AutoPaste is a capture mode.

**Off, always, until you say otherwise.** AutoPaste is off when Note-it starts, and switching it on
is a decision you make in *☰ › Captura*. While it is off there is no clipboard handler connected at
all: nothing is observed, read, hashed, stored, logged or sent. That is a property of the
arrangement rather than a promise about it — there is nothing subscribed to observe with.

**It does not come back on by itself.** Whether AutoPaste was on is written nowhere — not in the
note, not in `state.json`, not in `config.toml`. A restart, a logout, a crash or an update leaves it
off, and you decide again. A mode that watches what you copy should never resume without being
asked, and the only way to guarantee that is to have nothing to resume from.

**One note at a time.** The system clipboard is one thing, so exactly one note can be the target.
Switching it on in a second note switches it off in the first, in the same step, and the first
note's bar and menu stop claiming it.

**What it captures.** Text. A copied image, file or unknown format is declined from the formats the
clipboard offers, without a byte of it being read. An empty or blank copy files nothing — no line,
no delimiter, no modification date. And the clipboard as it was *before* you switched the mode on is
never captured: only a change after that moment counts, so whatever was there stays where it was.

**Where it lands.** At the end of the note, always. Not at your cursor and not over your selection:
you are in another application, so the caret in that note is wherever you left it and does not mean
"insert here". The note does not take focus, does not scroll, does not come to the front and does
not change layer. If you are looking at it you will see the text arrive; that is all that happens.

**As text, exactly.** A capture is a paste of plain text, with the same meaning a `Ctrl+V` has here:
`**isso é literal**` stays asterisks, `<script>alert(1)</script>` stays eleven characters and a
copied URL stays a URL you can read. Nothing is fetched — no title lookup, no preview, no favicon —
so AutoPaste works with the network off. Accents, emoji, 日本語 and multi-line copies all survive
unchanged.

**One capture, one undo.** `Ctrl+Z` takes back the last capture whole, delimiter and all, not one
character at a time.

**Separating captures.** *☰ › Captura › Separar capturas* offers three:

| | Between one capture and the next |
|---|---|
| **Linha** | the next line of the same paragraph |
| **Linha em branco** | a paragraph of its own — the default |
| **Separador** | a horizontal rule |

Exactly one is applied between each pair, never two, and never in front of the first capture into an
empty note. Changing the preference applies to the next capture and rewrites nothing already
written. The choice is remembered across restarts, because it says how you like captures laid out
and nothing about what you copied.

**It will not feed the note its own words back.** Copying or cutting inside the note that is
capturing does not append what you just copied. That is not a text comparison — it is the toolkit's
own answer to "did this application put that on the clipboard", checked before any read begins. The
distinction matters: copying `ABC` twice from another application, in two separate actions, files it
twice, because you asked for it twice.

**While it is on** the note keeps its bar out with a 📋 beside the other controls, so a mode that is
watching every copy is never running invisibly. The indicator is on the bar of a collapsed note too,
and pressing it opens the panel that switches it off.

**What it never does:** take ownership of the clipboard (after a capture, what you copied still
pastes normally anywhere else), keep a history of what you have copied, reach the network, write
clipboard content to any log, or put a marker of its own into your note. A capture is ordinary
content once it lands — searchable, deletable, and part of the note's own title if the note was
empty.

**It switches itself off** when the note is closed, sent to the trash, when Note-it is hidden and
when it quits — before any of those finish, so a read still in flight cannot arrive afterwards.
Collapsing the note, changing layer or switching to another application all leave it on; that last
one is what the mode is for.

## Timer & Pomodoro

A countdown on the note you are working in, without leaving it and without a second window.

**Where.** The ⏱ button at the end of the header bar opens a small panel under it. There are two
modes in the panel and one countdown per note: a note runs a Timer or a Pomodoro, never both, so the
mode tabs are unavailable while a run is live rather than being a way to end up with two.

**Timer.** Seven presets — 5, 10, 15, 25, 30, 45 and 60 minutes — and a field for anything else from
1 to 600 whole minutes. A duration that is not one of those is refused and says so; nothing is
rounded into range, because a timer that quietly ran for a duration you did not choose is worse than
one that declined to start. `Enter` in the field starts it.

**Pomodoro.** The classic cycle: 25 minutes of focus, 5 minutes of short break, and a 15-minute long
break after the fourth focus session, after which the count begins again. The panel shows which
phase you are on, which session of the four, and four marks for the cycle.

**Nothing starts on its own.** When a phase runs out it is marked finished and the *next* one is
offered on the button — "Iniciar pausa curta" — for you to start when you are ready. A break that
began by itself while you were still mid-sentence would be a Pomodoro you never agreed to. *Pular
etapa* moves to the next step without waiting for this one.

**Start, pause, continue, cancel.** Only the controls that apply are shown, so there is no Pause on a
paused timer and no Continue on one that never started. Cancelling a Timer keeps the duration you
chose; cancelling a Pomodoro keeps your place in the cycle, and *Reiniciar ciclo* is what goes back
to the beginning.

**It is honest about time.** A running countdown is stored as the *instant it ends*, not as a number
something has to decrement. Every reading is that instant minus the clock now, so nothing drifts, and
nothing is lost to a WebView that was throttled, a machine that was busy, or a laptop that was shut
for ten minutes. Suspend the machine for ten minutes with fifteen left and you come back to five.
Pausing is the mirror: the instant is discarded and the remainder frozen, so paused time cannot be
spent — not while the note is hidden, and not while the application is closed.

**It survives the note going away.** Collapse the note, hide everything, close the application and
come back: a run resumes with the time that really passed already taken off, and one whose end has
gone by comes back **finished** rather than counting through zero. A run that ended while the
application was not open does not ring when you return — an alarm about the past is not an alarm —
but the finished state is right there on the bar.

**On a collapsed note** the bar keeps the clock beside the ⏱, next to the note's name, so a running
countdown never needs the note expanded to be trusted. On a note too narrow to carry both, the digits
give way and the icon stays; the name and the close button never do.

**When it ends** the clock reads `00:00`, the bar and the panel say *Concluído*, a line at the foot
of the note says what finished, and the desktop gets one notification — "Timer concluído", or
"Pomodoro — Sessão de foco concluída." The notification carries nothing from the note: not its title,
not a line of its text. Exactly one is sent per run, however long the note sits at zero. A desktop
with no notification daemon simply gets no notification; nothing about the timer depends on it.

**A timer is not part of the note.** It is never written into the Markdown — no comment, no
front-matter key, no marker. Starting, pausing, finishing or cancelling one leaves the note file byte
for byte as it was and leaves its modification date where it was, so a note with a timer does not
jump to the top of the quick switcher. It is invisible to search, to the collapsed title and to the
trash: searching `25:00` will not find a note merely because it has a 25-minute Pomodoro running. The
state lives beside the window geometry in `state.json`, and it is written only when something
actually happens — a start, a pause, a resume, a cancel, a phase change or a completion. A running
countdown writes nothing at all, once a second or otherwise.

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

- **Recoverable Deletion:**
  - Deleting a note moves its file to `trash/`, from where it can be restored with its identifier,
    its bytes and its modification date intact. The save comes first: a note whose text could not be
    written is never moved.
- **Local Snapshots:**
  - At most one automatic backup per 24 hours, taken before the first change after that window, plus
    a manual one on request. Seven are kept, old ones removed only after a new one is complete.
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
