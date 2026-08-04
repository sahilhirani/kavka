//! User-defined environments: the identity a connection wears, and the
//! guardrail it carries.
//!
//! # The split this module exists to make
//!
//! An environment is two independent things that used to be one enum:
//!
//! - **Identity** — its `name` and its `color`. This is the chip in the
//!   sidebar, the gutter accent, the word in `kavka profiles list`. It says
//!   *which* cluster family you are looking at, and it is the user's
//!   vocabulary: `dev`, `QA`, `UAT`, `Production`, `eu-prod`.
//! - **Protection** — `protected`. This is the guardrail: the warm substrate,
//!   the ledger damper, type-to-confirm, the window-title suffix, the CLI's
//!   `--yes-prod`, the MCP server's `KAVKA_MCP_ALLOW_PROD`. It says *whether
//!   Kavka makes writing here deliberately harder*.
//!
//! Everything that used to be keyed on `environment == "prod"` is now keyed on
//! [`EffectiveEnvironment::protected`], and nothing is keyed on the name. That
//! is what makes an enterprise's `UAT` and `Production` work: the colour is
//! identity, the ground is protection, and the two are set independently.
//!
//! **The protected substrate stays the one warm-danger treatment regardless of
//! chip colour.** A violet `Production` still turns the world warm — identity
//! is the chip, protection is the ground, and giving each colour its own
//! danger treatment would mean seven guardrails to audit instead of one.
//!
//! # Why an unknown environment is never an error
//!
//! [`ConnectionProfile::environment`](crate::profiles::ConnectionProfile) is a
//! free string, and the definitions live in a *different* file. An imported
//! profile, a hand-edited `profiles.json`, a definition deleted in another
//! window — all of them produce a profile pointing at a name nothing defines.
//! Refusing to render that connection would mean losing a cluster because a
//! label went missing. So [`resolve`] always answers: an unknown name renders
//! neutral ([`NEUTRAL_COLOR`]), **unprotected**, and carries
//! [`EffectiveEnvironment::hint`] saying what to do about it.
//!
//! Unprotected is the honest fallback and it is worth saying why, because the
//! cautious-looking choice is wrong: if an unknown name were treated as
//! protected, deleting a definition would silently arm the guardrail on every
//! connection that referenced it, and the user would learn about it from a
//! refusal they cannot explain. Protection is a thing somebody switched on, in
//! a file, on purpose — and the deletion path
//! ([`EnvironmentStore::delete`]) refuses while any connection still points at
//! it, so a *protected* definition cannot vanish out from under a connection
//! in the first place.

use crate::profiles::ConnectionProfile;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Envelope version for `environments.json`, versioned for the same reason
/// `alerts.json` and profile exports are: a future shape gets migrated or
/// refused, never half-parsed.
pub const ENVIRONMENTS_VERSION: u32 = 1;

/// The colour tokens an environment may wear. Seven, and a closed set: each
/// one is a pair of audited ink/fill values in `apps/desktop/src/styles.css`
/// (docs/DESIGN.md §3), so a free-form colour would be a contrast failure
/// nobody measured.
pub const COLORS: [&str; 7] = ["green", "amber", "red", "blue", "violet", "cyan", "slate"];

/// What an environment nothing defines renders as. Also a legitimate choice in
/// the manager, for a family that wants no colour opinion at all.
pub const NEUTRAL_COLOR: &str = "slate";

/// One environment, as the user defined it.
///
/// `name` is displayed exactly as typed and compared case-insensitively — a
/// user who writes `Production` sees `Production`, and cannot then create a
/// second `production` beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentDef {
    pub name: String,
    /// One of [`COLORS`]. Identity only — it does not decide any guardrail.
    pub color: String,
    /// Whether writing to a connection in this environment is gated: the warm
    /// substrate, type-to-confirm, `--yes-prod`, `KAVKA_MCP_ALLOW_PROD`.
    ///
    /// `#[serde(default)]` so a hand-written definition that omits it is
    /// unprotected rather than unparseable — the same reading as "nobody said
    /// to guard this".
    #[serde(default)]
    pub protected: bool,
}

impl EnvironmentDef {
    pub fn new(name: impl Into<String>, color: impl Into<String>, protected: bool) -> Self {
        Self {
            name: name.into(),
            color: color.into(),
            protected,
        }
    }

    /// Whether this definition is the one called `name`, case-insensitively.
    pub fn is(&self, name: &str) -> bool {
        same_name(&self.name, name)
    }

    /// The name trimmed and checked, and the colour checked against
    /// [`COLORS`] — the validation [`EnvironmentStore::save`] runs at the door
    /// so nothing downstream has to wonder.
    fn validated(mut self) -> Result<Self> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            return Err(Error::Other(
                "an environment needs a name — it is what connections are tagged with and what \
                 the chip says"
                    .into(),
            ));
        }
        if !COLORS.contains(&self.color.as_str()) {
            return Err(Error::Other(format!(
                "{color:?} is not one of Kavka's environment colours ({known}). Each one is an \
                 audited ink/fill pair, so the set is closed.",
                color = self.color,
                known = COLORS.join(", "),
            )));
        }
        Ok(self)
    }
}

/// Case-insensitive name comparison, the one rule for "is this the same
/// environment".
///
/// `to_lowercase` rather than `eq_ignore_ascii_case`, unlike the profile-name
/// lookup next door: an environment name is a label somebody invented for
/// their own organisation, so `Produktion` and `PRODUKTION` — and every
/// non-ASCII pair like them — have to be one environment, not two.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The environments a machine has before anybody defines any: the three the
/// product shipped with, with `prod` protected.
///
/// These are written on first run **and** whenever `environments.json` is
/// absent, which is the migration: a Phase 6 install has profiles tagged
/// `dev`/`staging`/`prod` and no definitions file, and it must come up looking
/// and behaving exactly as it did.
pub fn defaults() -> Vec<EnvironmentDef> {
    vec![
        EnvironmentDef::new("dev", "green", false),
        EnvironmentDef::new("staging", "amber", false),
        EnvironmentDef::new("prod", "red", true),
    ]
}

/// What one connection's `environment` string actually means, once the
/// definitions have been consulted.
///
/// This is the type every guardrail reads. Nothing downstream compares
/// environment names to literals — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveEnvironment {
    /// The definition's name when one matched — so the casing the user typed
    /// in the manager wins over whatever a profile happens to store — and the
    /// profile's own string when none did.
    pub name: String,
    /// One of [`COLORS`]; [`NEUTRAL_COLOR`] when nothing matched.
    pub color: String,
    /// **The guardrail.** False when nothing matched.
    pub protected: bool,
    /// False when no definition matched. See [`Self::hint`].
    pub known: bool,
}

impl EffectiveEnvironment {
    /// The one sentence to show beside a connection whose environment nothing
    /// defines. `None` for a known environment, so a caller can render it
    /// unconditionally.
    pub fn hint(&self) -> Option<String> {
        (!self.known).then(|| {
            format!(
                "This connection is tagged {name:?}, which isn't one of this machine's \
                 environments. It's shown neutral and carries no protection. Add it under Manage \
                 environments to give it a colour — and to mark it protected if writes to it \
                 should ask first.",
                name = self.name,
            )
        })
    }

    /// The word that stands for this environment where there is no colour: a
    /// terminal, a `PROD`-style forced-colors label, a window title.
    ///
    /// Uppercased when protected and left as typed otherwise, so the guardrail
    /// still shouts in the one place colour cannot — docs/DESIGN.md §6 layer 2.
    pub fn word(&self) -> String {
        if self.protected {
            self.name.to_uppercase()
        } else {
            self.name.clone()
        }
    }
}

/// The meaning of one `environment` string against a set of definitions.
///
/// Pure and total: every string has an answer, and an unknown one is neutral
/// and unprotected rather than an error. See the module docs for why that is
/// the safe direction.
pub fn resolve(defs: &[EnvironmentDef], name: &str) -> EffectiveEnvironment {
    match defs.iter().find(|def| def.is(name)) {
        Some(def) => EffectiveEnvironment {
            name: def.name.clone(),
            color: def.color.clone(),
            protected: def.protected,
            known: true,
        },
        None => EffectiveEnvironment {
            name: name.to_string(),
            color: NEUTRAL_COLOR.to_string(),
            protected: false,
            known: false,
        },
    }
}

/// The names of the profiles tagged with `environment`, in file order.
///
/// The refusal [`EnvironmentStore::delete`] produces has to *name* them: "it's
/// in use" sends somebody through fourteen connections looking for the one.
pub fn referencing<'a>(profiles: &'a [ConnectionProfile], environment: &str) -> Vec<&'a str> {
    profiles
        .iter()
        .filter(|profile| same_name(&profile.environment, environment))
        .map(|profile| profile.name.as_str())
        .collect()
}

/// Outcome of merging an export's environment definitions into a store.
///
/// Skip-existing only, so there is no `replaced`: an import must not repaint
/// or — far worse — *unprotect* an environment this machine already defined.
/// Somebody else's `prod` arriving as `{"protected": false}` and silently
/// disarming the guardrail is the failure this whole struct's shape refuses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentImportReport {
    pub imported: usize,
    pub skipped: usize,
}

/// Merges `incoming` into `defs` by case-insensitive name, keeping what is
/// already there. Pure, so the import path's counting can be tested without a
/// disk.
pub fn merge(
    defs: &mut Vec<EnvironmentDef>,
    incoming: Vec<EnvironmentDef>,
) -> EnvironmentImportReport {
    let mut report = EnvironmentImportReport::default();
    for def in incoming {
        if defs.iter().any(|kept| kept.is(&def.name)) {
            report.skipped += 1;
        } else {
            defs.push(def);
            report.imported += 1;
        }
    }
    report
}

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    kavka_environments: u32,
    #[serde(default)]
    environments: Vec<EnvironmentDef>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            kavka_environments: ENVIRONMENTS_VERSION,
            environments: defaults(),
        }
    }
}

/// The environment definitions, in `environments.json` beside `profiles.json`.
///
/// Same discipline as [`crate::alerts::AlertStore`] and
/// [`crate::profiles::ProfileStore`], for the same reasons: a versioned
/// envelope so a future shape is migrated rather than half-parsed,
/// write-tmp-then-rename so a crash cannot leave a truncated file, and a write
/// lock so two IPC commands cannot lose each other's updates in a
/// read-modify-write.
///
/// **An absent file is [`defaults`], not an empty list.** That is the whole
/// migration: every existing install has no such file, and every existing
/// install has profiles tagged `dev`/`staging`/`prod` that must keep their
/// colours and keep `prod` protected.
pub struct EnvironmentStore {
    dir: PathBuf,
    write_lock: std::sync::Mutex<()>,
}

impl EnvironmentStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            write_lock: std::sync::Mutex::new(()),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("environments.json")
    }

    /// Every definition, in the order the user's manager shows them.
    pub fn list(&self) -> Result<Vec<EnvironmentDef>> {
        Ok(self.read()?.environments)
    }

    /// Upserts by case-insensitive name, so the manager's "save" is one call
    /// whether the environment is new or not — and renaming the *casing* of an
    /// existing one (`prod` → `Prod`) edits it rather than creating a twin.
    pub fn save(&self, def: EnvironmentDef) -> Result<()> {
        let def = def.validated()?;
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        match document
            .environments
            .iter_mut()
            .find(|kept| kept.is(&def.name))
        {
            Some(slot) => *slot = def,
            None => document.environments.push(def),
        }
        self.write(&document)
    }

    /// Removes an environment — **unless a connection still points at it**, in
    /// which case the refusal names them.
    ///
    /// The check is here rather than in the UI because the UI is not the only
    /// caller and because the window that lists the connections is not
    /// necessarily the window doing the deleting. Deleting one nothing
    /// references is silently fine, and deleting one that does not exist is
    /// **not an error**: the other window already did it, and idempotence is
    /// what makes that a non-event (same rule as
    /// [`crate::alerts::AlertStore::delete_rule`]).
    pub fn delete(&self, name: &str, profiles: &[ConnectionProfile]) -> Result<()> {
        let in_use = referencing(profiles, name);
        if !in_use.is_empty() {
            return Err(Error::Other(format!(
                "{name:?} is still the environment of {count} connection{plural}: {names}. Move \
                 {them} to another environment first — deleting it would leave {them} tagged with \
                 an environment this machine no longer defines.",
                count = in_use.len(),
                plural = if in_use.len() == 1 { "" } else { "s" },
                names = in_use.join(", "),
                them = if in_use.len() == 1 { "it" } else { "them" },
            )));
        }
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        let before = document.environments.len();
        document.environments.retain(|def| !def.is(name));
        if document.environments.len() == before {
            return Ok(());
        }
        self.write(&document)
    }

    /// What one profile's `environment` string means on this machine.
    ///
    /// The read can fail — an unreadable or wrong-version file — and it says
    /// so rather than answering "unprotected". Every guardrail is downstream
    /// of this, and a disk error that silently disarms one is the failure this
    /// `Result` exists for.
    pub fn resolve(&self, name: &str) -> Result<EffectiveEnvironment> {
        Ok(resolve(&self.read()?.environments, name))
    }

    /// Merges an export's definitions in, keeping what is already here.
    pub fn import(&self, incoming: Vec<EnvironmentDef>) -> Result<EnvironmentImportReport> {
        if incoming.is_empty() {
            return Ok(EnvironmentImportReport::default());
        }
        let _guard = self.write_lock.lock().unwrap();
        let mut document = self.read()?;
        let report = merge(&mut document.environments, incoming);
        if report.imported > 0 {
            self.write(&document)?;
        }
        Ok(report)
    }

    fn read(&self) -> Result<Document> {
        let path = self.file();
        if !path.exists() {
            return Ok(Document::default());
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| Error::Other(format!("reading {}: {e}", path.display())))?;
        let document: Document = serde_json::from_str(&raw)
            .map_err(|e| Error::Other(format!("parsing {}: {e}", path.display())))?;
        if document.kavka_environments != ENVIRONMENTS_VERSION {
            return Err(Error::Other(format!(
                "{} was written by a different Kavka (version {}); this build reads version \
                 {ENVIRONMENTS_VERSION}",
                path.display(),
                document.kavka_environments
            )));
        }
        Ok(document)
    }

    fn write(&self, document: &Document) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| Error::Other(format!("creating {}: {e}", self.dir.display())))?;
        let json = serde_json::to_vec_pretty(document)
            .map_err(|e| Error::Other(format!("serializing environments: {e}")))?;
        let tmp = self.dir.join("environments.json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| Error::Other(format!("writing {}: {e}", tmp.display())))?;
        fs::rename(&tmp, self.file())
            .map_err(|e| Error::Other(format!("replacing environments.json: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Scratch config dir, removed on drop — same shape as the profile and
    /// alert store tests, so these stay dependency-free.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kavka-environments-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn store(&self) -> EnvironmentStore {
            EnvironmentStore::new(self.0.clone())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn profile(name: &str, environment: &str) -> ConnectionProfile {
        serde_json::from_value(serde_json::json!({
            "id": name,
            "name": name,
            "environment": environment,
            "bootstrap_servers": ["localhost:9092"],
            "auth": { "kind": "plaintext" },
            "read_only": false,
        }))
        .expect("a ConnectionProfile")
    }

    /// The migration, stated once: no file means the three the product shipped
    /// with, and `prod` is protected. Every install upgrading into this build
    /// takes this path on its first launch.
    #[test]
    fn an_absent_file_is_the_shipped_three() {
        let dir = TempDir::new();
        let defs = dir.store().list().expect("defaults need no file");
        assert_eq!(defs, defaults());
        assert_eq!(
            defs.iter()
                .map(|d| (d.name.as_str(), d.color.as_str(), d.protected))
                .collect::<Vec<_>>(),
            vec![
                ("dev", "green", false),
                ("staging", "amber", false),
                ("prod", "red", true),
            ]
        );
    }

    /// The behaviour every guardrail in the product now reads.
    #[test]
    fn resolution_is_total_and_unknown_is_neutral_and_unprotected() {
        let defs = defaults();
        let prod = resolve(&defs, "prod");
        assert!(prod.protected && prod.known);
        assert_eq!(prod.color, "red");

        // Case-insensitive, both directions.
        assert!(resolve(&defs, "PROD").protected);
        assert!(resolve(&defs, "Prod").known);

        let unknown = resolve(&defs, "UAT");
        assert!(!unknown.known);
        assert!(!unknown.protected, "an undefined name must not arm a gate");
        assert_eq!(unknown.color, NEUTRAL_COLOR);
        // The name is kept verbatim, so the chip still says what the profile
        // says rather than going blank.
        assert_eq!(unknown.name, "UAT");
        assert!(unknown.hint().expect("a hint").contains("UAT"));
        assert!(prod.hint().is_none());
        // Even the empty string answers rather than failing.
        assert_eq!(resolve(&defs, "").color, NEUTRAL_COLOR);
    }

    /// A known environment is displayed with the casing from the *manager*,
    /// not the casing a profile happens to store.
    #[test]
    fn the_definitions_casing_wins_over_the_profiles() {
        let defs = vec![EnvironmentDef::new("Production", "violet", true)];
        let effective = resolve(&defs, "PRODUCTION");
        assert_eq!(effective.name, "Production");
        assert_eq!(effective.word(), "PRODUCTION");
        assert_eq!(resolve(&defs, "nope").word(), "nope");
    }

    /// Protection is the guardrail; colour is identity. A violet protected
    /// environment gates exactly like the red one — this is the design law of
    /// this module, so it gets an assertion rather than a comment.
    #[test]
    fn colour_and_protection_are_independent() {
        let defs = vec![
            EnvironmentDef::new("Production", "violet", true),
            EnvironmentDef::new("firedrill", "red", false),
        ];
        assert!(resolve(&defs, "Production").protected);
        assert!(!resolve(&defs, "firedrill").protected);
        assert_eq!(resolve(&defs, "firedrill").color, "red");
    }

    #[test]
    fn saving_upserts_by_case_insensitive_name() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .save(EnvironmentDef::new("UAT", "blue", false))
            .unwrap();
        assert_eq!(store.list().unwrap().len(), 4);

        // Same name, different casing: an edit, not a twin.
        store
            .save(EnvironmentDef::new("uat", "violet", true))
            .unwrap();
        let defs = store.list().unwrap();
        assert_eq!(defs.len(), 4);
        let uat = defs.iter().find(|d| d.is("UAT")).expect("still there");
        assert_eq!(uat.name, "uat", "the casing just typed wins");
        assert_eq!(uat.color, "violet");
        assert!(uat.protected);
        // Writing materialised the defaults rather than dropping them.
        assert!(defs.iter().any(|d| d.is("prod") && d.protected));
    }

    #[test]
    fn saving_validates_the_name_and_the_colour() {
        let dir = TempDir::new();
        let store = dir.store();
        let blank = store
            .save(EnvironmentDef::new("   ", "green", false))
            .unwrap_err()
            .to_string();
        assert!(blank.contains("needs a name"), "{blank}");

        let colour = store
            .save(EnvironmentDef::new("QA", "chartreuse", false))
            .unwrap_err()
            .to_string();
        assert!(colour.contains("chartreuse"), "{colour}");
        // The message lists the closed set, so the fix is in the refusal.
        for token in COLORS {
            assert!(colour.contains(token), "{colour} omits {token}");
        }
        // Neither wrote anything.
        assert_eq!(store.list().unwrap(), defaults());

        // A name with surrounding space is trimmed, not refused.
        store
            .save(EnvironmentDef::new("  QA  ", "cyan", false))
            .unwrap();
        assert!(store.list().unwrap().iter().any(|d| d.name == "QA"));
    }

    /// The refusal has to name the connections, or it sends somebody through
    /// fourteen of them looking for the one.
    #[test]
    fn deleting_a_referenced_environment_is_refused_and_names_the_connections() {
        let dir = TempDir::new();
        let store = dir.store();
        let profiles = [
            profile("orders", "prod"),
            profile("payments", "PROD"),
            profile("scratch", "dev"),
        ];

        let refusal = store.delete("prod", &profiles).unwrap_err().to_string();
        assert!(refusal.contains("orders"), "{refusal}");
        // Matched case-insensitively, so a differently-cased profile still
        // counts as a reference.
        assert!(refusal.contains("payments"), "{refusal}");
        assert!(!refusal.contains("scratch"), "{refusal}");
        assert!(refusal.contains('2'), "{refusal}");
        // And it is still there.
        assert!(store.list().unwrap().iter().any(|d| d.is("prod")));

        // One reference reads as one thing, not as "1 connections".
        let one = store.delete("dev", &profiles[2..]).unwrap_err().to_string();
        assert!(one.contains("1 connection:"), "{one}");
        assert!(one.contains(" it "), "{one}");
    }

    #[test]
    fn deleting_an_unreferenced_environment_removes_it_and_deleting_nothing_is_fine() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .delete("staging", &[profile("orders", "prod")])
            .unwrap();
        assert!(!store.list().unwrap().iter().any(|d| d.is("staging")));
        // Idempotent: the other window already did it.
        store.delete("staging", &[]).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn the_file_is_a_versioned_envelope_and_a_foreign_version_is_refused() {
        let dir = TempDir::new();
        let store = dir.store();
        store
            .save(EnvironmentDef::new("QA", "cyan", false))
            .unwrap();

        let raw = fs::read_to_string(dir.0.join("environments.json")).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(doc["kavka_environments"], ENVIRONMENTS_VERSION);
        assert_eq!(doc["environments"].as_array().unwrap().len(), 4);
        // No stray temp file survived the rename.
        assert!(!dir.0.join("environments.json.tmp").exists());

        fs::write(
            dir.0.join("environments.json"),
            r#"{"kavka_environments": 99, "environments": []}"#,
        )
        .unwrap();
        let err = store.list().unwrap_err().to_string();
        assert!(err.contains("different Kavka"), "{err}");
        // And the guardrail read says so rather than answering "unprotected".
        assert!(store.resolve("prod").is_err());
    }

    /// `protected` may be omitted by hand without making the file unparseable
    /// — it reads as "nobody said to guard this".
    #[test]
    fn an_omitted_protected_flag_is_unprotected() {
        let dir = TempDir::new();
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(
            dir.0.join("environments.json"),
            r#"{"kavka_environments": 1, "environments": [{"name": "QA", "color": "cyan"}]}"#,
        )
        .unwrap();
        let effective = dir.store().resolve("qa").unwrap();
        assert!(effective.known);
        assert!(!effective.protected);
    }

    #[test]
    fn importing_merges_by_name_and_keeps_what_is_here() {
        let dir = TempDir::new();
        let store = dir.store();
        let report = store
            .import(vec![
                // Same name as a default, different colour and — the dangerous
                // one — unprotected. Skipped, so the local guardrail survives.
                EnvironmentDef::new("PROD", "blue", false),
                EnvironmentDef::new("UAT", "violet", true),
            ])
            .unwrap();
        assert_eq!(
            report,
            EnvironmentImportReport {
                imported: 1,
                skipped: 1
            }
        );
        let defs = store.list().unwrap();
        assert_eq!(defs.len(), 4);
        let prod = defs.iter().find(|d| d.is("prod")).unwrap();
        assert!(prod.protected, "an import must never disarm a guardrail");
        assert_eq!(prod.color, "red");
        assert!(defs.iter().any(|d| d.is("uat") && d.protected));

        // Nothing to do writes nothing and reports nothing.
        assert_eq!(
            store.import(Vec::new()).unwrap(),
            EnvironmentImportReport::default()
        );
    }

    #[test]
    fn referencing_lists_the_profiles_in_file_order() {
        let profiles = [
            profile("a", "Production"),
            profile("b", "dev"),
            profile("c", "PRODUCTION"),
        ];
        assert_eq!(referencing(&profiles, "production"), vec!["a", "c"]);
        assert!(referencing(&profiles, "UAT").is_empty());
    }

    /// The seven tokens are a closed set and `slate` is the neutral one — both
    /// facts are load-bearing (the CSS pairs are audited per token, and the
    /// unknown-environment fallback names one of them).
    #[test]
    fn the_colour_vocabulary_is_closed_and_contains_the_neutral() {
        assert_eq!(COLORS.len(), 7);
        assert!(COLORS.contains(&NEUTRAL_COLOR));
        let mut sorted = COLORS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), COLORS.len(), "a token is listed twice");
        for def in defaults() {
            assert!(COLORS.contains(&def.color.as_str()), "{def:?}");
        }
    }
}
