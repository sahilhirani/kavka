//! Cooperative cancellation for long reads.
//!
//! One flag, shared by clone. The core blocks (docs/ARCHITECTURE.md), so a
//! fetch that has already started cannot be interrupted from outside — it can
//! only be *asked* to stop, and it notices between polls. That is enough for
//! every caller Kavka has: a browse is interactive, and the answer to a
//! question the user has already replaced is worth nothing.
//!
//! Deliberately not feature-gated. Phase 2's streaming search reuses this, and
//! the bare (no-`kafka`) tier has to keep compiling.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A "stop when you can" flag shared between the caller and a running read.
///
/// Cloning shares the flag; cancelling any clone cancels all of them.
/// `Relaxed` throughout: this orders nothing but itself, and a poll's worth of
/// latency is already the resolution of the whole mechanism.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks whoever holds this token to stop. Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Whether two handles are the same token rather than merely equal.
    ///
    /// A caller that keys tokens by profile has to be able to retire *its own*
    /// entry without stepping on the one that replaced it, and two fresh
    /// tokens are indistinguishable by value.
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_token_is_not_cancelled_and_cancelling_is_idempotent() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn clones_share_one_flag() {
        let token = CancelToken::new();
        let held_by_the_reader = token.clone();
        assert!(!held_by_the_reader.is_cancelled());
        token.cancel();
        assert!(held_by_the_reader.is_cancelled(), "the clone sees it");
    }

    #[test]
    fn identity_is_by_handle_not_by_value() {
        let token = CancelToken::new();
        let same = token.clone();
        let other = CancelToken::new();
        assert!(token.is_same(&same));
        assert!(
            !token.is_same(&other),
            "two fresh tokens are different reads"
        );
    }
}
