//! Images a note owns: where their bytes live, what they are called, and how
//! the page is allowed to ask for them.
//!
//! Three decisions are the whole of this file.
//!
//! **The format is decided by the bytes, never by the name.** A file called
//! `diagrama.png` that is really an SVG is an SVG, and an SVG is refused —
//! it can carry script, and a note is not a place that needs one. Sniffing
//! also means a paste, a drop and a file chooser all go through exactly the
//! same check, because none of them is trusted more than the others.
//!
//! **The name is ours.** Whatever the file was called on the reader's disk is
//! not what it is called here: an imported image becomes
//! `assets/<note-uuid>/<asset-uuid>.<ext>`. Nothing a filename could contain —
//! a `..`, a `/`, a newline, a control character, a locale-dependent case fold
//! — survives into a path, because none of it is used to build one.
//!
//! **The page never spells a filesystem path.** What is stored in the note is
//! `../assets/<note>/<asset>.<ext>`, relative to `notes/`; what the page loads
//! is `note-it-asset:/<note>/<asset>.<ext>`, which the host resolves itself.
//! Both halves are `Uuid`s and both are parsed as `Uuid`s before anything
//! touches the disk, so there is no string a request could carry that names a
//! file outside the note's own asset directory. See ADR-032.

use crate::atomic_file::write_atomic;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The scheme the page loads an image through. Registered by the host, served
/// by the host, and resolvable to nothing but a note's own asset directory.
pub const ASSET_SCHEME: &str = "note-it-asset";

/// The directory, under the data directory, that all of this lives in.
pub const ASSETS_DIRECTORY: &str = "assets";

/// How a note refers to one of its images, relative to `notes/`.
///
/// `../assets/…` rather than an absolute path, and that is the reason a note
/// can be moved to the trash without anything being rewritten: `trash/` is a
/// sibling of `notes/`, so the same `..` climbs to the same data directory
/// from either of them. It also keeps the reader's home directory out of a
/// file they may well put in Git.
pub const ASSET_RELATIVE_PREFIX: &str = "../assets/";

/// The image formats a note may hold.
///
/// SVG is deliberately absent. It is a document format that can carry script
/// and external references, and admitting it would mean auditing that whole
/// surface for a feature whose subject is a picture. PNG, JPEG, WebP and GIF
/// are decoded as images and nothing else.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Gif => "gif",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Gif => "image/gif",
        }
    }

    /// The format the bytes actually are, or `None`.
    ///
    /// Signatures only: the first few bytes of the four supported formats are
    /// unambiguous, and reading them is the whole check. A file whose name
    /// claims one thing and whose contents are another is what the contents
    /// are, and something that is not one of these four is not an image this
    /// accepts — including SVG, which has no binary signature at all and is
    /// therefore refused by construction rather than by a rule about it.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(ImageFormat::Png);
        }
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(ImageFormat::Jpeg);
        }
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            return Some(ImageFormat::Gif);
        }
        // RIFF....WEBP — the size sits between the two markers.
        if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            return Some(ImageFormat::Webp);
        }
        None
    }

    /// The format an extension names, used only to resolve a stored reference
    /// this application wrote itself. Never used to decide what an import is.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
            "webp" => Some(ImageFormat::Webp),
            "gif" => Some(ImageFormat::Gif),
            _ => None,
        }
    }
}

/// One stored image: whose note it belongs to, its own identifier, and what it
/// is. Every field is a value with a closed shape, so there is no string here
/// that could name something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetRef {
    pub note_id: Uuid,
    pub asset_id: Uuid,
    pub format: ImageFormat,
}

impl AssetRef {
    /// What the note stores: `../assets/<note>/<asset>.<ext>`.
    pub fn relative_path(&self) -> String {
        format!(
            "{ASSET_RELATIVE_PREFIX}{}/{}.{}",
            self.note_id,
            self.asset_id,
            self.format.extension()
        )
    }

    /// What the page loads: `note-it-asset:/<note>/<asset>.<ext>`.
    ///
    /// The page builds this itself from the reference it was given, so the
    /// host never sends one. It is written out here so the two halves of the
    /// contract can be checked against each other rather than only described.
    #[cfg(test)]
    pub fn display_uri(&self) -> String {
        format!(
            "{ASSET_SCHEME}:/{}/{}.{}",
            self.note_id,
            self.asset_id,
            self.format.extension()
        )
    }

    /// Where the bytes are, under a given assets directory.
    pub fn file_path(&self, assets_dir: &Path) -> PathBuf {
        assets_dir.join(self.note_id.to_string()).join(format!(
            "{}.{}",
            self.asset_id,
            self.format.extension()
        ))
    }
}

/// Reads a request path — `/<note>/<asset>.<ext>` — back into an [`AssetRef`].
///
/// The only way a URI reaches the disk, and every part of it is parsed rather
/// than trusted: two `Uuid`s and an extension from a closed set. `..`, an
/// absolute path, a percent-encoded separator, an extra segment, a missing
/// one, an empty one — none of them parse as a `Uuid`, so none of them names a
/// file. There is no string this can return that points outside
/// `assets/<note>/`.
pub fn parse_asset_request(path: &str) -> Option<AssetRef> {
    let trimmed = path.strip_prefix('/').unwrap_or(path);
    let mut parts = trimmed.split('/');
    let note = parts.next()?;
    let file = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let note_id = Uuid::parse_str(note).ok()?;
    let (asset, extension) = file.rsplit_once('.')?;
    let asset_id = Uuid::parse_str(asset).ok()?;
    let format = ImageFormat::from_extension(extension)?;
    // The canonical spelling of the extension is the one this writes, so a
    // request for `<uuid>.JPEG` resolves to the `<uuid>.jpg` on disk.
    Some(AssetRef {
        note_id,
        asset_id,
        format,
    })
}

/// Reads a stored `../assets/<note>/<asset>.<ext>` back into an [`AssetRef`].
///
/// Anything else — an absolute path, a URL, a relative path climbing further,
/// a hand-written `![](foto.png)` — is not one of ours and comes back `None`.
///
/// The host never resolves a stored reference: the page owns the note's text
/// and asks for the picture by the scheme above. This is here so the format
/// the page writes can be stated and checked from the side that stores it.
#[cfg(test)]
pub fn parse_stored_reference(reference: &str) -> Option<AssetRef> {
    let rest = reference.strip_prefix(ASSET_RELATIVE_PREFIX)?;
    if rest.contains("..") {
        return None;
    }
    parse_asset_request(rest)
}

/// The largest image a note will take in, in bytes.
///
/// Generous for a screenshot or a photograph and far below anything that would
/// make a note unopenable. A paste larger than this is refused whole rather
/// than truncated: half an image is not an image.
pub const MAX_IMAGE_BYTES: usize = 24 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    Empty,
    TooLarge,
    UnsupportedFormat,
}

impl ImportError {
    /// What the reader is told. One sentence per reason, and never the name of
    /// the file or anything else from the machine it came from.
    pub fn message(self) -> &'static str {
        match self {
            ImportError::Empty => "Não foi possível ler a imagem.",
            ImportError::TooLarge => "A imagem é grande demais para uma nota.",
            ImportError::UnsupportedFormat => {
                "Formato de imagem não suportado. Use PNG, JPEG, WebP ou GIF."
            }
        }
    }
}

/// Decides what an import is, before anything is written.
///
/// Separated from the writing so the whole policy — empty, oversized,
/// unsupported, SVG — is testable without a filesystem, and so that a refusal
/// happens before a directory is created or a byte is stored. A refused import
/// leaves nothing behind.
pub fn classify_import(bytes: &[u8]) -> Result<ImageFormat, ImportError> {
    if bytes.is_empty() {
        return Err(ImportError::Empty);
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImportError::TooLarge);
    }
    ImageFormat::sniff(bytes).ok_or(ImportError::UnsupportedFormat)
}

/// Writes an accepted image into the note's own asset directory.
///
/// The identifier is minted here, so two imports of the same picture are two
/// assets and neither can overwrite the other. The bytes go down under the
/// same commit-point rule as a note: a temp file renamed into place, so a
/// half-written image is never a file a note can point at.
pub fn import_image(
    assets_dir: &Path,
    note_id: Uuid,
    bytes: &[u8],
) -> Result<AssetRef, ImportError> {
    let format = classify_import(bytes)?;
    let asset = AssetRef {
        note_id,
        asset_id: Uuid::new_v4(),
        format,
    };

    let path = asset.file_path(assets_dir);
    if let Some(parent) = path.parent() {
        crate::permissions::create_private_dir_all(parent).map_err(|_| ImportError::Empty)?;
    }
    write_atomic(&path, bytes, "the image").map_err(|_| ImportError::Empty)?;
    Ok(asset)
}

/// Decodes the bytes a page sends for a pasted or dropped image.
///
/// Base64 only on the wire, and only for as long as one message: what reaches
/// the disk is the bytes themselves, and what a note stores is a path. Written
/// here rather than pulled in, because it is twenty lines and a dependency for
/// twenty lines is a dependency to keep patched forever.
///
/// Strict on purpose. Whitespace is skipped, because a long payload may arrive
/// wrapped; everything else outside the alphabet is a refusal rather than a
/// character quietly dropped, since a decoder that guesses turns a corrupted
/// message into a corrupted file.
pub fn decode_base64(encoded: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut out = Vec::with_capacity(encoded.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;
    let mut padding = 0usize;

    for byte in encoded.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            padding += 1;
            continue;
        }
        // Padding is only ever the end of the message.
        if padding > 0 {
            return None;
        }
        let decoded = value(byte)?;
        accumulator = (accumulator << 6) | decoded;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((accumulator >> bits) & 0xFF) as u8);
        }
    }

    // What is left over must be padding bits, and they must be zero: anything
    // else is a truncated message rather than a short one.
    if padding > 2 || bits >= 8 || (accumulator & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn png() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[0u8; 32]);
        bytes
    }

    fn jpeg() -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0];
        bytes.extend_from_slice(&[0u8; 32]);
        bytes
    }

    fn webp() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0x24, 0, 0, 0]);
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(&[0u8; 32]);
        bytes
    }

    fn gif() -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&[0u8; 32]);
        bytes
    }

    #[test]
    fn the_four_supported_formats_are_recognised_by_their_bytes() {
        assert_eq!(ImageFormat::sniff(&png()), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::sniff(&jpeg()), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::sniff(&webp()), Some(ImageFormat::Webp));
        assert_eq!(ImageFormat::sniff(&gif()), Some(ImageFormat::Gif));
        assert_eq!(ImageFormat::sniff(b"GIF87a....."), Some(ImageFormat::Gif));
    }

    #[test]
    fn an_svg_is_refused_however_it_is_dressed_up() {
        // The reason SVG is out: it is a document that can carry script. It has
        // no binary signature, so it is refused by the same rule that refuses
        // everything else — and renaming it does not help, because the name is
        // never consulted.
        for svg in [
            &b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>"[..],
            &b"<?xml version=\"1.0\"?><svg/>"[..],
            &b"   <svg onload=\"alert(1)\"/>"[..],
        ] {
            assert_eq!(ImageFormat::sniff(svg), None);
            assert_eq!(
                classify_import(svg),
                Err(ImportError::UnsupportedFormat),
                "an SVG was accepted"
            );
        }
    }

    #[test]
    fn the_bytes_decide_and_the_name_never_does() {
        // A PNG called `.txt` is a PNG; an executable called `.png` is not an
        // image. Nothing here has a filename to consult in the first place.
        assert_eq!(classify_import(&png()), Ok(ImageFormat::Png));
        assert_eq!(
            classify_import(b"\x7fELF\x02\x01\x01\x00 and the rest"),
            Err(ImportError::UnsupportedFormat)
        );
        assert_eq!(
            classify_import(b"apenas texto, chamado foto.png"),
            Err(ImportError::UnsupportedFormat)
        );
        assert_eq!(
            classify_import(b"%PDF-1.7 ..."),
            Err(ImportError::UnsupportedFormat)
        );
    }

    #[test]
    fn nothing_at_all_and_far_too_much_are_both_refused() {
        assert_eq!(classify_import(b""), Err(ImportError::Empty));

        let mut huge = png();
        huge.resize(MAX_IMAGE_BYTES + 1, 0);
        assert_eq!(classify_import(&huge), Err(ImportError::TooLarge));

        // Exactly at the ceiling is still an image.
        let mut large = png();
        large.resize(MAX_IMAGE_BYTES, 0);
        assert_eq!(classify_import(&large), Ok(ImageFormat::Png));
    }

    #[test]
    fn a_refusal_says_what_is_wrong_and_nothing_about_the_machine() {
        for error in [
            ImportError::Empty,
            ImportError::TooLarge,
            ImportError::UnsupportedFormat,
        ] {
            let message = error.message();
            assert!(!message.is_empty());
            assert!(!message.contains('/'));
            assert!(!message.contains("home"));
        }
    }

    #[test]
    fn a_stored_reference_is_relative_and_names_no_machine() {
        let asset = AssetRef {
            note_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            asset_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            format: ImageFormat::Png,
        };
        assert_eq!(
            asset.relative_path(),
            "../assets/11111111-1111-4111-8111-111111111111/22222222-2222-4222-8222-222222222222.png"
        );
        // Nothing absolute, nothing about a home directory, nothing a note put
        // in Git would leak.
        assert!(!asset.relative_path().starts_with('/'));
        assert!(!asset.relative_path().contains("home"));
    }

    #[test]
    fn the_same_reference_resolves_from_notes_and_from_the_trash() {
        // The whole reason the path is relative. `notes/` and `trash/` are
        // siblings, so `..` climbs to the same data directory from either, and
        // moving a note between them rewrites nothing.
        let data = Path::new("/tmp/store/note-it");
        let asset = AssetRef {
            note_id: Uuid::new_v4(),
            asset_id: Uuid::new_v4(),
            format: ImageFormat::Jpeg,
        };
        let reference = asset.relative_path();

        let from_notes = data.join("notes").join(reference.clone());
        let from_trash = data.join("trash").join(reference);
        let canonical = |path: PathBuf| {
            let mut parts: Vec<std::ffi::OsString> = Vec::new();
            for part in path.components() {
                match part {
                    std::path::Component::ParentDir => {
                        parts.pop();
                    }
                    other => parts.push(other.as_os_str().to_os_string()),
                }
            }
            parts
        };
        assert_eq!(canonical(from_notes), canonical(from_trash));
    }

    #[test]
    fn a_request_resolves_only_to_a_notes_own_asset() {
        let note_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let path = format!("/{note_id}/{asset_id}.png");

        let parsed = parse_asset_request(&path).expect("a well-formed request");
        assert_eq!(parsed.note_id, note_id);
        assert_eq!(parsed.asset_id, asset_id);
        assert_eq!(parsed.format, ImageFormat::Png);

        // ...and it lands where it says it does.
        let assets = Path::new("/store/assets");
        assert_eq!(
            parsed.file_path(assets),
            assets
                .join(note_id.to_string())
                .join(format!("{asset_id}.png"))
        );
    }

    #[test]
    fn nothing_that_is_not_two_identifiers_is_a_request() {
        // Traversal, absolute paths, extra segments, missing ones, an
        // encoded separator, a scheme smuggled into the path: none of them
        // parse as a pair of `Uuid`s, so none of them names a file.
        let note = Uuid::new_v4();
        let asset = Uuid::new_v4();
        for hostile in [
            "/../../../etc/passwd".to_string(),
            "/../../config/config.toml".to_string(),
            format!("/{note}/../../../etc/passwd"),
            format!("/{note}/..%2f..%2fpasswd.png"),
            format!("/{note}/{asset}.png/extra"),
            format!("/{note}/{asset}.svg"),
            format!("/{note}/{asset}.php"),
            format!("/{note}/{asset}"),
            format!("/{note}"),
            format!("/{note}//{asset}.png"),
            "/etc/passwd".to_string(),
            "//etc/passwd".to_string(),
            String::new(),
            "/".to_string(),
            format!("/{note}/{asset}.png?x=/../y"),
        ] {
            assert!(
                parse_asset_request(&hostile).is_none(),
                "accepted {hostile:?}"
            );
        }
    }

    #[test]
    fn a_stored_reference_that_is_not_ours_is_left_alone() {
        let note = Uuid::new_v4();
        let asset = Uuid::new_v4();
        assert!(parse_stored_reference(&format!("../assets/{note}/{asset}.png")).is_some());

        for foreign in [
            "https://exemplo.com/a.png".to_string(),
            "/home/alguem/foto.png".to_string(),
            "foto.png".to_string(),
            "../../etc/passwd".to_string(),
            format!("../assets/../../{note}/{asset}.png"),
            format!("../assets/{note}/{asset}.svg"),
            format!("assets/{note}/{asset}.png"),
        ] {
            assert!(
                parse_stored_reference(&foreign).is_none(),
                "resolved {foreign:?}"
            );
        }
    }

    #[test]
    fn an_import_writes_one_file_under_the_notes_own_directory() {
        let tmp = tempdir().expect("tempdir");
        let assets = tmp.path().join("assets");
        let note_id = Uuid::new_v4();

        let asset = import_image(&assets, note_id, &png()).expect("import a PNG");
        let path = asset.file_path(&assets);

        assert!(path.is_file());
        assert_eq!(std::fs::read(&path).expect("read it back"), png());
        assert_eq!(path.parent().unwrap(), assets.join(note_id.to_string()));
        assert_eq!(asset.note_id, note_id);
        assert_eq!(asset.format, ImageFormat::Png);
        // The stored reference points at the file that was just written.
        assert_eq!(
            parse_stored_reference(&asset.relative_path()).map(|a| a.file_path(&assets)),
            Some(path)
        );
    }

    #[test]
    fn two_imports_of_the_same_picture_are_two_assets() {
        let tmp = tempdir().expect("tempdir");
        let assets = tmp.path().join("assets");
        let note_id = Uuid::new_v4();

        let first = import_image(&assets, note_id, &png()).expect("first");
        let second = import_image(&assets, note_id, &png()).expect("second");

        assert_ne!(first.asset_id, second.asset_id);
        assert!(first.file_path(&assets).is_file());
        assert!(second.file_path(&assets).is_file());
    }

    #[test]
    fn a_refused_import_leaves_nothing_behind() {
        // Not a directory, not an empty file, not a temp file: the decision
        // happens before anything is created.
        let tmp = tempdir().expect("tempdir");
        let assets = tmp.path().join("assets");
        let note_id = Uuid::new_v4();

        assert_eq!(
            import_image(&assets, note_id, b"<svg/>"),
            Err(ImportError::UnsupportedFormat)
        );
        assert_eq!(import_image(&assets, note_id, b""), Err(ImportError::Empty));
        assert!(!assets.exists(), "a refused import created {assets:?}");
    }

    #[test]
    fn bytes_survive_the_wire_exactly() {
        // Every byte value, so nothing about the encoding is lossy for the
        // binary a picture actually is.
        let original: Vec<u8> = (0..=255u8).collect();
        let encoded = {
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut out = String::new();
            for chunk in original.chunks(3) {
                let b = |i: usize| chunk.get(i).copied().unwrap_or(0) as usize;
                let n = (b(0) << 16) | (b(1) << 8) | b(2);
                out.push(ALPHABET[(n >> 18) & 63] as char);
                out.push(ALPHABET[(n >> 12) & 63] as char);
                out.push(if chunk.len() > 1 {
                    ALPHABET[(n >> 6) & 63] as char
                } else {
                    '='
                });
                out.push(if chunk.len() > 2 {
                    ALPHABET[n & 63] as char
                } else {
                    '='
                });
            }
            out
        };

        assert_eq!(decode_base64(&encoded).as_deref(), Some(&original[..]));
        assert_eq!(decode_base64(""), Some(Vec::new()));
        // A payload that arrived wrapped is still the same bytes.
        let wrapped = encoded
            .as_bytes()
            .chunks(60)
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(decode_base64(&wrapped).as_deref(), Some(&original[..]));
    }

    #[test]
    fn a_damaged_payload_is_refused_rather_than_half_decoded() {
        for broken in [
            "!!!!",
            "AB*D",
            "QUJD=QUJD",
            "QQ===",
            "QUJDR",
            "../../etc/passwd",
            "data:image/png;base64,QUJD",
        ] {
            assert!(decode_base64(broken).is_none(), "accepted {broken:?}");
        }
    }

    #[test]
    fn each_format_keeps_one_canonical_extension_and_mime_type() {
        for (format, extension, mime) in [
            (ImageFormat::Png, "png", "image/png"),
            (ImageFormat::Jpeg, "jpg", "image/jpeg"),
            (ImageFormat::Webp, "webp", "image/webp"),
            (ImageFormat::Gif, "gif", "image/gif"),
        ] {
            assert_eq!(format.extension(), extension);
            assert_eq!(format.mime_type(), mime);
            assert_eq!(ImageFormat::from_extension(extension), Some(format));
        }
        // `.jpeg` is read back as the same format, and written as `.jpg`.
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("JPEG"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("svg"), None);
        assert_eq!(ImageFormat::from_extension(""), None);
    }

    #[test]
    fn the_page_is_handed_a_scheme_and_never_a_filesystem_path() {
        let asset = AssetRef {
            note_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            asset_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            format: ImageFormat::Webp,
        };
        assert_eq!(
            asset.display_uri(),
            "note-it-asset:/11111111-1111-4111-8111-111111111111/22222222-2222-4222-8222-222222222222.webp"
        );
        assert!(!asset.display_uri().contains("file:"));
        assert!(!asset.display_uri().contains("home"));
        // And what the page asks for resolves back to the same asset.
        let requested = asset
            .display_uri()
            .strip_prefix(&format!("{ASSET_SCHEME}:"))
            .and_then(parse_asset_request);
        assert_eq!(requested, Some(asset));
    }
}
