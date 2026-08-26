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

## Production Markdown pipeline

Raw Markdown remains the source format and is never passed wholesale through `DOMParser`. Before Tiptap parses it, Note-it inspects only embedded HTML fragments, removes dangerous blocks and unsupported tags, canonicalizes the supported custom tags, and validates their colors as 3- or 6-digit HEX. The same HEX validator is used by the custom Markdown tokenizers and serializers. Clipboard HTML is sanitized separately before ProseMirror parses it.
