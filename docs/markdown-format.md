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
