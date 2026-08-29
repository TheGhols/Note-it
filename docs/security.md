# Security & Content Sanitization

## Security Principles

Note-it handles local Markdown content, but because it renders rich text within a WebKit webview, it treats all input HTML with strict sanitization:

1. **Restricted HTML Whitelist:**
   - Permitted inline tags: `<u>`, `<span data-note-it-color="...">`, `<mark data-note-it-highlight="...">`.
   - Permitted attributes: `style="color: #..."`, `style="background-color: #..."`, `data-note-it-color`, `data-note-it-highlight`.
2. **Blocked Elements & Vectors:**
   - `<script>`, `<iframe>`, `<object>`, `<embed>`, `<form>`, `<style>` (standalone blocks).
   - Event attributes (`onclick`, `onload`, `onerror`, etc.).
   - Executable URI schemes (`javascript:`, dangerous `data:` URIs).
3. **External Links:**
   - Links in notes do not navigate the WebKit webview.
   - Clicking a link dispatches a request to the Rust host to open the system's default browser via `xdg-open` / GIO.
   - The Rust host independently parses the URI and allows only `https:`, `http:`, and `mailto:` before invoking GIO.
4. **Content Security Policy (CSP):**
   - The webview enforces strict CSP forbidding inline scripts from remote sources and restricting network connections.

## The math engine has no evaluator

Calculations in a note are read by a lexer and a recursive-descent parser written
for that grammar alone (`ui/src/math/`). There is no `eval`, no `Function`, no
dynamic import, no timer, no property access and no call syntax anywhere in it,
and no library was added to provide any. An expression is a sequence of ten
token shapes and becomes a tree of six node kinds, which is then walked to do
arithmetic.

That is why `= window.location`, `= process.exit()`, `= fetch(...)` and
`= constructor.constructor("return 1")()` are not inputs to be filtered. They
cannot be spelled: the grammar has no token for `.`, `[`, `"` or a call, so they
stop at the first character that is not one of the shapes above and are reported
as an invalid expression.

Variables are held in a `Map`, never in an object. An object would resolve
`constructor`, `__proto__`, `toString` and `valueOf` to real JavaScript values;
a `Map` has no inherited keys, so an unknown name is unknown whatever it is
called, and declaring one stores a key rather than reaching a prototype.

Expression length, token count and nesting depth are all capped, so a hostile or
accidental paste costs a fixed amount rather than the stack. Error messages are
seven constants; no part of a note is ever echoed back through one.

Units are resolved the same way, and for the same reason. `ui/src/units/registry.ts`
builds one `Map` from a literal table and every lookup goes through it, so
`= 10 constructor em m` and `= 10 km em __proto__` are unknown units rather than
reaching a JavaScript property. Nothing is ever indexed dynamically off a host
object, and the two characters conversions added to the lexer — `°` for `°C` and
`²`/`³` for `m²` and `cm³` — are identifier characters and grant no new
capability. The rule for what a *variable* may be called is unchanged and still
ASCII.

Nothing in the engine reaches the network, and a test asserts it: no `fetch`, no
`XMLHttpRequest`, no `WebSocket`, no `navigator`, no storage. Every unit Note-it
converts is a constant, which is exactly why currencies are not among them.

## Production Markdown pipeline

Raw Markdown remains the source format and is never passed wholesale through `DOMParser`. Before Tiptap parses it, Note-it inspects only embedded HTML fragments, removes dangerous blocks and unsupported tags, canonicalizes the supported custom tags, and validates their colors as 3- or 6-digit HEX. The same HEX validator is used by the custom Markdown tokenizers and serializers. Clipboard HTML is sanitized separately before ProseMirror parses it.

## Search reads notes; it never executes them

A query is text. It is folded for accents, lower-cased and matched as a literal substring — there
is no regex engine, so `.*`, `[a-z]` and `(foo|bar)` are those characters and cost what those
characters cost. Nothing is passed to a shell, to SQL or to any interpreter, because there is none
to pass it to.

The limits are explicit, and they say exactly what they bound: 512 characters of query, 100
results, about 240 characters of snippet. A query longer than the ceiling is refused rather than
truncated, and a store of any size produces at most a hundred rows. The scan reads note bodies
rather than loading a WebView for each one — searching a thousand notes creates zero additional
WebViews.

**What those limits do not bound is the note.** A note is a text file and anything can be pasted
into one, and search reads all of it: finding a word at the end of a large note requires reading to
the end of a large note, and cutting that short would mean text in the store that no search could
ever return. So a single enormous note costs what its size costs. That cost is measured rather
than capped — a thousand notes totalling about 1.1 MB are searched in roughly 40 ms, and a 2 MB
single note is searched, with its accents intact and without writing anything, in
`a_very_large_note_is_searched_correctly_and_never_written`. There is no formal guarantee that some
arbitrarily large individual file cannot make one keystroke slow, and this document does not
claim one.

**A snippet is text.** Labels and snippets are written with `textContent`, never `innerHTML`. A
note containing `<script>alert(1)</script>` or `<img onerror=...>` shows those characters in the
result list; no element is created from them and nothing runs. The note is a file the user
controls, and a search result is a rendering of it, not an execution of it.

**The interface cannot name a file.** A search result carries a `note_id`, and the message the
WebView sends back to open one carries a `Uuid` — a path cannot be spelled in it, so
`../../etc/passwd` is not a request that exists. The host resolves the identifier through the same storage
rules everything else uses and reports a missing note rather than creating one.

**Searching does not write.** No note is saved, flushed or rewritten to answer a query, no
`updated_at` moves, and there is no index file, so there is no second copy of the user's notes on
disk to protect, back up or leak.

## Pasting a URL creates a link through one gate

Pasting a URL over selected text makes that text a link, and the URL is judged by `safeLinkUrl` —
the same allowlist the rest of the application uses. `http`, `https` and `mailto` pass; everything
else, `javascript:`, `data:`, `file:`, `vbscript:` and `ftp:` among them, is pasted as ordinary
text. Whitespace, control characters, a scheme-only string and a hostless `http://` are all
refused.

There is deliberately exactly one opinion in the application about what a URL is. Tiptap's own
`linkOnPaste` is switched off, because it uses `linkifyjs` — a second parser, with a different
answer, that accepted schemes this application does not allow. A test asserts that pasting
`ftp://…`, `ssh://…` or `www.…` produces no link at all.

Nothing is fetched. No title, no favicon, no OpenGraph, no preview, and no HTTP client was added:
the clipboard already holds everything the feature needs, so the feature adds no network surface.
