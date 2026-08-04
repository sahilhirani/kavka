//! Kafka ACLs: read, create and delete access-control bindings.
//!
//! Everything here BLOCKS, like the rest of the core — the Tauri shell wraps
//! each call in `spawn_blocking`. Both mutating entry points ([`acls_create`],
//! [`acls_delete`]) call [`ClusterConnection::ensure_writable`] first, so
//! read-only mode is enforced here rather than in the UI
//! (docs/ARCHITECTURE.md D5).
//!
//! # Why there is unsafe code in this file
//!
//! docs/ARCHITECTURE.md D2 anticipates gaps in *librdkafka* and fills them with
//! hand-rolled protocol frames. This is the smaller, commoner cousin: the gap
//! is in **rdkafka**, the Rust binding. librdkafka has had CreateAcls,
//! DescribeAcls and DeleteAcls since 1.9, and rdkafka-sys generates bindings
//! for all three — but rdkafka 0.37's safe `AdminClient` exposes none of them
//! (nor IncrementalAlterConfigs, which [`crate::admin::broker_config_set`]
//! needs). Writing the ACL frames by hand when the C library already speaks
//! them would be strictly worse, so [`native`] wraps the C admin API in the
//! twenty lines of RAII that rdkafka happens not to ship, and everything above
//! it is safe Rust.
//!
//! # Vocabulary
//!
//! Every enum crosses the IPC boundary as a lowercase string, and the
//! translation tables in [`names`] are the whole of that contract: they are
//! exhaustive over librdkafka's enums (a length assertion against each
//! `_CNT` sentinel fails the build's tests if librdkafka grows a value), and a
//! value they don't hold degrades to librdkafka's own name rather than
//! panicking or being silently dropped.

use crate::connection::ClusterConnection;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Wire types. Field names are the IPC contract — the TypeScript in
// apps/desktop/src mirrors them exactly, so renaming one is a breaking change
// on both sides of the bridge.
// ---------------------------------------------------------------------------

/// One ACL binding, in Kavka's lowercase string vocabulary.
///
/// The same type is both a *record* (what [`acls_list`] returns, what
/// [`acls_create`] writes) and a *filter* (what [`acls_delete`] matches on).
/// That is deliberate: Kafka's own delete API takes an ACL-shaped filter, and
/// giving deletion a second type would only invite the two to drift. The
/// difference is in how blanks read — see [`acls_delete`].
///
/// `Ord` is derived so every list is returned in a stable order; a table that
/// reshuffles itself between reads is unusable for spotting what changed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AclBinding {
    /// `topic` · `group` · `cluster` · `transactional_id` (`any` in a filter).
    pub resource_type: String,
    /// The topic/group/transactional id, or `kafka-cluster` for a cluster ACL.
    pub resource_name: String,
    /// `literal` · `prefixed` (`any` or `match` in a filter).
    pub pattern_type: String,
    /// Fully qualified, e.g. `User:alice` — Kafka stores the principal type.
    pub principal: String,
    /// A host or `*` for any.
    pub host: String,
    /// Lowercase Kafka operation name, e.g. `read`, `describe_configs`.
    pub operation: String,
    /// `allow` or `deny` (`any` in a filter).
    pub permission: String,
}

/// What [`acls_list`] narrows to. Every field is optional and an absent one
/// means "any", so the default filter lists the whole cluster's ACLs.
///
/// Deliberately not an [`AclBinding`]: the three fields here are the ones an
/// operator actually filters by, and an all-fields-required filter type would
/// make the common case ("show me everything for User:alice") impossible to
/// express without seven placeholder strings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AclFilter {
    pub resource_type: Option<String>,
    pub resource_name: Option<String>,
    pub principal: Option<String>,
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Every ACL binding the cluster holds that matches `filter`, sorted.
///
/// The filter's pattern type is always `any`, which in Kafka's vocabulary
/// means "a pattern whose name is exactly this, whatever kind it is" — *not*
/// `match`, which would additionally return every prefixed pattern that
/// covers the name. Filtering by `orders` must not silently hand back the
/// `ord`-prefixed rule that also applies to it: those are different bindings
/// with different blast radii, and conflating them is how an operator deletes
/// the wrong one.
#[cfg(feature = "kafka")]
pub fn acls_list(conn: &ClusterConnection, filter: &AclFilter) -> Result<Vec<AclBinding>> {
    use rdkafka::bindings as rdsys;

    const WHAT: &str = "listing ACLs";

    let resource_type = match blank_to_none(filter.resource_type.as_deref()) {
        Some(name) => names::resource_type(name)?,
        None => rdsys::rd_kafka_ResourceType_t::RD_KAFKA_RESOURCE_ANY,
    };
    let native = NativeAcl::filter(&Parts {
        resource_type,
        resource_name: native::cstring(blank_to_none(filter.resource_name.as_deref()))?,
        pattern_type: rdsys::rd_kafka_ResourcePatternType_t::RD_KAFKA_RESOURCE_PATTERN_ANY,
        principal: native::cstring(blank_to_none(filter.principal.as_deref()))?,
        host: None,
        operation: rdsys::rd_kafka_AclOperation_t::RD_KAFKA_ACL_OPERATION_ANY,
        permission: rdsys::rd_kafka_AclPermissionType_t::RD_KAFKA_ACL_PERMISSION_TYPE_ANY,
    })?;

    let event = native::request(
        conn,
        WHAT,
        rdkafka::types::RDKafkaAdminOp::RD_KAFKA_ADMIN_OP_DESCRIBEACLS,
        rdsys::RD_KAFKA_EVENT_DESCRIBEACLS_RESULT,
        |client, options, queue| unsafe {
            rdsys::rd_kafka_DescribeAcls(client, native.ptr(), options, queue);
        },
    )?;

    let result = unsafe { rdsys::rd_kafka_event_DescribeAcls_result(event.ptr()) };
    if result.is_null() {
        return Err(crate::admin::admin_error(
            conn,
            WHAT,
            "the broker acknowledged the request without saying what happened",
        ));
    }
    let mut count = 0usize;
    let acls = unsafe { rdsys::rd_kafka_DescribeAcls_result_acls(result, &mut count) };
    let mut bindings = read_bindings(acls, count);
    bindings.sort();
    Ok(bindings)
}

/// Creates every binding in `bindings`.
///
/// Kafka's CreateAcls is not transactional — some bindings can be written
/// while others are rejected — so every per-binding result is checked and the
/// first failure names the binding it belongs to rather than reporting a bare
/// error code for the batch.
#[cfg(feature = "kafka")]
pub fn acls_create(conn: &ClusterConnection, bindings: &[AclBinding]) -> Result<()> {
    use rdkafka::bindings as rdsys;

    const WHAT: &str = "creating the ACL";

    conn.ensure_writable("create ACL")?;
    if bindings.is_empty() {
        return Err(Error::Other("no ACL to create".into()));
    }

    let natives = bindings
        .iter()
        .map(NativeAcl::binding)
        .collect::<Result<Vec<_>>>()?;
    let mut pointers: Vec<*mut rdsys::rd_kafka_AclBinding_t> =
        natives.iter().map(NativeAcl::ptr).collect();

    let event = native::request(
        conn,
        WHAT,
        rdkafka::types::RDKafkaAdminOp::RD_KAFKA_ADMIN_OP_CREATEACLS,
        rdsys::RD_KAFKA_EVENT_CREATEACLS_RESULT,
        |client, options, queue| unsafe {
            rdsys::rd_kafka_CreateAcls(
                client,
                pointers.as_mut_ptr(),
                pointers.len(),
                options,
                queue,
            );
        },
    )?;

    let result = unsafe { rdsys::rd_kafka_event_CreateAcls_result(event.ptr()) };
    if result.is_null() {
        return Err(crate::admin::admin_error(
            conn,
            WHAT,
            "the broker acknowledged the request without saying what happened",
        ));
    }
    let mut count = 0usize;
    let results = unsafe { rdsys::rd_kafka_CreateAcls_result_acls(result, &mut count) };
    for index in 0..count {
        let error = unsafe { rdsys::rd_kafka_acl_result_error(*results.add(index)) };
        if let Some(cause) = native::error_message(error) {
            // Results come back in request order, so the index names the
            // binding — a batch that half-succeeded has to say which half.
            let named = match bindings.get(index) {
                Some(binding) => format!("{}: {cause}", describe(binding)),
                None => cause,
            };
            return Err(crate::admin::admin_error(conn, WHAT, &named));
        }
    }
    Ok(())
}

/// Deletes every binding matching `filter` and returns what was deleted,
/// sorted.
///
/// The return value is Kafka's own answer, not an echo of the request: a
/// filter can match more than one binding, and "what did I just remove" is the
/// only question worth asking after a destructive call.
///
/// **A blank string means "any".** `resource_name`, `principal` and `host` are
/// passed through as wildcards when empty, and `resource_type`, `pattern_type`,
/// `operation` and `permission` accept the literal `any`. That is the same
/// vocabulary `kafka-acls.sh --remove` uses when an option is omitted, and it
/// is why this takes an [`AclBinding`] rather than a narrower type: an operator
/// deleting exactly one rule pastes the row they listed, and one broadening a
/// delete blanks a field.
#[cfg(feature = "kafka")]
pub fn acls_delete(conn: &ClusterConnection, filter: &AclBinding) -> Result<Vec<AclBinding>> {
    use rdkafka::bindings as rdsys;

    const WHAT: &str = "deleting ACLs";

    conn.ensure_writable("delete ACL")?;

    let native = NativeAcl::filter(&Parts::parse(filter)?)?;
    let mut pointers = [native.ptr()];

    let event = native::request(
        conn,
        WHAT,
        rdkafka::types::RDKafkaAdminOp::RD_KAFKA_ADMIN_OP_DELETEACLS,
        rdsys::RD_KAFKA_EVENT_DELETEACLS_RESULT,
        |client, options, queue| unsafe {
            rdsys::rd_kafka_DeleteAcls(
                client,
                pointers.as_mut_ptr(),
                pointers.len(),
                options,
                queue,
            );
        },
    )?;

    let result = unsafe { rdsys::rd_kafka_event_DeleteAcls_result(event.ptr()) };
    if result.is_null() {
        return Err(crate::admin::admin_error(
            conn,
            WHAT,
            "the broker acknowledged the request without saying what happened",
        ));
    }
    let mut count = 0usize;
    let responses = unsafe { rdsys::rd_kafka_DeleteAcls_result_responses(result, &mut count) };

    let mut deleted = Vec::new();
    for index in 0..count {
        let response = unsafe { *responses.add(index) };
        let error = unsafe { rdsys::rd_kafka_DeleteAcls_result_response_error(response) };
        if let Some(cause) = native::error_message(error) {
            return Err(crate::admin::admin_error(conn, WHAT, &cause));
        }
        let mut matched = 0usize;
        let acls = unsafe {
            rdsys::rd_kafka_DeleteAcls_result_response_matching_acls(response, &mut matched)
        };
        deleted.extend(read_bindings(acls, matched));
    }
    deleted.sort();
    Ok(deleted)
}

// ---------------------------------------------------------------------------
// Reading librdkafka's bindings back out
// ---------------------------------------------------------------------------

/// Copies `count` bindings out of a librdkafka result array.
///
/// # Safety-relevant invariant
///
/// `acls` and `count` must be the pair librdkafka just filled in, and the
/// event owning them must still be alive — every string is copied here, so the
/// result outlives the event but not this call.
#[cfg(feature = "kafka")]
fn read_bindings(
    acls: *mut *const rdkafka::bindings::rd_kafka_AclBinding_t,
    count: usize,
) -> Vec<AclBinding> {
    if acls.is_null() {
        return Vec::new();
    }
    (0..count)
        .map(|index| read_binding(unsafe { *acls.add(index) }))
        .collect()
}

#[cfg(feature = "kafka")]
fn read_binding(acl: *const rdkafka::bindings::rd_kafka_AclBinding_t) -> AclBinding {
    use rdkafka::bindings as rdsys;

    // A described binding always carries all three strings; a *filter* may
    // hold NULLs, and librdkafka's accessors are shared between the two. An
    // empty string is this type's own "any", so the two agree.
    AclBinding {
        resource_type: names::resource_type_name(unsafe {
            rdsys::rd_kafka_AclBinding_restype(acl)
        }),
        resource_name: native::owned(unsafe { rdsys::rd_kafka_AclBinding_name(acl) }),
        pattern_type: names::pattern_type_name(unsafe {
            rdsys::rd_kafka_AclBinding_resource_pattern_type(acl)
        }),
        principal: native::owned(unsafe { rdsys::rd_kafka_AclBinding_principal(acl) }),
        host: native::owned(unsafe { rdsys::rd_kafka_AclBinding_host(acl) }),
        operation: names::operation_name(unsafe { rdsys::rd_kafka_AclBinding_operation(acl) }),
        permission: names::permission_name(unsafe {
            rdsys::rd_kafka_AclBinding_permission_type(acl)
        }),
    }
}

/// One binding as a single line, for error messages.
#[cfg(feature = "kafka")]
fn describe(binding: &AclBinding) -> String {
    format!(
        "{} {} to {} on {} {} \"{}\" from {}",
        binding.permission,
        binding.principal,
        binding.operation,
        binding.pattern_type,
        binding.resource_type,
        binding.resource_name,
        binding.host,
    )
}

/// A trimmed string, or `None` when it holds nothing — the blank-is-any rule
/// [`acls_delete`] and [`acls_list`] both document.
#[cfg(feature = "kafka")]
fn blank_to_none(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

// ---------------------------------------------------------------------------
// Binding <-> librdkafka
// ---------------------------------------------------------------------------

/// One binding in librdkafka's own vocabulary, with every string already
/// converted to a NUL-terminated buffer that outlives the constructor call.
#[cfg(feature = "kafka")]
#[derive(Debug)]
struct Parts {
    resource_type: rdkafka::types::RDKafkaResourceType,
    resource_name: Option<std::ffi::CString>,
    pattern_type: rdkafka::bindings::rd_kafka_ResourcePatternType_t,
    principal: Option<std::ffi::CString>,
    host: Option<std::ffi::CString>,
    operation: rdkafka::bindings::rd_kafka_AclOperation_t,
    permission: rdkafka::bindings::rd_kafka_AclPermissionType_t,
}

#[cfg(feature = "kafka")]
impl Parts {
    /// Translates a binding for use as a **filter**: every enum is looked up by
    /// name (`any` included) and a blank string becomes a NULL wildcard.
    fn parse(binding: &AclBinding) -> Result<Self> {
        Ok(Self {
            resource_type: names::resource_type(&binding.resource_type)?,
            resource_name: native::cstring(blank_to_none(Some(&binding.resource_name)))?,
            pattern_type: names::pattern_type(&binding.pattern_type)?,
            principal: native::cstring(blank_to_none(Some(&binding.principal)))?,
            host: native::cstring(blank_to_none(Some(&binding.host)))?,
            operation: names::operation(&binding.operation)?,
            permission: names::permission(&binding.permission)?,
        })
    }

    /// Translates a binding for **creation**, where Kafka has no wildcards:
    /// a missing name, principal or host is refused here with the word the
    /// operator has to type, rather than as librdkafka's "Invalid principal".
    fn create(binding: &AclBinding) -> Result<Self> {
        require(&binding.resource_name, "a resource name")?;
        require(
            &binding.principal,
            "a principal, e.g. \"User:alice\" (Kafka stores the type as well as the name)",
        )?;
        require(
            &binding.host,
            "a host — \"*\" is how Kafka spells \"from anywhere\"",
        )?;
        Self::parse(binding)
    }
}

#[cfg(feature = "kafka")]
fn require(value: &str, what: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Other(format!("an ACL needs {what}")));
    }
    Ok(())
}

/// An owned `rd_kafka_AclBinding_t`, destroyed on drop.
#[cfg(feature = "kafka")]
struct NativeAcl(*mut rdkafka::bindings::rd_kafka_AclBinding_t);

#[cfg(feature = "kafka")]
impl NativeAcl {
    /// A binding to create. librdkafka refuses the wildcards (`any`) and the
    /// `unknown` placeholders here, which is correct: a stored ACL has to name
    /// exactly one of everything.
    ///
    /// Using this constructor rather than [`NativeAcl::filter`] is
    /// load-bearing, not stylistic: `rd_kafka_CreateAcls` copies each binding
    /// through `rd_kafka_AclBinding_new` behind an `rd_assert`, so handing it a
    /// binding this constructor would have rejected aborts the process instead
    /// of returning an error.
    fn binding(binding: &AclBinding) -> Result<Self> {
        Self::new(
            &Parts::create(binding)?,
            rdkafka::bindings::rd_kafka_AclBinding_new,
        )
    }

    /// A binding to match on. Accepts every wildcard the record form refuses.
    fn filter(parts: &Parts) -> Result<Self> {
        Self::new(parts, rdkafka::bindings::rd_kafka_AclBindingFilter_new)
    }

    fn new(
        parts: &Parts,
        construct: unsafe extern "C" fn(
            rdkafka::types::RDKafkaResourceType,
            *const std::ffi::c_char,
            rdkafka::bindings::rd_kafka_ResourcePatternType_t,
            *const std::ffi::c_char,
            *const std::ffi::c_char,
            rdkafka::bindings::rd_kafka_AclOperation_t,
            rdkafka::bindings::rd_kafka_AclPermissionType_t,
            *mut std::ffi::c_char,
            usize,
        ) -> *mut rdkafka::bindings::rd_kafka_AclBinding_t,
    ) -> Result<Self> {
        let mut errstr = native::ErrBuf::new();
        let acl = unsafe {
            construct(
                parts.resource_type,
                native::as_ptr(&parts.resource_name),
                parts.pattern_type,
                native::as_ptr(&parts.principal),
                native::as_ptr(&parts.host),
                parts.operation,
                parts.permission,
                errstr.as_mut_ptr(),
                errstr.size(),
            )
        };
        if acl.is_null() {
            return Err(Error::Other(format!(
                "that is not a valid ACL: {}",
                errstr.take()
            )));
        }
        Ok(Self(acl))
    }

    fn ptr(&self) -> *mut rdkafka::bindings::rd_kafka_AclBinding_t {
        self.0
    }
}

#[cfg(feature = "kafka")]
impl Drop for NativeAcl {
    fn drop(&mut self) {
        unsafe { rdkafka::bindings::rd_kafka_AclBinding_destroy(self.0) };
    }
}

// ---------------------------------------------------------------------------
// The string vocabulary
// ---------------------------------------------------------------------------

/// The enum <-> string translation tables, in both directions.
///
/// Each table is asserted exhaustive against librdkafka's own `_CNT` sentinel,
/// so a librdkafka upgrade that adds an operation fails these tests rather
/// than quietly returning `unsupported` to the UI.
#[cfg(feature = "kafka")]
mod names {
    use crate::{Error, Result};
    use rdkafka::bindings as rdsys;
    use rdsys::rd_kafka_AclOperation_t as Operation;
    use rdsys::rd_kafka_AclPermissionType_t as Permission;
    use rdsys::rd_kafka_ResourcePatternType_t as PatternType;
    use rdsys::rd_kafka_ResourceType_t as ResourceType;
    use std::ffi::{c_char, CStr};

    /// Kafka's ACL resource types.
    ///
    /// **`RD_KAFKA_RESOURCE_BROKER` is spelled `cluster` here on purpose.**
    /// librdkafka reuses one enum for ACLs and DescribeConfigs; wire value 4 is
    /// `BROKER` in a config request and `CLUSTER` in an ACL, and this module
    /// only ever speaks ACL. `cluster` is what the protocol calls it, what
    /// `kafka-acls.sh --cluster` writes, and what an operator reading a table
    /// of ACLs expects to see — `broker` would be a translation error dressed
    /// up as a librdkafka detail.
    const RESOURCE_TYPES: &[(ResourceType, &str)] = &[
        (ResourceType::RD_KAFKA_RESOURCE_UNKNOWN, "unknown"),
        (ResourceType::RD_KAFKA_RESOURCE_ANY, "any"),
        (ResourceType::RD_KAFKA_RESOURCE_TOPIC, "topic"),
        (ResourceType::RD_KAFKA_RESOURCE_GROUP, "group"),
        (ResourceType::RD_KAFKA_RESOURCE_BROKER, "cluster"),
        (
            ResourceType::RD_KAFKA_RESOURCE_TRANSACTIONAL_ID,
            "transactional_id",
        ),
    ];

    const PATTERN_TYPES: &[(PatternType, &str)] = &[
        (PatternType::RD_KAFKA_RESOURCE_PATTERN_UNKNOWN, "unknown"),
        (PatternType::RD_KAFKA_RESOURCE_PATTERN_ANY, "any"),
        (PatternType::RD_KAFKA_RESOURCE_PATTERN_MATCH, "match"),
        (PatternType::RD_KAFKA_RESOURCE_PATTERN_LITERAL, "literal"),
        (PatternType::RD_KAFKA_RESOURCE_PATTERN_PREFIXED, "prefixed"),
    ];

    /// Kafka's operation names, lowercased. `all` and `any` are Kafka's own
    /// wildcards, not Kavka inventions: `all` is a real stored operation
    /// meaning "every operation", and `any` is only ever a filter.
    const OPERATIONS: &[(Operation, &str)] = &[
        (Operation::RD_KAFKA_ACL_OPERATION_UNKNOWN, "unknown"),
        (Operation::RD_KAFKA_ACL_OPERATION_ANY, "any"),
        (Operation::RD_KAFKA_ACL_OPERATION_ALL, "all"),
        (Operation::RD_KAFKA_ACL_OPERATION_READ, "read"),
        (Operation::RD_KAFKA_ACL_OPERATION_WRITE, "write"),
        (Operation::RD_KAFKA_ACL_OPERATION_CREATE, "create"),
        (Operation::RD_KAFKA_ACL_OPERATION_DELETE, "delete"),
        (Operation::RD_KAFKA_ACL_OPERATION_ALTER, "alter"),
        (Operation::RD_KAFKA_ACL_OPERATION_DESCRIBE, "describe"),
        (
            Operation::RD_KAFKA_ACL_OPERATION_CLUSTER_ACTION,
            "cluster_action",
        ),
        (
            Operation::RD_KAFKA_ACL_OPERATION_DESCRIBE_CONFIGS,
            "describe_configs",
        ),
        (
            Operation::RD_KAFKA_ACL_OPERATION_ALTER_CONFIGS,
            "alter_configs",
        ),
        (
            Operation::RD_KAFKA_ACL_OPERATION_IDEMPOTENT_WRITE,
            "idempotent_write",
        ),
    ];

    const PERMISSIONS: &[(Permission, &str)] = &[
        (Permission::RD_KAFKA_ACL_PERMISSION_TYPE_UNKNOWN, "unknown"),
        (Permission::RD_KAFKA_ACL_PERMISSION_TYPE_ANY, "any"),
        (Permission::RD_KAFKA_ACL_PERMISSION_TYPE_DENY, "deny"),
        (Permission::RD_KAFKA_ACL_PERMISSION_TYPE_ALLOW, "allow"),
    ];

    pub(super) fn resource_type(name: &str) -> Result<ResourceType> {
        value(RESOURCE_TYPES, name, "ACL resource type")
    }

    pub(super) fn pattern_type(name: &str) -> Result<PatternType> {
        value(PATTERN_TYPES, name, "ACL resource pattern type")
    }

    pub(super) fn operation(name: &str) -> Result<Operation> {
        value(OPERATIONS, name, "ACL operation")
    }

    pub(super) fn permission(name: &str) -> Result<Permission> {
        value(PERMISSIONS, name, "ACL permission")
    }

    pub(super) fn resource_type_name(value: ResourceType) -> String {
        name(RESOURCE_TYPES, value, rdsys::rd_kafka_ResourceType_name)
    }

    pub(super) fn pattern_type_name(value: PatternType) -> String {
        name(
            PATTERN_TYPES,
            value,
            rdsys::rd_kafka_ResourcePatternType_name,
        )
    }

    pub(super) fn operation_name(value: Operation) -> String {
        name(OPERATIONS, value, rdsys::rd_kafka_AclOperation_name)
    }

    pub(super) fn permission_name(value: Permission) -> String {
        name(PERMISSIONS, value, rdsys::rd_kafka_AclPermissionType_name)
    }

    /// The wire name for one enum value.
    ///
    /// A value the table doesn't hold — a librdkafka newer than this file —
    /// falls back to librdkafka's own name lowercased. Those `*_name` functions
    /// bounds-check and answer `UNSUPPORTED` rather than reading past their
    /// array, so an unmapped value degrades to a readable string and this
    /// never panics. A row the UI can't act on beats a list that fails to load
    /// because one binding was written by a newer broker.
    fn name<T: Copy + PartialEq>(
        table: &[(T, &'static str)],
        value: T,
        raw: unsafe extern "C" fn(T) -> *const c_char,
    ) -> String {
        if let Some((_, mapped)) = table.iter().find(|(candidate, _)| *candidate == value) {
            return (*mapped).to_string();
        }
        unsafe { CStr::from_ptr(raw(value)) }
            .to_string_lossy()
            .to_ascii_lowercase()
    }

    /// The enum value for one wire name, case- and whitespace-insensitive
    /// because these arrive from a text field as often as from a dropdown.
    fn value<T: Copy>(table: &[(T, &'static str)], name: &str, what: &str) -> Result<T> {
        let wanted = name.trim().to_ascii_lowercase();
        table
            .iter()
            .find(|(_, candidate)| *candidate == wanted.as_str())
            .map(|(value, _)| *value)
            .ok_or_else(|| {
                let known: Vec<&str> = table.iter().map(|(_, candidate)| *candidate).collect();
                Error::Other(format!(
                    "\"{name}\" is not a Kafka {what} — it is one of: {}",
                    known.join(", ")
                ))
            })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every table is exhaustive over librdkafka's enum, and every entry
        /// round-trips. The length assertions are the tripwire: librdkafka
        /// growing an operation moves `_CNT` and fails here, rather than
        /// shipping `unsupported` to the UI months later.
        #[test]
        fn every_resource_type_round_trips() {
            assert_eq!(
                RESOURCE_TYPES.len(),
                ResourceType::RD_KAFKA_RESOURCE__CNT as usize,
                "librdkafka has a resource type this table does not"
            );
            for (value, name) in RESOURCE_TYPES {
                assert_eq!(&resource_type_name(*value), name);
                assert_eq!(resource_type(name).unwrap(), *value);
            }
        }

        #[test]
        fn every_pattern_type_round_trips() {
            assert_eq!(
                PATTERN_TYPES.len(),
                PatternType::RD_KAFKA_RESOURCE_PATTERN_TYPE__CNT as usize,
                "librdkafka has a pattern type this table does not"
            );
            for (value, name) in PATTERN_TYPES {
                assert_eq!(&pattern_type_name(*value), name);
                assert_eq!(pattern_type(name).unwrap(), *value);
            }
        }

        #[test]
        fn every_operation_round_trips() {
            assert_eq!(
                OPERATIONS.len(),
                Operation::RD_KAFKA_ACL_OPERATION__CNT as usize,
                "librdkafka has an ACL operation this table does not"
            );
            for (value, name) in OPERATIONS {
                assert_eq!(&operation_name(*value), name);
                assert_eq!(operation(name).unwrap(), *value);
            }
        }

        #[test]
        fn every_permission_round_trips() {
            assert_eq!(
                PERMISSIONS.len(),
                Permission::RD_KAFKA_ACL_PERMISSION_TYPE__CNT as usize,
                "librdkafka has a permission type this table does not"
            );
            for (value, name) in PERMISSIONS {
                assert_eq!(&permission_name(*value), name);
                assert_eq!(permission(name).unwrap(), *value);
            }
        }

        /// Wire value 4 is `BROKER` to librdkafka and `CLUSTER` to the ACL
        /// protocol. The whole table exists for cases like this one, so it gets
        /// its own test.
        #[test]
        fn the_acl_name_for_librdkafkas_broker_resource_is_cluster() {
            assert_eq!(
                resource_type("cluster").unwrap(),
                ResourceType::RD_KAFKA_RESOURCE_BROKER
            );
            assert_eq!(
                resource_type_name(ResourceType::RD_KAFKA_RESOURCE_BROKER),
                "cluster"
            );
            assert!(resource_type("broker").is_err());
        }

        #[test]
        fn names_arrive_from_text_fields_so_case_and_padding_are_forgiven() {
            assert_eq!(
                operation("  DESCRIBE_CONFIGS ").unwrap(),
                Operation::RD_KAFKA_ACL_OPERATION_DESCRIBE_CONFIGS
            );
            assert_eq!(
                permission("Allow").unwrap(),
                Permission::RD_KAFKA_ACL_PERMISSION_TYPE_ALLOW
            );
        }

        #[test]
        fn an_unknown_name_lists_the_ones_that_exist() {
            let err = operation("publish").unwrap_err().to_string();
            assert!(
                err.contains("\"publish\" is not a Kafka ACL operation"),
                "{err}"
            );
            assert!(err.contains("idempotent_write"), "{err}");
        }

        /// A value outside the table — what a newer librdkafka would hand back
        /// — degrades to librdkafka's own name instead of panicking. `_CNT` is
        /// the only such value reachable from this build, and it is exactly
        /// what "one past everything we know" looks like.
        #[test]
        fn a_value_the_table_does_not_hold_degrades_to_its_raw_name() {
            assert_eq!(
                operation_name(Operation::RD_KAFKA_ACL_OPERATION__CNT),
                "unsupported"
            );
            assert_eq!(
                resource_type_name(ResourceType::RD_KAFKA_RESOURCE__CNT),
                "unsupported"
            );
            assert_eq!(
                pattern_type_name(PatternType::RD_KAFKA_RESOURCE_PATTERN_TYPE__CNT),
                "unsupported"
            );
            assert_eq!(
                permission_name(Permission::RD_KAFKA_ACL_PERMISSION_TYPE__CNT),
                "unsupported"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Raw librdkafka admin plumbing
// ---------------------------------------------------------------------------

/// The twenty lines of RAII rdkafka 0.37 happens not to ship.
///
/// rdkafka's `AdminClient` keeps its queue, its `AdminOptions` conversion and
/// its event wrapper private, so an admin call it does not implement cannot
/// borrow any of them — the only way in is librdkafka's C API. This module is
/// that way in, and it is deliberately generic rather than ACL-shaped:
/// [`crate::admin::broker_config_set`] is its second user, for
/// IncrementalAlterConfigs, which rdkafka also does not expose.
///
/// The design is the boring one on purpose. Each call gets a **private** queue
/// (`rd_kafka_queue_new`) and blocks on it, so nothing here races rdkafka's own
/// admin polling thread, no callback is registered, and no pointer outlives the
/// stack frame that made it. librdkafka's main thread services the request
/// regardless of who is polling what, which is what makes a private queue
/// sufficient.
#[cfg(feature = "kafka")]
pub(crate) mod native {
    use crate::admin::{admin_error, ADMIN_TIMEOUT};
    use crate::connection::ClusterConnection;
    use crate::{Error, Result};
    use rdkafka::bindings as rdsys;
    use rdkafka::types::{
        RDKafka, RDKafkaAdminOp, RDKafkaAdminOptions, RDKafkaEvent, RDKafkaQueue, RDKafkaRespErr,
    };
    use std::ffi::{c_char, c_int, CStr, CString};
    use std::time::{Duration, Instant};

    /// How much longer than librdkafka's own request timeout to wait on the
    /// queue. librdkafka answers a timeout with an error *event*, which carries
    /// the reason; giving up on the queue first would replace that reason with
    /// "no answer", so the slack exists to let the better message win.
    const POLL_SLACK: Duration = Duration::from_secs(5);

    /// librdkafka's errstr convention: a caller-owned buffer it fills on
    /// failure.
    pub(crate) struct ErrBuf([c_char; 512]);

    impl ErrBuf {
        pub(crate) fn new() -> Self {
            Self([0; 512])
        }

        pub(crate) fn as_mut_ptr(&mut self) -> *mut c_char {
            self.0.as_mut_ptr()
        }

        /// Deliberately not `len`: this is the buffer's capacity for
        /// librdkafka, not the length of anything, and `len` on a type with no
        /// `is_empty` is the wrong shape as well as the wrong word.
        pub(crate) fn size(&self) -> usize {
            self.0.len()
        }

        /// What librdkafka wrote, or a stand-in — an empty buffer would
        /// otherwise produce an error message that trails off into nothing.
        pub(crate) fn take(&self) -> String {
            let text = unsafe { CStr::from_ptr(self.0.as_ptr()) }.to_string_lossy();
            if text.is_empty() {
                "librdkafka rejected it without saying why".to_string()
            } else {
                text.into_owned()
            }
        }
    }

    /// A NUL-terminated copy of `value`, or `None` when there is nothing to
    /// copy. An interior NUL is refused here rather than truncating the string
    /// silently — a topic name that loses its tail matches the wrong ACL.
    pub(crate) fn cstring(value: Option<&str>) -> Result<Option<CString>> {
        value
            .map(|text| {
                CString::new(text).map_err(|_| {
                    Error::Other(format!(
                        "\"{text}\" contains a NUL byte, which Kafka names cannot"
                    ))
                })
            })
            .transpose()
    }

    /// The pointer for an optional C string: NULL is librdkafka's wildcard.
    pub(crate) fn as_ptr(value: &Option<CString>) -> *const c_char {
        match value {
            Some(text) => text.as_ptr(),
            None => std::ptr::null(),
        }
    }

    /// An owned copy of a librdkafka string, with NULL read as empty.
    pub(crate) fn owned(text: *const c_char) -> String {
        if text.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    }

    /// The message inside an `rd_kafka_error_t`, or `None` when it reports
    /// success. NULL means success too — librdkafka only allocates one when
    /// something went wrong.
    pub(crate) fn error_message(error: *const rdsys::rd_kafka_error_t) -> Option<String> {
        if error.is_null() {
            return None;
        }
        let code = unsafe { rdsys::rd_kafka_error_code(error) };
        if code == RDKafkaRespErr::RD_KAFKA_RESP_ERR_NO_ERROR {
            return None;
        }
        let text = owned(unsafe { rdsys::rd_kafka_error_string(error) });
        Some(if text.is_empty() {
            format!("{:?}", rdkafka::error::RDKafkaErrorCode::from(code))
        } else {
            text
        })
    }

    /// An owned `rd_kafka_queue_t`. Destroyed before the client it came from,
    /// because it is created and dropped inside one [`request`] call.
    struct Queue(*mut RDKafkaQueue);

    impl Drop for Queue {
        fn drop(&mut self) {
            unsafe { rdsys::rd_kafka_queue_destroy(self.0) };
        }
    }

    /// An owned `rd_kafka_AdminOptions_t`.
    struct Options(*mut RDKafkaAdminOptions);

    impl Options {
        /// Only the request timeout is set. `operation_timeout` is *not*, and
        /// that is not an omission: librdkafka accepts it for CreateTopics and
        /// friends only, and rejects it outright for every operation this
        /// module runs.
        fn new(client: *mut RDKafka, op: RDKafkaAdminOp) -> Result<Self> {
            let options = unsafe { rdsys::rd_kafka_AdminOptions_new(client, op) };
            if options.is_null() {
                return Err(Error::Other(
                    "this build of librdkafka does not support that admin operation".into(),
                ));
            }
            let options = Self(options);
            let mut errstr = ErrBuf::new();
            let millis = c_int::try_from(ADMIN_TIMEOUT.as_millis()).unwrap_or(c_int::MAX);
            let err = unsafe {
                rdsys::rd_kafka_AdminOptions_set_request_timeout(
                    options.0,
                    millis,
                    errstr.as_mut_ptr(),
                    errstr.size(),
                )
            };
            if err != RDKafkaRespErr::RD_KAFKA_RESP_ERR_NO_ERROR {
                return Err(Error::Other(format!(
                    "setting the admin request timeout failed: {}",
                    errstr.take()
                )));
            }
            Ok(options)
        }
    }

    impl Drop for Options {
        fn drop(&mut self) {
            unsafe { rdsys::rd_kafka_AdminOptions_destroy(self.0) };
        }
    }

    /// An owned `rd_kafka_event_t`. Every result librdkafka hands back lives
    /// inside its event, so callers must copy what they need out before this
    /// drops — which is why [`request`] returns it rather than the result.
    pub(crate) struct Event(*mut RDKafkaEvent);

    impl Event {
        pub(crate) fn ptr(&self) -> *mut RDKafkaEvent {
            self.0
        }
    }

    impl Drop for Event {
        fn drop(&mut self) {
            unsafe { rdsys::rd_kafka_event_destroy(self.0) };
        }
    }

    /// An owned `rd_kafka_ConfigResource_t`, for IncrementalAlterConfigs.
    pub(crate) struct ConfigResource(*mut rdkafka::types::RDKafkaConfigResource);

    impl ConfigResource {
        /// One broker, addressed by id — librdkafka takes the id as a string
        /// and routes the request to that broker rather than the controller.
        pub(crate) fn broker(id: i32) -> Result<Self> {
            let name = CString::new(id.to_string()).expect("a decimal integer has no NUL");
            let resource = unsafe {
                rdsys::rd_kafka_ConfigResource_new(
                    rdkafka::types::RDKafkaResourceType::RD_KAFKA_RESOURCE_BROKER,
                    name.as_ptr(),
                )
            };
            if resource.is_null() {
                return Err(Error::Other(format!(
                    "librdkafka would not address broker {id}"
                )));
            }
            Ok(Self(resource))
        }

        /// Queues one incremental change: `Some` sets the value, `None`
        /// deletes the override so the config falls back to what it inherits.
        pub(crate) fn set(&self, name: &str, value: Option<&str>) -> Result<()> {
            let key = CString::new(name).map_err(|_| {
                Error::Other(format!(
                    "\"{name}\" contains a NUL byte, which a config key cannot"
                ))
            })?;
            let text = cstring(value)?;
            let op = match value {
                Some(_) => rdsys::rd_kafka_AlterConfigOpType_t::RD_KAFKA_ALTER_CONFIG_OP_TYPE_SET,
                None => rdsys::rd_kafka_AlterConfigOpType_t::RD_KAFKA_ALTER_CONFIG_OP_TYPE_DELETE,
            };
            let error = unsafe {
                rdsys::rd_kafka_ConfigResource_add_incremental_config(
                    self.0,
                    key.as_ptr(),
                    op,
                    as_ptr(&text),
                )
            };
            let message = error_message(error);
            if !error.is_null() {
                unsafe { rdsys::rd_kafka_error_destroy(error) };
            }
            match message {
                Some(cause) => Err(Error::Other(format!(
                    "\"{name}\" cannot be changed: {cause}"
                ))),
                None => Ok(()),
            }
        }

        pub(crate) fn ptr(&self) -> *mut rdkafka::types::RDKafkaConfigResource {
            self.0
        }
    }

    impl Drop for ConfigResource {
        fn drop(&mut self) {
            unsafe { rdsys::rd_kafka_ConfigResource_destroy(self.0) };
        }
    }

    /// Runs one admin request to completion and returns its result event.
    ///
    /// `dispatch` receives the client, the options and the queue and does
    /// nothing but call the matching `rd_kafka_*` function — every argument it
    /// needs beyond those three is captured, and librdkafka copies each of them
    /// before returning, so nothing has to outlive the call.
    ///
    /// The returned event has already been checked for a top-level failure;
    /// per-item results are the caller's, because only the caller knows what an
    /// item is.
    pub(crate) fn request(
        conn: &ClusterConnection,
        what: &str,
        op: RDKafkaAdminOp,
        expected: rdsys::rd_kafka_event_type_t,
        dispatch: impl FnOnce(*mut RDKafka, *const RDKafkaAdminOptions, *mut RDKafkaQueue),
    ) -> Result<Event> {
        let client = conn.admin()?.inner().native_ptr();
        let queue = unsafe { rdsys::rd_kafka_queue_new(client) };
        if queue.is_null() {
            return Err(admin_error(conn, what, "librdkafka would not open a queue"));
        }
        let queue = Queue(queue);
        let options = Options::new(client, op)?;

        dispatch(client, options.0, queue.0);

        let event = poll(&queue, expected, ADMIN_TIMEOUT + POLL_SLACK).ok_or_else(|| {
            admin_error(
                conn,
                what,
                "the cluster did not answer, and librdkafka did not say why",
            )
        })?;

        let err = unsafe { rdsys::rd_kafka_event_error(event.0) };
        if err != RDKafkaRespErr::RD_KAFKA_RESP_ERR_NO_ERROR {
            let cause = owned(unsafe { rdsys::rd_kafka_event_error_string(event.0) });
            let cause = if cause.is_empty() {
                format!("{:?}", rdkafka::error::RDKafkaErrorCode::from(err))
            } else {
                cause
            };
            return Err(admin_error(conn, what, &cause));
        }
        Ok(event)
    }

    /// Blocks until the queue produces an event of the expected type, or the
    /// budget runs out.
    ///
    /// The queue is private to one request, so the first event is always the
    /// answer — but an event of another type is dropped and the wait resumes
    /// rather than being returned as the result, because handing a
    /// `DescribeAcls` reader a `CreateAcls` event would read the wrong union
    /// member out of it.
    fn poll(
        queue: &Queue,
        expected: rdsys::rd_kafka_event_type_t,
        budget: Duration,
    ) -> Option<Event> {
        let deadline = Instant::now() + budget;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let millis = c_int::try_from(remaining.as_millis()).unwrap_or(c_int::MAX);
            let event = unsafe { rdsys::rd_kafka_queue_poll(queue.0, millis) };
            if !event.is_null() {
                let event = Event(event);
                if unsafe { rdsys::rd_kafka_event_type(event.0) } == expected {
                    return Some(event);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Builds without the `kafka` feature. Every entry point still exists so the
// crate's API is the same shape on a toolchain with no CMake; each one refuses
// rather than silently doing nothing.
// ---------------------------------------------------------------------------

#[cfg(not(feature = "kafka"))]
fn unsupported<T>() -> Result<T> {
    Err(Error::Other(
        "kavka-core was built without the `kafka` feature".into(),
    ))
}

#[cfg(not(feature = "kafka"))]
pub fn acls_list(_conn: &ClusterConnection, _filter: &AclFilter) -> Result<Vec<AclBinding>> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn acls_create(_conn: &ClusterConnection, _bindings: &[AclBinding]) -> Result<()> {
    unsupported()
}

#[cfg(not(feature = "kafka"))]
pub fn acls_delete(_conn: &ClusterConnection, _filter: &AclBinding) -> Result<Vec<AclBinding>> {
    unsupported()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> AclBinding {
        AclBinding {
            resource_type: "topic".into(),
            resource_name: "orders".into(),
            pattern_type: "literal".into(),
            principal: "User:alice".into(),
            host: "*".into(),
            operation: "read".into(),
            permission: "allow".into(),
        }
    }

    /// The IPC contract: a filter with no keys at all must deserialize, because
    /// "list everything" is the call the UI makes first.
    #[test]
    fn an_empty_filter_document_means_no_filtering() {
        let filter: AclFilter = serde_json::from_str("{}").expect("empty filter");
        assert_eq!(filter, AclFilter::default());
        let explicit: AclFilter =
            serde_json::from_str(r#"{"resource_type":null,"resource_name":null,"principal":null}"#)
                .expect("explicit nulls");
        assert_eq!(explicit, AclFilter::default());
    }

    /// Field names are the wire contract with apps/desktop/src, so they get a
    /// test rather than a comment.
    #[test]
    fn a_binding_serializes_with_the_names_the_ui_reads() {
        let json = serde_json::to_value(binding()).expect("serialize");
        for key in [
            "resource_type",
            "resource_name",
            "pattern_type",
            "principal",
            "host",
            "operation",
            "permission",
        ] {
            assert!(json.get(key).is_some(), "missing {key}: {json}");
        }
    }

    #[cfg(feature = "kafka")]
    #[test]
    fn creating_an_acl_needs_a_principal_and_a_host() {
        let mut without_principal = binding();
        without_principal.principal = "  ".into();
        let err = Parts::create(&without_principal).unwrap_err().to_string();
        assert!(err.contains("User:alice"), "{err}");

        let mut without_host = binding();
        without_host.host = String::new();
        let err = Parts::create(&without_host).unwrap_err().to_string();
        assert!(err.contains('*'), "{err}");
    }

    /// The blank-is-any rule [`acls_delete`] documents, at the one place it is
    /// implemented.
    #[cfg(feature = "kafka")]
    #[test]
    fn a_blank_filter_field_becomes_a_wildcard_but_a_created_one_may_not_be() {
        let mut wide = binding();
        wide.resource_name = String::new();
        wide.host = String::new();
        let parts = Parts::parse(&wide).expect("a filter may leave fields open");
        assert!(parts.resource_name.is_none());
        assert!(parts.host.is_none());

        assert!(
            Parts::create(&wide).is_err(),
            "a stored ACL has to name a resource"
        );
    }

    #[cfg(feature = "kafka")]
    #[test]
    fn a_filter_accepts_the_wildcards_a_stored_acl_cannot() {
        let wildcard = AclBinding {
            resource_type: "any".into(),
            resource_name: String::new(),
            pattern_type: "any".into(),
            principal: String::new(),
            host: String::new(),
            operation: "any".into(),
            permission: "any".into(),
        };
        NativeAcl::filter(&Parts::parse(&wildcard).expect("parse")).expect("librdkafka filter");
        assert!(
            NativeAcl::binding(&wildcard).is_err(),
            "\"any\" is not something Kafka can store"
        );
    }

    #[cfg(feature = "kafka")]
    #[test]
    fn a_valid_binding_reaches_librdkafka() {
        NativeAcl::binding(&binding()).expect("librdkafka binding");
    }

    #[cfg(feature = "kafka")]
    #[test]
    fn a_name_with_an_interior_nul_is_refused_rather_than_truncated() {
        let mut poisoned = binding();
        poisoned.resource_name = "orders\0evil".into();
        let err = Parts::parse(&poisoned).unwrap_err().to_string();
        assert!(err.contains("NUL"), "{err}");
    }
}
