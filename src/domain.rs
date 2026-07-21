//! Domain-layer composition over the storage primitives.
//!
//! The storage interfaces expose only single-query primitives (`get`/`insert`/`delete`/
//! `update`) so they map 1:1 onto a future KV backend. Multi-step logic — most notably
//! "insert, or read back the existing row on a duplicate-key conflict" — is sequenced
//! here, one function per table, by calling those primitives in turn.

pub mod fingerprint;
pub mod hash;
pub mod locker;
pub mod merchant;
pub mod vault;

pub(crate) fn record_get_or_insert_outcome(
    resource: crate::observability::metrics::Resource,
    outcome: crate::observability::metrics::DomainGetOrInsertOutcome,
) {
    crate::observability::metrics::DOMAIN_GET_OR_INSERT_COUNT.add(
        1,
        crate::metric_attributes!(("resource", resource), ("outcome", outcome)),
    );
}
