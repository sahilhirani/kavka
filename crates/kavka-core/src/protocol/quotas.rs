//! DescribeClientQuotas (48) and AlterClientQuotas (49).
//!
//! Version 1 of each — the first FLEXIBLE version (Kafka 2.7), and the one the
//! integration suite exercises. v0 exists (Kafka 2.6) and is not implemented:
//! see the module docs on [`super`] for why a legacy encoder no test can reach
//! is worse than a refusal that names the API and both version windows.
//!
//! THE `<default>` ENTITY. A quota entity is a list of parts — `user`,
//! `client-id`, `ip` — and each part either names something or is Kafka's
//! `<default>`, the fallback that applies to everyone the named entries miss.
//! On the wire that distinction is a nullable string: `null` IS `<default>`.
//! The IPC contract carries it the same way (`name: str | null`), so a UI
//! showing "<default>" and a UI showing an empty box are the same bug, and the
//! type will not let either of them mean "no entity".

use super::conn::{BrokerConnection, ALTER_CLIENT_QUOTAS, DESCRIBE_CLIENT_QUOTAS};
use super::errors;
use super::wire::{Decoder, Encoder};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// The entity types Kafka quotas are keyed on. A typo here is worth catching
/// before the round trip, which is what [`validate_entity`] does.
const ENTITY_TYPES: [&str; 3] = ["user", "client-id", "ip"];

/// One component of a quota entity. `name: None` is Kafka's `<default>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaEntityPart {
    pub entity_type: String,
    pub name: Option<String>,
}

/// One quota value — `producer_byte_rate`, `consumer_byte_rate`,
/// `request_percentage`, `controller_mutation_rate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaValue {
    pub key: String,
    pub value: f64,
}

/// One entity and everything set on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaEntity {
    pub entity: Vec<QuotaEntityPart>,
    pub values: Vec<QuotaValue>,
}

/// A single change. `value: None` REMOVES the quota rather than setting it to
/// zero — which would be a quota of zero bytes per second, i.e. a total block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaOp {
    pub key: String,
    pub value: Option<f64>,
}

/// Every quota the cluster has, named entities and `<default>` alike.
pub(crate) fn list(conn: &mut BrokerConnection) -> Result<Vec<QuotaEntity>> {
    let version = conn.negotiate(DESCRIBE_CLIENT_QUOTAS)?;
    let payload = conn.call(DESCRIBE_CLIENT_QUOTAS, version, describe_request_body())?;
    decode_describe(&payload)
}

/// No filter components, `strict = false` — Kafka's `ClientQuotaFilter.all()`,
/// which matches every entity of every type including the `<default>` ones.
///
/// The obvious-looking alternative, one `MatchType: ANY` component per entity
/// type, is REJECTED by the broker: `ClientQuotasImage.describe` refuses any
/// filter that mentions `ip` alongside `user` or `client-id`
/// ("Invalid entity filter component combination"), so spelling the three types
/// out would need two round trips whose results then have to be merged. An
/// empty component list sidesteps the rule entirely and is what `AdminClient`
/// itself sends for "describe everything".
///
/// `strict = false` is the other half: strict mode drops entities that have
/// components the filter did not name, which on an empty filter would drop
/// every entity there is.
fn describe_request_body() -> Vec<u8> {
    let mut body = Encoder::new();
    body.compact_array_len(Some(0)) // components: none
        .bool(false) // strict
        .tagged_fields();
    body.finish()
}

fn decode_describe(payload: &[u8]) -> Result<Vec<QuotaEntity>> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;
    let code = decoder.int16()?;
    let message = decoder.compact_nullable_string()?;
    errors::check("listing client quotas", code, message.as_deref())?;

    let entries = decoder.compact_array_len()?.unwrap_or(0);
    let mut out = Vec::with_capacity(entries);
    for _ in 0..entries {
        let parts = decoder.compact_array_len()?.unwrap_or(0);
        let mut entity = Vec::with_capacity(parts);
        for _ in 0..parts {
            let entity_type = decoder.compact_string()?;
            let name = decoder.compact_nullable_string()?;
            decoder.tagged_fields()?;
            entity.push(QuotaEntityPart { entity_type, name });
        }
        let value_count = decoder.compact_array_len()?.unwrap_or(0);
        let mut values = Vec::with_capacity(value_count);
        for _ in 0..value_count {
            let key = decoder.compact_string()?;
            let value = decoder.float64()?;
            decoder.tagged_fields()?;
            values.push(QuotaValue { key, value });
        }
        decoder.tagged_fields()?;

        // Stable ordering so a list view doesn't shuffle between refreshes:
        // the broker makes no promise about either array's order.
        entity.sort_by(|a, b| a.entity_type.cmp(&b.entity_type).then(a.name.cmp(&b.name)));
        values.sort_by(|a, b| a.key.cmp(&b.key));
        out.push(QuotaEntity { entity, values });
    }
    out.sort_by_key(|entry| entity_key(&entry.entity));
    Ok(out)
}

/// A sortable rendering of an entity. `<default>` sorts before any named value
/// because `None` does, which puts the fallback at the top of a list where it
/// belongs.
fn entity_key(entity: &[QuotaEntityPart]) -> Vec<(String, Option<String>)> {
    entity
        .iter()
        .map(|part| (part.entity_type.clone(), part.name.clone()))
        .collect()
}

/// Sets or removes quota values on one entity.
pub(crate) fn alter(
    conn: &mut BrokerConnection,
    entity: &[QuotaEntityPart],
    ops: &[QuotaOp],
) -> Result<()> {
    validate_entity(entity)?;
    validate_ops(ops)?;
    let version = conn.negotiate(ALTER_CLIENT_QUOTAS)?;
    let payload = conn.call(
        ALTER_CLIENT_QUOTAS,
        version,
        alter_request_body(entity, ops),
    )?;
    decode_alter(&payload)
}

fn validate_entity(entity: &[QuotaEntityPart]) -> Result<()> {
    if entity.is_empty() {
        return Err(Error::Other(
            "a quota needs an entity — a user, a client id, or an IP (name it, or leave the name \
             empty for the <default> entity)"
                .into(),
        ));
    }
    for part in entity {
        if !ENTITY_TYPES.contains(&part.entity_type.as_str()) {
            return Err(Error::Other(format!(
                "{:?} is not a quota entity type — Kafka has {}",
                part.entity_type,
                ENTITY_TYPES.join(", ")
            )));
        }
    }
    Ok(())
}

/// A non-finite value would encode as an IEEE-754 NaN or infinity, which Kafka
/// stores and then fails to parse back out of its own config — a quota that
/// cannot be read or removed afterwards.
fn validate_ops(ops: &[QuotaOp]) -> Result<()> {
    if ops.is_empty() {
        return Err(Error::Other("no quota changes were requested".into()));
    }
    for op in ops {
        if op.key.trim().is_empty() {
            return Err(Error::Other("a quota change named no setting".into()));
        }
        match op.value {
            Some(value) if !value.is_finite() => {
                return Err(Error::Other(format!(
                    "{} was given a value that is not a finite number",
                    op.key
                )))
            }
            _ => {}
        }
    }
    Ok(())
}

fn alter_request_body(entity: &[QuotaEntityPart], ops: &[QuotaOp]) -> Vec<u8> {
    let mut body = Encoder::new();
    body.compact_array_len(Some(1)) // one entry: one entity's changes
        .compact_array_len(Some(entity.len()));
    for part in entity {
        body.compact_string(&part.entity_type)
            .compact_nullable_string(part.name.as_deref()) // null = <default>
            .tagged_fields();
    }
    body.compact_array_len(Some(ops.len()));
    for op in ops {
        body.compact_string(&op.key)
            // `Remove` is the field that decides; `Value` is ignored when it is
            // set, so the 0.0 below never reaches a quota.
            .float64(op.value.unwrap_or(0.0))
            .bool(op.value.is_none())
            .tagged_fields();
    }
    body.tagged_fields() // entry
        .bool(false) // validate_only
        .tagged_fields();
    body.finish()
}

fn decode_alter(payload: &[u8]) -> Result<()> {
    let mut decoder = Decoder::new(payload);
    let _throttle_time_ms = decoder.int32()?;
    let entries = decoder.compact_array_len()?.unwrap_or(0);
    for _ in 0..entries {
        let code = decoder.int16()?;
        let message = decoder.compact_nullable_string()?;
        let parts = decoder.compact_array_len()?.unwrap_or(0);
        let mut names = Vec::with_capacity(parts);
        for _ in 0..parts {
            let entity_type = decoder.compact_string()?;
            let name = decoder.compact_nullable_string()?;
            decoder.tagged_fields()?;
            names.push(match name {
                Some(name) => format!("{entity_type} {name}"),
                None => format!("the <default> {entity_type}"),
            });
        }
        decoder.tagged_fields()?;
        errors::check(
            &format!("altering the quota for {}", names.join(" + ")),
            code,
            message.as_deref(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn user(name: Option<&str>) -> Vec<QuotaEntityPart> {
        vec![QuotaEntityPart {
            entity_type: "user".into(),
            name: name.map(str::to_string),
        }]
    }

    /// GOLDEN BYTES — setting `producer_byte_rate` to 1 MiB/s on user "alice".
    ///
    /// 02              Entries: compact array, 1 entry
    /// 02              Entity: compact array, 1 part
    /// 05 75736572     "user"
    /// 06 616c696365   "alice" (nullable string, non-null)
    /// 00              entity part tag buffer
    /// 02              Ops: compact array, 1 op
    /// 13 70726f64756365725f627974655f72617465  "producer_byte_rate" (18 + 1)
    /// 4130000000000000   1048576.0 as IEEE-754 big-endian
    /// 00              remove = false
    /// 00              op tag buffer
    /// 00              entry tag buffer
    /// 00              validate_only = false
    /// 00              body tag buffer
    #[test]
    fn an_alter_quotas_v1_request_body_is_byte_for_byte() {
        let ops = vec![QuotaOp {
            key: "producer_byte_rate".into(),
            value: Some(1_048_576.0),
        }];
        assert_eq!(
            hex(&alter_request_body(&user(Some("alice")), &ops)),
            "02\
             02\
             0575736572\
             06616c696365\
             00\
             02\
             1370726f64756365725f627974655f72617465\
             4130000000000000\
             00\
             00\
             00\
             00\
             00"
        );
    }

    /// The `<default>` entity is a NULL name (`00`), and removal is the
    /// `Remove` flag rather than a value of zero — which would be a quota of
    /// zero bytes per second, i.e. a total block.
    #[test]
    fn the_default_entity_is_a_null_name_and_removal_is_a_flag() {
        let remove = vec![QuotaOp {
            key: "producer_byte_rate".into(),
            value: None,
        }];
        let body = alter_request_body(&user(None), &remove);
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.compact_array_len().unwrap(), Some(1));
        assert_eq!(decoder.compact_array_len().unwrap(), Some(1));
        assert_eq!(decoder.compact_string().unwrap(), "user");
        assert_eq!(
            decoder.compact_nullable_string().unwrap(),
            None,
            "<default>"
        );
        decoder.tagged_fields().unwrap();
        assert_eq!(decoder.compact_array_len().unwrap(), Some(1));
        assert_eq!(decoder.compact_string().unwrap(), "producer_byte_rate");
        assert_eq!(decoder.float64().unwrap(), 0.0);
        assert!(decoder.bool().unwrap(), "remove must be set");

        // A named entity is byte-different from <default> at exactly that spot.
        assert_ne!(
            hex(&alter_request_body(&user(None), &remove)),
            hex(&alter_request_body(&user(Some("")), &remove)),
            "the <default> entity and an empty name must not encode alike"
        );
    }

    /// GOLDEN BYTES — the describe filter is three bytes, and each one matters.
    ///
    /// 01   Components: compact array, EMPTY (0 + 1) — note this is not `00`,
    ///      which would be a NULL component array
    /// 00   strict = false; `true` here would return nothing at all, because
    ///      strict mode drops every entity carrying a component the filter did
    ///      not name, and this filter names none
    /// 00   body tag buffer
    #[test]
    fn the_describe_filter_is_kafkas_match_everything_filter() {
        assert_eq!(hex(&describe_request_body()), "010000");

        let body = describe_request_body();
        let mut decoder = Decoder::new(&body);
        assert_eq!(
            decoder.compact_array_len().unwrap(),
            Some(0),
            "an empty component list, not a null one"
        );
        assert!(!decoder.bool().unwrap(), "strict must be false");
    }

    /// One canned describe entry: the entity's parts (type, name) and the
    /// values set on it.
    type CannedEntry<'a> = (Vec<(&'a str, Option<&'a str>)>, Vec<(&'a str, f64)>);

    fn describe_response(entries: &[CannedEntry<'_>]) -> Vec<u8> {
        let mut enc = Encoder::new();
        enc.int32(0)
            .int16(0)
            .compact_nullable_string(None)
            .compact_array_len(Some(entries.len()));
        for (entity, values) in entries {
            enc.compact_array_len(Some(entity.len()));
            for (entity_type, name) in entity {
                enc.compact_string(entity_type)
                    .compact_nullable_string(*name)
                    .tagged_fields();
            }
            enc.compact_array_len(Some(values.len()));
            for (key, value) in values {
                enc.compact_string(key).float64(*value).tagged_fields();
            }
            enc.tagged_fields();
        }
        enc.tagged_fields();
        enc.finish()
    }

    #[test]
    fn a_describe_response_keeps_the_default_entity_distinct_from_a_named_one() {
        let quotas = decode_describe(&describe_response(&[
            (
                vec![("user", Some("alice"))],
                vec![("producer_byte_rate", 1_048_576.0)],
            ),
            (
                vec![("user", None)],
                vec![("consumer_byte_rate", 2_097_152.0)],
            ),
        ]))
        .expect("decode");

        assert_eq!(quotas.len(), 2);
        // `<default>` sorts first.
        assert_eq!(quotas[0].entity, user(None));
        assert_eq!(quotas[0].values[0].key, "consumer_byte_rate");
        assert_eq!(quotas[1].entity, user(Some("alice")));
        assert_eq!(quotas[1].values[0].value, 1_048_576.0);
    }

    #[test]
    fn a_multi_part_entity_survives_the_round_trip_in_a_stable_order() {
        let quotas = decode_describe(&describe_response(&[(
            vec![("user", Some("alice")), ("client-id", Some("etl"))],
            vec![
                ("request_percentage", 200.0),
                ("producer_byte_rate", 1024.0),
            ],
        )]))
        .expect("decode");

        assert_eq!(
            quotas[0].entity,
            vec![
                QuotaEntityPart {
                    entity_type: "client-id".into(),
                    name: Some("etl".into()),
                },
                QuotaEntityPart {
                    entity_type: "user".into(),
                    name: Some("alice".into()),
                },
            ]
        );
        assert_eq!(
            quotas[0]
                .values
                .iter()
                .map(|v| v.key.as_str())
                .collect::<Vec<_>>(),
            vec!["producer_byte_rate", "request_percentage"]
        );
    }

    #[test]
    fn an_empty_quota_list_is_an_empty_vec() {
        assert_eq!(decode_describe(&describe_response(&[])).unwrap(), vec![]);
    }

    #[test]
    fn a_per_entity_alter_failure_names_the_entity() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .compact_array_len(Some(1))
            .int16(44) // POLICY_VIOLATION
            .compact_nullable_string(Some("Quota below the configured floor"))
            .compact_array_len(Some(1))
            .compact_string("user")
            .compact_nullable_string(None)
            .tagged_fields()
            .tagged_fields()
            .tagged_fields();

        let err = decode_alter(&enc.finish()).expect_err("policy violation");
        let message = err.to_string();
        assert!(message.contains("the <default> user"), "{message}");
        assert!(message.contains("POLICY_VIOLATION (44)"), "{message}");
        assert!(message.contains("configured floor"), "{message}");
    }

    #[test]
    fn a_successful_alter_reads_the_whole_frame_and_returns_nothing() {
        let mut enc = Encoder::new();
        enc.int32(0)
            .compact_array_len(Some(1))
            .int16(0)
            .compact_nullable_string(None)
            .compact_array_len(Some(1))
            .compact_string("user")
            .compact_nullable_string(Some("alice"))
            .tagged_fields()
            .tagged_fields()
            .tagged_fields();
        assert!(decode_alter(&enc.finish()).is_ok());
    }

    #[test]
    fn a_nonsense_entity_type_is_refused_before_the_round_trip() {
        let err = validate_entity(&[QuotaEntityPart {
            entity_type: "users".into(), // Kafka's type is singular
            name: Some("alice".into()),
        }])
        .unwrap_err()
        .to_string();
        assert!(err.contains("\"users\""), "got {err}");
        assert!(err.contains("client-id"), "got {err}");

        let err = validate_entity(&[]).unwrap_err().to_string();
        assert!(err.contains("<default>"), "got {err}");

        assert!(validate_entity(&user(None)).is_ok());
        assert!(validate_entity(&[QuotaEntityPart {
            entity_type: "ip".into(),
            name: Some("10.0.0.1".into()),
        }])
        .is_ok());
    }

    /// A NaN or infinite quota is stored by Kafka and then unreadable — and
    /// therefore unremovable — so it never leaves this process.
    #[test]
    fn a_quota_value_that_is_not_a_number_is_refused_before_the_round_trip() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = validate_ops(&[QuotaOp {
                key: "producer_byte_rate".into(),
                value: Some(value),
            }])
            .unwrap_err()
            .to_string();
            assert!(err.contains("finite number"), "got {err}");
        }
        assert!(validate_ops(&[]).is_err());
        assert!(validate_ops(&[QuotaOp {
            key: "  ".into(),
            value: Some(1.0),
        }])
        .is_err());
        // Removal (`None`) and a legitimate value both pass.
        assert!(validate_ops(&[
            QuotaOp {
                key: "producer_byte_rate".into(),
                value: None,
            },
            QuotaOp {
                key: "request_percentage".into(),
                value: Some(200.0),
            },
        ])
        .is_ok());
    }
}
