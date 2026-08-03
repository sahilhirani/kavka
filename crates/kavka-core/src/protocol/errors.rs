//! Broker error codes, mapped into the vocabulary users already see.
//!
//! Two rules decide everything here.
//!
//! **The broker's own name for the failure is kept.** `apps/desktop/src/errors.ts`
//! (docs/DESIGN.md §7) classifies a raw string into a plain-language title, and
//! several of its rows key off exactly these tokens — `cluster_authorization`,
//! `authentication failed`, `not authorized`. So a code becomes
//! `CLUSTER_AUTHORIZATION_FAILED (31): <broker's message>`: the screaming-snake
//! token is what the classifier and a web search both recognise, and the
//! broker's own sentence follows it whenever the broker sent one.
//!
//! **Some codes are not failures.** A preferred-leader election on a partition
//! that already has its preferred leader answers ELECTION_NOT_NEEDED, and a
//! cancel on a partition that is not being reassigned answers
//! NO_REASSIGNMENT_IN_PROGRESS. Both mean *the cluster is already in the state
//! you asked for*, which is a successful outcome dressed as an error code —
//! reporting them as failures would make a "make every leader preferred" button
//! on a healthy cluster light up entirely red. They are the [`BENIGN`] set.

/// Codes that mean "already in the requested state", not "the request failed".
///
/// Deliberately a small, named set rather than a per-call judgement: the rule
/// is "the end state you asked for already holds", and anything else — even
/// something as mild as PREFERRED_LEADER_NOT_AVAILABLE — is a real result the
/// user has to see.
pub(crate) const BENIGN: &[i16] = &[ELECTION_NOT_NEEDED, NO_REASSIGNMENT_IN_PROGRESS];

pub(crate) const NONE: i16 = 0;
pub(crate) const UNSUPPORTED_VERSION: i16 = 35;
pub(crate) const NOT_CONTROLLER: i16 = 41;
pub(crate) const NOT_LEADER_OR_FOLLOWER: i16 = 6;
pub(crate) const ELECTION_NOT_NEEDED: i16 = 84;
pub(crate) const NO_REASSIGNMENT_IN_PROGRESS: i16 = 85;

/// The codes these six APIs (and the handshake in front of them) can actually
/// produce, plus the handful every API shares. Anything outside the table is
/// still reported — by number — rather than swallowed.
const NAMES: &[(i16, &str)] = &[
    (-1, "UNKNOWN_SERVER_ERROR"),
    (0, "NONE"),
    (3, "UNKNOWN_TOPIC_OR_PARTITION"),
    (5, "LEADER_NOT_AVAILABLE"),
    (6, "NOT_LEADER_OR_FOLLOWER"),
    (7, "REQUEST_TIMED_OUT"),
    (8, "BROKER_NOT_AVAILABLE"),
    (9, "REPLICA_NOT_AVAILABLE"),
    (13, "NETWORK_EXCEPTION"),
    (17, "INVALID_TOPIC_EXCEPTION"),
    (29, "TOPIC_AUTHORIZATION_FAILED"),
    (31, "CLUSTER_AUTHORIZATION_FAILED"),
    (33, "UNSUPPORTED_SASL_MECHANISM"),
    (34, "ILLEGAL_SASL_STATE"),
    (35, "UNSUPPORTED_VERSION"),
    (37, "INVALID_PARTITIONS"),
    (38, "INVALID_REPLICATION_FACTOR"),
    (39, "INVALID_REPLICA_ASSIGNMENT"),
    (40, "INVALID_CONFIG"),
    (41, "NOT_CONTROLLER"),
    (42, "INVALID_REQUEST"),
    (44, "POLICY_VIOLATION"),
    (54, "SECURITY_DISABLED"),
    (56, "KAFKA_STORAGE_ERROR"),
    (58, "SASL_AUTHENTICATION_FAILED"),
    (60, "REASSIGNMENT_IN_PROGRESS"),
    (72, "LISTENER_NOT_FOUND"),
    (80, "PREFERRED_LEADER_NOT_AVAILABLE"),
    (83, "ELIGIBLE_LEADERS_NOT_AVAILABLE"),
    (84, "ELECTION_NOT_NEEDED"),
    (85, "NO_REASSIGNMENT_IN_PROGRESS"),
    (89, "THROTTLING_QUOTA_EXCEEDED"),
    (91, "RESOURCE_NOT_FOUND"),
    (93, "UNACCEPTABLE_CREDENTIAL"),
    (100, "UNKNOWN_TOPIC_ID"),
    (102, "BROKER_ID_NOT_REGISTERED"),
    (107, "INELIGIBLE_REPLICA"),
    (108, "NEW_LEADER_ELECTED"),
    (116, "UNKNOWN_CONTROLLER_ID"),
];

/// Kafka's own name for a code, or a synthetic one carrying the number so an
/// unrecognised code is still searchable rather than anonymous.
pub(crate) fn name(code: i16) -> String {
    NAMES.iter().find(|(value, _)| *value == code).map_or_else(
        || format!("BROKER_ERROR_{code}"),
        |(_, name)| (*name).to_string(),
    )
}

/// Whether this code means the requested end state already holds — see
/// [`BENIGN`].
pub(crate) fn is_benign(code: i16) -> bool {
    BENIGN.contains(&code)
}

/// One error code, plus the broker's own message when it sent one, as a single
/// user-facing line.
///
/// POLICY_VIOLATION is the case this shape exists for: the code alone says
/// nothing, and the message ("Topic replication factor must be 3") is the whole
/// answer — so the message is never dropped, only ever appended.
pub(crate) fn describe(code: i16, message: Option<&str>) -> String {
    let head = format!("{} ({code})", name(code));
    match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(detail) => format!("{head}: {detail}"),
        None => head,
    }
}

/// `Ok(())` for success and for the benign codes; a described error otherwise.
/// `what` names the operation as the user would.
pub(crate) fn check(what: &str, code: i16, message: Option<&str>) -> crate::Result<()> {
    if code == NONE || is_benign(code) {
        return Ok(());
    }
    Err(crate::Error::Other(format!(
        "{what} failed: {}",
        describe(code, message)
    )))
}

/// The per-partition half of the same judgement: `None` when there is nothing
/// to report, so a benign code reaches the UI as a plain success.
pub(crate) fn partition_error(code: i16, message: Option<&str>) -> Option<String> {
    if code == NONE || is_benign(code) {
        return None;
    }
    Some(describe(code, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_described_error_leads_with_kafkas_own_token() {
        // The token the desktop classifier (§7's authorization row) keys off.
        assert_eq!(describe(31, None), "CLUSTER_AUTHORIZATION_FAILED (31)");
        assert!(describe(31, None)
            .to_lowercase()
            .contains("cluster_authorization"));
    }

    /// POLICY_VIOLATION without its message is useless, so the message is
    /// quoted verbatim rather than summarised.
    #[test]
    fn a_policy_violation_quotes_the_brokers_own_sentence() {
        assert_eq!(
            describe(44, Some("Topic replication factor must be 3")),
            "POLICY_VIOLATION (44): Topic replication factor must be 3"
        );
    }

    #[test]
    fn an_unknown_code_still_carries_its_number() {
        assert_eq!(name(9_999), "BROKER_ERROR_9999");
        assert!(describe(9_999, Some("what")).contains("9999"));
    }

    #[test]
    fn empty_broker_messages_do_not_leave_a_dangling_colon() {
        assert_eq!(describe(60, Some("")), "REASSIGNMENT_IN_PROGRESS (60)");
        assert_eq!(describe(60, Some("   ")), "REASSIGNMENT_IN_PROGRESS (60)");
        assert_eq!(describe(60, None), "REASSIGNMENT_IN_PROGRESS (60)");
    }

    /// The whole point of the benign set: a single-broker cluster answers
    /// ELECTION_NOT_NEEDED for every partition, and that is a healthy cluster,
    /// not six failures.
    #[test]
    fn already_in_the_requested_state_is_not_a_failure() {
        assert_eq!(partition_error(ELECTION_NOT_NEEDED, None), None);
        assert_eq!(partition_error(NO_REASSIGNMENT_IN_PROGRESS, None), None);
        assert_eq!(partition_error(NONE, None), None);
        assert!(check("electing leaders", ELECTION_NOT_NEEDED, None).is_ok());
    }

    #[test]
    fn a_real_failure_names_the_operation_and_the_code() {
        let err = check("altering reassignments", 60, Some("in progress"))
            .expect_err("REASSIGNMENT_IN_PROGRESS is not benign");
        let message = err.to_string();
        assert!(
            message.contains("altering reassignments failed"),
            "{message}"
        );
        assert!(
            message.contains("REASSIGNMENT_IN_PROGRESS (60)"),
            "{message}"
        );
        assert!(message.contains("in progress"), "{message}");

        // Not benign: the leader is unavailable, which the user must see.
        assert!(partition_error(80, None).is_some());
    }

    #[test]
    fn the_codes_this_module_branches_on_are_the_ones_kafka_defines() {
        for (constant, expected) in [
            (NONE, "NONE"),
            (NOT_LEADER_OR_FOLLOWER, "NOT_LEADER_OR_FOLLOWER"),
            (UNSUPPORTED_VERSION, "UNSUPPORTED_VERSION"),
            (NOT_CONTROLLER, "NOT_CONTROLLER"),
            (ELECTION_NOT_NEEDED, "ELECTION_NOT_NEEDED"),
            (NO_REASSIGNMENT_IN_PROGRESS, "NO_REASSIGNMENT_IN_PROGRESS"),
        ] {
            assert_eq!(name(constant), expected);
        }
    }
}
