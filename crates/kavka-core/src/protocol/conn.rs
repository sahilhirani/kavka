//! One socket to one broker: framing, correlation ids, and the ApiVersions
//! negotiation everything else in this module depends on.
//!
//! BLOCKING, like the rest of the core (docs/ARCHITECTURE.md) — the Tauri shell
//! wraps each call in `spawn_blocking`. Both directions carry a socket timeout
//! so a broker that accepts a connection and then says nothing fails as a
//! timeout rather than parking a pool thread forever.

use super::errors;
use super::wire::{malformed, Decoder, Encoder};
use crate::{Error, Result};
use rustls::ClientConfig;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

/// Matches `client.id` in [`crate::connection`], so one Kavka session looks
/// like one client in the broker's request log whichever path a call took.
const CLIENT_ID: &str = "kavka";
const CLIENT_SOFTWARE_NAME: &str = "kavka";
const CLIENT_SOFTWARE_VERSION: &str = env!("CARGO_PKG_VERSION");

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a request may say the BROKER has to do the work — the `TimeoutMs`
/// field the admin APIs carry. Matches `crate::admin::ADMIN_TIMEOUT`, so one
/// admin action in the app means one budget however it is routed.
pub(crate) const REQUEST_TIMEOUT_MS: i32 = 30_000;

/// Read/write budget on the socket. Deliberately LONGER than
/// [`REQUEST_TIMEOUT_MS`], so the broker's own deadline is always the one that
/// fires first: a Kafka REQUEST_TIMED_OUT names the operation that ran out of
/// time, whereas a socket timeout only says the connection went quiet.
const IO_TIMEOUT: Duration = Duration::from_secs(45);

/// Ceiling on one response. Kafka's own default `socket.request.max.bytes` is
/// 100 MiB; nothing this module asks for approaches it, and the point of the
/// bound is that a corrupt length prefix becomes an error rather than an
/// allocation.
const MAX_FRAME: usize = 100 * 1024 * 1024;

/// One Kafka API, with the version window THIS build can encode and decode.
///
/// The windows are narrow on purpose. Every version listed here is exercised —
/// by a golden-byte test, by the integration suite, or both — and a version
/// that is neither is not listed, because an encoder no test can reach is worse
/// than a refusal that names the API and both version ranges. See the module
/// docs on [`super`] for the per-API reasoning.
#[derive(Clone, Copy)]
pub(crate) struct Api {
    pub(crate) key: i16,
    pub(crate) name: &'static str,
    /// Lowest version this build implements.
    pub(crate) min: i16,
    /// Highest version this build implements.
    pub(crate) max: i16,
    /// First version of this API that uses KIP-482's flexible encoding;
    /// [`i16::MAX`] for an API that has none.
    first_flexible: i16,
}

impl Api {
    pub(crate) fn flexible(&self, version: i16) -> bool {
        version >= self.first_flexible
    }
}

pub(crate) const METADATA: Api = Api {
    key: 3,
    name: "Metadata",
    min: 9,
    max: 12,
    first_flexible: 9,
};
/// v4 is the first version whose reply is the `Coordinators` ARRAY rather than
/// a single flat coordinator — a different shape, not a wider one. Only the
/// share-group calls route through it, and those need Kafka 4.1 regardless, so
/// the older shape is refused rather than implemented untested.
pub(crate) const FIND_COORDINATOR: Api = Api {
    key: 10,
    name: "FindCoordinator",
    min: 4,
    max: 6,
    first_flexible: 3,
};
/// v5 only: `TypesFilter` (KIP-848) is the whole reason this module lists
/// groups at all — without it the broker answers with every group of every
/// type and "which of these are share groups" is unanswerable. The classic
/// list stays on librdkafka ([`crate::admin::groups_list`]).
pub(crate) const LIST_GROUPS: Api = Api {
    key: 16,
    name: "ListGroups",
    min: 5,
    max: 5,
    first_flexible: 3,
};
pub(crate) const SASL_HANDSHAKE: Api = Api {
    key: 17,
    name: "SaslHandshake",
    min: 1,
    max: 1,
    // SaslHandshake never became flexible: it is parsed before the connection
    // knows anything about the peer, so it is frozen at the legacy encoding.
    first_flexible: i16::MAX,
};
pub(crate) const API_VERSIONS: Api = Api {
    key: 18,
    name: "ApiVersions",
    min: 0,
    max: 3,
    first_flexible: 3,
};
pub(crate) const SASL_AUTHENTICATE: Api = Api {
    key: 36,
    name: "SaslAuthenticate",
    min: 2,
    max: 2,
    first_flexible: 2,
};
pub(crate) const ELECT_LEADERS: Api = Api {
    key: 43,
    name: "ElectLeaders",
    min: 2,
    max: 2,
    first_flexible: 2,
};
pub(crate) const ALTER_PARTITION_REASSIGNMENTS: Api = Api {
    key: 45,
    name: "AlterPartitionReassignments",
    min: 0,
    max: 0,
    first_flexible: 0,
};
pub(crate) const LIST_PARTITION_REASSIGNMENTS: Api = Api {
    key: 46,
    name: "ListPartitionReassignments",
    min: 0,
    max: 0,
    first_flexible: 0,
};
pub(crate) const DESCRIBE_CLIENT_QUOTAS: Api = Api {
    key: 48,
    name: "DescribeClientQuotas",
    min: 1,
    max: 1,
    first_flexible: 1,
};
pub(crate) const ALTER_CLIENT_QUOTAS: Api = Api {
    key: 49,
    name: "AlterClientQuotas",
    min: 1,
    max: 1,
    first_flexible: 1,
};
pub(crate) const DESCRIBE_QUORUM: Api = Api {
    key: 55,
    name: "DescribeQuorum",
    min: 0,
    max: 1,
    first_flexible: 0,
};
/// v1 only, and that is Kafka's own window: v0 carried KIP-932's early access
/// in Kafka 4.0 and was DELETED in 4.1 (`"validVersions": "1"`), so there is no
/// older version to be compatible with.
pub(crate) const SHARE_GROUP_DESCRIBE: Api = Api {
    key: 77,
    name: "ShareGroupDescribe",
    min: 1,
    max: 1,
    first_flexible: 0,
};
pub(crate) const DESCRIBE_SHARE_GROUP_OFFSETS: Api = Api {
    key: 90,
    name: "DescribeShareGroupOffsets",
    min: 0,
    max: 0,
    first_flexible: 0,
};

/// The KRaft feature flag that turns share groups on
/// (`kafka-features.sh upgrade --feature share.version=1`).
///
/// It is read from the ApiVersions reply rather than inferred, because the
/// broker advertises APIs 77 and 90 whether or not the feature is finalized —
/// verified against apache/kafka:4.1.0, which lists both at `share.version=0`
/// and then answers UNSUPPORTED_VERSION to every call. Without this the only
/// honest answer to "are there share groups here" would be an empty list.
pub(crate) const SHARE_VERSION_FEATURE: &str = "share.version";

/// A TCP or TLS socket, so the rest of the module never branches on which.
enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(buf),
            Stream::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(buf),
            Stream::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

pub(crate) struct BrokerConnection {
    stream: Stream,
    address: String,
    /// The address as `(host, port)`, so "is this the broker metadata just
    /// named?" is answered by comparison rather than by string equality against
    /// whatever the user typed — `SASL_SSL://broker:9093` and `broker:9093` are
    /// the same endpoint, and dialling a second socket to find that out is the
    /// cost this field avoids (see [`super::ProtocolClient::with_broker`]).
    endpoint: (String, u16),
    /// Monotonic per connection. Kafka guarantees in-order replies on one
    /// connection, so this is a consistency check rather than a demultiplexer —
    /// but it is the check that catches a decoder that read one byte too few on
    /// the previous response, which otherwise corrupts everything after it.
    correlation: i32,
    versions: HashMap<i16, (i16, i16)>,
    /// Cluster-wide FINALIZED feature levels, from the ApiVersions reply's
    /// tagged fields. Empty when the broker sent none, when it sent them with
    /// an unknown epoch, or when the trailer did not parse — see
    /// [`decode_finalized_features`], and [`Self::feature_level`] for why that
    /// is "unknown" rather than "zero".
    features: HashMap<String, i16>,
    /// Cleared the moment a call fails at the TRANSPORT — see
    /// [`Self::is_healthy`]. One-way: nothing sets it back.
    healthy: bool,
}

impl BrokerConnection {
    /// Opens the socket, completes the TLS handshake if there is one, and
    /// negotiates protocol versions. A returned connection is known-good.
    pub(crate) fn connect(address: &str, tls: Option<Arc<ClientConfig>>) -> Result<Self> {
        let (host, port) = split_host_port(address)?;
        let socket_addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| Error::Other(format!("could not resolve {address}: {e}")))?
            .next()
            .ok_or_else(|| Error::Other(format!("could not resolve {address}: no addresses")))?;
        let socket = TcpStream::connect_timeout(&socket_addr, CONNECT_TIMEOUT)
            .map_err(|e| Error::Other(format!("could not connect to {address}: {e}")))?;
        socket
            .set_read_timeout(Some(IO_TIMEOUT))
            .and_then(|()| socket.set_write_timeout(Some(IO_TIMEOUT)))
            .and_then(|()| socket.set_nodelay(true))
            .map_err(|e| {
                Error::Other(format!("could not configure the socket to {address}: {e}"))
            })?;

        let stream = match tls {
            None => Stream::Plain(socket),
            Some(config) => {
                let server_name = rustls::pki_types::ServerName::try_from(host.clone())
                    .map_err(|_| Error::Other(format!("{host} is not a valid TLS server name")))?
                    .to_owned();
                let session = rustls::ClientConnection::new(config, server_name).map_err(|e| {
                    Error::Other(format!("TLS handshake with {address} failed: {e}"))
                })?;
                Stream::Tls(Box::new(rustls::StreamOwned::new(session, socket)))
            }
        };

        let mut conn = Self {
            stream,
            address: address.to_string(),
            endpoint: (host, port),
            correlation: 0,
            versions: HashMap::new(),
            features: HashMap::new(),
            healthy: true,
        };
        conn.negotiate_api_versions()?;
        Ok(conn)
    }

    pub(crate) fn address(&self) -> &str {
        &self.address
    }

    /// Whether this socket is already pointed at the endpoint given — the
    /// host/port comparison, not a string one.
    ///
    /// Host names are compared case-insensitively (DNS is), and not resolved:
    /// two names for one machine read as two endpoints here, which costs a
    /// redundant connection and never a wrong one.
    pub(crate) fn is_at(&self, host: &str, port: u16) -> bool {
        self.endpoint.1 == port && self.endpoint.0.eq_ignore_ascii_case(host)
    }

    /// Whether the broker speaks this API AT ALL, at any version.
    ///
    /// Distinct from [`Self::negotiate`], which also fails when the windows do
    /// not overlap: "your Kafka has never heard of share groups" and "your
    /// Kafka speaks a version of them Kavka does not" are different sentences
    /// to the person reading them.
    pub(crate) fn supports(&self, api: Api) -> bool {
        self.versions.contains_key(&api.key)
    }

    /// The cluster-wide finalized level of a KRaft feature, or `None` when the
    /// broker did not say.
    ///
    /// `None` is NOT zero and must not be treated as "off": a broker that sends
    /// no feature table (anything before Kafka 2.7, or a reply whose trailer
    /// this build could not parse) has told us nothing, and refusing a feature
    /// on the strength of silence would break clusters that support it. The
    /// callers gate on `Some(level) if level < needed` and otherwise let the
    /// broker's own error code answer.
    pub(crate) fn feature_level(&self, name: &str) -> Option<i16> {
        self.features.get(name).copied()
    }

    /// Whether this socket's framing can still be trusted.
    ///
    /// A request that could not be written, a reply that could not be read, a
    /// frame length that cannot be a Kafka response, or a correlation id that
    /// does not match all leave the stream at an UNKNOWN offset — the next
    /// `call` would then read some other message's bytes as its own reply and
    /// decode nonsense from a connection that still looks open. A caller that
    /// keeps this connection alive between calls asks first and reconnects when
    /// the answer is false; the one-shot callers drop it either way.
    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// The highest version of `api` both this build and this broker speak.
    pub(crate) fn negotiate(&self, api: Api) -> Result<i16> {
        choose_version(api, &self.address, &self.versions)
    }

    /// Sends one request and returns the response BODY — the response header is
    /// consumed and checked here.
    ///
    /// A failure anywhere past the first byte written condemns the connection
    /// ([`Self::is_healthy`]): there is no way to tell how much of the request
    /// the broker saw or how much of the reply is still queued, so the stream
    /// cannot be resynchronised — only replaced.
    pub(crate) fn call(&mut self, api: Api, version: i16, body: Vec<u8>) -> Result<Vec<u8>> {
        self.correlation = self.correlation.wrapping_add(1);
        let correlation = self.correlation;
        // Built BEFORE anything is written, so an unencodable header fails
        // without leaving half a frame on the socket — and WITHOUT condemning a
        // connection that never saw a byte of it.
        let frame = request_frame(api, version, correlation, Some(CLIENT_ID), &body)?;

        let outcome = self.transact(api, version, correlation, &frame);
        if outcome.is_err() {
            self.healthy = false;
        }
        outcome
    }

    /// The half of [`Self::call`] that touches the socket.
    fn transact(
        &mut self,
        api: Api,
        version: i16,
        correlation: i32,
        frame: &[u8],
    ) -> Result<Vec<u8>> {
        self.stream
            .write_all(frame)
            .and_then(|()| self.stream.flush())
            .map_err(|e| {
                Error::Other(format!(
                    "sending {} to {} failed: {e}",
                    api.name, self.address
                ))
            })?;

        let payload = self.read_frame(api)?;
        let mut header = Decoder::new(&payload);
        let echoed = header.int32()?;
        if echoed != correlation {
            return Err(Error::Other(format!(
                "{} answered {} out of order (expected correlation {correlation}, got {echoed})",
                self.address, api.name
            )));
        }
        // The ApiVersions RESPONSE header is v0 at every version — the client
        // has not learned the broker's versions yet when it parses this one, so
        // Kafka freezes it. Every other flexible API uses response header v1.
        if api.flexible(version) && api.key != API_VERSIONS.key {
            header.tagged_fields()?;
        }
        Ok(payload[header.position()..].to_vec())
    }

    fn read_frame(&mut self, api: Api) -> Result<Vec<u8>> {
        let mut size = [0u8; 4];
        self.stream.read_exact(&mut size).map_err(|e| {
            Error::Other(format!(
                "reading the {} reply from {} failed: {e}",
                api.name, self.address
            ))
        })?;
        let size = i32::from_be_bytes(size);
        if !(4..=MAX_FRAME as i32).contains(&size) {
            return Err(Error::Other(format!(
                "{} sent a {size}-byte reply to {}, which cannot be a Kafka response",
                self.address, api.name
            )));
        }
        let mut buf = vec![0u8; size as usize];
        self.stream.read_exact(&mut buf).map_err(|e| {
            Error::Other(format!(
                "reading the {} reply from {} failed: {e}",
                api.name, self.address
            ))
        })?;
        Ok(buf)
    }

    /// ApiVersions is the one call that cannot negotiate its own version, so it
    /// guesses high and falls back.
    ///
    /// A broker that predates v3 answers UNSUPPORTED_VERSION — serialized at
    /// v0, and (per `KafkaApis.handleApiVersionsRequest`) with an EMPTY version
    /// table, so the fallback has to be a second request at v0 rather than a
    /// read of the error reply.
    fn negotiate_api_versions(&mut self) -> Result<()> {
        let body = api_versions_body(
            CLIENT_SOFTWARE_NAME,
            CLIENT_SOFTWARE_VERSION,
            API_VERSIONS.max,
        );
        let payload = self.call(API_VERSIONS, API_VERSIONS.max, body)?;
        let mut decoder = Decoder::new(&payload);
        let code = decoder.int16()?;

        let (versions, features) = if code == errors::UNSUPPORTED_VERSION {
            let payload = self.call(API_VERSIONS, 0, Vec::new())?;
            let mut decoder = Decoder::new(&payload);
            let code = decoder.int16()?;
            errors::check(
                "asking the broker which protocol versions it speaks",
                code,
                None,
            )?;
            decode_api_versions(&mut decoder, 0)?
        } else {
            errors::check(
                "asking the broker which protocol versions it speaks",
                code,
                None,
            )?;
            decode_api_versions(&mut decoder, API_VERSIONS.max)?
        };
        self.versions = versions;
        self.features = features;
        Ok(())
    }
}

/// The negotiation itself: pure over the broker's advertised table, so it is
/// unit-testable without a socket.
///
/// The failure messages name the API and BOTH version windows, because "the
/// broker is too old" is only actionable if the reader can see by how much —
/// and because the same sentence has to serve someone whose cluster is too old
/// and someone whose Kavka is.
fn choose_version(api: Api, address: &str, versions: &HashMap<i16, (i16, i16)>) -> Result<i16> {
    let Some(&(broker_min, broker_max)) = versions.get(&api.key) else {
        return Err(Error::Other(format!(
            "{address} does not support {} (API key {}) at all — this feature needs a newer Kafka",
            api.name, api.key
        )));
    };
    let chosen = api.max.min(broker_max);
    if chosen < api.min || chosen < broker_min {
        return Err(Error::Other(format!(
            "{address} speaks {} v{broker_min}-v{broker_max} and Kavka speaks v{}-v{} — no common \
             version, so {} cannot be used against this cluster",
            api.name, api.min, api.max, api.name
        )));
    }
    Ok(chosen)
}

/// `client_software_name`/`version` only exist from v3; below that the request
/// body is empty.
fn api_versions_body(software_name: &str, software_version: &str, version: i16) -> Vec<u8> {
    let mut enc = Encoder::new();
    if API_VERSIONS.flexible(version) {
        enc.compact_string(software_name)
            .compact_string(software_version)
            .tagged_fields();
    }
    enc.finish()
}

/// What a broker says about itself when a connection opens: the API version
/// table (key -> min, max) and the cluster's finalized feature levels
/// (name -> max level).
type BrokerCapabilities = (HashMap<i16, (i16, i16)>, HashMap<String, i16>);

/// The response body of ApiVersions, minus the leading error code the caller
/// has already read: the version table, and the finalized feature levels that
/// ride in its tagged fields.
fn decode_api_versions(decoder: &mut Decoder<'_>, version: i16) -> Result<BrokerCapabilities> {
    let flexible = API_VERSIONS.flexible(version);
    let count = if flexible {
        decoder.compact_array_len()?
    } else {
        decoder.legacy_array_len()?
    }
    .unwrap_or(0);

    let mut table = HashMap::with_capacity(count);
    for _ in 0..count {
        let key = decoder.int16()?;
        let min = decoder.int16()?;
        let max = decoder.int16()?;
        if flexible {
            decoder.tagged_fields()?;
        }
        table.insert(key, (min, max));
    }
    if table.is_empty() {
        return Err(malformed("the broker listed no supported APIs"));
    }
    // The trailer — throttle time, then (v3) the tag buffer carrying
    // SupportedFeatures, FinalizedFeaturesEpoch and FinalizedFeatures — used to
    // be left deliberately unread, on the rule that a decoder which stops at
    // what it needs cannot be broken by a field the broker adds. One thing in
    // there is now needed: `share.version`, the KRaft feature level that
    // decides whether share groups exist at all, and which no other reply
    // carries (the broker advertises the share APIs either way).
    //
    // The rule is kept where it counts. This read is BEST EFFORT: anything it
    // cannot parse leaves the feature map empty, which every caller already
    // treats as "the broker did not say" rather than as "off". A tagged field
    // this build has never seen is skipped by the walk itself. So the trailer
    // can grow, shrink or arrive mangled and a connection still opens.
    let features = decode_features_trailer(decoder, version).unwrap_or_default();
    Ok((table, features))
}

/// Kafka's own tag numbers on ApiVersionsResponse v3+.
const TAG_FINALIZED_FEATURES_EPOCH: u32 = 1;
const TAG_FINALIZED_FEATURES: u32 = 2;

/// The trailer of an ApiVersions v3+ reply, as a map of feature name to
/// finalized MAX version level.
///
/// `FinalizedFeaturesEpoch` gates the whole answer — Kafka's own schema says
/// "the information is valid only if FinalizedFeaturesEpoch >= 0", and -1 means
/// the broker has not learned the cluster's feature state yet (it is still
/// catching up on the metadata log). Reporting a stale or absent level as `0`
/// there would tell a user their cluster has share groups turned off during
/// exactly the window when it cannot say.
fn decode_features_trailer(
    decoder: &mut Decoder<'_>,
    version: i16,
) -> Result<HashMap<String, i16>> {
    if !API_VERSIONS.flexible(version) {
        return Ok(HashMap::new());
    }
    let _throttle_time_ms = decoder.int32()?;

    let mut epoch: i64 = -1;
    let mut finalized: Option<&[u8]> = None;
    decoder.tagged_fields_visit(|tag, payload| match tag {
        TAG_FINALIZED_FEATURES_EPOCH => {
            if let Ok(value) = Decoder::new(payload).int64() {
                epoch = value;
            }
        }
        TAG_FINALIZED_FEATURES => finalized = Some(payload),
        _ => {}
    })?;

    let (Some(payload), true) = (finalized, epoch >= 0) else {
        return Ok(HashMap::new());
    };
    let mut features = Decoder::new(payload);
    let count = features.compact_array_len()?.unwrap_or(0);
    let mut out = HashMap::with_capacity(count);
    for _ in 0..count {
        let name = features.compact_string()?;
        let max_version_level = features.int16()?;
        let _min_version_level = features.int16()?;
        features.tagged_fields()?;
        out.insert(name, max_version_level);
    }
    Ok(out)
}

/// Request header + body, length-prefixed.
///
/// Header v2 (flexible APIs) differs from v1 by a trailing tag buffer ONLY —
/// `client_id` stays a legacy int16-prefixed string, because the header schema
/// marks that one field `"flexibleVersions": "none"`.
fn request_frame(
    api: Api,
    version: i16,
    correlation: i32,
    client_id: Option<&str>,
    body: &[u8],
) -> Result<Vec<u8>> {
    let mut enc = Encoder::new();
    enc.int16(api.key).int16(version).int32(correlation);
    // Fallible: `client_id` is a legacy int16-prefixed string, which is the one
    // field in this module that can be too long to encode. It is a constant
    // here, so this never fires — the `?` is what guarantees that a future
    // caller passing something longer gets a refusal rather than a frame whose
    // declared length disagrees with its payload.
    enc.legacy_nullable_string(client_id)?;
    if api.flexible(version) {
        enc.tagged_fields();
    }
    let mut message = enc.finish();
    message.extend_from_slice(body);

    let mut framed = Vec::with_capacity(message.len() + 4);
    framed.extend_from_slice(&(message.len() as i32).to_be_bytes());
    framed.extend_from_slice(&message);
    Ok(framed)
}

/// `[scheme://]host[:port]`, defaulting to Kafka's 9092. IPv6 literals keep
/// their colons when bracketed.
fn split_host_port(address: &str) -> Result<(String, u16)> {
    let trimmed = address.trim();
    let trimmed = trimmed.split_once("://").map_or(trimmed, |(_, rest)| rest);
    let (host, port) = match trimmed.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((host, tail)) => (host, tail.strip_prefix(':')),
            None => return Err(Error::Other(format!("{address} has an unclosed [ ]"))),
        },
        None => match trimmed.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (trimmed, None),
        },
    };
    if host.is_empty() {
        return Err(Error::Other(format!("{address} names no host")));
    }
    let port = match port {
        None => 9092,
        Some(text) => text
            .parse()
            .map_err(|_| Error::Other(format!("{address} has an invalid port ({text})")))?,
    };
    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders bytes as lowercase hex so a mismatch is readable in the failure
    /// output instead of a wall of decimal.
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// GOLDEN BYTES — hand-verified against the Kafka protocol guide, field by
    /// field, for the request every connection opens with.
    ///
    /// 0000001d  frame length = 29
    ///   0012    api key 18 (ApiVersions)
    ///   0003    api version 3
    ///   00000001  correlation id 1
    ///   0005 6b61766b61  client id, LEGACY int16-prefixed: "kavka"
    ///   00      request header v2 tag buffer (empty)
    ///   06 6b61766b61    client_software_name, COMPACT (5 + 1 = 6): "kavka"
    ///   06 302e312e30    client_software_version, COMPACT: "0.1.0"
    ///   00      body tag buffer (empty)
    ///
    /// The mixed encodings on one line — legacy `client_id` inside an otherwise
    /// flexible header — are the reason this test exists.
    #[test]
    fn api_versions_v3_request_is_byte_for_byte() {
        let frame = request_frame(
            API_VERSIONS,
            3,
            1,
            Some("kavka"),
            &api_versions_body("kavka", "0.1.0", 3),
        )
        .expect("the client id fits");
        assert_eq!(
            hex(&frame),
            "0000001d0012000300000001\
             00056b61766b6100\
             066b61766b6106302e312e3000"
        );
        // The length prefix is not part of the length it declares.
        assert_eq!(frame.len(), 4 + 29);
    }

    /// v0 has no `client_software_*` and no tag buffers anywhere — the fallback
    /// path for a broker that refuses v3.
    ///
    /// 0000000f  frame length = 15 (2 + 2 + 4 + 7), two bytes shorter than the
    ///           v3 header because there is no tag buffer
    ///   0012 0000 00000007        key 18, version 0, correlation 7
    ///   0005 6b61766b61           client id "kavka" (no tag buffer follows)
    #[test]
    fn api_versions_v0_request_is_byte_for_byte() {
        let frame = request_frame(
            API_VERSIONS,
            0,
            7,
            Some("kavka"),
            &api_versions_body("kavka", "0.1.0", 0),
        )
        .expect("the client id fits");
        assert_eq!(hex(&frame), "0000000f001200000000000700056b61766b61");
        assert_eq!(frame.len(), 4 + 15);
        // v0 carries no client_software_* fields at all.
        assert!(api_versions_body("kavka", "0.1.0", 0).is_empty());
    }

    /// GOLDEN BYTES — an ApiVersions v0 response body, as an ancient broker
    /// would frame it.
    ///
    /// 0000    error code NONE
    /// 00000002  legacy array, 2 entries
    ///   0012 0000 0003   ApiVersions (18), v0-v3
    ///   0037 0000 0001   DescribeQuorum (55), v0-v1
    /// 00000000  throttle_time_ms (deliberately not read)
    #[test]
    fn an_api_versions_v0_response_decodes_into_the_version_table() {
        let body = [
            0x00, 0x00, // error code
            0x00, 0x00, 0x00, 0x02, // array length 2
            0x00, 0x12, 0x00, 0x00, 0x00, 0x03, // ApiVersions v0-v3
            0x00, 0x37, 0x00, 0x00, 0x00, 0x01, // DescribeQuorum v0-v1
            0x00, 0x00, 0x00, 0x00, // throttle time
        ];
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.int16().unwrap(), 0);
        let (table, features) = decode_api_versions(&mut decoder, 0).expect("decode");
        assert_eq!(table.get(&18), Some(&(0, 3)));
        assert_eq!(table.get(&55), Some(&(0, 1)));
        // v0 has no tagged fields at all, so there is nowhere for a feature
        // table to be — and "no answer" must not read as "the feature is off".
        assert!(features.is_empty());
    }

    /// The same table in the flexible (v3) encoding: compact array length
    /// `n + 1`, and a tag buffer after each entry.
    #[test]
    fn an_api_versions_v3_response_decodes_into_the_version_table() {
        let body = [
            0x00, 0x00, // error code
            0x03, // compact array, 2 entries
            0x00, 0x12, 0x00, 0x00, 0x00, 0x03, 0x00, // ApiVersions v0-v3 + tags
            0x00, 0x37, 0x00, 0x00, 0x00, 0x01, 0x00, // DescribeQuorum v0-v1 + tags
            0x00, 0x00, 0x00, 0x00, // throttle time
            0x00, // top-level tags
        ];
        let mut decoder = Decoder::new(&body);
        assert_eq!(decoder.int16().unwrap(), 0);
        let (table, features) = decode_api_versions(&mut decoder, 3).expect("decode");
        assert_eq!(table.len(), 2);
        assert_eq!(table.get(&18), Some(&(0, 3)));
        assert!(features.is_empty(), "this reply carries no tagged fields");
    }

    /// An ApiVersions v3 reply built the way a KRaft broker builds one: the
    /// version table, then a tag buffer carrying the finalized feature epoch
    /// (tag 1) and the finalized feature list (tag 2).
    ///
    /// `epoch` is a parameter because it is the field that decides whether the
    /// list may be believed at all.
    fn api_versions_v3_with_features(epoch: i64, features: &[(&str, i16, i16)]) -> Vec<u8> {
        let mut list = Encoder::new();
        list.compact_array_len(Some(features.len()));
        for (name, max_level, min_level) in features {
            list.compact_string(name)
                .int16(*max_level)
                .int16(*min_level)
                .tagged_fields();
        }
        let list = list.finish();

        let mut enc = Encoder::new();
        enc.int16(0) // error code
            .compact_array_len(Some(1))
            .int16(SHARE_GROUP_DESCRIBE.key)
            .int16(1)
            .int16(1)
            .tagged_fields()
            .int32(0); // throttle_time_ms
                       // Two tagged fields: FinalizedFeaturesEpoch (int64) and
                       // FinalizedFeatures (a compact array, encoded as the tag's payload).
        enc.uvarint(2)
            .uvarint(TAG_FINALIZED_FEATURES_EPOCH)
            .uvarint(8);
        enc.int64(epoch);
        enc.uvarint(TAG_FINALIZED_FEATURES)
            .uvarint(list.len() as u32);
        let mut body = enc.finish();
        body.extend_from_slice(&list);
        body
    }

    fn features_of(body: &[u8]) -> HashMap<String, i16> {
        let mut decoder = Decoder::new(body);
        assert_eq!(decoder.int16().unwrap(), 0, "error code");
        decode_api_versions(&mut decoder, 3).expect("decode").1
    }

    /// The feature level share groups are gated on, read out of the reply every
    /// connection already makes.
    #[test]
    fn finalized_feature_levels_are_read_from_the_api_versions_trailer() {
        let features = features_of(&api_versions_v3_with_features(
            72_247,
            &[
                (SHARE_VERSION_FEATURE, 1, 0),
                ("metadata.version", 28, 7),
                ("transaction.version", 2, 0),
            ],
        ));
        assert_eq!(features.get(SHARE_VERSION_FEATURE), Some(&1));
        // The MAX level is the finalized one; the min is read past, not kept.
        assert_eq!(features.get("metadata.version"), Some(&28));
        assert_eq!(features.len(), 3);

        // The disabled cluster: present, and zero. That is a different answer
        // from silence, and the whole reason this is read.
        let off = features_of(&api_versions_v3_with_features(
            72_247,
            &[(SHARE_VERSION_FEATURE, 0, 0)],
        ));
        assert_eq!(off.get(SHARE_VERSION_FEATURE), Some(&0));
    }

    /// Kafka's schema says the finalized feature list is valid only when
    /// `FinalizedFeaturesEpoch` is zero or more. A broker still catching up on
    /// the metadata log sends -1, and believing its list would report every
    /// feature as off for exactly as long as the broker cannot say.
    #[test]
    fn an_unknown_feature_epoch_discards_the_whole_feature_list() {
        let features = features_of(&api_versions_v3_with_features(
            -1,
            &[(SHARE_VERSION_FEATURE, 1, 0)],
        ));
        assert!(features.is_empty(), "got {features:?}");
    }

    /// The trailer is BEST EFFORT: the version table is what a connection needs
    /// to work, and a feature list that cannot be parsed must cost a feature
    /// gate, never the connection.
    #[test]
    fn a_mangled_feature_trailer_still_yields_the_version_table() {
        let full = api_versions_v3_with_features(1, &[(SHARE_VERSION_FEATURE, 1, 0)]);
        // Cut inside the tagged-field buffer, past the version table.
        let truncated = &full[..full.len() - 4];
        let mut decoder = Decoder::new(truncated);
        assert_eq!(decoder.int16().unwrap(), 0);
        let (table, features) = decode_api_versions(&mut decoder, 3).expect("the table survives");
        assert_eq!(table.get(&SHARE_GROUP_DESCRIBE.key), Some(&(1, 1)));
        assert!(features.is_empty(), "unparseable is unknown, not zero");
    }

    fn negotiate_with(table: &[(i16, (i16, i16))], api: Api) -> Result<i16> {
        choose_version(api, "broker-1:9092", &table.iter().copied().collect())
    }

    #[test]
    fn negotiation_picks_the_highest_version_both_sides_speak() {
        // Broker newer than us: capped at what this build implements.
        let table = [(DESCRIBE_QUORUM.key, (0, 2))];
        assert_eq!(
            negotiate_with(&table, DESCRIBE_QUORUM).expect("negotiate"),
            1
        );
        // Broker older than us but inside our window.
        let table = [(DESCRIBE_QUORUM.key, (0, 0))];
        assert_eq!(
            negotiate_with(&table, DESCRIBE_QUORUM).expect("negotiate"),
            0
        );
        // Exact match.
        let table = [(ELECT_LEADERS.key, (0, 2))];
        assert_eq!(negotiate_with(&table, ELECT_LEADERS).expect("negotiate"), 2);
    }

    #[test]
    fn a_broker_too_old_for_an_api_is_refused_by_name() {
        // Supports ElectLeaders, but only the versions Kavka does not implement.
        let err = negotiate_with(&[(ELECT_LEADERS.key, (0, 1))], ELECT_LEADERS)
            .expect_err("no common version");
        let message = err.to_string();
        assert!(message.contains("ElectLeaders"), "got {message}");
        assert!(message.contains("v0-v1"), "got {message}");
        assert!(message.contains("v2-v2"), "got {message}");
        assert!(message.contains("broker-1:9092"), "got {message}");
    }

    #[test]
    fn an_api_the_broker_never_heard_of_is_refused_by_name_and_key() {
        let err =
            negotiate_with(&[(METADATA.key, (0, 12))], DESCRIBE_QUORUM).expect_err("no such API");
        let message = err.to_string();
        assert!(message.contains("DescribeQuorum"), "got {message}");
        assert!(message.contains("API key 55"), "got {message}");
        assert!(message.contains("newer Kafka"), "got {message}");
    }

    #[test]
    fn addresses_split_into_host_and_port() {
        for (address, host, port) in [
            ("localhost:9092", "localhost", 9092u16),
            ("broker-1.internal", "broker-1.internal", 9092),
            (" broker:9093 ", "broker", 9093),
            (
                "SASL_SSL://b-1.kafka.eu-west-1.amazonaws.com:9098",
                "b-1.kafka.eu-west-1.amazonaws.com",
                9098,
            ),
            ("[2001:db8::1]:9092", "2001:db8::1", 9092),
            ("[::1]", "::1", 9092),
        ] {
            assert_eq!(
                split_host_port(address).expect(address),
                (host.to_string(), port),
                "for {address}"
            );
        }
    }

    #[test]
    fn a_malformed_address_is_refused_with_the_address_in_the_message() {
        for address in ["broker:not-a-port", "[2001:db8::1", ":9092"] {
            let err = split_host_port(address).expect_err(address).to_string();
            assert!(err.contains(address.trim()), "got {err}");
        }
    }
}
