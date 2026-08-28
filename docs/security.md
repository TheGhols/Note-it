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
