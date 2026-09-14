//! The note-maths engine, ported from the graphical editor.
//!
//! The graphical editor has done arithmetic in notes since before this
//! terminal interface existed, and a second engine that merely *resembles* it
//! would be worse than none: a note would answer one number in one window and
//! a different number in the other, and the reader would have no way to know
//! which to believe.
//!
//! So this is a port, not an interpretation. Every rule below has a counterpart
//! in `ui/src/math/`, and the two are held to the same written-down
//! expectations in `tests/fixtures/math-conformance.json` — one file, two
//! implementations, each asserting against it in its own test suite. Comparing
//! the two directly would only prove they agree; comparing both to a fixture
//! proves what they agree *on*.
//!
//! ## What cannot be spelled here
//!
//! There is no token for a dot, a bracket, a string or a call. `window`,
//! `constructor` and `fetch(...)` are not dangerous input to be filtered —
//! they are simply not expressible in this grammar, and stop at the first
//! character that is not one of its shapes. Variables live in a map, so an
//! unknown name is unknown whatever it is called, and a conversion carries two
//! units resolved from a table rather than anything the note wrote.

pub mod document;
pub mod errors;
pub mod evaluate;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod units;

pub use document::{classify_line, evaluate_note, LineKind, LineResult};
pub use errors::{message_for, MathError};
