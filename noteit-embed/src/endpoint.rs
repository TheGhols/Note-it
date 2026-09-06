//! Where a request may go, decided at compile time.
//!
//! **This module is the SSRF answer.** The protocol carries a `ProviderId`
//! enum and never a URL (`noteit-embed-protocol`), and this is the other half
//! of that sentence: the set of hosts this process can reach is the set of
//! constants below. There is no configuration key, no environment variable and
//! no request field that adds one to it, so "a caller cannot make the worker
//! fetch `http://169.254.169.254/`" is a property of the type system rather
//! than of a filter somebody has to keep correct.
//!
//! Every URL here is `https`. That is asserted by a test rather than left to
//! reading, because a single character is the difference between a request
//! with TLS and a credential sent in the clear.

use noteit_embed_protocol::ProviderId;

/// OpenAI, verified 2026-09-06 at `developers.openai.com/api/docs/guides/embeddings`.
pub const OPENAI_BASE: &str = "https://api.openai.com";

/// Google Gemini, verified 2026-09-06 at `ai.google.dev/gemini-api/docs/embeddings`
/// and `ai.google.dev/api/embeddings`.
pub const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com";

/// Voyage AI, verified 2026-09-06 at `docs.voyageai.com/reference/embeddings-api`.
pub const VOYAGE_BASE: &str = "https://api.voyageai.com";

/// The base URL a provider's requests are built from.
///
/// A function and not a map lookup, so adding a provider is a compile error
/// somewhere rather than a `None` at run time.
pub const fn pinned_base(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::OpenAi => OPENAI_BASE,
        ProviderId::Gemini => GEMINI_BASE,
        ProviderId::Voyage => VOYAGE_BASE,
    }
}

/// The base URL this process will actually use for a provider.
///
/// In a shipped build this is [`pinned_base`] and there is no other branch:
/// the override below is compiled out entirely, so the released binary has no
/// code path that reads a base URL from anywhere.
///
/// With `--features test-endpoints` it may be redirected to a loopback mock,
/// which is how the adapters are tested against 401, 429, truncated JSON and a
/// vector full of `NaN` without an account, a key or a network
/// (§47, §49). `scripts/check-embed-boundary` fails the build if that feature
/// is ever defaulted on.
pub fn base_for(provider: ProviderId) -> String {
    #[cfg(feature = "test-endpoints")]
    {
        if let Some(base) = test_override::get(provider) {
            return base;
        }
    }
    pinned_base(provider).to_string()
}

/// The redirection used by the suite, and by nothing else.
///
/// Two layers, because the two callers need different scopes. The suite runs
/// its tests in parallel threads and each one points a provider at *its own*
/// mock, so a process-wide slot would have them overwriting each other — the
/// thread-local is what makes those tests independent. The worker binary sets
/// the process-wide slot once, on the main thread, and serves each connection
/// on a thread of its own, so a thread-local alone would be invisible where it
/// matters.
#[cfg(feature = "test-endpoints")]
pub mod test_override {
    use noteit_embed_protocol::ProviderId;
    use std::cell::RefCell;
    use std::sync::{Mutex, OnceLock};

    const fn slot(provider: ProviderId) -> usize {
        match provider {
            ProviderId::OpenAi => 0,
            ProviderId::Gemini => 1,
            ProviderId::Voyage => 2,
        }
    }

    thread_local! {
        static LOCAL: RefCell<[Option<String>; 3]> = const { RefCell::new([None, None, None]) };
    }

    fn shared() -> &'static Mutex<[Option<String>; 3]> {
        static SHARED: OnceLock<Mutex<[Option<String>; 3]>> = OnceLock::new();
        SHARED.get_or_init(|| Mutex::new([None, None, None]))
    }

    /// Points one provider at a base URL, for this thread only.
    pub fn set(provider: ProviderId, base: impl Into<String>) {
        let base = base.into();
        LOCAL.with(|slots| slots.borrow_mut()[slot(provider)] = Some(base));
    }

    /// Points one provider at a base URL for every thread of this process.
    /// Used by the worker binary, which serves on threads it spawns later.
    pub fn set_for_process(provider: ProviderId, base: impl Into<String>) {
        let mut guard = shared().lock().expect("the override lock");
        guard[slot(provider)] = Some(base.into());
    }

    pub fn get(provider: ProviderId) -> Option<String> {
        if let Some(base) = LOCAL.with(|slots| slots.borrow()[slot(provider)].clone()) {
            return Some(base);
        }
        shared().lock().ok()?[slot(provider)].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_endpoint_is_https() {
        for provider in ProviderId::ALL {
            let base = pinned_base(provider);
            assert!(
                base.starts_with("https://"),
                "{provider} is pinned to {base}, which is not TLS"
            );
        }
    }

    #[test]
    fn every_pinned_endpoint_is_a_bare_origin() {
        // No path, no query, no fragment, no credentials, no port. Each
        // adapter appends its own path to this, and a base that already
        // carried one would make that concatenation ambiguous.
        for provider in ProviderId::ALL {
            let base = pinned_base(provider);
            let host = base.trim_start_matches("https://");
            assert!(!host.is_empty());
            assert!(
                !host.contains('/') && !host.contains('?') && !host.contains('#'),
                "{provider}'s base is not a bare origin: {base}"
            );
            assert!(!host.contains('@'), "{provider}'s base carries userinfo");
            assert!(!host.contains(':'), "{provider}'s base names a port");
        }
    }

    #[test]
    fn the_three_pinned_hosts_are_the_three_documented_ones() {
        // Written out rather than derived, so that changing one is a diff a
        // reviewer sees rather than a constant that moved.
        assert_eq!(pinned_base(ProviderId::OpenAi), "https://api.openai.com");
        assert_eq!(
            pinned_base(ProviderId::Gemini),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(pinned_base(ProviderId::Voyage), "https://api.voyageai.com");
    }

    #[test]
    fn the_pinned_base_is_what_a_shipped_build_uses() {
        // With the test feature off — which is every released build — there is
        // no branch here at all. With it on, nothing has been set yet in this
        // process unless a test set it, and the tests that do use their own
        // provider slots.
        #[cfg(not(feature = "test-endpoints"))]
        for provider in ProviderId::ALL {
            assert_eq!(base_for(provider), pinned_base(provider));
        }
    }
}
