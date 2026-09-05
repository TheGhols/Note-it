//! One deterministic, documented hash, shared by everything that needs a
//! stable short identifier.
//!
//! Two things in Note-it name something by hashing it: the runtime
//! coordination directory for a store, and the optimistic reference to a task
//! inside a note. Both are compared across processes and across runs, so the
//! algorithm has to be a written-down contract rather than whatever the
//! standard library's default hasher happens to be this release —
//! `DefaultHasher` explicitly does not promise stability between versions, and
//! a task reference that changed meaning after a toolchain upgrade would be
//! worse than no reference at all.
//!
//! FNV-1a is the algorithm, in its 64-bit form, exactly as specified:
//!
//! ```text
//! hash = 0xcbf29ce484222325
//! for each byte:
//!     hash = hash XOR byte
//!     hash = hash * 0x00000100000001b3   (wrapping)
//! ```
//!
//! It is chosen because it is short enough to be reproduced from this comment
//! alone, needs no dependency, and is not seeded — the same bytes give the
//! same digest in every process, on every machine, forever.
//!
//! **It is not a cryptographic hash and nothing here treats it as one.** A
//! collision is never allowed to decide anything on its own: the task
//! reference is resolved by recomputing every candidate and refusing when more
//! than one matches, and the store key only names a directory whose contents
//! are checked separately.

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The 64-bit FNV-1a digest of `bytes`.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The digest as sixteen lowercase hexadecimal characters.
pub fn fnv1a_64_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a_64(bytes))
}

/// Feeds several pieces into one digest with an unambiguous separator.
///
/// Concatenating the parts directly would let two different tuples produce the
/// same bytes — `("ab", "c")` and `("a", "bc")` — so each part is preceded by
/// its length. Two different tuples therefore cannot be spelled the same way.
pub fn fnv1a_64_of_parts(parts: &[&[u8]]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for byte in (part.len() as u64).to_be_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

/// The SHA-256 digest of `bytes`, as the thirty-two bytes FIPS 180-4 defines.
///
/// FNV-1a above names things; this one *compares* them. A revision token says
/// "the note is still the one I read", and two different notes hashing the same
/// would let a stale write through — so the property that matters here is
/// collision resistance, which FNV-1a does not have and never claimed to.
///
/// It is written out rather than taken from a crate for the same reason FNV-1a
/// is: the algorithm is fully specified, it is a few dozen lines of integer
/// arithmetic with no state beyond its own buffers, and the published vectors
/// in the tests below prove it byte for byte. Nothing here is a secret, a key
/// or a signature — a revision is a change detector, so there is no timing or
/// side-channel surface to get wrong, and adding a dependency tree to compute
/// it would buy nothing this file cannot show.
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// One 64-byte block, folded into the state. FIPS 180-4 §6.2.2, unrolled.
///
/// The unrolling is not decoration and it is not premature: 4.3C hashes the
/// embedding artifact — half a gigabyte — every time the provider loads, so
/// this function's throughput became a budget. Written with the round variables
/// rotated by hand, the eight moves per round (`h = g; g = f; …`) disappear
/// into the naming, which measured 13% on the artifact and changes no output.
/// The arithmetic is the same arithmetic, and the published vectors in the
/// tests below still prove it byte for byte.
#[inline(always)]
fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    macro_rules! round {
        ($a:ident, $b:ident, $c:ident, $d:ident, $e:ident, $f:ident, $g:ident, $h:ident, $i:expr, $w:expr) => {{
            let s1 = $e.rotate_right(6) ^ $e.rotate_right(11) ^ $e.rotate_right(25);
            let ch = ($e & $f) ^ ((!$e) & $g);
            let temp1 = $h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[$i])
                .wrapping_add($w);
            let s0 = $a.rotate_right(2) ^ $a.rotate_right(13) ^ $a.rotate_right(22);
            let maj = ($a & $b) ^ ($a & $c) ^ ($b & $c);
            $d = $d.wrapping_add(temp1);
            $h = temp1.wrapping_add(s0.wrapping_add(maj));
        }};
    }

    let mut w = [0u32; 64];
    for (index, word) in block.as_chunks::<4>().0.iter().enumerate() {
        w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for index in 16..64 {
        let s0 =
            w[index - 15].rotate_right(7) ^ w[index - 15].rotate_right(18) ^ (w[index - 15] >> 3);
        let s1 =
            w[index - 2].rotate_right(17) ^ w[index - 2].rotate_right(19) ^ (w[index - 2] >> 10);
        w[index] = w[index - 16]
            .wrapping_add(s0)
            .wrapping_add(w[index - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    let mut index = 0;
    while index < 64 {
        round!(a, b, c, d, e, f, g, h, index, w[index]);
        round!(h, a, b, c, d, e, f, g, index + 1, w[index + 1]);
        round!(g, h, a, b, c, d, e, f, index + 2, w[index + 2]);
        round!(f, g, h, a, b, c, d, e, index + 3, w[index + 3]);
        round!(e, f, g, h, a, b, c, d, index + 4, w[index + 4]);
        round!(d, e, f, g, h, a, b, c, index + 5, w[index + 5]);
        round!(c, d, e, f, g, h, a, b, index + 6, w[index + 6]);
        round!(b, c, d, e, f, g, h, a, index + 7, w[index + 7]);
        index += 8;
    }

    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

fn sha256_digest(bytes: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // The input is read where it lies. The previous shape copied it whole in
    // order to append the padding, which for the embedding artifact meant a
    // second half-gigabyte allocation on every load — a cost that had nothing
    // to do with the digest.
    let (blocks, tail) = bytes.as_chunks::<64>();
    for block in blocks {
        sha256_compress(&mut state, block);
    }

    // The padding: the remaining bytes, a single one bit, zeroes, and the
    // length in bits as a 64-bit big-endian integer, landing on a 64-byte
    // boundary. It needs one block, or two when the tail leaves no room for
    // the length.
    let mut last = [0u8; 128];
    last[..tail.len()].copy_from_slice(tail);
    last[tail.len()] = 0x80;
    let padded = if tail.len() < 56 { 64 } else { 128 };
    last[padded - 8..padded].copy_from_slice(&(bytes.len() as u64).wrapping_mul(8).to_be_bytes());
    for block in last[..padded].as_chunks::<64>().0 {
        sha256_compress(&mut state, block);
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// The SHA-256 digest as sixty-four lowercase hexadecimal characters.
///
/// Lowercase is part of the published format: a revision travels through JSON
/// and a command line, and two spellings of one digest would compare unequal
/// for no reason a caller could see.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in sha256_digest(bytes) {
        use std::fmt::Write as _;
        // Writing into a String cannot fail; the digest is fixed width.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_fnv1a_vectors_still_hold() {
        // The reference vectors from the FNV specification. If any of these
        // moves, the algorithm changed and every stored expectation with it.
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_digest_is_stable_and_case_sensitive() {
        assert_eq!(fnv1a_64_hex(b"foobar"), "85944171f73967e8");
        assert_ne!(fnv1a_64(b"Tarefa"), fnv1a_64(b"tarefa"));
    }

    #[test]
    fn length_prefixing_keeps_different_tuples_apart() {
        // The whole point: without the length prefix these two would hash the
        // same bytes and a task reference could name the wrong task.
        assert_ne!(
            fnv1a_64_of_parts(&[b"ab", b"c"]),
            fnv1a_64_of_parts(&[b"a", b"bc"])
        );
        assert_eq!(
            fnv1a_64_of_parts(&[b"ab", b"c"]),
            fnv1a_64_of_parts(&[b"ab", b"c"])
        );
    }

    #[test]
    fn an_empty_part_is_not_the_same_as_no_part() {
        assert_ne!(fnv1a_64_of_parts(&[b"a", b""]), fnv1a_64_of_parts(&[b"a"]));
    }

    #[test]
    fn the_published_sha256_vectors_still_hold() {
        // FIPS 180-4 and the NIST examples. If any of these moves, the
        // implementation is wrong and every revision token with it.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_spans_the_padding_boundaries() {
        // 55, 56 and 64 bytes: the last message that fits in one block with its
        // length, the first that needs a second block, and an exact block. A
        // padding mistake shows up here and nowhere else.
        assert_eq!(
            sha256_hex(&b"a".repeat(55)),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            sha256_hex(&b"a".repeat(56)),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            sha256_hex(&b"a".repeat(64)),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
        assert_eq!(
            sha256_hex(&b"a".repeat(1_000_000)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_spans_the_second_padding_block() {
        // 4.3C rewrote the padding to fill a fixed 128-byte buffer instead of
        // copying the whole message, so the arithmetic that decides between one
        // padding block and two is new code. These lengths are every case it
        // has: a tail that leaves room for the length (63 → 55 spare), one that
        // does not (120 → 56 spare), and the two exact multiples around them.
        // The values come from an independent SHA-256, not from this one.
        for (length, expected) in [
            (
                63usize,
                "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
            ),
            (
                119,
                "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb",
            ),
            (
                120,
                "2f3d335432c70b580af0e8e1b3674a7c020d683aa5f73aaaedfdc55af904c21c",
            ),
            (
                127,
                "c57e9278af78fa3cab38667bef4ce29d783787a2f731d4e12200270f0c32320a",
            ),
            (
                128,
                "6836cf13bac400e9105071cd6af47084dfacad4e5e302c94bfed24e013afb73e",
            ),
            (
                129,
                "c12cb024a2e5551cca0e08fce8f1c5e314555cc3fef6329ee994a3db752166ae",
            ),
        ] {
            assert_eq!(
                sha256_hex(&b"a".repeat(length)),
                expected,
                "the digest of {length} bytes moved"
            );
        }
    }

    #[test]
    fn sha256_is_sixty_four_lowercase_hex_characters() {
        let digest = sha256_hex(b"note");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(digest, digest.to_lowercase());
    }

    #[test]
    fn sha256_separates_inputs_that_fnv_would_be_trusted_less_for() {
        assert_ne!(sha256_hex(b"SHARED-BASE"), sha256_hex(b"SHARED-BASE\n"));
        assert_eq!(sha256_hex(b"same"), sha256_hex(b"same"));
    }
}
