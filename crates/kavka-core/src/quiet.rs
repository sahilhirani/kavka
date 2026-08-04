//! The clock that decides when a scan has read everything readable.
//!
//! Every engine in this crate that reads a bounded window of a topic —
//! [`crate::search`], [`crate::sql`], [`crate::xcluster`] — stops on whichever
//! comes first: every partition reaching the end watermark captured at the
//! start, a ceiling the caller set, a cancel, or **the source going quiet**.
//!
//! The watermarks are the primary signal and they are not sufficient on their
//! own. A transactional topic's commit markers and aborted records occupy
//! offsets a consumer is never given, so the cursor cannot reach the watermark
//! and a watermark-only loop would run until the user gave up. The quiet
//! deadline is the backstop that makes "never hang on a quiet topic" true.
//!
//! Because it is a backstop for a case that is indistinguishable from a broker
//! that stopped answering half way through, everything about it has to be
//! measured honestly — this module exists so that "how long has the SOURCE been
//! quiet" is a value with one definition rather than a `quiet_deadline` local
//! re-derived, slightly differently, in each engine.
//!
//! # Two tiers, and why the first one is the long one
//!
//! [`QUIET_BEFORE_DATA`] covers the wait for the *first* record and
//! [`QUIET_AFTER_DATA`] the wait between records once they are flowing. They are
//! different budgets because they measure different things. Once records are
//! arriving, the broker has been proved reachable and a five-second gap really
//! is the source going quiet. Before the first one, nothing has been proved:
//! the budget has to cover the client's connect, its metadata round trip, the
//! coordinator lookup that a `group.id` client makes even when it assigns its
//! partitions by hand, and only then the first fetch — on a cluster that may
//! itself have started seconds ago.
//!
//! The cold-start budget is therefore derived rather than picked:
//! `METADATA_TIMEOUT + QUIET_AFTER_DATA`. A scan that gave the whole of connect,
//! metadata *and* first fetch the same ten seconds it separately allows the
//! metadata call alone has left nothing for the fetch, and on a loaded machine
//! it loses the race — which is not a hypothetical. It is the flake this module
//! was extracted to fix: a cold, fully loaded CI runner delivered the scan's
//! first record later than its whole cold-start budget, so a six-record topic
//! was scanned as zero records and an aggregate over it answered `0`.
//!
//! # The three waits that are not the source
//!
//! A single deadline spanning the whole loop measures the loop, not the source.
//! Several waits run through these engines that are nobody's silence: a rate
//! limiter's pacing gap, the in-flight drain that lets delivery reports come
//! back, and a decode, which can sit on a Schema Registry HTTP call for that
//! call's whole timeout — [`crate::sr`]'s 10s, *twice* [`QUIET_AFTER_DATA`], for
//! a single record. Spend the source's budget on any of those and the scan stops
//! early, and then reports a partial answer as if the topic had gone silent.
//!
//! So the deadline is re-armed on every edge: when a record arrives (the source
//! spoke), after every wait spent on the destination, and after every decode.
//! What is left measures exactly what it claims to.
//!
//! Pure over a clock it is handed, and outside the `kafka` gate, so all of this
//! arithmetic is a unit test rather than a five-second sleep.

use std::time::Duration;
use std::time::Instant;

/// The ceiling on a single metadata round trip, mirrored from the engines that
/// set it on their own clients. Named here because [`QUIET_BEFORE_DATA`] is
/// derived from it and the derivation is the justification.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
pub(crate) const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a scan waits for its *first* record before concluding there is
/// nothing to read.
///
/// Generous, and deliberately larger than [`METADATA_TIMEOUT`]: it covers the
/// client's connect, its metadata round trip **and** the first fetch on a cold
/// cluster, and the metadata call alone is already allowed the whole of
/// [`METADATA_TIMEOUT`]. See the module docs.
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
pub(crate) const QUIET_BEFORE_DATA: Duration = METADATA_TIMEOUT.saturating_add(QUIET_AFTER_DATA);

/// How long a scan waits between records once data is flowing.
///
/// Longer than [`crate::consume`]'s equivalent because a scan is not an
/// interactive fetch — it is expected to run for minutes, and a slow broker
/// mid-scan must not read as "finished".
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
pub(crate) const QUIET_AFTER_DATA: Duration = Duration::from_secs(5);

/// **How long the SOURCE has been quiet** — and nothing else.
///
/// Constructed cold ([`SourceSilence::waiting_for_first`]), re-armed on each of
/// the edges the module docs name, and asked one question
/// ([`SourceSilence::expired`]). The three re-arming methods are identical
/// arithmetic under three names, deliberately: they state three separate facts,
/// and a reader has to be able to tell which one a call site is claiming.
#[derive(Debug)]
#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
pub(crate) struct SourceSilence {
    /// The budget every re-arm grants. Carried rather than read from the
    /// constant so an engine with its own pacing — [`crate::consume`]'s
    /// interactive fetch — can share this type instead of re-deriving it.
    after: Duration,
    deadline: Instant,
}

#[cfg_attr(not(feature = "kafka"), allow(dead_code))]
impl SourceSilence {
    /// Before the first record: the long, cold-cluster budget.
    pub(crate) fn waiting_for_first(now: Instant) -> Self {
        Self::waiting_for_first_with(now, QUIET_BEFORE_DATA, QUIET_AFTER_DATA)
    }

    /// The same clock on budgets the caller chooses, for an engine whose
    /// contract sets a different pace. The tiering is the part that matters and
    /// it is preserved: `before` is still spent only on the first record.
    pub(crate) fn waiting_for_first_with(now: Instant, before: Duration, after: Duration) -> Self {
        Self {
            after,
            deadline: now + before,
        }
    }

    /// The source spoke. The budget starts again from `now`.
    pub(crate) fn heard_from_source(&mut self, now: Instant) {
        self.deadline = now + self.after;
    }

    /// Time spent on the destination is not the source being quiet.
    pub(crate) fn waited_on_destination(&mut self, now: Instant) {
        self.deadline = now + self.after;
    }

    /// Time spent decoding a record — which can reach a Schema Registry over
    /// HTTP — is not the source being quiet either.
    pub(crate) fn waited_on_decode(&mut self, now: Instant) {
        self.deadline = now + self.after;
    }

    pub(crate) fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

/// The deadline that decides when a scan has read everything readable. Its
/// arithmetic is the difference between an honest partial answer and a
/// confidently wrong one, so it is tested over a handed clock rather than by
/// sleeping.
#[cfg(test)]
mod tests {
    use super::*;

    /// The cold-start budget has to be strictly longer than the metadata round
    /// trip it contains, or a scan that spent its whole allowance getting
    /// metadata has nothing left to fetch with. This is the flake that put this
    /// module here, expressed as arithmetic.
    #[test]
    fn the_cold_start_budget_outlasts_a_metadata_round_trip() {
        assert!(
            QUIET_BEFORE_DATA > METADATA_TIMEOUT,
            "connect + metadata + first fetch cannot fit in the metadata call's own budget"
        );
        assert!(
            QUIET_BEFORE_DATA > QUIET_AFTER_DATA,
            "the first record gets a longer budget than the ones after it"
        );
    }

    #[test]
    fn the_first_record_gets_the_long_cold_start_budget() {
        let start = Instant::now();
        let silence = SourceSilence::waiting_for_first(start);
        assert!(!silence.expired(start + QUIET_BEFORE_DATA - Duration::from_millis(1)));
        assert!(silence.expired(start + QUIET_BEFORE_DATA));
    }

    #[test]
    fn a_record_restarts_the_budget_from_when_it_arrived() {
        let start = Instant::now();
        let mut silence = SourceSilence::waiting_for_first(start);
        // Late in the cold-start budget, so the after-data budget granted from
        // here outlasts it — which is what makes the next assertion mean
        // "the cold-start deadline is GONE" rather than "it has not arrived
        // yet". Expressed against the constants so it keeps saying that if they
        // ever move again.
        let arrived = start + QUIET_BEFORE_DATA - Duration::from_secs(1);
        silence.heard_from_source(arrived);
        // The cold-start deadline is gone; the after-data one runs from here.
        assert!(!silence.expired(start + QUIET_BEFORE_DATA));
        assert!(!silence.expired(arrived + QUIET_AFTER_DATA - Duration::from_millis(1)));
        assert!(silence.expired(arrived + QUIET_AFTER_DATA));
    }

    /// THE BUG THIS TYPE EXISTS FOR. A copy paced at one record per second, or
    /// one whose destination sits in the in-flight drain, spends real time
    /// between polls — and none of it is the source going quiet. Without the
    /// re-arm the budget runs out mid-copy and every unread partition is
    /// silently marked finished.
    #[test]
    fn waiting_on_the_destination_never_spends_the_sources_budget() {
        let start = Instant::now();
        let mut silence = SourceSilence::waiting_for_first(start);
        let mut now = start;

        // Twenty records, each of them a full QUIET_AFTER_DATA + change spent
        // pacing and draining — four times the whole budget, twenty times over.
        for _ in 0..20 {
            silence.heard_from_source(now);
            now += QUIET_AFTER_DATA * 4;
            silence.waited_on_destination(now);
            assert!(
                !silence.expired(now),
                "a wait on the destination expired the source's deadline"
            );
        }

        // And the source genuinely going quiet still ends it, on schedule.
        assert!(!silence.expired(now + QUIET_AFTER_DATA - Duration::from_millis(1)));
        assert!(silence.expired(now + QUIET_AFTER_DATA));
    }

    /// THE SAME BUG, ON THE THIRD EDGE. A CEL filter — or a `value_text` column
    /// — decodes through the schema registry, and one lookup there is bounded by
    /// an HTTP timeout of 10s, twice the whole budget for a single record. Worse
    /// than the destination case: a record the filter rejects continues straight
    /// back to the expiry check with no pacing gap and no drain in between, so
    /// nothing else on the loop would ever re-arm the deadline.
    #[test]
    fn a_slow_decode_never_spends_the_sources_budget() {
        let start = Instant::now();
        let mut silence = SourceSilence::waiting_for_first(start);
        let mut now = start;

        // Ten records in a row that the filter throws away, each one having sat
        // on a registry lookup for its full timeout.
        for _ in 0..10 {
            silence.heard_from_source(now);
            now += Duration::from_secs(10);
            silence.waited_on_decode(now);
            assert!(
                !silence.expired(now),
                "a decode expired the source's deadline"
            );
        }

        // And the two kinds of wait compose: a record that decoded slowly and
        // was then paced slowly is still not the source going quiet.
        silence.heard_from_source(now);
        now += Duration::from_secs(10);
        silence.waited_on_decode(now);
        now += QUIET_AFTER_DATA * 4;
        silence.waited_on_destination(now);
        assert!(!silence.expired(now));

        // The source genuinely going quiet still ends it, on schedule.
        assert!(!silence.expired(now + QUIET_AFTER_DATA - Duration::from_millis(1)));
        assert!(silence.expired(now + QUIET_AFTER_DATA));
    }

    /// An engine with its own pace still gets the tiering, which is the part
    /// that stops a cold client being read as a quiet topic.
    #[test]
    fn a_caller_chosen_pace_keeps_both_tiers() {
        let start = Instant::now();
        let before = Duration::from_secs(9);
        let after = Duration::from_millis(1_500);
        let mut silence = SourceSilence::waiting_for_first_with(start, before, after);
        assert!(!silence.expired(start + before - Duration::from_millis(1)));
        assert!(silence.expired(start + before));

        let arrived = start + Duration::from_secs(1);
        silence.heard_from_source(arrived);
        assert!(!silence.expired(arrived + after - Duration::from_millis(1)));
        assert!(silence.expired(arrived + after));
    }
}
