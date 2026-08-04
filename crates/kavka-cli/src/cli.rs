//! The command line: what `kavka` accepts, and the pure conversions from it
//! into kavka-core's own specs.
//!
//! **The doc comments in this file are the help text.** clap derives `--help`
//! from them, so a sentence here is a sentence a user reads at 3am — every one
//! of them states the unit, the cap and the default, for the same reason
//! `kavka-mcp`'s tool descriptions do: the reader is deciding what to type
//! next. docs/DESIGN.md §7's rules apply to a terminal unchanged — sentence
//! case, the verb survives, no "Invalid input", and the Kafka term is never
//! hidden.
//!
//! Everything below the argument types is pure: [`ScanArgs::seek_spec`],
//! [`parse_since`] and [`ProduceArgs::record`] take arguments and return specs
//! or a [`CliError`], with no cluster, no clock they did not receive and no
//! I/O except the one stream `--value-file` names. That is what makes the flag
//! surface testable without a broker.
//!
//! # Every numeric flag states its range and clap enforces it
//!
//! A range in a help text that the parser does not enforce is a help text that
//! lies: `--max 5000` was accepted, silently clamped to 2 000 by the core, and
//! the answer said "returned the cap of 5 000" about 2 000 records. So every
//! bounded flag below carries a `value_parser` whose range IS the range in its
//! own doc comment, and the two are read together — change one and change the
//! other. The bounds come from kavka-core's own constants
//! ([`kavka_core::consume::MAX_MESSAGES`], [`kavka_core::search::MAX_BUFFERED`],
//! [`kavka_core::sql::MAX_SCANNED`], [`kavka_core::sql::MAX_ROWS`]) rather than
//! from literals, so a ceiling that moves in the core moves here.
//!
//! Being refused at the front door is better than being clamped: clap's error
//! names the flag, the value and the range, exits 2, and costs no round trip to
//! a broker.

use crate::errors::{CliError, ExitCode};
use crate::output::Mode;
use clap::{value_parser, Args, Parser, Subcommand, ValueEnum};
use kavka_core::consume::{SeekSpec, MAX_MESSAGES};
use kavka_core::produce::{ProduceHeader, ProduceRecordSpec, ProduceValueSpec};
use kavka_core::search::MAX_BUFFERED;
use kavka_core::sql::{MAX_ROWS, MAX_SCANNED};
use std::io::Read;
use std::path::{Path, PathBuf};

/// `--value-file -` reads the payload from standard input.
const STDIN: &str = "-";

/// Kavka's Kafka client, on a terminal.
#[derive(Parser, Debug)]
#[command(
    name = "kavka",
    version,
    about = "Kavka's Kafka client, on a terminal — the connections saved in the app.",
    // Hand-wrapped to 78 columns, here and in every `long_help` below. clap's
    // `wrap_help` would do it against the real terminal width and costs four
    // more crates (the measurement is in Cargo.toml); this program's help is
    // written once and read on every width, so the wrapping is done by the
    // author rather than at runtime.
    long_about = "Kavka's Kafka client, on a terminal.\n\n\
        It reads the connections the Kavka desktop app saved on this machine —\n\
        the same profiles.json, the same OS keychain, the same read-only and\n\
        masking rules — so there is nothing to configure here. Start with\n\
        `kavka profiles list`.\n\n\
        stdout is the answer; stderr is everything Kavka has to say about it. So\n\
        `kavka fetch orders | jq` and `kavka search orders failed > hits.ndjson`\n\
        are correct with no flags at all: a terminal gets a table, a pipe gets\n\
        NDJSON.",
    after_help = "EXIT CODES\n  \
        0  answered — an empty result is still an answer\n  \
        1  the cluster, the keychain or the profile file failed\n  \
        2  the command line was wrong\n  \
        3  a guardrail refused it: read-only, or a protected environment\n     \
           without --yes-prod. Nothing was sent\n  \
        4  no such connection on this machine\n\n\
    EXAMPLES\n  \
        kavka profiles list\n  \
        kavka -p orders topics list\n  \
        kavka -p orders fetch orders --last 20\n  \
        kavka -p orders search orders \"failed\" --earliest\n  \
        kavka -p orders search orders --cel 'value.amount > 100' --since 2h\n  \
        kavka -p orders sql orders \"select count(*) from messages\"\n  \
        kavka -p orders produce dead-letter --key A-102 --json '{\"retry\":1}'\n\n\
    ENVIRONMENT\n  \
        KAVKA_PROFILE     the connection to use when --profile is not given\n  \
        KAVKA_CONFIG_DIR  the folder holding profiles.json, instead of the app's own"
)]
pub struct Cli {
    /// Which saved connection to use — its id, or its name.
    #[arg(
        long,
        short = 'p',
        global = true,
        value_name = "ID|NAME",
        env = "KAVKA_PROFILE"
    )]
    pub profile: Option<String>,

    /// How to write the answer. Default: a table on a terminal, NDJSON in a pipe.
    #[arg(long, short = 'o', global = true, value_name = "FORMAT")]
    pub output: Option<Format>,

    /// Print the broker's own reply under any failure.
    #[arg(long, global = true)]
    pub details: bool,

    /// Return payloads exactly as the cluster holds them.
    #[arg(
        long,
        global = true,
        long_help = "Return payloads exactly as the cluster holds them.\n\n\
            By default Kavka applies the display-masking rules saved for this\n\
            connection in the app, and says so on stderr whenever a rule\n\
            rewrote something. A terminal is exactly the surface those rules\n\
            were written for — the screen share, the pasted snippet, the\n\
            scrollback — so honouring them is the default here as it is in the\n\
            app itself."
    )]
    pub unmasked: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// `--output`. [`Mode`] itself stays clap-free so the renderer has no opinion
/// about how it was chosen.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A human-readable table.
    Table,
    /// One JSON object per line — the rows, and nothing else.
    Ndjson,
    /// One document: the rows, plus the limits and progress they came with.
    Json,
}

impl From<Format> for Mode {
    fn from(format: Format) -> Self {
        match format {
            Format::Table => Mode::Table,
            Format::Ndjson => Mode::Ndjson,
            Format::Json => Mode::Json,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// The Kafka connections saved in the Kavka app on this machine.
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
    /// Topics on one cluster.
    Topics {
        #[command(subcommand)]
        command: TopicsCommand,
    },
    /// Read a bounded window of a topic.
    Fetch(FetchArgs),
    /// Scan a topic for records matching a substring, a CEL expression, or both.
    Search(SearchArgs),
    /// Run SQL over a bounded scan of a topic.
    Sql(SqlArgs),
    /// Send one record to a topic.
    Produce(ProduceArgs),
    /// Consumer groups, their members and their lag.
    Groups {
        #[command(subcommand)]
        command: GroupsCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProfilesCommand {
    /// Every connection saved on this machine. Contacts nothing.
    #[command(long_about = "Every connection saved on this machine: id, name,\n\
            environment, bootstrap servers, sign-in method, and whether the\n\
            connection is read-only.\n\n\
            START HERE — every other command takes one of these with\n\
            --profile. It contacts nothing: it reads the app's own\n\
            profiles.json, so it answers with every broker unreachable.\n\
            Secrets are never in it; a profile carries a keychain reference,\n\
            not a password.")]
    List,
}

#[derive(Subcommand, Debug)]
pub enum TopicsCommand {
    /// Every topic on the cluster.
    List(TopicsListArgs),
    /// One topic: its partitions, leaders, ISR, watermarks and configuration.
    Detail(TopicDetailArgs),
}

#[derive(Args, Debug)]
pub struct TopicsListArgs {
    /// Only topics whose name contains this, case-insensitively.
    #[arg(long, value_name = "TEXT")]
    pub contains: Option<String>,
    /// Include Kafka's own internal topics (__consumer_offsets and friends).
    #[arg(long)]
    pub internal: bool,
    /// Most topics to list, 1 or more. Default 500 — a cluster with 20,000 of
    /// them is a real thing.
    ///
    /// Spelled out rather than `value_parser!(usize)`, which is the one
    /// integer type clap has no factory for — the macro would hand back an
    /// unranged parser and the bound above would be a claim nothing checks.
    #[arg(
        long,
        default_value_t = 500,
        value_name = "N",
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..)
    )]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct TopicDetailArgs {
    /// The topic.
    pub topic: String,
    /// Also show configuration inherited from the broker.
    #[arg(
        long,
        long_help = "Also show configuration inherited from the broker.

            Without this, only the entries set on the topic itself are shown —
            which is what \"how does this topic differ from the default\" means,
            and it is usually five lines instead of forty."
    )]
    pub defaults: bool,
}

#[derive(Subcommand, Debug)]
pub enum GroupsCommand {
    /// Every consumer group on the cluster, with its state and member count.
    List,
    /// One group: its members, and its lag on every partition it has committed.
    Detail {
        /// The group id.
        group: String,
    },
}

// ---------------------------------------------------------------------------
// Reading records
// ---------------------------------------------------------------------------

/// Where a read starts. Mirrors `kavka_core::consume::SeekSpec` one flag per
/// variant, which is what keeps "the app and the CLI seek the same way" a
/// property of the type rather than of a comment.
///
/// The four are mutually exclusive; with none of them, a read takes the newest
/// records of each partition (the bound differs per command and is named in
/// each command's help).
#[derive(Args, Debug, Default)]
#[group(multiple = false)]
pub struct SeekArgs {
    /// Start at the oldest record each partition still holds.
    #[arg(long)]
    pub earliest: bool,
    /// Start N records back from the end of each partition, N at least 1.
    #[arg(long, value_name = "N", value_parser = value_parser!(u32).range(1..))]
    pub last: Option<u32>,
    /// Start at this offset, 0 or greater. Needs exactly one --partition.
    ///
    /// `allow_negative_numbers` stays even though the range refuses them: with
    /// it, `--offset -1` is a value out of range, which names the flag and the
    /// bound; without it, clap reads `-1` as an unknown flag and says so
    /// instead. Same refusal, better sentence.
    #[arg(
        long,
        value_name = "OFFSET",
        allow_negative_numbers = true,
        value_parser = value_parser!(i64).range(0..)
    )]
    pub offset: Option<i64>,
    /// Start at the first record at or after this time.
    #[arg(
        long,
        value_name = "WHEN",
        long_help = "Start at the first record at or after this time.\n\n\
            Two forms: an age — 30s, 45m, 6h, 7d — counted back from now, or\n\
            epoch milliseconds. A bare number is epoch milliseconds, which is\n\
            the unit every answer here prints, so what you read in one command\n\
            can be pasted into the next."
    )]
    pub since: Option<String>,
}

/// The seek, plus which partitions to read. Flattened into every command that
/// reads records.
#[derive(Args, Debug, Default)]
pub struct ScanArgs {
    #[command(flatten)]
    pub seek: SeekArgs,
    /// Read only this partition, 0 or greater. Repeat for several; the default
    /// is all of them.
    #[arg(long, value_name = "N", value_parser = value_parser!(i32).range(0..))]
    pub partition: Vec<i32>,
}

impl ScanArgs {
    /// `None` means every partition — the shape kavka-core takes.
    pub fn partitions(&self) -> Option<Vec<i32>> {
        if self.partition.is_empty() {
            None
        } else {
            Some(self.partition.clone())
        }
    }

    /// The seek these flags describe.
    ///
    /// `default_last_n` is the window a command reads when no seek flag was
    /// given: enough records for its own cap to be reachable, which is why it
    /// is the caller's number rather than a constant here. `now_ms` is passed
    /// in rather than read, so `--since 2h` is testable.
    pub fn seek_spec(&self, default_last_n: u32, now_ms: i64) -> Result<SeekSpec, CliError> {
        let seek = &self.seek;
        if seek.earliest {
            return Ok(SeekSpec::Earliest);
        }
        if let Some(last_n) = seek.last {
            return Ok(SeekSpec::Latest {
                last_n: last_n.max(1),
            });
        }
        if let Some(offset) = seek.offset {
            // An offset is a position inside ONE partition, so reading it
            // across a topic is a question with no answer. Naming the fix
            // rather than the failure, per §7.
            let [partition] = self.partition[..] else {
                return Err(CliError::stated(
                    ExitCode::Usage,
                    "--offset needs to know which partition",
                    format!(
                        "An offset is a position inside one partition, and {given} \
                         --partition were given. Add exactly one, e.g. --partition 0 --offset \
                         {offset}.",
                        given = if self.partition.is_empty() {
                            "none".to_string()
                        } else {
                            self.partition.len().to_string()
                        },
                    ),
                ));
            };
            return Ok(SeekSpec::Offset { partition, offset });
        }
        if let Some(since) = &seek.since {
            return Ok(SeekSpec::Timestamp {
                timestamp_ms: parse_since(now_ms, since)?,
            });
        }
        Ok(SeekSpec::Latest {
            last_n: default_last_n.max(1),
        })
    }
}

/// `--since`: epoch milliseconds, or an age like `30s`, `45m`, `6h`, `7d`.
///
/// A bare number is epoch milliseconds — the same unit
/// `SeekSpec::Timestamp` takes, and the same one every Kafka tool prints — so
/// what a user copies out of one answer can be pasted into the next. Anything
/// with a unit suffix is relative to now, which is what a person types.
pub fn parse_since(now_ms: i64, value: &str) -> Result<i64, CliError> {
    let text = value.trim();
    if let Ok(epoch_ms) = text.parse::<i64>() {
        return Ok(epoch_ms);
    }
    let (digits, unit) = text.split_at(text.len().saturating_sub(1));
    let scale = match unit {
        "s" => 1_000i64,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => 0,
    };
    let age = digits.parse::<i64>().ok().filter(|value| *value >= 0);
    match (scale, age) {
        (0, _) | (_, None) => Err(CliError::stated(
            ExitCode::Usage,
            format!("Can't read {text:?} as a time"),
            "--since takes epoch milliseconds (1735689600000) or an age (30s, 45m, 6h, 7d).",
        )),
        (scale, Some(age)) => Ok(now_ms - age.saturating_mul(scale)),
    }
}

#[derive(Args, Debug)]
pub struct FetchArgs {
    /// The topic to read.
    pub topic: String,
    #[command(flatten)]
    pub scan: ScanArgs,
    /// Most records to return, 1–2000. Default 50.
    #[arg(
        long,
        default_value_t = 50,
        value_name = "N",
        value_parser = value_parser!(u32).range(1..=i64::from(MAX_MESSAGES)),
        long_help = "Most records to return, 1–2000. Default 50.\n\n\
            2000 is kavka-core's own ceiling for one fetch, so a larger number\n\
            is refused here rather than clamped somewhere the answer would\n\
            then misreport.\n\n\
            Which end of the window it keeps follows the seek: the NEWEST\n\
            records with --last or with no seek flag at all, and the FIRST\n\
            records from the seek with --earliest, --offset or --since."
    )]
    pub max: u32,
    /// Bytes of decoded key and value text to carry per record, at least 1.
    #[arg(
        long,
        value_name = "N",
        value_parser = value_parser!(u32).range(1..),
        long_help = "Bytes of decoded key and value text to carry per record,\n\
            at least 1. Defaults to kavka-core's own 262 144.\n\n\
            Longer payloads are cut for display and the record still reports\n\
            its true byte length, so a cut is never silent."
    )]
    pub max_value_bytes: Option<u32>,
    /// Every field kavka-core decoded, rather than the compact shape.
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// The topic to scan.
    pub topic: String,
    /// Raw bytes to look for, case-insensitively.
    #[arg(
        value_name = "SUBSTRING",
        long_help = "Raw bytes to look for, case-insensitively.\n\n\
            Matched against the key, the value, and header names and values,\n\
            before anything is decoded — a record that does not contain it is\n\
            never deserialized, which is what makes a scan fast. Give this, a\n\
            --cel expression, or both (both is an AND)."
    )]
    pub substring: Option<String>,
    /// A CEL expression over the decoded record.
    #[arg(
        long,
        value_name = "EXPR",
        long_help = "A CEL expression over the decoded record.\n\n  \
            value.status == \"failed\"        the value as a map, for JSON\n  \
            key.startsWith(\"A-\")            the key as text\n  \
            value_text.contains(\"timeout\")  the body whatever its shape\n  \
            timestamp_ms > 1735689600000    epoch milliseconds\n\n\
            Use value_text to look anywhere in the body: `value` has the SHAPE\n\
            of the payload — a map for JSON — so string(value) is an error on\n\
            every JSON record, while value_text is bound for every record\n\
            (a tombstone's is \"\") and holds the text the table is showing."
    )]
    pub cel: Option<String>,
    #[command(flatten)]
    pub scan: ScanArgs,
    /// Most matches to return, 1–10 000. Default 50.
    #[arg(
        long,
        default_value_t = 50,
        value_name = "N",
        value_parser = value_parser!(u32).range(1..=i64::from(MAX_BUFFERED)),
        long_help = "Most matches to return, 1–10 000. Default 50.\n\n\
            The scan keeps counting past this rather than stopping, which is\n\
            what lets the summary say \"the first 50 of 4 812 matches\" — a\n\
            search that stopped at the cap could only ever report 50. 10 000\n\
            is how many kavka-core will hold at once."
    )]
    pub max_matches: u32,
    /// Most records to read off the topic before stopping, 1 or more.
    #[arg(
        long,
        default_value_t = 20_000,
        value_name = "N",
        value_parser = value_parser!(u32).range(1..)
    )]
    pub scan_cap: u32,
    /// Give up after this long, whatever the scan has found. At least 1 ms.
    #[arg(
        long,
        default_value_t = 30_000,
        value_name = "MS",
        value_parser = value_parser!(u32).range(1..)
    )]
    pub timeout_ms: u32,
    /// Bytes of decoded key and value text to carry per match, at least 1.
    #[arg(long, value_name = "N", value_parser = value_parser!(u32).range(1..))]
    pub max_value_bytes: Option<u32>,
    /// Every field kavka-core decoded, rather than the compact shape.
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Args, Debug)]
pub struct SqlArgs {
    /// The topic to query.
    pub topic: String,
    /// The query. The one table is `messages`.
    #[arg(
        long_help = "The query. The one table is `messages` — the scan window \
            of this\ntopic. There is no FROM to choose and no catalogue to \
            browse.\n\n  \
            partition     BIGINT   never null\n  \
            offset        BIGINT   a SQL keyword, so quote it: \"offset\"\n  \
            timestamp_ms  BIGINT   null before KIP-32\n  \
            key_text      VARCHAR  null for a keyless record\n  \
            value_text    VARCHAR  null for a tombstone\n  \
            value_json    VARCHAR  COMPACT JSON TEXT when it decoded to JSON\n  \
            headers_json  VARCHAR  every header as JSON, {} when there are none\n\n\
            This build ships no JSON functions, so value_json is matched as\n\
            text: WHERE value_json LIKE '%\"status\":\"failed\"%'. It is compact\n\
            — no space after : or , — which is what makes that pattern stable.\n\n\
            Reads only: CREATE, INSERT, UPDATE, DELETE, COPY and SET are\n\
            refused before anything runs."
    )]
    pub query: String,
    #[command(flatten)]
    pub scan: ScanArgs,
    /// Records to read off the topic before the query runs on what it has,
    /// 1–100 000. Default 20 000.
    #[arg(
        long,
        default_value_t = 20_000,
        value_name = "N",
        value_parser = value_parser!(u32).range(1..=i64::from(MAX_SCANNED))
    )]
    pub scan_cap: u32,
    /// Most rows to return, 1–10 000. Default 100.
    #[arg(
        long,
        default_value_t = 100,
        value_name = "N",
        value_parser = value_parser!(u32).range(1..=i64::from(MAX_ROWS))
    )]
    pub max_rows: u32,
    /// Give up after this long. At least 1 ms.
    #[arg(
        long,
        default_value_t = 30_000,
        value_name = "MS",
        value_parser = value_parser!(u32).range(1..)
    )]
    pub timeout_ms: u32,
}

// ---------------------------------------------------------------------------
// Writing one record
// ---------------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct ProduceArgs {
    /// The topic to send to.
    pub topic: String,
    /// The record's key. Without one, Kafka partitions round-robin.
    #[arg(long, short = 'k', value_name = "TEXT")]
    pub key: Option<String>,
    #[command(flatten)]
    pub payload: Payload,
    /// A header, as name=value. Repeatable.
    #[arg(
        long = "header",
        short = 'H',
        value_name = "NAME=VALUE",
        long_help = "A header, as name=value. Repeatable.\n\n\
            The value may be empty (--header retry=); the name may not. Kafka\n\
            allows the same name twice and so does this — two --header flags\n\
            with one name send two headers, they do not overwrite."
    )]
    pub headers: Vec<String>,
    /// Send to this partition, 0 or greater, instead of letting Kafka's
    /// partitioner choose.
    #[arg(long, value_name = "N", value_parser = value_parser!(i32).range(0..))]
    pub partition: Option<i32>,
    /// Required before this writes to a connection tagged `prod`.
    #[arg(
        long,
        long_help = "Required when the connection's environment is marked\n\
            PROTECTED in the Kavka app.\n\n\
            Environments are yours to define — `dev`, `QA`, `UAT`, `Production`\n\
            — and each one is protected or not. A protected environment always\n\
            asks; an unprotected one never does, whatever it is called. Without\n\
            this flag the command exits 3 and nothing is sent. `kavka profiles\n\
            list --output json` reports `environment_protected` per connection,\n\
            so a script can tell without guessing from the name.\n\n\
            The flag is spelled `--yes-prod` for the shell histories and CI\n\
            scripts that already carry it. A connection marked read-only\n\
            refuses either way — no flag lifts that, because it is a property\n\
            of the connection rather than of this command."
    )]
    pub yes_prod: bool,
}

/// What goes in the record's value. Exactly one, and one is required: a
/// produce with no payload at all is a mistake, and a tombstone — which is a
/// null value, not an empty one — has to be asked for by name.
///
/// Avro/Protobuf/JSON Schema framing against a Schema Registry is the app's
/// produce panel; this sends text or JSON bytes.
#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
pub struct Payload {
    /// The value, as text. On a shared machine, prefer --value-file.
    #[arg(
        long,
        short = 'v',
        value_name = "TEXT",
        long_help = "The value, as text.\n\n\
            ON A SHARED MACHINE, PREFER --value-file. A command's arguments\n\
            are visible to every other account on the box for as long as the\n\
            command runs — `ps auxww`, /proc/<pid>/cmdline, Windows' own\n\
            process listing — and they are written to the shell's history file\n\
            afterwards. That is fine for `order-102` and wrong for a payload\n\
            carrying a token, a customer record or anything else you would not\n\
            paste into a ticket. --value-file reads the same bytes from a file\n\
            or from standard input, and neither ever becomes an argv."
    )]
    pub value: Option<String>,
    /// The value, as JSON. Parsed here, so a typo is caught before anything is
    /// sent.
    #[arg(long, value_name = "JSON")]
    pub json: Option<String>,
    /// The value, read from a UTF-8 file — or from standard input with `-`.
    #[arg(
        long,
        value_name = "PATH",
        long_help = "The value, read from a UTF-8 file.\n\n\
            `--value-file -` reads standard input instead, so a payload can\n\
            arrive down a pipe or a here-doc:\n\n  \
            kavka produce dead-letter --value-file payload.json\n  \
            jq -c . big.json | kavka produce dead-letter --value-file -\n\n\
            The payload never appears in the command line, which is what makes\n\
            this the right form on a shared machine (see --value) and the only\n\
            comfortable one for anything longer than a line. Mutually\n\
            exclusive with --value, --json and --tombstone, like every payload\n\
            form here: one record has one value."
    )]
    pub value_file: Option<PathBuf>,
    /// Send a null value — a tombstone.
    #[arg(
        long,
        long_help = "Send a null value — a tombstone.\n\n\
            On a compacted topic that DELETES the key. A null value is not an\n\
            empty one, which is why it has a flag of its own rather than an\n\
            empty --value."
    )]
    pub tombstone: bool,
}

impl ProduceArgs {
    /// The record these flags describe. The only I/O is `--value-file`'s file
    /// (or standard input, when it is `-`).
    pub fn record(&self) -> Result<ProduceRecordSpec, CliError> {
        Ok(ProduceRecordSpec {
            key: self.key.clone(),
            value: self.payload.value_spec()?,
            headers: self.header_specs()?,
            partition: self.partition,
        })
    }

    fn header_specs(&self) -> Result<Vec<ProduceHeader>, CliError> {
        self.headers
            .iter()
            .map(|raw| {
                let (key, value) = raw.split_once('=').ok_or_else(|| {
                    CliError::stated(
                        ExitCode::Usage,
                        format!("Can't read the header {raw:?}"),
                        "A header is name=value, e.g. --header trace-id=abc123. The value may be \
                         empty; the name may not.",
                    )
                })?;
                if key.is_empty() {
                    return Err(CliError::stated(
                        ExitCode::Usage,
                        format!("The header {raw:?} has no name"),
                        "A header is name=value, e.g. --header trace-id=abc123.",
                    ));
                }
                Ok(ProduceHeader {
                    key: key.to_string(),
                    value: value.to_string(),
                })
            })
            .collect()
    }
}

impl Payload {
    /// `None` is a tombstone — a null value, which is a different fact from an
    /// empty one on a compacted topic (docs/DESIGN.md §5.2).
    pub fn value_spec(&self) -> Result<Option<ProduceValueSpec>, CliError> {
        if self.tombstone {
            return Ok(None);
        }
        if let Some(text) = &self.value {
            return Ok(Some(ProduceValueSpec::Text { text: text.clone() }));
        }
        if let Some(raw) = &self.json {
            let json = serde_json::from_str(raw).map_err(|e| {
                CliError::stated(
                    ExitCode::Usage,
                    "Can't read --json as JSON",
                    // What the parser said and where it said it — never a
                    // guess about the reader's shell. "Try single quotes" is
                    // advice that is right in bash, wrong in cmd.exe, and
                    // unverifiable from here; --value-file is the form that
                    // works in every shell, so that is the one named.
                    format!(
                        "{e}. Nothing was sent. To send a payload the shell can't reach, put it \
                         in a file and pass --value-file (or pipe it in with --value-file -)."
                    ),
                )
            })?;
            return Ok(Some(ProduceValueSpec::Json { json }));
        }
        if let Some(path) = &self.value_file {
            return Ok(Some(ProduceValueSpec::Text {
                text: read_payload(path)?,
            }));
        }
        // clap's `required = true` on the group makes this unreachable from the
        // command line; it is still an answer rather than a panic, because the
        // type is public and a future caller may not go through clap.
        Err(CliError::stated(
            ExitCode::Usage,
            "This produce has no value",
            "Give one of --value, --json, --value-file or --tombstone.",
        ))
    }
}

/// `--value-file`: one file, or standard input when the path is `-`.
///
/// `-` is the convention every tool in the neighbourhood already keeps
/// (`cat`, `jq`, `kubectl apply -f -`), and it is what makes the flag usable
/// from a pipe rather than only from a file somebody had to write first.
///
/// **This is the only I/O in this module**, and it is why the payload can stay
/// off the command line: neither branch puts a byte of it in argv.
fn read_payload(path: &Path) -> Result<String, CliError> {
    if path == Path::new(STDIN) {
        let mut text = String::new();
        return std::io::stdin()
            .read_to_string(&mut text)
            .map(|_| text)
            .map_err(|e| {
                CliError::stated(
                    ExitCode::Usage,
                    "Can't read the value from standard input",
                    format!(
                        "{e}. --value-file - takes the payload from a pipe or a here-doc; it has \
                         to be UTF-8 text. Nothing was sent."
                    ),
                )
            });
    }
    std::fs::read_to_string(path).map_err(|e| {
        CliError::stated(
            ExitCode::Usage,
            format!("Can't read {}", path.display()),
            format!(
                "{e}. --value-file takes a path to a UTF-8 file holding the value, or - for \
                 standard input. Nothing was sent."
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// clap's own debug assertions — duplicate ids, a `requires` naming an
    /// argument that does not exist, a group with no members. They run only
    /// when something builds the command, so something has to.
    #[test]
    fn the_command_is_well_formed() {
        Cli::command().debug_assert();
    }

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("kavka").chain(args.iter().copied()))
    }

    #[test]
    fn a_command_is_required_and_an_unknown_one_is_refused() {
        assert!(parse(&[]).is_err());
        let error = parse(&["topic", "list"]).expect_err("no such subcommand");
        // `suggestions` earns its place here: the fix is in the message.
        let rendered = error.to_string();
        assert!(rendered.contains("topics"), "{rendered}");
    }

    #[test]
    fn the_profile_is_global_and_reads_the_environment() {
        let cli = parse(&["--profile", "orders", "topics", "list"]).expect("parses");
        assert_eq!(cli.profile.as_deref(), Some("orders"));
        // …and after the subcommand, which is where a person actually types it.
        let cli = parse(&["topics", "list", "-p", "orders"]).expect("parses");
        assert_eq!(cli.profile.as_deref(), Some("orders"));
    }

    /// Every documented range is a range clap enforces, at both ends.
    ///
    /// The failure this prevents is silent: a number past a core ceiling used
    /// to parse, be clamped inside kavka-core, and then be quoted back in the
    /// answer's own "limits" as if it had been applied.
    #[test]
    fn every_documented_numeric_range_is_enforced() {
        // --max, 1–2000, the ceiling being kavka-core's MAX_MESSAGES.
        assert!(parse(&["fetch", "orders", "--max", "0"]).is_err());
        assert!(parse(&["fetch", "orders", "--max", "2001"]).is_err());
        assert!(parse(&["fetch", "orders", "--max", "2000"]).is_ok());
        assert_eq!(MAX_MESSAGES, 2000, "the help text says 1–2000");

        // The error names the flag and the bound, which is why being refused
        // here beats being clamped later.
        let refusal = parse(&["fetch", "orders", "--max", "5000"])
            .expect_err("past the ceiling")
            .to_string();
        assert!(refusal.contains("--max"), "{refusal}");
        assert!(refusal.contains("2000"), "{refusal}");

        // --limit, --last, --max-value-bytes, --timeout-ms: a floor of 1,
        // because zero of any of them is a command that cannot answer.
        assert!(parse(&["topics", "list", "--limit", "0"]).is_err());
        assert!(parse(&["topics", "list", "--limit", "1"]).is_ok());
        assert!(parse(&["fetch", "orders", "--last", "0"]).is_err());
        assert!(parse(&["fetch", "orders", "--max-value-bytes", "0"]).is_err());
        assert!(parse(&["search", "orders", "x", "--timeout-ms", "0"]).is_err());
        assert!(parse(&["search", "orders", "x", "--scan-cap", "0"]).is_err());

        // Partitions and offsets are indices, so they start at zero and there
        // is no negative form of either.
        assert!(parse(&["fetch", "orders", "--partition", "-1"]).is_err());
        assert!(parse(&["fetch", "orders", "--partition", "0"]).is_ok());
        assert!(parse(&["fetch", "orders", "--partition", "0", "--offset", "-1"]).is_err());
        assert!(parse(&["produce", "orders", "-v", "a", "--partition", "-1"]).is_err());

        // --max-matches and the SQL caps take kavka-core's own ceilings.
        assert!(parse(&["search", "orders", "x", "--max-matches", "10001"]).is_err());
        assert!(parse(&["search", "orders", "x", "--max-matches", "10000"]).is_ok());
        assert_eq!(MAX_BUFFERED, 10_000, "the help text says 1–10 000");
        assert!(parse(&["sql", "orders", "select 1", "--max-rows", "10001"]).is_err());
        assert!(parse(&["sql", "orders", "select 1", "--scan-cap", "100001"]).is_err());
        assert!(parse(&["sql", "orders", "select 1", "--scan-cap", "100000"]).is_ok());
        assert_eq!(MAX_ROWS, 10_000);
        assert_eq!(MAX_SCANNED, 100_000);
    }

    #[test]
    fn the_output_format_maps_to_a_mode() {
        let cli = parse(&["-o", "json", "profiles", "list"]).expect("parses");
        assert_eq!(Mode::from(cli.output.expect("a format")), Mode::Json);
        assert!(parse(&["-o", "yaml", "profiles", "list"]).is_err());
    }

    // -- Seeking ------------------------------------------------------------

    fn scan_of(args: &[&str]) -> Result<ScanArgs, clap::Error> {
        let all: Vec<&str> = ["fetch", "orders"]
            .iter()
            .copied()
            .chain(args.iter().copied())
            .collect();
        parse(&all).map(|cli| match cli.command {
            Command::Fetch(fetch) => fetch.scan,
            _ => unreachable!("the fetch command was parsed"),
        })
    }

    #[test]
    fn the_four_seeks_are_mutually_exclusive() {
        assert!(scan_of(&["--earliest", "--last", "10"]).is_err());
        assert!(scan_of(&["--last", "10", "--since", "2h"]).is_err());
        assert!(scan_of(&["--offset", "0", "--earliest"]).is_err());
    }

    #[test]
    fn every_seek_flag_becomes_its_spec() {
        let now = 1_735_689_600_000;
        assert_eq!(
            scan_of(&[]).unwrap().seek_spec(50, now).unwrap(),
            SeekSpec::Latest { last_n: 50 },
            "no flag reads the newest records of each partition"
        );
        assert_eq!(
            scan_of(&["--earliest"])
                .unwrap()
                .seek_spec(50, now)
                .unwrap(),
            SeekSpec::Earliest
        );
        assert_eq!(
            scan_of(&["--last", "7"])
                .unwrap()
                .seek_spec(50, now)
                .unwrap(),
            SeekSpec::Latest { last_n: 7 }
        );
        assert_eq!(
            scan_of(&["--partition", "3", "--offset", "8412"])
                .unwrap()
                .seek_spec(50, now)
                .unwrap(),
            SeekSpec::Offset {
                partition: 3,
                offset: 8_412
            }
        );
        assert_eq!(
            scan_of(&["--since", "2h"])
                .unwrap()
                .seek_spec(50, now)
                .unwrap(),
            SeekSpec::Timestamp {
                timestamp_ms: now - 7_200_000
            }
        );
    }

    /// An offset with no partition is the one seek that cannot be guessed, and
    /// the message names the fix rather than the failure (§7).
    #[test]
    fn an_offset_without_exactly_one_partition_is_a_usage_error() {
        let error = scan_of(&["--offset", "10"])
            .unwrap()
            .seek_spec(50, 0)
            .expect_err("no partition");
        assert_eq!(error.code, ExitCode::Usage);
        assert!(
            error.detail.contains("--partition 0 --offset 10"),
            "{error:?}"
        );

        let error = scan_of(&["--offset", "10", "--partition", "0", "--partition", "1"])
            .unwrap()
            .seek_spec(50, 0)
            .expect_err("two partitions");
        assert!(error.detail.contains('2'), "{error:?}");
    }

    #[test]
    fn partitions_are_a_repeatable_flag_and_none_means_all() {
        assert_eq!(scan_of(&[]).unwrap().partitions(), None);
        assert_eq!(
            scan_of(&["--partition", "0", "--partition", "5"])
                .unwrap()
                .partitions(),
            Some(vec![0, 5])
        );
    }

    #[test]
    fn since_takes_an_age_or_epoch_milliseconds() {
        let now = 1_735_689_600_000;
        assert_eq!(parse_since(now, "30s").unwrap(), now - 30_000);
        assert_eq!(parse_since(now, "45m").unwrap(), now - 2_700_000);
        assert_eq!(parse_since(now, "6h").unwrap(), now - 21_600_000);
        assert_eq!(parse_since(now, "7d").unwrap(), now - 604_800_000);
        assert_eq!(parse_since(now, " 90m ").unwrap(), now - 5_400_000);
        // A bare number is the unit every Kafka tool prints.
        assert_eq!(
            parse_since(now, "1700000000000").unwrap(),
            1_700_000_000_000
        );
        for bad in ["", "soon", "2w", "-3h", "h", "2 h"] {
            let error = parse_since(now, bad).expect_err("not a time");
            assert_eq!(error.code, ExitCode::Usage);
            assert!(error.detail.contains("30s"), "{error:?}");
        }
    }

    // -- Producing ----------------------------------------------------------

    fn produce_of(args: &[&str]) -> Result<ProduceArgs, clap::Error> {
        let all: Vec<&str> = ["produce", "orders"]
            .iter()
            .copied()
            .chain(args.iter().copied())
            .collect();
        parse(&all).map(|cli| match cli.command {
            Command::Produce(produce) => produce,
            _ => unreachable!("the produce command was parsed"),
        })
    }

    #[test]
    fn a_produce_needs_exactly_one_payload() {
        assert!(produce_of(&[]).is_err(), "no payload at all");
        assert!(
            produce_of(&["--value", "a", "--tombstone"]).is_err(),
            "a tombstone with a value is two answers to one question"
        );
        assert!(produce_of(&["--value", "a", "--json", "{}"]).is_err());
        assert!(produce_of(&["--value", "a"]).is_ok());
    }

    #[test]
    fn the_payload_forms_become_their_specs() {
        let text = produce_of(&["--value", "hello"]).unwrap().record().unwrap();
        assert_eq!(
            text.value,
            Some(ProduceValueSpec::Text {
                text: "hello".into()
            })
        );
        let json = produce_of(&["--json", r#"{"retry":1}"#])
            .unwrap()
            .record()
            .unwrap();
        assert_eq!(
            json.value,
            Some(ProduceValueSpec::Json {
                json: serde_json::json!({ "retry": 1 })
            })
        );
        // A tombstone is a null value, and it is asked for by name.
        let tombstone = produce_of(&["--tombstone", "--key", "A-102"])
            .unwrap()
            .record()
            .unwrap();
        assert_eq!(tombstone.value, None);
        assert_eq!(tombstone.key.as_deref(), Some("A-102"));
    }

    /// The payload can come off the command line entirely — which is the
    /// point of the flag, because argv is readable by every other account on
    /// the machine while the command runs.
    #[test]
    fn a_payload_can_come_from_a_file_instead_of_argv() {
        let path = std::env::temp_dir().join(format!(
            "kavka-cli-value-{}-{}.txt",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, "{\"from\":\"a file\"}").expect("a scratch payload");
        let record = produce_of(&["--value-file", &path.to_string_lossy()])
            .expect("parses")
            .record()
            .expect("the file is read");
        assert_eq!(
            record.value,
            Some(ProduceValueSpec::Text {
                text: "{\"from\":\"a file\"}".into()
            }),
            "--value-file sends the bytes as text; --json is the parsing form"
        );
        let _ = std::fs::remove_file(&path);

        // It is one of the mutually exclusive payload forms, like the rest.
        assert!(produce_of(&["--value", "a", "--value-file", "x"]).is_err());
        assert!(produce_of(&["--tombstone", "--value-file", "x"]).is_err());

        // A path that isn't there is a usage error naming the path — and
        // saying nothing was sent, because nothing was.
        let error = produce_of(&["--value-file", "kavka-no-such-payload-file"])
            .expect("parses")
            .record()
            .expect_err("no such file");
        assert_eq!(error.code, ExitCode::Usage);
        assert!(
            error.title.contains("kavka-no-such-payload-file"),
            "{error:?}"
        );
        assert!(error.detail.contains("Nothing was sent"), "{error:?}");
        // …and it names the stdin form, which is the other half of the flag.
        assert!(error.detail.contains("standard input"), "{error:?}");
    }

    /// The help is the documentation surface, so the argv warning is asserted
    /// rather than assumed: it is the sentence that makes `--value-file` a
    /// choice somebody knows they have.
    #[test]
    fn the_value_flag_says_where_a_payload_can_be_seen() {
        let command = Cli::command();
        let produce = command
            .find_subcommand("produce")
            .expect("the produce command");
        let long_help = |id: &str| {
            produce
                .get_arguments()
                .find(|arg| arg.get_id() == id)
                .unwrap_or_else(|| panic!("--{id} exists"))
                .get_long_help()
                .unwrap_or_else(|| panic!("--{id} has long help"))
                .to_string()
        };

        let value = long_help("value");
        assert!(value.contains("--value-file"), "{value}");
        assert!(value.contains("shell's history"), "{value}");
        assert!(
            value.contains("visible to every other account"),
            "the sentence names WHO can see it: {value}"
        );

        // …and the flag it points at documents the pipe form.
        let file = long_help("value_file");
        assert!(file.contains("--value-file -"), "{file}");
        assert!(file.contains("standard input"), "{file}");
    }

    #[test]
    fn broken_json_is_caught_before_anything_is_sent() {
        let error = produce_of(&["--json", "{not json}"])
            .unwrap()
            .record()
            .expect_err("a parse failure");
        assert_eq!(error.code, ExitCode::Usage);
        assert!(error.detail.contains("Nothing was sent"), "{error:?}");
        // The fix it names is one that works in every shell. It used to
        // suggest single quotes, which is right in bash and wrong in cmd.exe —
        // an action Kavka cannot stand behind from inside a Rust process.
        assert!(error.detail.contains("--value-file"), "{error:?}");
        assert!(!error.detail.contains("single quotes"), "{error:?}");
    }

    #[test]
    fn headers_are_name_equals_value_and_repeatable() {
        let record = produce_of(&[
            "--value",
            "a",
            "-H",
            "trace-id=abc",
            "--header",
            "retry=",
            "--header",
            "trace-id=def",
        ])
        .unwrap()
        .record()
        .unwrap();
        assert_eq!(record.headers.len(), 3, "a repeated name is kept twice");
        assert_eq!(record.headers[0].key, "trace-id");
        assert_eq!(record.headers[1].value, "", "an empty value is allowed");
        assert_eq!(record.headers[2].value, "def");
    }

    #[test]
    fn a_header_without_an_equals_names_the_form() {
        for bad in ["trace-id", "=orphan"] {
            let error = produce_of(&["--value", "a", "-H", bad])
                .unwrap()
                .record()
                .expect_err("not a header");
            assert_eq!(error.code, ExitCode::Usage);
            assert!(error.detail.contains("name=value"), "{error:?}");
        }
    }

    /// `--yes-prod` is on the write command, not global: a flag that could be
    /// exported into a shell once and forgotten is not a confirmation.
    #[test]
    fn yes_prod_belongs_to_produce_alone() {
        assert!(
            produce_of(&["--value", "a", "--yes-prod"])
                .unwrap()
                .yes_prod
        );
        assert!(parse(&["--yes-prod", "topics", "list"]).is_err());
    }
}
