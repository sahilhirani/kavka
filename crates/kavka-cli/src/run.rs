//! The commands: one function each, over kavka-core, each answering with one
//! [`Answer`].
//!
//! Nothing here writes to stdout. A command builds the answer and [`run`] emits
//! it, which is what keeps the three output modes from drifting apart one
//! `println!` at a time — and what makes "stderr carries the caveats" a
//! property of one function rather than a habit.
//!
//! **Every bound is reported.** A fetch that returned its cap, a scan that
//! stopped at `--scan-cap` or ran out of clock, a partition that finished
//! because nothing more arrived, records a CEL expression could not be
//! evaluated against, a masking rule that rewrote a payload: each one is a
//! sentence on stderr. docs/ROADMAP.md's Phase 2 gate is "search never silently
//! truncates", and a terminal is the surface where silent truncation is
//! easiest — nobody scrolls back up.

use crate::cli::{
    Cli, Command, FetchArgs, GroupsCommand, ProduceArgs, ProfilesCommand, SearchArgs, SqlArgs,
    TopicDetailArgs, TopicsCommand, TopicsListArgs,
};
use crate::config;
use crate::errors::{CliError, Context, ExitCode};
use crate::gate;
use crate::output::{self, grouped, Align, Answer, Body, Column, Mode, Table, ABSENT};
use kavka_core::admin;
use kavka_core::connection::ClusterConnection;
use kavka_core::consume::{self, FetchSpec};
use kavka_core::masking::{MaskSet, MaskStore, MASK_NOTICE_MARKER};
use kavka_core::produce;
use kavka_core::profiles::{ConnectionProfile, Environment, ProfileStore};
use kavka_core::search::{self, SearchProgress, SearchQuery, SearchSession, SearchSpec};
use kavka_core::serdes::MessageRecord;
use kavka_core::sql::{self, SqlSession, SqlSpec};
use serde_json::{json, Value};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long a scan reader waits for a batch before looking at the clock, the
/// caps and the progress line again. Also the granularity of the deadline.
const DRAIN_POLL: Duration = Duration::from_millis(250);

/// How often the progress sentence is rewritten on a terminal.
const PROGRESS_EVERY: Duration = Duration::from_millis(400);

/// Runs one command and writes its answer.
pub fn run(cli: Cli) -> Result<(), CliError> {
    let mode = Mode::for_stdout(cli.output.map(Mode::from));
    let dir = config::config_dir().map_err(|reason| {
        CliError::stated(
            ExitCode::Failed,
            "Kavka can't tell where its connections are kept on this machine",
            reason,
        )
    })?;
    let session = Session {
        profiles_path: config::profiles_file(&dir),
        masks: MaskStore::new(dir.clone()),
        store: ProfileStore::new(dir),
        requested: cli.profile.clone(),
        unmasked: cli.unmasked,
    };

    let answer = match &cli.command {
        Command::Profiles { command } => match command {
            ProfilesCommand::List => session.profiles_list(),
        },
        Command::Topics { command } => match command {
            TopicsCommand::List(args) => session.topics_list(args),
            TopicsCommand::Detail(args) => session.topic_detail(args),
        },
        Command::Fetch(args) => session.fetch(args),
        Command::Search(args) => session.search(args),
        Command::Sql(args) => session.sql(args),
        Command::Produce(args) => session.produce(args),
        Command::Groups { command } => match command {
            GroupsCommand::List => session.groups_list(),
            GroupsCommand::Detail { group } => session.group_detail(group),
        },
    }?;

    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    match output::emit(&answer, mode, &mut out, &mut err) {
        // `kavka fetch orders | head -3` closes the pipe on the fourth line.
        // That is the reader saying "enough", not a failure of this program: it
        // exits 0 and says nothing, exactly as `cat` does.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(CliError::stated(
            ExitCode::Failed,
            "Kavka couldn't write the answer",
            e.to_string(),
        )),
        Ok(()) => Ok(()),
    }
}

/// One run: where the connections are, which one was asked for, and whether
/// masking applies.
struct Session {
    store: ProfileStore,
    /// The app's `masking.json`, from the same directory as `profiles.json`.
    masks: MaskStore,
    /// Kept for messages: "no connections in <file>" is actionable, "no
    /// connections" is not.
    profiles_path: PathBuf,
    requested: Option<String>,
    unmasked: bool,
}

impl Session {
    fn profiles(&self) -> Result<Vec<ConnectionProfile>, CliError> {
        self.store.list().map_err(|e| {
            CliError::stated(
                ExitCode::Failed,
                "Kavka couldn't read its connection file",
                format!(
                    "{path}: {e}. Your connections are still on disk — this is a read failure, \
                     not a delete.",
                    path = self.profiles_path.display(),
                ),
            )
        })
    }

    /// The connection this run is about.
    ///
    /// Matched by id, then — only when exactly one matches — by display name,
    /// case-insensitively: what `kavka profiles list` prints is what a person
    /// types back, and answering that with a lecture helps nobody. An ambiguous
    /// name is refused rather than guessed.
    ///
    /// **With no `--profile` at all and exactly one connection saved, that one
    /// is used**, and stderr says which. A machine with one cluster should not
    /// have to name it every time; a machine with several must, because
    /// right-action-wrong-cluster is the accident docs/DESIGN.md §6 is written
    /// against, and "whichever was first in the file" is exactly how it
    /// happens.
    fn profile(&self) -> Result<(ConnectionProfile, Option<String>), CliError> {
        let profiles = self.profiles()?;
        if profiles.is_empty() {
            return Err(CliError::stated(
                ExitCode::NotFound,
                "No connections are saved on this machine",
                format!(
                    "Kavka looked in {path}. Add a connection in the Kavka app — it stores the \
                     credentials in this machine's keychain, and this command can't create one.",
                    path = self.profiles_path.display(),
                ),
            ));
        }
        let Some(wanted) = self.requested.as_deref().filter(|name| !name.is_empty()) else {
            if let [only] = &profiles[..] {
                return Ok((
                    only.clone(),
                    Some(format!(
                        "Using {name} — the only connection saved on this machine.",
                        name = only.name
                    )),
                ));
            }
            return Err(CliError::stated(
                ExitCode::Usage,
                "Which connection?",
                format!(
                    "This machine has {count}, so Kavka won't guess. Add --profile (or set \
                     KAVKA_PROFILE): {known}.",
                    count = profiles.len(),
                    known = known(&profiles),
                ),
            ));
        };
        if let Some(found) = profiles.iter().find(|profile| profile.id == wanted) {
            return Ok((found.clone(), None));
        }
        let by_name: Vec<&ConnectionProfile> = profiles
            .iter()
            .filter(|profile| profile.name.eq_ignore_ascii_case(wanted))
            .collect();
        if let [only] = by_name[..] {
            return Ok((only.clone(), None));
        }
        Err(CliError::stated(
            ExitCode::NotFound,
            format!("No connection here is called {wanted:?}"),
            format!(
                "{ambiguity}Run `kavka profiles list` to see them: {known}.",
                ambiguity = if by_name.len() > 1 {
                    "More than one connection has that name, so it can't be resolved by name — \
                     use its id. "
                } else {
                    ""
                },
                known = known(&profiles),
            ),
        ))
    }

    /// Resolves the profile, opens the connection, and hands both to `body`.
    ///
    /// The prod banner and the "using the only connection" line are added here,
    /// at the front of the answer's notes, so every command that touches a
    /// cluster carries them and no command has to remember to.
    fn with_connection<F>(&self, body: F) -> Result<Answer, CliError>
    where
        F: FnOnce(&ClusterConnection, &ConnectionProfile) -> Result<Answer, CliError>,
    {
        let (profile, chosen) = self.profile()?;
        let conn = ClusterConnection::connect(profile.clone()).map_err(|e| {
            CliError::failed(
                format!(
                    "couldn't connect to {name} ({servers}): {e}",
                    name = profile.name,
                    servers = profile.bootstrap_servers.join(", "),
                ),
                &Context::on(profile.environment),
            )
        })?;
        let mut answer = body(&conn, &profile)?;
        // Front of the list, so the loudest thing on stderr is which cluster
        // this was: docs/DESIGN.md §6 layer 3, the bootstrap address is always
        // on screen.
        if let Some(banner) = prod_banner(&profile) {
            answer.notes.insert(0, banner);
        }
        if let Some(line) = chosen {
            answer.notes.insert(0, line);
        }
        Ok(answer)
    }

    /// The masking rules in force for one profile, compiled.
    ///
    /// Built per run: this process reads at most a few hundred records and then
    /// exits, so a small JSON file and two or three regexes cost less than the
    /// broker round trip beside them — and a rule switched on in the app
    /// applies to the very next command with nothing to restart.
    ///
    /// A rules file that cannot be read is [`MaskSet::none`] **and a line on
    /// stderr saying the answer is unmasked**. Silently returning raw payloads
    /// because a file was unreadable is the one failure this feature cannot
    /// have.
    fn mask_set(&self, profile_id: &str, notes: &mut Vec<String>) -> MaskSet {
        if self.unmasked {
            return MaskSet::none();
        }
        match self.masks.mask_set(profile_id) {
            Ok((rules, refused)) => {
                for problem in refused {
                    notes.push(format!("kavka: {problem}"));
                }
                rules
            }
            Err(e) => {
                notes.push(format!(
                    "kavka: couldn't read the masking rules for this connection ({e}) — this \
                     answer is UNMASKED."
                ));
                MaskSet::none()
            }
        }
    }

    /// Masks a batch in place, answering with the note it earned.
    ///
    /// The note is added **only when something was actually rewritten**, which
    /// is deliberately not "there are rules on this connection": a rule can be
    /// enabled and match nothing in the range being read, and saying data was
    /// redacted when it was not is the same class of lie as the reverse.
    fn mask(&self, rules: &MaskSet, records: &mut [MessageRecord]) -> Option<String> {
        if rules.is_empty() {
            return None;
        }
        let mut masked = false;
        for record in records.iter_mut() {
            masked |= rules.mask_record(record);
        }
        masked.then(|| {
            format!(
                "{MASK_NOTICE_MARKER}: {count} masking {word} saved for this connection in the \
                 Kavka app rewrote text below. Those values are not the bytes on the topic — pass \
                 --unmasked for those.",
                count = rules.len(),
                word = if rules.len() == 1 { "rule" } else { "rules" },
            )
        })
    }

    // -----------------------------------------------------------------------
    // Connections
    // -----------------------------------------------------------------------

    fn profiles_list(&self) -> Result<Answer, CliError> {
        let profiles = self.profiles()?;
        let rows: Vec<Value> = profiles
            .iter()
            .map(|profile| {
                json!({
                    "id": profile.id,
                    "name": profile.name,
                    "environment": serde_json::to_value(profile.environment).unwrap_or(Value::Null),
                    "bootstrap_servers": profile.bootstrap_servers,
                    "auth": serde_json::to_value(&profile.auth)
                        .ok()
                        .and_then(|auth| auth.get("kind").cloned())
                        .unwrap_or(Value::Null),
                    "read_only": profile.read_only,
                    "schema_registry": profile.schema_registry.is_some(),
                })
            })
            .collect();
        let mut table = Table::new(vec![
            Column::left("ID"),
            Column::left("NAME"),
            Column::left("ENV"),
            Column::left("BOOTSTRAP"),
            Column::left("MODE"),
            Column::left("AUTH"),
        ]);
        for (profile, row) in profiles.iter().zip(&rows) {
            table.push(vec![
                profile.id.clone(),
                profile.name.clone(),
                environment_word(profile.environment).to_string(),
                profile.bootstrap_servers.join(", "),
                // Law 2: the state that matters most is a word, never a colour
                // and never an empty cell.
                if profile.read_only {
                    "read-only".into()
                } else {
                    "writable".into()
                },
                row["auth"].as_str().unwrap_or("?").to_string(),
            ]);
        }
        Ok(Answer::new(
            json!({
                "profiles": rows,
                "count": rows.len(),
                "profiles_file": self.profiles_path.display().to_string(),
            }),
            rows,
            table,
            format!(
                "No connections saved on this machine yet ({path}). Add one in the Kavka app — \
                 it stores the credentials in this machine's keychain.",
                path = self.profiles_path.display(),
            ),
        ))
    }

    // -----------------------------------------------------------------------
    // Topics
    // -----------------------------------------------------------------------

    fn topics_list(&self, args: &TopicsListArgs) -> Result<Answer, CliError> {
        self.with_connection(|conn, profile| {
            let all = conn
                .list_topics()
                .map_err(|e| CliError::failed(e.to_string(), &Context::on(profile.environment)))?;
            let internal_total = all.iter().filter(|topic| topic.internal).count();
            let needle = args.contains.as_ref().map(|text| text.to_lowercase());
            let matching: Vec<&admin::TopicInfo> = all
                .iter()
                .filter(|topic| args.internal || !topic.internal)
                .filter(|topic| {
                    needle
                        .as_deref()
                        .is_none_or(|needle| topic.name.to_lowercase().contains(needle))
                })
                .collect();
            let total = matching.len();
            let shown: Vec<&admin::TopicInfo> = matching.into_iter().take(args.limit).collect();

            let rows: Vec<Value> = shown
                .iter()
                .map(|topic| {
                    json!({
                        "name": topic.name,
                        "partitions": topic.partitions,
                        "replication_factor": topic.replication_factor,
                        "internal": topic.internal,
                    })
                })
                .collect();
            let mut table = Table::new(vec![
                Column::left("NAME"),
                Column::right("PARTITIONS"),
                Column::right("REPLICATION"),
                Column::left("KIND"),
            ]);
            for topic in &shown {
                table.push(vec![
                    topic.name.clone(),
                    grouped(i64::from(topic.partitions)),
                    topic.replication_factor.to_string(),
                    // §5.2: an internal topic is dimmed AND tagged. A terminal
                    // has no dimming, so it gets the tag — and the normal case
                    // gets a word too, because a blank cell is not a state.
                    if topic.internal { "internal" } else { "topic" }.to_string(),
                ]);
            }

            let mut answer = Answer::new(
                json!({
                    "topics": rows,
                    "returned": rows.len(),
                    "total": total,
                    "truncated": total > rows.len(),
                    "internal_topics_hidden": if args.internal { 0 } else { internal_total },
                }),
                rows,
                table,
                empty_topics(&all, args, internal_total),
            );
            if total > shown.len() {
                answer.add_note(format!(
                    "Showing {shown} of {total} topics — pass --limit for more.",
                    shown = grouped(shown.len() as i64),
                    total = grouped(total as i64),
                ));
            }
            if !args.internal && internal_total > 0 {
                answer.add_note(format!(
                    "{count} internal {word} hidden — pass --internal to include them.",
                    count = internal_total,
                    word = if internal_total == 1 {
                        "topic"
                    } else {
                        "topics"
                    },
                ));
            }
            Ok(answer)
        })
    }

    fn topic_detail(&self, args: &TopicDetailArgs) -> Result<Answer, CliError> {
        self.with_connection(|conn, profile| {
            let detail = admin::topic_detail(conn, &args.topic)
                .map_err(|e| CliError::failed(e.to_string(), &Context::on(profile.environment)))?;
            let approx: i64 = detail
                .partitions
                .iter()
                .map(|p| p.latest_offset.saturating_sub(p.earliest_offset).max(0))
                .sum();
            let under_replicated: Vec<i32> = detail
                .partitions
                .iter()
                .filter(|p| p.isr.len() < p.replicas.len())
                .map(|p| p.partition)
                .collect();
            let configs: Vec<&admin::ConfigEntry> = detail
                .configs
                .iter()
                .filter(|entry| args.defaults || !entry.is_default)
                .collect();

            let mut partitions = Table::new(vec![
                Column::right("PARTITION"),
                Column::right("LEADER"),
                Column::left("REPLICAS"),
                Column::left("ISR"),
                Column::right("EARLIEST"),
                Column::right("LATEST"),
                Column::right("MESSAGES"),
                Column::left("HEALTH"),
            ]);
            for p in &detail.partitions {
                partitions.push(vec![
                    p.partition.to_string(),
                    p.leader.to_string(),
                    join_ids(&p.replicas),
                    join_ids(&p.isr),
                    grouped(p.earliest_offset),
                    grouped(p.latest_offset),
                    grouped(p.latest_offset.saturating_sub(p.earliest_offset).max(0)),
                    // Dot plus word in the app; here the word does all the work.
                    if p.isr.len() < p.replicas.len() {
                        "under-replicated"
                    } else {
                        "in sync"
                    }
                    .to_string(),
                ]);
            }
            let mut config_table = Table::new(vec![
                Column::left("CONFIG"),
                Column::payload("VALUE", 60),
                Column::left("SOURCE"),
            ]);
            for entry in &configs {
                config_table.push(vec![
                    entry.name.clone(),
                    match &entry.value {
                        // A sensitive entry has no value on the wire, and Kavka
                        // never invents one.
                        None if entry.is_sensitive => format!("{ABSENT} sensitive"),
                        None => ABSENT.to_string(),
                        Some(value) => value.clone(),
                    },
                    entry.source.clone(),
                ]);
            }

            let document = json!({
                "name": detail.name,
                "internal": detail.internal,
                "partition_count": detail.partitions.len(),
                "partitions": serde_json::to_value(&detail.partitions).unwrap_or(Value::Null),
                "approx_message_count": approx,
                "under_replicated_partitions": under_replicated,
                "configs": serde_json::to_value(&configs).unwrap_or(Value::Null),
                "default_configs_omitted": detail.configs.len() - configs.len(),
            });
            let mut answer = Answer::single(document, Body::Rows(partitions))
                .section("CONFIGURATION", config_table)
                .note(format!(
                    "{name}: {partitions} partitions · about {approx} messages · {urp} \
                     under-replicated.",
                    name = detail.name,
                    partitions = detail.partitions.len(),
                    approx = grouped(approx),
                    urp = under_replicated.len(),
                ));
            if !args.defaults {
                let omitted = detail.configs.len() - configs.len();
                if omitted > 0 {
                    answer.add_note(format!(
                        "{omitted} configuration entries inherited from the broker are hidden — \
                         pass --defaults to see them."
                    ));
                }
            }
            Ok(answer)
        })
    }

    // -----------------------------------------------------------------------
    // Records
    // -----------------------------------------------------------------------

    fn fetch(&self, args: &FetchArgs) -> Result<Answer, CliError> {
        // BEFORE the connection, like every other argument check here: a flag
        // combination that cannot work must not cost a round trip to a broker
        // to discover — and against an unreachable cluster it would cost the
        // whole metadata timeout before saying "--offset needs a partition".
        let spec = FetchSpec {
            topic: args.topic.clone(),
            seek: args.scan.seek_spec(args.max, now_ms())?,
            partitions: args.scan.partitions(),
            max_messages: args.max,
            max_value_bytes: args.max_value_bytes,
        };
        self.with_connection(|conn, profile| {
            let mut records =
                consume::fetch_messages(conn, conn.profile().schema_registry.as_ref(), &spec, None)
                    .map_err(|e| {
                        CliError::failed(e.to_string(), &Context::on(profile.environment))
                    })?;

            let mut notes = Vec::new();
            let rules = self.mask_set(&profile.id, &mut notes);
            if let Some(note) = self.mask(&rules, &mut records) {
                notes.push(note);
            }

            let at_ceiling = args.max >= consume::MAX_MESSAGES;
            let capped = records.len() as u32 >= args.max.min(consume::MAX_MESSAGES);
            let mut answer = records_answer(
                &records,
                args.verbose,
                json!({
                    "topic": args.topic,
                    "returned": records.len(),
                    "limits": { "max": args.max, "max_messages": consume::MAX_MESSAGES },
                }),
                format!(
                    "No messages in {topic} in that window. Try --earliest, a wider --last, or \
                     --since 24h.",
                    topic = args.topic,
                ),
            );
            answer.prepend_notes(notes);
            if capped {
                // AT THE CEILING THE NOTE STOPS NAMING --max. §7 rule 4 asks
                // for the next click, and "pass --max for more (up to 2 000)"
                // to somebody who just passed --max 2000 is a click that does
                // nothing — the flag is range-checked to that ceiling, so
                // there is no larger value to suggest.
                answer.add_note(if at_ceiling {
                    format!(
                        "Returned {hard} records, which is the most one fetch reads. Narrow the \
                         window instead — --partition, --since, or read on from where this \
                         stopped with --partition P --offset N.",
                        hard = grouped(i64::from(consume::MAX_MESSAGES)),
                    )
                } else {
                    format!(
                        "Returned the cap of {max} records — pass --max for more (up to {hard}).",
                        max = grouped(i64::from(args.max)),
                        hard = grouped(i64::from(consume::MAX_MESSAGES)),
                    )
                });
            }
            Ok(answer)
        })
    }

    fn search(&self, args: &SearchArgs) -> Result<Answer, CliError> {
        // Before the connection, for the reason `fetch` gives. A CEL syntax
        // error still costs one, because compiling the expression is
        // kavka-core's own first act inside `SearchSession::start` — and it is
        // its first act for exactly this reason.
        let query = SearchQuery {
            substring: args.substring.clone().filter(|value| !value.is_empty()),
            cel: args.cel.clone().filter(|value| !value.is_empty()),
        };
        if query.substring.is_none() && query.cel.is_none() {
            return Err(CliError::stated(
                ExitCode::Usage,
                "This search has nothing to look for",
                "Give a substring, a --cel expression, or both. A search with neither would \
                 return the whole scan window, which `kavka fetch` does better.",
            ));
        }
        let max_matches = args.max_matches.clamp(1, search::MAX_BUFFERED);
        let spec = SearchSpec {
            topic: args.topic.clone(),
            // The scan cap is a record count, so the window has to be at least
            // that big for the cap to be reachable at all.
            seek: args.scan.seek_spec(args.scan_cap, now_ms())?,
            partitions: args.scan.partitions(),
            query,
            max_buffered: max_matches,
            max_value_bytes: args.max_value_bytes,
        };
        self.with_connection(|conn, profile| {
            let context = Context::on(profile.environment);
            let session =
                SearchSession::start(conn, conn.profile().schema_registry.as_ref(), &spec)
                    .map_err(|e| CliError::failed(e.to_string(), &context))?;

            // The scan deliberately CONTINUES after the result buffer is full.
            // The core stops buffering at `max_buffered` and keeps counting,
            // which is what makes "the first 50 of 4,812 matches" a thing this
            // can say — and stopping early would make `matched` a restatement
            // of `returned`, which answers nothing.
            let deadline = Instant::now() + Duration::from_millis(u64::from(args.timeout_ms));
            let mut ticker = Ticker::new();
            let mut matches = Vec::new();
            let mut stopped = "complete";
            while let Some(batch) = session.next_results(DRAIN_POLL) {
                matches.extend(batch);
                let progress = session.progress();
                ticker.tick(&scan_sentence(&progress));
                if progress.scanned >= u64::from(args.scan_cap) {
                    stopped = "scan_cap";
                    break;
                }
                if Instant::now() >= deadline {
                    stopped = "deadline";
                    break;
                }
            }
            session.stop();
            let progress = session.progress();
            // Dropping joins the workers, so the consumers are gone before the
            // answer is written rather than during the next command.
            drop(session);
            ticker.clear();

            if let Some(error) = progress.error {
                return Err(CliError::failed(
                    format!(
                        "the search failed after {scanned} records: {error}",
                        scanned = progress.scanned,
                    ),
                    &context,
                ));
            }
            matches.truncate(max_matches as usize);
            let mut notes = Vec::new();
            // The SCAN is unmasked and the ANSWER is masked, in that order: the
            // CEL filter reads the record the cluster holds, which is the only
            // thing it can mean, so `matched` counts what actually matched.
            // Masking first would make a rule silently change what a query
            // means.
            let rules = self.mask_set(&profile.id, &mut notes);
            if let Some(note) = self.mask(&rules, &mut matches) {
                notes.push(note);
            }

            let mut answer = records_answer(
                &matches,
                args.verbose,
                json!({
                    "topic": args.topic,
                    "returned": matches.len(),
                    "matched": progress.matched,
                    "scanned": progress.scanned,
                    "unevaluated": progress.unevaluated,
                    "filter_error": progress.filter_error,
                    "assumed_complete_partitions": progress.assumed_complete,
                    "matches_truncated": progress.matched > matches.len() as u64,
                    "stopped_because": stopped,
                    "limits": {
                        "max_matches": max_matches,
                        "scan_cap": args.scan_cap,
                        "timeout_ms": args.timeout_ms,
                    },
                }),
                format!(
                    "Nothing matched. Checked {scanned} records in {topic} — the default only \
                     looks at the newest {cap}, so try --earliest or a wider --since.",
                    scanned = grouped(progress.scanned as i64),
                    topic = args.topic,
                    cap = grouped(i64::from(args.scan_cap)),
                ),
            );
            answer.prepend_notes(notes);
            for note in truncation_notes(&progress, &matches, stopped, args) {
                answer.add_note(note);
            }
            Ok(answer)
        })
    }

    fn sql(&self, args: &SqlArgs) -> Result<Answer, CliError> {
        // Before the connection, for the reason `fetch` gives.
        let spec = SqlSpec {
            topic: args.topic.clone(),
            query: args.query.clone(),
            seek: args.scan.seek_spec(args.scan_cap, now_ms())?,
            partitions: args.scan.partitions(),
            scan_cap: args.scan_cap,
            max_rows: args.max_rows,
        };
        self.with_connection(|conn, profile| {
            let context = Context::on(profile.environment);
            let session = SqlSession::start(conn, conn.profile().schema_registry.as_ref(), &spec)
                .map_err(|e| CliError::failed(e.to_string(), &context))?;

            let deadline = Instant::now() + Duration::from_millis(u64::from(args.timeout_ms));
            let mut ticker = Ticker::new();
            let mut rows: Vec<Vec<Value>> = Vec::new();
            let mut stopped = "complete";
            while let Some(batch) = session.next_rows(DRAIN_POLL) {
                rows.extend(batch);
                let progress = session.progress();
                ticker.tick(&format!(
                    "Scanned {scanned} records · {rows} rows so far",
                    scanned = grouped(progress.scanned as i64),
                    rows = grouped(rows.len() as i64),
                ));
                if rows.len() >= args.max_rows as usize {
                    stopped = "max_rows";
                    break;
                }
                if Instant::now() >= deadline {
                    stopped = "deadline";
                    break;
                }
            }
            session.stop();
            let progress = session.progress();
            let columns = session.columns();
            drop(session);
            ticker.clear();

            if let Some(error) = progress.error {
                return Err(CliError::failed(
                    format!(
                        "the query failed after {scanned} records: {error}",
                        scanned = progress.scanned,
                    ),
                    &context,
                ));
            }
            rows.truncate(args.max_rows as usize);
            let mut notes = Vec::new();
            // A result set is projected columns rather than records, so the
            // pass is by COLUMN NAME — the same function the app's SQL view
            // runs, so a column means the same thing in both.
            let rules = self.mask_set(&profile.id, &mut notes);
            let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
            if !rules.is_empty() && kavka_core::masking::mask_sql_rows(&rules, &names, &mut rows) {
                notes.push(format!(
                    "{MASK_NOTICE_MARKER}: {count} masking {word} rewrote values below.",
                    count = rules.len(),
                    word = if rules.len() == 1 { "rule" } else { "rules" },
                ));
            }

            // NDJSON gets objects rather than arrays: a row of a query the
            // caller wrote is only readable next to its column names.
            let objects: Vec<Value> = rows
                .iter()
                .map(|row| {
                    Value::Object(
                        names
                            .iter()
                            .zip(row)
                            .map(|(name, value)| ((*name).to_string(), value.clone()))
                            .collect(),
                    )
                })
                .collect();
            let mut table = Table::new(
                columns
                    .iter()
                    .map(|column| Column {
                        head: column.name.to_uppercase(),
                        align: if numeric(&column.data_type) {
                            Align::Right
                        } else {
                            Align::Left
                        },
                        max_width: Some(40),
                    })
                    .collect(),
            );
            for row in &rows {
                table.push(row.iter().map(cell_of).collect());
            }

            let mut answer = Answer::new(
                json!({
                    "topic": args.topic,
                    "columns": serde_json::to_value(&columns).unwrap_or(Value::Null),
                    "rows": rows,
                    "row_count": rows.len(),
                    "scanned": progress.scanned,
                    "capped": progress.capped,
                    "stopped_because": stopped,
                    "limits": { "scan_cap": args.scan_cap, "max_rows": args.max_rows },
                }),
                objects,
                table,
                format!(
                    "No rows. The query read {scanned} records from {topic}.",
                    scanned = grouped(progress.scanned as i64),
                    topic = args.topic,
                ),
            );
            answer.prepend_notes(notes);
            answer.add_note(format!(
                "{rows} rows from {scanned} records scanned.",
                rows = grouped(rows.len() as i64),
                scanned = grouped(progress.scanned as i64),
            ));
            // `stopped_because` says what ended the READ LOOP; `capped` is the
            // core's own flag and is the one that decides whether the ANSWER is
            // partial. They are reported separately rather than folded, because
            // "the query finished" and "the query saw everything" are different
            // claims.
            if progress.capped {
                answer.add_note(if args.scan_cap >= sql::MAX_SCANNED {
                    format!(
                        "The scan stopped at {cap} records, which is the most one query reads, so \
                         this answers about that window and not about the whole topic. Narrow it \
                         with --partition or --since.",
                        cap = grouped(i64::from(sql::MAX_SCANNED)),
                    )
                } else {
                    format!(
                        "The scan stopped at {cap} records, so this answers about that window and \
                         not about the whole topic — pass --scan-cap for a deeper scan.",
                        cap = grouped(i64::from(args.scan_cap)),
                    )
                });
            }
            if stopped == "max_rows" {
                answer.add_note(if args.max_rows >= sql::MAX_ROWS {
                    format!(
                        "Stopped at {cap} rows, which is the most one answer holds. Aggregate in \
                         the query, or add a WHERE.",
                        cap = grouped(i64::from(sql::MAX_ROWS)),
                    )
                } else {
                    format!(
                        "Stopped at the row cap of {cap} — pass --max-rows for more.",
                        cap = grouped(i64::from(args.max_rows)),
                    )
                });
            }
            if stopped == "deadline" {
                answer.add_note(format!(
                    "Stopped after {ms} ms — pass --timeout-ms for longer.",
                    ms = grouped(i64::from(args.timeout_ms)),
                ));
            }
            Ok(answer)
        })
    }

    // -----------------------------------------------------------------------
    // The one write
    // -----------------------------------------------------------------------

    fn produce(&self, args: &ProduceArgs) -> Result<Answer, CliError> {
        // The record is built BEFORE the gate and before the connection: a
        // typo in --json must not cost a round trip to a broker to discover,
        // and a refusal must not depend on whether the payload happened to
        // parse.
        let record = args.record()?;
        let (profile, chosen) = self.profile()?;
        // …and the gate runs before anything is opened, so a read-only
        // connection never authenticates on behalf of a write.
        gate::authorize_write(&profile, "produce", args.yes_prod)?;

        let context = Context::on(profile.environment);
        let conn = ClusterConnection::connect(profile.clone()).map_err(|e| {
            CliError::failed(
                format!(
                    "couldn't connect to {name} ({servers}): {e}",
                    name = profile.name,
                    servers = profile.bootstrap_servers.join(", "),
                ),
                &context,
            )
        })?;
        let delivery = produce::send(
            &conn,
            conn.profile().schema_registry.as_ref(),
            &args.topic,
            &record,
        )
        .map_err(|e| CliError::failed(e.to_string(), &context))?;

        let tombstone = record.value.is_none();
        // §7 rule 2: the verb survives the whole flow, and the answer names
        // where the record landed rather than saying "OK".
        let sentence = format!(
            "Sent {what} to {topic}[{partition}] at offset {offset}.",
            what = if tombstone { "a tombstone" } else { "1 record" },
            topic = args.topic,
            partition = delivery.partition,
            offset = grouped(delivery.offset),
        );
        let mut answer = Answer::single(
            json!({
                "topic": args.topic,
                "partition": delivery.partition,
                "offset": delivery.offset,
                "tombstone": tombstone,
            }),
            Body::Text(sentence),
        );
        if let Some(banner) = prod_banner(&profile) {
            answer.notes.insert(0, banner);
        }
        if let Some(line) = chosen {
            answer.notes.insert(0, line);
        }
        Ok(answer)
    }

    // -----------------------------------------------------------------------
    // Consumer groups
    // -----------------------------------------------------------------------

    fn groups_list(&self) -> Result<Answer, CliError> {
        self.with_connection(|conn, profile| {
            let groups = admin::groups_list(conn)
                .map_err(|e| CliError::failed(e.to_string(), &Context::on(profile.environment)))?;
            let rows: Vec<Value> = groups
                .iter()
                .map(|group| serde_json::to_value(group).unwrap_or(Value::Null))
                .collect();
            let mut table = Table::new(vec![
                Column::left("GROUP"),
                Column::left("STATE"),
                Column::right("MEMBERS"),
                Column::left("PROTOCOL"),
            ]);
            for group in &groups {
                table.push(vec![
                    group.group_id.clone(),
                    group.state.clone(),
                    grouped(i64::from(group.member_count)),
                    if group.protocol_type.is_empty() {
                        ABSENT.to_string()
                    } else {
                        group.protocol_type.clone()
                    },
                ]);
            }
            Ok(Answer::new(
                json!({ "groups": rows, "count": rows.len() }),
                rows,
                table,
                "No consumer groups yet. Groups appear here as soon as an application starts \
                 reading from this cluster — one that has only ever produced won't show up.",
            ))
        })
    }

    fn group_detail(&self, group_id: &str) -> Result<Answer, CliError> {
        self.with_connection(|conn, profile| {
            let detail = admin::group_detail(conn, group_id)
                .map_err(|e| CliError::failed(e.to_string(), &Context::on(profile.environment)))?;
            let total_lag: i64 = detail.offsets.iter().filter_map(|offset| offset.lag).sum();

            let mut offsets = Table::new(vec![
                Column::left("TOPIC"),
                Column::right("PARTITION"),
                Column::right("COMMITTED"),
                Column::right("END"),
                Column::right("LAG"),
            ]);
            for offset in &detail.offsets {
                offsets.push(vec![
                    offset.topic.clone(),
                    offset.partition.to_string(),
                    // A group that has never committed here has no position,
                    // and no measurable backlog either. `∅` is the truth; `0`
                    // would be an invention.
                    offset.committed.map_or_else(|| ABSENT.to_string(), grouped),
                    grouped(offset.end_offset),
                    offset.lag.map_or_else(|| ABSENT.to_string(), grouped),
                ]);
            }
            let mut members = Table::new(vec![
                Column::left("MEMBER"),
                Column::left("CLIENT"),
                Column::left("HOST"),
                Column::right("ASSIGNED"),
            ]);
            for member in &detail.members {
                members.push(vec![
                    member.member_id.clone(),
                    member.client_id.clone(),
                    member.client_host.clone(),
                    grouped(member.assignments.len() as i64),
                ]);
            }

            Ok(Answer::single(
                json!({
                    "group_id": detail.group_id,
                    "state": detail.state,
                    "member_count": detail.members.len(),
                    "members": serde_json::to_value(&detail.members).unwrap_or(Value::Null),
                    "offsets": serde_json::to_value(&detail.offsets).unwrap_or(Value::Null),
                    "total_lag": total_lag,
                }),
                Body::Rows(offsets),
            )
            .section("MEMBERS", members)
            .note(format!(
                "{group}: {state} · {members} members · {lag} total lag.",
                group = detail.group_id,
                state = detail.state,
                members = detail.members.len(),
                lag = grouped(total_lag),
            )))
        })
    }
}

// ---------------------------------------------------------------------------
// Shared shaping
// ---------------------------------------------------------------------------

/// The answer for any command that returns records: the same rows, the same
/// table and the same compact shape for `fetch` and `search`, so a script that
/// reads one reads the other.
fn records_answer(
    records: &[MessageRecord],
    verbose: bool,
    mut document: Value,
    empty: String,
) -> Answer {
    let rows: Vec<Value> = records
        .iter()
        .map(|record| output::record_json(record, verbose))
        .collect();
    let mut table = Table::new(output::record_columns());
    for record in records {
        table.push(output::record_row(record));
    }
    if let Some(object) = document.as_object_mut() {
        object.insert("records".into(), Value::Array(rows.clone()));
    }
    Answer::new(document, rows, table, empty)
}

/// Everything a scan has to say about what it did NOT see.
///
/// docs/ROADMAP.md Phase 2's gate is "search never silently truncates", and
/// each of these is one of the four ways a scan can be partial: the buffer
/// filled, the record cap was reached, the clock ran out, or a partition was
/// assumed finished because nothing more arrived. `unevaluated` is the fifth
/// and is not truncation at all — it is records the expression could not be
/// evaluated against, read but not judged, and folding them into either
/// `scanned` or `matched` would be a lie in one direction or the other.
fn truncation_notes(
    progress: &SearchProgress,
    matches: &[MessageRecord],
    stopped: &str,
    args: &SearchArgs,
) -> Vec<String> {
    let mut notes = vec![format!(
        "Scanned {scanned} records · {matched} matched · {returned} shown.",
        scanned = grouped(progress.scanned as i64),
        matched = grouped(progress.matched as i64),
        returned = grouped(matches.len() as i64),
    )];
    if progress.matched > matches.len() as u64 {
        // Same rule as the fetch cap: at the ceiling `--max-matches` is not a
        // next click, so the sentence names one that is.
        notes.push(if args.max_matches >= search::MAX_BUFFERED {
            format!(
                "Showing the first {shown} of {matched} matches, which is the most Kavka holds at \
                 once. Narrow the query, or scan a smaller window.",
                shown = grouped(matches.len() as i64),
                matched = grouped(progress.matched as i64),
            )
        } else {
            format!(
                "Showing the first {shown} of {matched} matches — pass --max-matches for more (up \
                 to {hard}).",
                shown = grouped(matches.len() as i64),
                matched = grouped(progress.matched as i64),
                hard = grouped(i64::from(search::MAX_BUFFERED)),
            )
        });
    }
    if stopped == "scan_cap" {
        notes.push(format!(
            "Stopped at the scan cap of {cap} records, so there may be more — pass --scan-cap for \
             a deeper scan.",
            cap = grouped(i64::from(args.scan_cap)),
        ));
    }
    if stopped == "deadline" {
        notes.push(format!(
            "Stopped after {ms} ms, so there may be more — pass --timeout-ms for longer.",
            ms = grouped(i64::from(args.timeout_ms)),
        ));
    }
    if progress.unevaluated > 0 {
        notes.push(format!(
            "{count} records could not be evaluated against the expression — they were read, not \
             judged, and are not matches.",
            count = grouped(progress.unevaluated as i64),
        ));
    }
    if let Some(error) = &progress.filter_error {
        notes.push(format!("The filter reported: {error}"));
    }
    if !progress.assumed_complete.is_empty() {
        notes.push(format!(
            "Partitions {list} finished because nothing more arrived, not because they reached \
             the offset captured when the scan started.",
            list = join_ids(&progress.assumed_complete),
        ));
    }
    notes
}

/// The progress sentence — docs/DESIGN.md §7 rule 6: a spinner says *wait*, a
/// sentence says *what for*.
fn scan_sentence(progress: &SearchProgress) -> String {
    let remaining: i64 = progress
        .per_partition
        .iter()
        .map(|p| p.end_offset.saturating_sub(p.current_offset).max(0))
        .sum();
    let matched = format!(
        "{} {} so far",
        grouped(progress.matched as i64),
        if progress.matched == 1 {
            "match"
        } else {
            "matches"
        }
    );
    if remaining > 0 {
        format!(
            "Scanned {scanned} of about {total} · {matched}",
            scanned = grouped(progress.scanned as i64),
            total = grouped(progress.scanned as i64 + remaining),
        )
    } else {
        format!(
            "Scanned {scanned} · {matched}",
            scanned = grouped(progress.scanned as i64),
        )
    }
}

/// The progress line, on a terminal only.
///
/// Rewritten in place with a carriage return — no ANSI, no cursor codes, so it
/// behaves the same in Windows Terminal, an SSH session and a macOS terminal.
/// **Off whenever stderr is not a terminal**, because a CI log does not want
/// forty copies of a sentence, and a `2>` redirect wants the summary only.
struct Ticker {
    on: bool,
    last: Instant,
    width: usize,
}

impl Ticker {
    fn new() -> Self {
        Self {
            on: io::stderr().is_terminal(),
            last: Instant::now()
                .checked_sub(PROGRESS_EVERY)
                .unwrap_or_else(Instant::now),
            width: 0,
        }
    }

    fn tick(&mut self, sentence: &str) {
        if !self.on || self.last.elapsed() < PROGRESS_EVERY {
            return;
        }
        self.last = Instant::now();
        self.width = self.width.max(sentence.chars().count());
        let mut err = io::stderr().lock();
        let _ = write!(err, "\r{sentence}");
        let _ = err.flush();
    }

    /// Wipes the line so the summary that follows starts clean.
    fn clear(&mut self) {
        if !self.on || self.width == 0 {
            return;
        }
        let mut err = io::stderr().lock();
        let _ = write!(err, "\r{}\r", " ".repeat(self.width));
        let _ = err.flush();
    }
}

/// docs/DESIGN.md §6 layers 2 and 3, as one line of text: the environment and
/// the address, before anything else on stderr.
///
/// A word, not a colour — this program has no colour support and does not want
/// any (Law 2), and a `PROD` a script can grep for is worth more than a red one
/// it cannot.
fn prod_banner(profile: &ConnectionProfile) -> Option<String> {
    (profile.environment == Environment::Prod).then(|| {
        format!(
            "! PROD · {name} · {servers}",
            name = profile.name,
            servers = profile.bootstrap_servers.join(", "),
        )
    })
}

fn environment_word(environment: Environment) -> &'static str {
    match environment {
        Environment::Dev => "dev",
        Environment::Staging => "staging",
        Environment::Prod => "PROD",
    }
}

/// The empty state that fits, out of docs/DESIGN.md §7's list. Three different
/// situations, three different next actions — "no topics" for all of them would
/// be true and useless.
fn empty_topics(all: &[admin::TopicInfo], args: &TopicsListArgs, internal: usize) -> String {
    if all.is_empty() {
        return "This cluster has no topics. They appear here as soon as something creates one."
            .into();
    }
    if let Some(needle) = &args.contains {
        return format!(
            "No topic here contains {needle:?}. {count} topics were checked.",
            count = all.len(),
        );
    }
    if internal == all.len() {
        return "This cluster only has Kafka's own internal topics. Pass --internal to see them."
            .into();
    }
    "No topics matched.".into()
}

fn known(profiles: &[ConnectionProfile]) -> String {
    profiles
        .iter()
        .map(|profile| format!("{} — {}", profile.id, profile.name))
        .collect::<Vec<_>>()
        .join("; ")
}

fn join_ids(ids: &[i32]) -> String {
    if ids.is_empty() {
        return ABSENT.to_string();
    }
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Whether a SQL column holds a quantity, and is therefore right-aligned
/// (docs/DESIGN.md §5.2). DataFusion's own type names.
fn numeric(data_type: &str) -> bool {
    let lowered = data_type.to_lowercase();
    ["int", "float", "decimal", "double"]
        .iter()
        .any(|kind| lowered.contains(kind))
}

/// One SQL cell. A JSON string prints as its text — a quoted string in a table
/// column is noise — and everything else prints as its JSON.
fn cell_of(value: &Value) -> String {
    match value {
        Value::Null => ABSENT.to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ScanArgs;

    fn profile(environment: Environment) -> ConnectionProfile {
        serde_json::from_value(json!({
            "id": "p1",
            "name": "payments-prod",
            "environment": serde_json::to_value(environment).expect("an environment"),
            "bootstrap_servers": ["10.0.4.19:9093", "10.0.4.20:9093"],
            "auth": { "kind": "plaintext" },
            "read_only": false,
        }))
        .expect("a ConnectionProfile")
    }

    /// The guardrail that costs nothing and is on every prod command: the
    /// environment and the address, in words, before the answer.
    #[test]
    fn prod_announces_itself_with_the_address_and_dev_says_nothing() {
        let banner = prod_banner(&profile(Environment::Prod)).expect("a prod banner");
        assert!(banner.contains("PROD"), "{banner}");
        assert!(banner.contains("10.0.4.19:9093"), "{banner}");
        assert!(banner.contains("payments-prod"), "{banner}");
        assert!(prod_banner(&profile(Environment::Dev)).is_none());
        assert!(prod_banner(&profile(Environment::Staging)).is_none());
    }

    #[test]
    fn every_environment_has_a_word() {
        assert_eq!(environment_word(Environment::Dev), "dev");
        assert_eq!(environment_word(Environment::Staging), "staging");
        assert_eq!(environment_word(Environment::Prod), "PROD");
    }

    /// Every way a scan can be partial produces a sentence. This is the Phase 2
    /// "never silently truncates" gate, on a terminal.
    #[test]
    fn a_partial_scan_says_so_in_every_way_it_can_be_partial() {
        let args = SearchArgs {
            topic: "orders".into(),
            substring: Some("failed".into()),
            cel: None,
            scan: ScanArgs::default(),
            max_matches: 50,
            scan_cap: 20_000,
            timeout_ms: 30_000,
            max_value_bytes: None,
            verbose: false,
        };
        let progress = SearchProgress {
            scanned: 20_000,
            matched: 4_812,
            buffered: 50,
            unevaluated: 3,
            per_partition: Vec::new(),
            assumed_complete: vec![2, 5],
            msgs_per_sec: 0.0,
            done: false,
            error: None,
            filter_error: Some("value.amount: no such key".into()),
        };
        let notes = truncation_notes(&progress, &[], "scan_cap", &args).join("\n");
        assert!(notes.contains("20 000 records"), "{notes}");
        assert!(notes.contains("first 0 of 4 812 matches"), "{notes}");
        assert!(notes.contains("scan cap"), "{notes}");
        assert!(
            notes.contains("3 records could not be evaluated"),
            "{notes}"
        );
        assert!(notes.contains("no such key"), "{notes}");
        assert!(notes.contains("Partitions 2,5"), "{notes}");

        // …and a complete scan says only what it did.
        let complete = SearchProgress {
            scanned: 50,
            matched: 11,
            unevaluated: 0,
            assumed_complete: Vec::new(),
            filter_error: None,
            done: true,
            ..progress
        };
        let notes = truncation_notes(&complete, &[], "complete", &args);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("11 matched"), "{notes:?}");
    }

    #[test]
    fn the_progress_sentence_says_what_it_is_waiting_for() {
        let progress = SearchProgress {
            scanned: 412_000,
            matched: 0,
            buffered: 0,
            unevaluated: 0,
            per_partition: vec![kavka_core::search::PartitionProgress {
                partition: 0,
                current_offset: 412_000,
                end_offset: 2_400_000,
            }],
            assumed_complete: Vec::new(),
            msgs_per_sec: 0.0,
            done: false,
            error: None,
            filter_error: None,
        };
        let sentence = scan_sentence(&progress);
        assert_eq!(
            sentence,
            "Scanned 412 000 of about 2 400 000 · 0 matches so far"
        );
        // One match is singular, and a finished scan drops the estimate rather
        // than claiming a total it no longer has.
        let done = SearchProgress {
            matched: 1,
            per_partition: Vec::new(),
            ..progress
        };
        assert_eq!(scan_sentence(&done), "Scanned 412 000 · 1 match so far");
    }

    #[test]
    fn the_three_empty_topic_states_are_three_different_sentences() {
        let args = |contains: Option<&str>, internal: bool| TopicsListArgs {
            contains: contains.map(str::to_string),
            internal,
            limit: 500,
        };
        let topic = |name: &str, internal: bool| admin::TopicInfo {
            name: name.into(),
            partitions: 1,
            replication_factor: 1,
            internal,
        };
        assert!(empty_topics(&[], &args(None, false), 0).contains("has no topics"));
        assert!(
            empty_topics(&[topic("__consumer_offsets", true)], &args(None, false), 1)
                .contains("only has Kafka's own internal topics")
        );
        let searched = empty_topics(&[topic("orders", false)], &args(Some("payments"), false), 0);
        assert!(searched.contains("\"payments\""), "{searched}");
        assert!(searched.contains("1 topics were checked"), "{searched}");
    }

    #[test]
    fn absent_ids_and_values_are_the_glyph_never_the_word_null() {
        assert_eq!(join_ids(&[]), "∅");
        assert_eq!(join_ids(&[1, 2, 3]), "1,2,3");
        assert_eq!(cell_of(&Value::Null), "∅");
        assert_eq!(cell_of(&json!("failed")), "failed", "no quotes in a cell");
        assert_eq!(cell_of(&json!(42)), "42");
    }

    #[test]
    fn quantities_are_right_aligned_by_their_datafusion_type() {
        for kind in ["Int64", "Float64", "Decimal128(10, 2)", "UInt32"] {
            assert!(numeric(kind), "{kind}");
        }
        for kind in ["Utf8", "Boolean", "Timestamp(Millisecond, None)"] {
            assert!(!numeric(kind), "{kind}");
        }
    }
}
