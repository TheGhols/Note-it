//! What one answer is allowed to weigh on the wire.
//!
//! Every other bound in this server is a bound on *how many* of something is
//! published — a hundred results, fifty candidates, two hundred and forty
//! characters of snippet. [`crate::server::NoteItMcpServer::noteit_read`] is
//! the one tool with nothing of that shape to count: it answers with a note in
//! full, because answering with part of one would be worse (see the module
//! documentation of [`crate::contract`] and ADR-053). So the bound it needs is
//! on the answer itself.
//!
//! ## The bound is on the wire, not on the note
//!
//! `content.len() <= N` would be the obvious check and it would be wrong twice
//! over, and both are measured rather than assumed:
//!
//! ```text
//! a note body of                                        1 048 576 bytes
//! the same body inside the JSON payload      ~2 % more, from \n and \"
//! the payload published a second time              twice, see below
//!                                              ------------------------
//! plain ASCII, measured                           2.04 x the body
//! quotes, backslashes, emoji, measured            2.88 x the body
//! ```
//!
//! The doubling is not this crate's choice. A `CallToolResult` carrying
//! structured content publishes it **twice**: once as `structuredContent`, and
//! once as a text block holding the same JSON as a string, which is what a
//! host that predates structured content reads. The second copy is the first
//! one escaped again, so a note full of `"` costs about twice as much there as
//! it does in the payload.
//!
//! The escaping is not this crate's choice either: a single control character
//! becomes `\u0001`, six bytes for one, and seven in the duplicate.
//!
//! So the measurement here is of the bytes that actually leave the process, and
//! it is exact rather than estimated: [`result_bytes`] serialises the payload
//! through a writer that counts and keeps nothing, adding up both what the
//! payload weighs and what embedding it in a JSON string will cost.
//!
//! What it deliberately does not count is the JSON-RPC frame around the result
//! — `jsonrpc`, `id`, `result`. Those bytes are the host's: the identifier is
//! whatever the host chose to send, and a server cannot bound what it is handed
//! back. Thirty-five bytes, for the one-digit identifier a test uses.

use serde::Serialize;

/// The most a single `noteit_read` answer may weigh, as the bytes its
/// `CallToolResult` serialises to.
///
/// **Four megabytes, and the number is derived rather than chosen.**
///
/// [`noteit_core::control::MAX_FRAME_BYTES`] is one megabyte, and it is the
/// ceiling that already exists on this same data: when a Note-it window holds
/// the store, every write travels to it as one frame, so a megabyte is the
/// largest whole note body the write path can carry. A read that refused
/// below that would publish notes this application cannot round-trip, which
/// would be a bound in the wrong place.
///
/// A read publishes the body twice and escapes it, measured at 2.04x for plain
/// ASCII and 2.88x for text dense in quotes, backslashes and emoji. Four
/// megabytes is that megabyte with the measured expansion covered and room
/// above it, so the property holds:
///
/// > a note whose whole body a write can carry is a note a read can publish.
///
/// For scale: an ordinary note answers in about 800 bytes, and the largest
/// note any suite in this repository builds — 400 000 characters — answers in
/// about 820 000. The store on the machine this was written on holds 154 notes
/// whose largest file is 1 595 bytes.
///
/// A note *deliberately* built out of control characters expands by up to
/// thirteen, and is refused sooner. That is the honest consequence of bounding
/// the wire rather than the file, and it is the right one: the number that
/// matters to whoever receives this answer is the number of bytes they receive.
pub const MAX_READ_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// What a `CallToolResult` costs around the payload it carries.
///
/// ```text
/// {"content":[{"type":"text","text":"   35
/// "}],"structuredContent":               24
/// ,"isError":false}                      17
/// ```
///
/// Exact, and pinned from outside by `mcp_second_brain_red_team.rs`, which
/// measures the bytes a real server writes to a real pipe and compares them
/// against this arithmetic. If the SDK ever changes the shape of a tool result
/// that test fails, which is the point of it.
pub const RESULT_ENVELOPE_BYTES: usize = 76;

/// The bytes a payload will occupy in the tool result that carries it.
///
/// Exact, and computed without building any of them: the payload is serialised
/// through [`WireMeter`], which counts and discards. A sixteen-megabyte note
/// therefore costs one pass over sixteen megabytes rather than the fifty-odd
/// megabytes of `Value` tree and duplicated string the answer itself would
/// have taken.
///
/// The error is `serde_json`'s own and cannot come from the writer, which never
/// fails. A [`crate::contract::ReadResult`] is strings, vectors and options, so
/// in practice there is nothing in it that can refuse to serialise — the case
/// is carried rather than unwrapped because a server does not panic to save a
/// branch.
pub fn result_bytes<T: Serialize>(payload: &T) -> Result<usize, serde_json::Error> {
    let mut meter = WireMeter::default();
    serde_json::to_writer(&mut meter, payload)?;
    Ok(meter.published + meter.embedded + RESULT_ENVELOPE_BYTES)
}

/// Whether an answer of this size may be published as a full read.
pub fn within_read_budget(bytes: usize) -> bool {
    bytes <= MAX_READ_RESPONSE_BYTES
}

/// Counts what a value serialises to, and keeps none of it.
///
/// Two totals, because a tool result carries the payload twice:
///
/// - `published` is the payload as `structuredContent` holds it;
/// - `embedded` is the same bytes as the duplicate text block holds them,
///   which is the payload escaped a second time.
#[derive(Debug, Default)]
struct WireMeter {
    published: usize,
    embedded: usize,
}

impl std::io::Write for WireMeter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.published += buffer.len();
        self.embedded += buffer.iter().copied().map(embedded_width).sum::<usize>();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What one byte of JSON costs when that JSON is itself put inside a JSON
/// string.
///
/// `serde_json`'s own escaping, byte for byte: the two-character escapes it
/// knows, `\u00XX` for every other control character, and everything else —
/// every byte of a multi-byte character included — carried through as it
/// stands.
const fn embedded_width(byte: u8) -> usize {
    match byte {
        b'"' | b'\\' => 2,
        // \b \t \n \f \r
        0x08 | 0x09 | 0x0a | 0x0c | 0x0d => 2,
        0x00..=0x1f => 6,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The meter counts what `serde_json` writes, and `serde_json` is what the
    /// server hands the SDK.
    ///
    /// The published half is checked against the string the SDK will actually
    /// build, so the arithmetic above is anchored to the library rather than to
    /// a belief about it. Field order differs — the value goes through a map on
    /// its way to the wire — and the length does not, which is the only thing
    /// this counts.
    #[test]
    fn the_meter_agrees_with_serde_json() {
        for payload in [
            json!({ "status": "ok" }),
            json!({ "note": { "content": "plain text", "revision": "a".repeat(64) } }),
            json!({ "note": { "content": "quotes \" and \\ backslashes" } }),
            json!({ "note": { "content": "\u{1}\u{2}\u{1f} control" } }),
            json!({ "note": { "content": "tab\tnewline\nreturn\r" } }),
            json!({ "note": { "content": "acentuação 漢字 😀 \u{7f}" } }),
            json!({ "note": { "content": "\u{2028}\u{2029} line separators" } }),
        ] {
            let published = serde_json::to_string(&payload).expect("serialise");
            let duplicated = serde_json::to_string(&published).expect("serialise the duplicate");
            // The duplicate is the payload inside a JSON string: the quotes
            // around it are part of the envelope, not of the escaped bytes.
            let expected = published.len() + (duplicated.len() - 2) + RESULT_ENVELOPE_BYTES;
            assert_eq!(
                result_bytes(&payload).expect("measure"),
                expected,
                "the meter disagrees with serde_json for {payload}"
            );
        }
    }

    #[test]
    fn a_control_character_is_six_bytes_and_seven_in_the_duplicate() {
        let one = result_bytes(&json!("\u{1}")).expect("measure");
        let none = result_bytes(&json!("x")).expect("measure");
        // "x" is one byte published and one embedded; `\u0001` is six and
        // seven, so the difference is five plus six.
        assert_eq!(one - none, 11);
    }

    #[test]
    fn the_budget_is_the_control_frame_with_the_measured_expansion_covered() {
        assert_eq!(
            MAX_READ_RESPONSE_BYTES,
            4 * noteit_core::control::MAX_FRAME_BYTES as usize,
            "the read budget is derived from the write path's own frame ceiling"
        );
    }
}
