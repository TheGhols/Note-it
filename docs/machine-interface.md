# Machine Interface — `noteit --json`

The stable, versioned contract between the `noteit` command line and the scripts and agents that
call it. Everything a consumer needs in order to decide what to do next is a typed field. No
decision requires reading, translating or pattern-matching a sentence.

This document is the contract. If the implementation and this file disagree, that is a bug.

---

## 1. Turning it on

```bash
noteit --json listar
noteit listar --json
noteit --json ler <ID>
noteit adicionar <ID> "texto" --json
noteit tags adicionar <ID> Medicina --json
```

`--json` is a global option: it is accepted before the command, after it, and at every level of a
grouped command. It works with the international aliases exactly as it does with the Portuguese
spellings, and produces the same document either way.

**`--json` is an option, never a word.** After the `--` escape everything is a value:

```bash
noteit adicionar <ID> -- --json      # appends the literal text "--json"; human output
```

The mode is decided from the real option and never from a substring, never from standard input, and
never from anything after `--`.

---

## 2. One document per execution

Exactly one JSON document is written, ending in a single `\n`:

| result                     | stdout            | stderr            | exit |
| -------------------------- | ----------------- | ----------------- | ---- |
| success                    | the document      | *empty*           | 0    |
| success carrying a warning | the document      | *empty*           | 0    |
| execution error            | *empty*           | the document      | 1    |
| usage error                | *empty*           | the document      | 2    |
| indeterminate              | *empty*           | the document      | 1    |

Nothing else is ever written to either channel in machine mode: no `Aviso:`, no `Erro:`, no usage
prose, no progress, no ANSI. Parsing a whole channel always works. There is never a second document
— NDJSON is deliberately not part of this contract.

ANSI is never emitted in machine mode, whether or not the process is attached to a terminal.
`NO_COLOR` is irrelevant here because there is nothing to turn off.

---

## 3. The envelope

```json
{
  "schema_version": 1,
  "status": "ok",
  "command": "append",
  "data": { "...": "..." },
  "error": null,
  "warnings": []
}
```

All six keys are always present. `data` is `null` on failure and `error` is `null` on success.
Key order is not part of the contract.

### `schema_version`

An integer. `1` today.

- New **optional** fields may be added without changing it.
- Renaming a field, removing one, or changing what one means requires an explicit new version.
- Consumers must ignore fields they do not know and must not depend on key order.

### `status`

Stable machine tokens, never translated:

| value           | meaning                                                                 |
| --------------- | ----------------------------------------------------------------------- |
| `ok`            | the command did what was asked and reported nothing else                |
| `warning`       | the command did what was asked and `warnings` is not empty              |
| `error`         | the command did not do what was asked                                   |
| `indeterminate` | the request went out and the result is genuinely unknown — see §8       |

`status` is `warning` if and only if `warnings` is non-empty on a successful command.

### `command`

The canonical name of the logical command, independent of how it was spelled:

```text
welcome   help    version   status
list      read    search    tags    properties   tasks   trash
create    append  edit
tag_add   tag_remove
property_set   property_remove
task_complete  task_reopen
trash_restore
```

`listar` and `list` both produce `"command": "list"`. `command` is `null` only when the arguments
never named a command this build recognises — a parse error that failed before a command was
identified.

### `warnings`

An array of objects. Each has `code` (a stable token), `message` (diagnostic prose) and `note_id`
(a full UUID or `null`).

```text
unreadable_note              a note could not be read and was left out of the result
corrupted_front_matter       a note's front matter could not be parsed
symlink_refused              a note file is a symbolic link and was refused
io_error                     the store could not be read at that point
ui_sync_window_not_confirmed the write committed; the open window did not confirm it — see §7
```

A warning never means data was lost from the result: the notes that *could* be read are still in
`data`, and the exit code is still `0`.

### `error`

```json
{ "code": "not_found", "message": "…", "commit_state": "not_committed" }
```

`commit_state` is `null` for a command that could not have committed anything (any read, and a parse
error that named no command).

---

## 4. Data by command

Timestamps are always RFC 3339 in UTC (`2026-09-02T00:35:58Z`), or `null` when the store has none.
Identifiers are always full UUIDs — never the eight-character prefix the human output abbreviates
to. Booleans are booleans, counts are numbers, lists are arrays.

```jsonc
// welcome
{ "version": "0.1.0", "machine_interface": true }

// help
{ "usage": "noteit [--json] <comando> [opções]", "help": "…plain text…" }

// version
{ "version": "0.1.0" }

// status
{ "version": "0.1.0", "cli_ready": true, "core_available": true, "store_exists": true,
  "data_path": "…", "config_path": "…", "state_path": "…" }

// list
{ "notes": [ { "note_id": "…", "label": "…", "snippet": "…", "tags": [],
               "properties": [ { "key": "…", "value": "…" } ],
               "created_at": "…Z", "updated_at": "…Z" } ],
  "count": 1 }

// read
{ "note": { "note_id": "…", "label": "…", "content": "…raw Markdown…", "tags": [],
            "properties": [], "created_at": "…Z", "updated_at": "…Z" } }

// search
{ "query": "biopsia",
  "results": [ { "note_id": "…", "label": "…", "snippet": "…",
                 "match_count": 2, "matched_text": "Biópsia" } ],
  "count": 1 }

// tags
{ "tags": [ { "name": "Medicina", "note_count": 3 } ], "count": 1 }

// properties
{ "properties": [ { "key": "fonte", "note_count": 3 } ], "count": 1 }

// tasks
{ "state": "pending",
  "tasks": [ { "task_ref": "a71bc920", "note_id": "…", "note_label": "…",
               "text": "Revisar noradrenalina", "checked": false,
               "completed_at": null, "depth": 0 } ],
  "count": 1 }

// trash
{ "entries": [ { "note_id": "…", "label": "…", "snippet": "…", "deleted_at": "…Z" } ],
  "count": 1 }
```

`state` is `pending`, `completed` or `all`. An empty result is `[]` with `"count": 0` and
`"status": "ok"` — never a sentence.

`content` is the note's Markdown exactly as the Core holds it. The terminal sanitizer that protects
a person's terminal is **not** applied to it: JSON escaping is what makes a control character safe
in a document nobody is rendering as text, and mangling the body would hand a script text the note
does not contain. Quotes, backslashes, newlines, tabs, emoji and escape sequences round-trip
unchanged through any JSON parser.

`task_ref` is produced by the Core and is directly usable in `tasks complete` and `tasks reopen`.
`note_id` from `trash` is directly usable in `trash restore`. No text needs to be parsed to move
between listing and acting.

---

## 5. Write results

Every write command answers with the same shape:

```json
{
  "schema_version": 1,
  "status": "ok",
  "command": "append",
  "data": {
    "write": {
      "note_id": "8c4f1a2b-1111-2222-3333-444444444444",
      "kind": "content_appended",
      "changed": true,
      "commit_state": "committed",
      "ui_sync": { "status": "ok", "code": null, "message": null }
    }
  },
  "error": null,
  "warnings": []
}
```

`kind` is one of:

```text
note_created   content_appended   content_replaced   content_cleared
tag_added      tag_removed        property_set       property_removed
task_completed task_reopened      note_restored
```

### `commit_state`

The one field a consumer must read before deciding whether to run a command again.

| value           | meaning                                                            |
| --------------- | ------------------------------------------------------------------ |
| `committed`     | the change is on disk                                              |
| `not_needed`    | the store already said exactly that; nothing was written           |
| `not_committed` | nothing was written                                                |
| `unknown`       | the request went out and whether it committed cannot be determined |

On success `commit_state` follows `changed`: `true` → `committed`, `false` → `not_needed`.
A `changed: false` result is a **success**, not a failure — asking for a tag a note already has is a
valid request whose desired state already held.

### Retry rule

| status          | commit_state    | meaning                             | repeat automatically?           |
| --------------- | --------------- | ----------------------------------- | ------------------------------- |
| `ok`            | `committed`     | the change was written              | **no**                          |
| `warning`       | `committed`     | written, plus something to report   | **no**                          |
| `ok`            | `not_needed`    | the state asked for already held    | unnecessary                     |
| `error`         | `not_committed` | nothing was written                 | only after fixing the cause     |
| `indeterminate` | `unknown`       | it may or may not have been written | **never** — a person must look  |

`not_committed` does not mean "retry now". It means nothing was written; whether repeating helps
depends on `error.code` — a `not_found` will not become a `found` on the second try.

---

## 6. Which note is open is not the consumer's problem

The same operation produces the same public document whether `noteit` wrote the file itself or a
running Note-it desktop instance wrote it on request. Which of the two happened is an implementation
detail and is deliberately not reported: there is nothing a consumer can do differently about it.

The one legitimate difference is `ui_sync`, because a window can only be out of step when there is
a window.

---

## 7. `ui_sync` — committed, with the window behind

```json
"ui_sync": {
  "status": "warning",
  "code": "window_not_confirmed",
  "message": "a nota aberta não conseguiu adotar o documento gravado"
}
```

When a note is open on screen, Note-it freezes its editor, folds any unsaved text into the same
commit, writes the file, and then hands the committed document back to the window. If the window
does not confirm that it took the document, the write is **still committed** — the file on disk holds
the new text — and only the screen is behind.

That case is reported as:

```text
status              warning
data.write.changed  true
commit_state        committed
ui_sync.status      warning
ui_sync.code        window_not_confirmed
warnings[]          contains ui_sync_window_not_confirmed
exit code           0
stderr              empty
```

It is never `status: error`, never `commit_state: not_committed`, and never a non-zero exit.
**Repeating the command would append the same text twice.** A consumer that branches on
`ui_sync.status` and `commit_state` cannot make that mistake; one that reads the message might.

`ui_sync.status` is `ok` whenever nothing reported the window as out of step, which includes every
write made with no window involved at all.

---

## 8. `indeterminate` — the result is unknown

The request reached the authority and the answer did not come back: the connection dropped, or the
response did not belong to this request. The authority may have committed before that happened, and
there is no way to tell from the calling side.

```json
{
  "schema_version": 1,
  "status": "indeterminate",
  "command": "append",
  "data": null,
  "error": { "code": "indeterminate", "message": "…", "commit_state": "unknown" },
  "warnings": []
}
```

Exit code is non-zero, but this is **not** "the write failed". `commit_state` is `unknown` and never
`not_committed`, precisely so an agent cannot treat it as a clean failure and retry.

**Never repeat an operation automatically after `unknown`.** Read the note, decide what the store
actually holds, and act on that.

---

## 9. Error codes

Stable tokens. The `message` beside them is human-readable diagnostic prose; its wording, and even
its language, are not part of the contract.

| code                    | exit  | commit_state on a write | meaning                                              |
| ----------------------- | ----- | ----------------------- | ---------------------------------------------------- |
| `usage_error`           | 2     | `not_committed` \*      | the request was not well formed                      |
| `invalid_input`         | 2 / 1 | `not_committed`         | a selector, payload or reference that is not one     |
| `validation`            | 2     | `not_committed`         | a domain rule refused the value                      |
| `not_found`             | 1     | `not_committed`         | no note or trash entry answers to that selector      |
| `ambiguous_selector`    | 1     | `not_committed`         | more than one note answers to that selector          |
| `stale_task_ref`        | 1     | `not_committed`         | the note changed; that reference no longer names it  |
| `ambiguous_task_ref`    | 1     | `not_committed`         | the reference matches more than one task             |
| `writer_busy`           | 1     | `not_committed`         | another Note-it writer holds the store               |
| `authority_unavailable` | 1     | `not_committed`         | the store is held and the holder could not be asked  |
| `trash_target_occupied` | 1     | `not_committed`         | a live note already carries that identifier          |
| `persistence`           | 1     | `not_committed`         | the write was attempted and did not happen           |
| `store_unavailable`     | 1     | `not_committed`         | the store itself could not be read                   |
| `indeterminate`         | 1     | `unknown`               | the result is unknown — see §8                       |
| `read_failed`           | 1     | `null`                  | a note or the store could not be read                |
| `internal_error`        | 1     | `null`                  | the answer itself could not be produced              |

\* `usage_error` carries `not_committed` when it was raised against a command that writes, and
`null` when the command was a read or could not be identified.

`invalid_input` is the one code that carries two exit codes, and it is inherited rather than
introduced here: a malformed selector given to a **write** has always exited `2` and the same
selector given to a **read** has always exited `1`. The machine interface preserves both rather than
quietly renumbering a sealed contract. Branch on `error.code`, not on the exit code, whenever the two
could differ.

Every one of these except `indeterminate` means, under the current contract, that nothing was
written.

### Parse errors keep machine mode

Machine mode survives an argument list the parser could not read at all:

```bash
noteit --json batata                  # → usage_error on stderr, exit 2
noteit --json adicionar               # → usage_error on stderr, exit 2
noteit --json --flag-inexistente      # → usage_error on stderr, exit 2
noteit --json buscar                  # → usage_error on stderr, exit 2
```

A consumer that asked for JSON never receives a paragraph of Portuguese instead.

---

## 10. What is deliberately not here

- **The private control protocol.** Request identifiers, the protocol version, the socket path, the
  writer lock, the window generation and which write path ran are a conversation between two Note-it
  processes. They are not this API and are never serialised into it, even though both happen to use
  JSON.
- **Filesystem paths**, except in `status`, where they are the point of the command.
- **Input in JSON.** `--json` describes output only. Payloads still arrive as arguments or on
  standard input via `--stdin`, unchanged and unencoded.
- **Pretty printing, NDJSON, batching, streaming, a daemon, MCP.** One command, one document.

---

## 11. Answering the questions that matter, without reading a word

| question                             | field                                             |
| ------------------------------------ | ------------------------------------------------- |
| did the command work?                | `status`                                          |
| was there something to report?       | `warnings`, `status == "warning"`                 |
| was anything actually changed?       | `data.write.changed`                              |
| did the commit happen?               | `data.write.commit_state == "committed"`          |
| was a commit even needed?            | `commit_state == "not_needed"`                    |
| did the commit definitely not happen?| `commit_state == "not_committed"`                 |
| is the commit result unknown?        | `status == "indeterminate"`, `commit_state == "unknown"` |
| which note was affected?             | `data.write.note_id` (a full UUID)                |
| which operation happened?            | `command`, `data.write.kind`                      |
| is the open window out of step?      | `data.write.ui_sync.status`                       |
| what went wrong?                     | `error.code`                                      |

`message` fields exist for a person reading a log. A consumer that branches on one has misread this
interface.
