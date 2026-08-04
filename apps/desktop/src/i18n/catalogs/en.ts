/**
 * English — the source catalog, and the only complete one by definition.
 *
 * Every other catalog is typed against this object's keys, so a typo in a
 * translation is a compile error and a missing key is a fallback to the line
 * below rather than a blank in the UI. Adding a string anywhere in the shell
 * starts here: add the key, use it, and `npx tsc --noEmit` will not complain
 * about the other five — they fall back until someone translates them.
 *
 * House style is DESIGN.md §7 and it survives translation: sentence case,
 * the verb surviving the whole flow, errors that name the next click, no
 * apologies and no exclamation marks.
 *
 * COVERAGE: the shell only — App, Sidebar, Palette, AboutDialog,
 * ImportExportDialog, ProfileEditor. The cluster views (topics, groups,
 * messages, monitoring, ACLs, schemas, …) are still English. See docs/I18N.md.
 */

const en = {
  // ── Shared across more than one shell surface ────────────────────────────
  "common.close": "Close",
  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.connect": "Connect",
  "common.tryAgain": "Try again",
  "common.remove": "Remove",
  "common.dismiss": "Dismiss",
  "common.showDetails": "Show details",
  "common.addConnection": "Add connection",
  "common.support": "Support Kavka ☕",
  "common.readingConnections": "Reading your saved connections…",
  "common.linkFailed":
    "Kavka couldn't hand that link to your browser. The address is {url} — copy it from here.",

  // ── Sidebar ─────────────────────────────────────────────────────────────
  "sidebar.navLabel": "Saved connections",
  "sidebar.title": "Clusters",
  "sidebar.loading": "Reading your connections…",
  "sidebar.empty": "Nothing here yet. Add your first connection below.",
  // Law 2 (DESIGN.md §1): the status dot never carries the meaning alone, so
  // this line reads "address · state" in EVERY state. Keep both slots.
  "sidebar.profileMeta": "{address} · {status}",
  "sidebar.status.disconnected": "not connected",
  "sidebar.status.connecting": "connecting…",
  "sidebar.status.connected": "connected",
  "sidebar.draftName": "New connection",
  "sidebar.draftMeta": "not saved yet",
  "sidebar.about": "About",

  // ── App shell: status bar, empty states, global errors ───────────────────
  "app.status.disconnected": "Not connected",
  "app.status.connecting": "Connecting…",
  "app.status.connected": "Connected",
  "app.error.unknownProfile":
    "That connection isn't on this machine any more. It may have been deleted in another window.",
  "app.profilesFailed.title": "Kavka couldn't read its connection file",
  "app.profilesFailed.hint":
    "Your connections are still on disk — nothing was lost. Kavka stores them in its config directory, alongside this app's settings.",
  "app.firstRun.title": "Point Kavka at a broker",
  "app.firstRun.what":
    "A connection is a saved address for one Kafka cluster — a name, one broker to start from, and how to sign in. Kavka finds the rest of the cluster from there.",
  "app.firstRun.example":
    "A bootstrap server usually looks like {example}. Running this repo's dev cluster? Use {local}.",
  // The single most valuable sentence on first launch: the target user has
  // often just been handed production credentials. Do not soften it.
  "app.firstRun.footnote":
    "Passwords go to your operating system's keychain. Nothing about your clusters leaves this machine.",
  "app.pick.title": "Pick a connection",
  "app.pick.hint":
    "Choose a cluster on the left to see its brokers and topics, or add another connection.",
  "app.readonlyChip": "read-only",
  "app.readonlyTitle":
    "This connection is read-only. Turn that off in the connection's settings to produce or edit.",
  "app.statusbar.draft": "New connection — not saved yet",
  "app.statusbar.none": "No connection selected",
  "app.statusbar.commands": "commands",
  "app.statusbar.coreVersion": "core v{version}",
  "app.cmd.search": "Search in {topic}",
  "app.cmd.search.kw": "find filter cel scan query messages grep",
  "app.cmd.sql": "Query {topic} with SQL",
  "app.cmd.sql.kw": "sql select query aggregate count group datafusion analyse",
  "app.cmd.produce": "Produce to {topic}",
  "app.cmd.produce.kw": "send write publish message record bulk producer",
  "app.cmd.produce.confirmContext": "{cluster} · asks for confirmation",

  // ── Command palette ─────────────────────────────────────────────────────
  "palette.label": "Commands",
  "palette.searchLabel": "Search commands and clusters",
  "palette.searchPlaceholder": "Search commands and clusters…",
  "palette.empty":
    "Nothing matches “{query}”. Try a cluster name, or clear the box to see everything Kavka can do.",
  "palette.foot.move": "move",
  "palette.foot.run": "run",
  "palette.foot.close": "close",
  // §7 rule 2: the verb survives the flow. A cluster that is already up is
  // somewhere you GO, not something you connect.
  "palette.goTo": "Go to {name}",
  "palette.connectTo": "Connect to {name}",
  "palette.state.connected": "connected",
  "palette.state.connecting": "connecting…",
  "palette.prodCluster": "prod cluster",
  "palette.profile.kw": "connect open switch cluster broker bootstrap",
  "palette.add.context": "A name, one broker, and how to sign in",
  "palette.add.kw": "new connection profile cluster create bootstrap broker",
  "palette.disconnect": "Disconnect",
  "palette.disconnect.kw": "close leave cluster session",
  "palette.disconnect.none": "Nothing is connected right now",
  "palette.disconnect.ambiguous":
    "Pick the cluster you want to disconnect in the sidebar first",
  "palette.refresh": "Refresh topics",
  "palette.refresh.kw": "reload metadata list topics partitions cluster",
  "palette.export": "Export connections…",
  "palette.export.context": "Every connection on this machine, as JSON",
  "palette.export.kw": "backup save copy share json profiles",
  "palette.import": "Import connections…",
  "palette.import.context": "Paste JSON from another copy of Kavka",
  "palette.import.kw": "restore paste load json profiles",
  "palette.about.context": "Version and licence",
  "palette.about.kw": "version licence license agpl source github help",
  "palette.support.context": "Kavka is free — donations keep it that way",
  "palette.support.kw": "donate coffee sponsor fund open source",

  // ── About dialog ────────────────────────────────────────────────────────
  "about.title": "About Kavka",
  "about.body":
    "A desktop client for Apache Kafka. Kavka runs entirely on this machine: passwords go to your operating system's keychain, and nothing about your clusters leaves this computer.",
  "about.coreVersion": "Core version",
  "about.versionLoading": "Reading it now…",
  "about.licence": "Licence",
  "about.licenceValue": "Free and open source under AGPL-3.0",
  "about.language": "Language",
  "about.language.hint":
    "Kavka's shell — the sidebar, the command palette, these dialogs and the connection form. Cluster views are still English; they are the next thing to be translated.",
  "about.language.machine":
    "{language} was machine-translated and has not been reviewed by a native speaker. Corrections are welcome — docs/I18N.md says how.",

  // ── Export / import dialog ──────────────────────────────────────────────
  "transfer.title": "Connections",
  "transfer.tablist": "Export or import",
  "transfer.tab.export": "Export",
  "transfer.tab.import": "Import",
  "transfer.export.body":
    "Every connection on this machine, as JSON. Paste it into another copy of Kavka to set the same clusters up there.",
  "transfer.export.promise":
    "Passwords and keys never leave this machine — exports carry references, not secrets.",
  "transfer.export.failed":
    "Kavka couldn't read its connection file. Your connections are still on disk — nothing was lost.",
  "transfer.export.label": "Your connections, as JSON",
  "transfer.export.copied": "Copied to the clipboard.",
  "transfer.export.copyManual":
    "Kavka couldn't reach the clipboard. The text is selected — press {key} to copy it.",
  "transfer.export.copy": "Copy to clipboard",
  "transfer.export.nothingToCopy":
    "There's nothing to copy — Kavka couldn't read its connection file",
  "transfer.export.stillReading": "Kavka is still reading your connections",
  "transfer.import.body":
    "Paste an export from another copy of Kavka. Passwords aren't in it — each imported connection asks for its own the first time you connect.",
  "transfer.import.label": "Exported JSON",
  "transfer.import.kbd": "import",
  "transfer.import.kbdClose": "close",
  "transfer.import.legend": "If a connection is already here",
  "transfer.import.skip": "Keep the one on this machine",
  "transfer.import.skipHint":
    "Connections already saved here are left exactly as they are. Everything new in the JSON is still added.",
  "transfer.import.replace": "Replace it with the one in the JSON",
  "transfer.import.replaceHint":
    "The pasted version wins — name, address, environment and sign-in method. Passwords already in your keychain stay where they are.",
  "transfer.import.failed":
    "Kavka couldn't read that as an export. Check you pasted the whole file, including the outer braces — the text Kavka got is below.",
  "transfer.import.needsJson": "Paste the JSON from an export first",
  "transfer.import.busy": "Kavka is importing those connections now",
  "transfer.import.run": "Import connections",
  "transfer.import.running": "Importing…",
  "transfer.report.empty.title": "That JSON had no connections in it",
  "transfer.report.empty.detail":
    "Check you pasted the whole export, including the outer braces — Kavka read it fine, there was just nothing to add.",
  // English needs no plural arm for these three; the `plural` wrapper is here
  // so a language that does can add `one {…}` without touching the code.
  "transfer.report.added": "{count, plural, other {# added}}",
  "transfer.report.replaced": "{count, plural, other {# replaced}}",
  "transfer.report.skipped":
    "{count, plural, other {# skipped — already on this machine}}",
  "transfer.report.unchanged.title":
    "{count, plural, one {Nothing changed — # connection was already here} other {Nothing changed — # connections were already here}}",
  "transfer.report.unchanged.detail":
    "{bits}. Choose “Replace it with the one in the JSON” above if you meant to overwrite them.",
  "transfer.report.imported.title":
    "{count, plural, one {Imported # connection} other {Imported # connections}}",
  "transfer.report.imported.detail":
    "{bits}. Passwords aren't in an export — open each new connection and enter its password before connecting.",

  // ── Connection form ─────────────────────────────────────────────────────
  "editor.new.title": "Add a connection",
  "editor.new.subtitle":
    "One broker is enough to start — Kavka discovers the rest of the cluster from there.",
  "editor.saved.subtitle": "Not connected. Check the details below, then connect.",
  "editor.name.label": "Connection name",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "Whatever you'll recognise in the sidebar. Only Kavka sees it.",
  "editor.env.label": "Environment",
  "editor.env.hint.prod":
    "Prod turns the ledger rule coral in every table, marks this cluster in the sidebar and puts a warning bar across the top of the window. Turn on read-only below unless you actually need to write.",
  "editor.env.hint.other":
    "Kavka colours every view by environment, so you can't mistake one cluster for another.",
  "editor.bootstrap.label": "Bootstrap servers",
  "editor.bootstrap.hint":
    "Any single broker in your cluster — Kavka finds the rest from there. One per line, or comma separated. Running this repo's dev cluster? Use {local}.",

  "editor.auth.legend": "Sign-in",
  "editor.auth.kerberos":
    "This connection signs in with Kerberos ({service} as {principal}), which Kavka can't set up yet. Saving keeps it exactly as it is; every other field here still works.",
  "editor.auth.label": "How does this cluster check who you are?",
  "editor.auth.plaintext": "It doesn't — anyone can connect (PLAINTEXT)",
  "editor.auth.saslPlain": "Username and password — SASL/PLAIN",
  "editor.auth.saslScram": "Username and password — SASL/SCRAM",
  "editor.auth.mtls": "A certificate this machine presents — mTLS",
  "editor.auth.mskIam": "The AWS credentials on this machine — MSK IAM",
  "editor.auth.oauth": "A token from your identity provider — OAuth 2.0 / OIDC",
  "editor.auth.kerberosOption": "A Kerberos ticket — GSSAPI (not yet)",
  // §5.5: every disabled control says why, and that includes a disabled
  // <option>. An unexplained grey choice reads as a broken build.
  "editor.auth.notYet":
    "Kavka can't set this up yet. A connection that already uses it keeps working and is preserved exactly as it is when you save.",
  "editor.auth.hint":
    "Managed Kafka usually wants SASL/SCRAM with TLS on. A local broker usually wants nothing at all. Kerberos is the one method Kavka can't set up yet.",
  "editor.mechanism.label": "SCRAM mechanism",
  "editor.mechanism.hint":
    "If the broker rejects one, it will tell you which it wants.",
  "editor.username.label": "Username",
  "editor.password.label": "Password",
  "editor.password.placeholder": "Password",
  "editor.secret.unchanged": "••••••••  (unchanged)",
  "editor.password.hint":
    "Goes to your operating system's keychain — never into the connection file, and never off this machine.",
  "editor.tls.label": "Encrypt the connection (TLS)",
  "editor.tls.hint":
    "Managed Kafka almost always needs this on. If the broker answers but the handshake fails, this is the first thing to try.",

  "editor.mtls.hint":
    "Kavka reads PEM files exactly as they are — there is no JKS or PKCS#12 keystore to convert first.",
  "editor.caPath.label": "CA certificate",
  "editor.caPath.hint":
    "Path to the CA .pem — leave empty to use the system trust store.",
  "editor.clientCert.label": "Client certificate",
  "editor.clientCert.hint":
    "Path to the certificate this machine shows the broker — leave empty if the broker doesn't ask for one.",
  "editor.clientKey.label": "Client private key",
  "editor.clientKey.hint":
    "Paste the key itself, not a path to it. It goes to your operating system's keychain — never into the connection file, and never off this machine.",
  "editor.clientKey.storedHint":
    "Leave it empty to keep the stored key; clearing the certificate path above removes it.",

  "editor.aws.hint":
    "Kavka signs each request with the AWS credentials already on this machine. The bootstrap servers above have to be this cluster's IAM endpoint — the {host} hosts from the MSK console, usually on port 9098.",
  "editor.region.label": "Region",
  "editor.region.hint":
    "The AWS region the cluster runs in. It has to match the bootstrap hosts, or the signature won't be accepted.",
  "editor.awsProfile.label": "AWS profile name",
  "editor.awsProfile.hint":
    "A named profile from {config}. Leave empty to use the default credential chain — environment variables, then {dir}, then SSO.",

  "editor.oauth.hint":
    "Kavka asks your identity provider for a token with the client credentials grant, then presents it to the broker as SASL/OAUTHBEARER.",
  "editor.tokenEndpoint.label": "Token endpoint",
  "editor.tokenEndpoint.hint":
    "The URL that issues the token, not the sign-in page a browser would use.",
  "editor.clientId.label": "Client id",
  "editor.clientId.hint":
    "The application your identity provider registered for Kafka — not your own user account.",
  "editor.clientSecret.label": "Client secret",
  "editor.clientSecret.placeholder": "Client secret",
  "editor.clientSecret.hint":
    "Goes to your operating system's keychain — never into the connection file, and never off this machine.",

  "editor.sr.legend": "Schema Registry (optional)",
  "editor.sr.hint":
    "If this cluster's messages are Avro, Protobuf or JSON Schema, Kavka reads the schema from here to decode them — and shows the subject, version and id beside each message. Without it those payloads are shown as raw bytes.",
  "editor.srUrl.label": "Registry address",
  "editor.srUrl.hint":
    "The whole URL, including the scheme. Confluent, Apicurio and Glue all speak the same read API here. Leave it empty if this cluster has no registry.",
  "editor.srUsername.label": "Registry username",
  "editor.srUsername.hint":
    "Only if the registry asks for one. Managed registries usually do; a registry inside your own network usually doesn't.",
  "editor.srPassword.label": "Registry password",
  "editor.srPassword.storedHint":
    "Leave it empty to keep the stored one; clearing the address above removes it.",

  "editor.connect.legend": "Kafka Connect clusters (optional)",
  "editor.connect.hint":
    "Kafka Connect runs source and sink connectors, and it answers on its own REST port rather than through the brokers — so Kavka has to be told where the workers are. Add one per worker group; the name is how you'll pick between them in the Connect tab.",
  "editor.connect.unnamed": "Cluster {number}",
  "editor.connect.removeLabel": "Remove {name}",
  "editor.connect.unnamedLong": "Connect cluster {number}",
  "editor.connect.remove": "Remove this Connect cluster from the connection",
  "editor.connect.name.label": "Name",
  "editor.connect.name.placeholder": "orders connect",
  "editor.connect.name.hint":
    "Whatever you'll recognise. Renaming it later keeps its stored password.",
  "editor.connect.url.label": "Workers' address",
  "editor.connect.url.hint":
    "The REST endpoint of any worker in the group — they all answer for the whole cluster. Usually port 8083, and not the same host or port as the brokers.",
  "editor.connect.username.hint":
    "Only if the workers sit behind basic auth. Most don't.",
  "editor.connect.password.storedHint":
    "Leave it empty to keep the stored one; removing this cluster removes it.",
  "editor.connect.add": "Add a Connect cluster",

  "editor.monitoring.legend": "Monitoring (optional)",
  "editor.monitoring.hint":
    "Kafka's brokers don't serve throughput, storage or replication figures over the Kafka protocol — they publish them as JMX, and almost everyone puts a Prometheus exporter in front of that. Point Kavka at the exporter and the Monitoring tab fills in. Lag history needs none of this: Kavka reads that from the brokers itself.",
  "editor.metricsUrl.label": "Metrics address",
  "editor.metricsUrl.hint":
    "The whole URL, including the path. If you run the brokers, this is usually the {agent} Java agent on one of them ({flag}). A Prometheus server that already scrapes those brokers works too — give Kavka its address instead. Leave it empty if this cluster has no exporter.",
  "editor.metricsUsername.label": "Metrics username",
  "editor.metricsUsername.hint":
    "Only if the endpoint sits behind basic auth. A jmx_exporter usually doesn't; a shared Prometheus usually does.",
  "editor.metricsPassword.label": "Metrics password",
  "editor.metricsPassword.storedHint":
    "Leave it empty to keep the stored one; clearing the address above removes it.",
  "editor.sampler.label": "Take a lag reading every",
  "editor.sampler.hint":
    "Seconds. Kafka doesn't remember lag, so Kavka takes its own reading on this interval and keeps {days} of it in a file on this machine. {warning} The floor is {floor}; the default is {default}, which costs one small request per group per reading.",
  "editor.sampler.warning":
    "Readings only happen while this connection is up — nothing is collected while Kavka is closed or this cluster is disconnected, and a gap in the chart means exactly that.",

  "editor.readonly.label": "Read-only connection",
  "editor.readonly.hint":
    "Kavka will still browse everything, but it won't produce messages, change topics or commit offsets over this connection.",

  "editor.busy.connecting": "Wait for the connection attempt to finish",
  "editor.busy.saving": "Kavka is saving this connection",
  "editor.delete": "Delete connection",
  // §7 rule 8: the blast radius before the button, and the confirm restates
  // the verb — never "OK".
  "editor.delete.confirm":
    "Remove {name} from this machine? The cluster itself isn't touched.",
  "editor.kbd.connect": "connect",
  "editor.kbd.cancel": "cancel",
  "editor.kbd.undo": "undo edits",

  // Validation — §5.3. Every one of these names the fix, not the failure, and
  // is rendered under the control it belongs to, never in a banner.
  "editor.err.name":
    "Give this connection a name so you can find it in the sidebar.",
  "editor.err.bootstrap":
    "Add at least one broker, as host:port — e.g. broker-1:9092",
  "editor.err.srUrl":
    "Use the whole URL, starting with http:// or https:// — e.g. http://localhost:8081",
  "editor.err.srUserNoUrl":
    "Add the registry's address, or clear the username — a sign-in with nothing to sign in to can't be saved.",
  "editor.err.metricsUrl":
    "Use the whole URL, starting with http:// or https:// — e.g. http://broker-1.internal:7071/metrics",
  "editor.err.metricsUserNoUrl":
    "Add the metrics address, or clear the username — a sign-in with nothing to sign in to can't be saved.",
  "editor.err.sampler":
    "Sample at least every {seconds, plural, one {# second} other {# seconds}}. Anything faster asks the brokers for offsets more often than they change.",
  "editor.err.connectName":
    "Give this Connect cluster a name — every action Kavka sends names the cluster it goes to.",
  "editor.err.connectDuplicate":
    "Two Connect clusters on one connection can't share a name — Kavka stores their passwords under it.",
  "editor.err.connectUrlMissing":
    "Add the workers' REST address — e.g. http://connect-1.internal:8083",
  "editor.err.connectUrl":
    "Use the whole URL, starting with http:// or https:// — e.g. http://connect-1.internal:8083",
  "editor.err.username":
    "This sign-in method needs the username the broker knows you by.",
  "editor.err.password": "This sign-in method needs a password.",
  "editor.err.clientKey":
    "Paste the private key that goes with that certificate — Kavka needs both halves.",
  "editor.err.clientCert":
    "Add the path to the certificate this key belongs to — Kavka needs both halves.",
  "editor.err.region": "Name the region the cluster runs in — e.g. eu-west-1",
  "editor.err.tokenEndpoint":
    "Add the URL your identity provider issues tokens at — e.g. https://login.example.com/oauth2/token",
  "editor.err.tokenEndpointUrl":
    "Use the whole URL, starting with https:// — e.g. https://login.example.com/oauth2/token",
  "editor.err.clientId":
    "Add the client id your identity provider issued for this application.",
  "editor.err.clientSecret":
    "This sign-in method needs the secret that goes with that client id.",

  // Spans. ProfileEditor formats its own rather than importing monitoring.ts's
  // English `formatSpan`, so the sampler sentence has no English island in it.
  // Same thresholds as that function — change both or neither.
  "unit.seconds": "{count, plural, one {# second} other {# seconds}}",
  "unit.minutes": "{count, plural, one {# minute} other {# minutes}}",
  "unit.hours": "{count, plural, one {# hour} other {# hours}}",
  "unit.days": "{count, plural, one {# day} other {# days}}",
} as const;

export type MessageKey = keyof typeof en;

/** Every catalog is checked against this shape. Partial is allowed; `en` fills the gaps. */
export type Catalog = Partial<Record<MessageKey, string>>;

export default en;
