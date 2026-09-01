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
}
