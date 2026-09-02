//! Programmatic tests for R-011: Guarding the protocol timeout budget hierarchy against drift.
//!
//! Note-it coordinates write operations across CLI clients and running Desktop instances
//! through a strictly ordered timeout budget hierarchy:
//!
//! 4s freeze (editor flush) <= 4s ack (document refresh) < 15s CLI authority < 30s desktop worker
//!
//! These tests ensure that no refactoring or modification can silently invert or disrupt
//! the causal mathematical relationships between these timeouts.

use noteit_core::coordination::{
    PROTOCOL_ACK_TIMEOUT, PROTOCOL_CLI_AUTHORITY_TIMEOUT, PROTOCOL_DESKTOP_WORKER_TIMEOUT,
    PROTOCOL_FREEZE_TIMEOUT,
};
use std::time::Duration;

#[test]
fn r011_1_mathematical_ordering_of_timeout_budget_hierarchy() {
    // 1. Freeze timeout must not exceed ACK timeout
    assert!(
        PROTOCOL_FREEZE_TIMEOUT <= PROTOCOL_ACK_TIMEOUT,
        "Freeze timeout ({:?}) must be <= ACK timeout ({:?})",
        PROTOCOL_FREEZE_TIMEOUT,
        PROTOCOL_ACK_TIMEOUT
    );

    // 2. ACK timeout must be strictly less than CLI authority timeout
    assert!(
        PROTOCOL_ACK_TIMEOUT < PROTOCOL_CLI_AUTHORITY_TIMEOUT,
        "ACK timeout ({:?}) must be strictly less than CLI authority timeout ({:?})",
        PROTOCOL_ACK_TIMEOUT,
        PROTOCOL_CLI_AUTHORITY_TIMEOUT
    );

    // 3. CLI authority timeout must be strictly less than desktop worker timeout
    assert!(
        PROTOCOL_CLI_AUTHORITY_TIMEOUT < PROTOCOL_DESKTOP_WORKER_TIMEOUT,
        "CLI authority timeout ({:?}) must be strictly less than desktop worker timeout ({:?})",
        PROTOCOL_CLI_AUTHORITY_TIMEOUT,
        PROTOCOL_DESKTOP_WORKER_TIMEOUT
    );
}

#[test]
fn r011_2_causal_headroom_margin_invariant() {
    // In order for a write mutation to complete deterministically without triggering
    // a spurious CLI timeout, the sum of worst-case editor freeze and window ACK
    // MUST fit comfortably within the CLI authority timeout with a safety margin.
    let combined_client_window = PROTOCOL_FREEZE_TIMEOUT + PROTOCOL_ACK_TIMEOUT;
    let margin = Duration::from_secs(5);

    assert!(
        combined_client_window + margin <= PROTOCOL_CLI_AUTHORITY_TIMEOUT,
        "Combined freeze ({:?}) + ack ({:?}) + safety margin ({:?}) = {:?} must fit within CLI authority timeout ({:?})",
        PROTOCOL_FREEZE_TIMEOUT,
        PROTOCOL_ACK_TIMEOUT,
        margin,
        combined_client_window + margin,
        PROTOCOL_CLI_AUTHORITY_TIMEOUT
    );
}

#[test]
fn r011_3_timeout_exact_and_sanity_bounds() {
    // Exact baseline values mandated by protocol specification
    assert_eq!(PROTOCOL_FREEZE_TIMEOUT, Duration::from_millis(4000));
    assert_eq!(PROTOCOL_ACK_TIMEOUT, Duration::from_millis(4000));
    assert_eq!(PROTOCOL_CLI_AUTHORITY_TIMEOUT, Duration::from_secs(15));
    assert_eq!(PROTOCOL_DESKTOP_WORKER_TIMEOUT, Duration::from_secs(30));

    // Absolute sanity bounds: no timeout reduced to unusable fragility or expanded to freeze the UI
    assert!(PROTOCOL_FREEZE_TIMEOUT >= Duration::from_millis(2000));
    assert!(PROTOCOL_FREEZE_TIMEOUT <= Duration::from_millis(8000));

    assert!(PROTOCOL_ACK_TIMEOUT >= Duration::from_millis(2000));
    assert!(PROTOCOL_ACK_TIMEOUT <= Duration::from_millis(8000));

    assert!(PROTOCOL_CLI_AUTHORITY_TIMEOUT >= Duration::from_secs(10));
    assert!(PROTOCOL_CLI_AUTHORITY_TIMEOUT <= Duration::from_secs(25));

    assert!(PROTOCOL_DESKTOP_WORKER_TIMEOUT >= Duration::from_secs(20));
    assert!(PROTOCOL_DESKTOP_WORKER_TIMEOUT <= Duration::from_secs(60));
}
