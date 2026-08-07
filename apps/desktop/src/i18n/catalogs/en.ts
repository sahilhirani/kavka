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
 * COVERAGE: the shell — App, the rail, ClusterSwitcher, Palette, AboutDialog,
 * ImportExportDialog, ProfileEditor, Settings — and, since Jackdaw, the Perch
 * on every cluster screen: the one-line verdict each of them opens with, plus
 * the small amount of furniture that verdict leans on. The tables, forms and
 * modals BENEATH those sentences are still English. See docs/I18N.md.
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

  // ── Destructive confirmations ───────────────────────────────────────────
  // Law 2 (DESIGN.md §1): the modal's red edge never carries the meaning
  // alone — `confirm.kicker.destructive` spells it out in a word, in every
  // theme, for every reader.
  "confirm.kicker.destructive": "Destructive",
  "confirm.busy": "Kavka is working on it",
  "confirm.type.label": "Type {name} to confirm",
  "confirm.type.reason": "Type {name} exactly to confirm this",


  // ── The cluster switcher and the rail's cluster card ──────────────────
  // The sidebar is gone (DESIGN.md §5.1); its rows are this menu's rows and
  // its identity block is the rail's cluster card. Law 2 (§1) survives the
  // move: the status dot never carries the meaning alone, so `rowMeta` reads
  // "address · state" in EVERY state. Keep both slots.
  "switcher.trigger": "Switch cluster",
  "switcher.menuLabel": "Your connections",
  "switcher.empty": "No connections saved yet.",
  "switcher.noEnvironment": "No environment",
  "switcher.rowMeta": "{address} · {status}",
  "switcher.status.disconnected": "not connected",
  "switcher.status.connecting": "connecting…",
  "switcher.status.connected": "connected",
  "switcher.connect": "Connect",
  "switcher.connecting": "Connecting…",
  "switcher.disconnect": "Disconnect",
  "switcher.connectTitle": "Connect to {name}",
  "switcher.disconnectTitle": "Disconnect from {name}",
  "switcher.draftName": "New connection",
  "switcher.draftMeta": "not saved yet",
  // The WORD channel of the protected signal on a cluster row, in the switcher
  // menu and on the Connections screen. Visually the row already carries the
  // warm ground and the padlock; this is what a screen reader gets, so it is a
  // whole word rather than the lower-case fragment `env.mgr.row.protected` is.
  "switcher.protected": "Protected",
  "brand.versionTitle": "Kavka core version {version}",
  "card.none": "No cluster",
  "card.state.none": "Nothing selected yet",
  "card.state.connected":
    "Connected · {count, plural, one {# broker} other {# brokers}}",
  "card.state.connecting": "Connecting…",
  "card.state.disconnected": "Not connected",

  // ── App shell: status bar, empty states, global errors ─────────────────
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
    "Open the cluster switcher at the top left to choose one, or add another connection.",
  "app.readonlyChip": "read-only",
  "app.readonlyTitle":
    "This connection is read-only. Turn that off in the connection's settings to produce or edit.",
  "app.statusbar.draft": "New connection — not saved yet",
  "app.statusbar.none": "No connection selected",
  "app.statusbar.commands": "commands",
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
  "palette.protectedCluster": "protected cluster",
  "palette.profile.kw": "connect open switch cluster broker bootstrap",
  "palette.add.context": "A name, one broker, and how to sign in",
  "palette.add.kw": "new connection profile cluster create bootstrap broker",
  "palette.disconnect": "Disconnect",
  "palette.disconnect.kw": "close leave cluster session",
  "palette.disconnect.none": "Nothing is connected right now",
  "palette.disconnect.ambiguous":
    "Pick the cluster you want to disconnect in the cluster switcher first",
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
  // Beside the version, and only on an install that carries one. Every build
  // ships the same `tauri.conf.json` version — MSI can't take a prerelease
  // string — so on an automated build the version alone cannot tell you which
  // one you are running, and the run number is the only thing that can.
  "about.build": "Build {number}",
  "about.licence": "Licence",
  "about.licenceValue": "Free and open source under AGPL-3.0",
  "about.language": "Language",
  "about.language.hint":
    "Kavka's shell and the verdict each cluster screen opens with — the rail, the command palette, these dialogs, the connection form and every screen's opening sentence. The tables and forms beneath them are still English.",
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
  "transfer.report.envAdded":
    "{count, plural, one {# environment added} other {# environments added}}",
  "transfer.report.envSkipped":
    "{count, plural, one {# environment already defined} other {# environments already defined}}",
  "transfer.report.envOnly.title": "No new connections — only environments",
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
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "Whatever you'll recognise in the cluster switcher. Only Kavka sees it.",
  "editor.env.label": "Environment",
  "editor.env.hint.protected":
    "This environment is marked protected: the ledger rule carries its colour in every table, the cluster switcher marks this cluster, a warning bar sits across the top of the window, and every destructive action asks you to type the name first. Turn on read-only below unless you actually need to write.",
  "editor.env.hint.other":
    "Kavka colours every view by environment, so you can't mistake one cluster for another.",
  "editor.env.manage": "Manage environments…",
  "editor.env.hint.unknown":
    "Nothing on this machine defines {name}, so Kavka shows it in neutral grey and applies no guardrails. Add it under Manage environments to give it a colour and decide whether it is protected.",
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

  "editor.tls.hintPlainCleartext":
    "With this off and SASL/PLAIN selected, your password is sent to the broker unencrypted — anything on the network path can read it. Managed Kafka almost always needs this on.",
  "editor.tls.hintScramCleartext":
    "With this off, SCRAM does not send your password itself, but everything it does send can be captured and attacked offline — and no other traffic is encrypted either. Managed Kafka almost always needs this on.",

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
    "Give this connection a name so you can find it in the cluster switcher.",
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

  // Jackdaw: the form's own Perch, its two new panel headings, and the
  // note a closed disclosure carries so folding never hides a set value.
  "editor.perch.screen": "Connection",
  "editor.perch.new":
    "Nothing is saved yet — Kavka has not contacted a broker, so nothing on this screen has been checked.",
  "editor.perch.saved":
    "Saved, but not connected. Kavka has not talked to {name} yet, so none of these details have been checked against the cluster.",
  "editor.perch.connected":
    "Connected to {name}. Kavka is still reading the cluster's overview.",
  "editor.perch.caveat.protected":
    "{name} is marked protected: every destructive action on this cluster asks you to type its name first.",
  "editor.perch.caveat.unknown":
    "Nothing on this machine defines {name}, so no guardrails apply to this connection.",
  "editor.perch.caveat.readonly":
    "Read-only is on — Kavka will browse this cluster but never write to it.",
  "editor.fold.set": "Set up",
  "editor.fold.notSet": "Not set up",
  "editor.fold.connectCount":
    "{count, plural, one {# cluster} other {# clusters}}",

  "editor.head.unsaved": "Not saved yet",
  "editor.step.name": "What should we call it?",
  "editor.step.env": "Which environment is this?",
  "editor.step.env.why":
    "The environment sets the colour you will see on this cluster everywhere in the app — and whether Kavka treats it as protected.",
  "editor.step.bootstrap": "Where does it live?",
  "editor.step.bootstrap.why":
    "One {term} is enough. Kavka asks it for the rest of the cluster.",
  "editor.bootstrap.term": "bootstrap server",
  "editor.step.sr": "Is there a Schema Registry?",
  "editor.step.optional": "(optional)",
  "editor.step.readonly": "Should Kavka be allowed to change anything here?",
  "editor.step.readonly.why":
    "Read-only is the safest way to look at someone else's cluster.",
  "editor.saveConnection": "Save connection",
  "editor.state.connecting": "Connecting now…",
  "editor.state.connected": "Connected right now.",
  "editor.state.failed": "The last attempt failed — the reason is above.",
  "editor.state.draft": "Nothing saved yet, so nothing has been tried.",
  "editor.state.idle":
    "Not connected. Kavka keeps no record of when it last was.",

  // ── The Connections screen — its head and the saved-clusters panel ───────
  "connections.list.title": "Saved clusters",
  "connections.list.empty":
    "No connections saved yet. The one you are writing now will be the first.",
  "connections.list.foot":
    "{protected} means Kavka asks you to type the cluster's name before anything destructive. Colour is identity; protected is the guardrail.",
  "connections.list.footProtected": "Protected",
  "connections.list.footSession":
    "“Connected” and “not connected” describe this session only — Kavka never contacts a cluster it is not connected to, so it cannot tell you whether one is up.",
  "connections.sub":
    "{count, plural, =0 {No saved clusters yet. Start the first one.} one {One saved cluster. Pick it to edit, or start another.} other {# saved clusters. Pick one to edit, or start a new one.}}",
  "connections.sub.unknown": "Pick a cluster to edit, or start a new one.",
  "connections.manageEnvironments": "Manage environments",

  // Spans. ProfileEditor formats its own rather than importing monitoring.ts's
  // English `formatSpan`, so the sampler sentence has no English island in it.
  // Same thresholds as that function — change both or neither.

  // ── Environments: the registry, and the manager that edits it ───────────
  "env.color.green": "green",
  "env.color.amber": "amber",
  "env.color.red": "red",
  "env.color.blue": "blue",
  "env.color.violet": "violet",
  "env.color.cyan": "cyan",
  "env.color.slate": "slate",

  "env.mgr.title": "Environments",
  "env.mgr.intro":
    "Name the environments your organisation actually runs. Colour tells them apart at a glance; protected is the guardrail.",
  "env.mgr.hint.title": "What protected actually does",
  "env.mgr.hint.detail":
    "Kavka asks you to type the cluster's name before anything destructive, puts the environment in the window title, and refuses destructive commands from the CLI and the MCP server without an explicit flag. Two of those happen in other processes, which is why they are written down here. Colour is only how you recognise it — every chip spells its name too.",
  "env.mgr.failed": "That didn't go through",
  "env.mgr.working": "Kavka is working on it",
  "env.mgr.namesAreYours":
    "Names are yours. Add as many as your organisation actually has — Kavka does not assume there are only three.",
  "env.mgr.add": "Add environment",
  "env.mgr.edit": "Edit",

  "env.mgr.row.protected": "protected",
  "env.mgr.row.unprotected": "not protected",
  "env.mgr.row.used":
    "{count, plural, =0 {no connections} one {# connection} other {# connections}}",

  "env.mgr.name.label": "Name",
  "env.mgr.name.hint":
    "Whatever your team calls it — dev, QA, UAT, production. Shown exactly as you type it, and never translated.",
  "env.mgr.name.taken": "There's already an environment with this name.",
  "env.mgr.name.required": "Give the environment a name first",

  "env.mgr.color.label": "Colour",
  "env.mgr.color.hint":
    "Identity only. The colour tints the ledger rule and the chip; it never decides what Kavka lets you do.",

  "env.mgr.protected.label": "Treat this environment as protected",
  "env.mgr.protected.hint":
    "Kavka switches to the warning substrate, asks you to type the topic or group name before anything destructive, marks the window, and refuses writes from the command line and from AI assistants unless they are explicitly told otherwise.",
  "env.mgr.unprotect.prompt": "Type {name} to remove its protection",
  "env.mgr.unprotect.hint":
    "Every connection in {name} loses its guardrails: no typed confirmations, and the command line and AI assistants stop refusing writes.",

  "env.mgr.delete.title": "Remove {name}?",
  "env.mgr.delete.unused":
    "No connection uses {name}, so nothing else changes.",
  "env.mgr.delete.used":
    "{count, plural, one {# connection uses} other {# connections use}} {name}. Pick where they go — Kavka moves them before removing it.",
  "env.mgr.delete.moveTo": "Move those connections to",
  "env.mgr.delete.moveHint": "These connections will move: {names}.",
  "env.mgr.delete.confirm": "Remove environment",
  "env.mgr.delete.needTarget":
    "Pick an environment to move those connections to.",
  "env.mgr.delete.last":
    "This is the only environment left — add another one first",

  // ── The cluster rail (Jackdaw) ──────────────────────────────────────────
  // Group names are named after what their screens are ABOUT, not after
  // Kafka's own nouns: someone who does not yet know what an ACL is can still
  // guess that it lives under Safety.
  // The two app-level groups. They render with NOTHING connected, which is
  // the whole reason Settings is a rail item: on first launch there is no
  // cluster, and the theme and the font size are what a new user needs first.
  // The read-only readout states its answer in BOTH directions — a guardrail
  // that is silent in its dangerous state is not a guardrail.
  "rail.navLabel": "Screens",
  "rail.group.setup": "Set up",
  "rail.group.application": "Application",
  "rail.item.connections": "Connections",
  "rail.item.settings": "Settings",
  "rail.readonly.label": "Read-only: {state}",
  "rail.readonly.on": "on",
  "rail.readonly.off": "off",
  "rail.readonly.on.why": "Kavka will not produce or delete here.",
  "rail.readonly.off.why": "Kavka can produce and delete here.",

  "rail.group.cluster": "Cluster",
  "rail.group.observe": "Observe",
  "rail.group.safety": "Safety",
  "rail.group.integrations": "Integrations",
  "rail.item.overview": "Home",
  "rail.item.topics": "Topics",
  "rail.item.groups": "Consumer groups",
  "rail.item.brokers": "Brokers",
  "rail.item.monitoring": "Monitoring",
  "rail.item.alerts": "Alerts",
  "rail.item.streams": "Streams",
  "rail.item.acls": "ACLs",
  "rail.item.masking": "Masking",
  "rail.item.connect": "Connect",
  // Law 2: the badge's number always has this word beside it, in the title
  // and in an sr-only span.
  "rail.firing": "firing",
  "rail.firingTitle":
    "{count, plural, one {# alert rule is firing right now} other {# alert rules are firing right now}}",

  // ── The stage head ──────────────────────────────────────────────────────
  // Every screen opens with a title and ONE sentence of context, and that
  // sentence is where the screen says what it cannot tell you — before the
  // numbers, not in a footnote under them. The titles are the rail's own
  // labels wherever they read as a title on their own; "Home" does not, so it
  // gets the noun back.
  "stage.overview.title": "Cluster home",
  "stage.overview.sub":
    "What this cluster is made of, from the metadata it answered with when you connected.",
  "stage.overview.refresh": "Refresh",
  "stage.overview.refresh.title":
    "Reads this screen again — the groups, the alert log, the quorum and the broker settings. The tiles and the broker list came with the connection and only change when you reconnect.",
  "stage.topics.sub":
    "Every topic this cluster reported, with what Kavka can and cannot tell you about each one.",
  "stage.groups.sub":
    "Who is reading, how far behind they are, and the moment that was measured.",
  "stage.brokers.sub":
    "The machines in this cluster and the settings each one is running with.",
  "stage.monitoring.sub":
    "Drawn only from readings Kavka took while it was open — there is a gap for every hour it was not.",
  "stage.alerts.sub":
    "Rules Kavka checks for you while it is running, and everything that has fired.",
  "stage.streams.sub":
    "Kafka Streams applications, read from the consumer groups behind them.",
  "stage.acls.sub":
    "Who is allowed to do what here, exactly as the cluster itself reports it.",
  "stage.masking.sub":
    "Kavka's own rules for hiding values on screen. Nothing here changes the cluster or what it stores.",
  "stage.connect.sub":
    "Kafka Connect workers this connection knows about, and the connectors running on them.",

  // ── Cluster home (Jackdaw) — the tiles, the triage list, the two tables ──
  "home.clusterId": "Cluster ID",
  "home.clusterId.absent": "This cluster did not report an id.",
  "home.tile.reading": "still reading",
  "home.tile.brokers.sub": "as the cluster named them when you connected",
  "home.tile.brokers.none":
    "the cluster reported none — the connection is up but metadata came back empty",
  "home.tile.topics.sub":
    "{partitions, plural, one {# partition} other {# partitions}} in total",
  "home.tile.partitions": "Partitions",
  "home.tile.partitions.sub":
    "across every topic — copies on other brokers are not counted twice",
  "home.tile.groups.allStable": "every one of them stable",
  "home.tile.groups.unsettled":
    "{count, plural, one {# not stable right now} other {# not stable right now}}",
  "home.tile.groups.idle":
    "{count, plural, one {# has nobody connected} other {# have nobody connected}}",
  "home.tile.groups.none": "nothing is reading this cluster right now",
  "home.tile.groups.unread":
    "Kavka couldn't read the group list, so it can't say.",
  "home.attention.title": "Needs a look",
  "home.attention.provenance":
    "Kavka only lists what it can prove from this snapshot.",
  "home.attention.reading":
    "Reading this connection's alert log and its group list…",
  "home.attention.unread":
    "Kavka couldn't read this connection's alert log, so it can't say whether anything is firing. An empty list here would not mean everything is fine.",
  "home.attention.clear": "Nothing in this snapshot needs a look.",
  "home.attention.clear.sub":
    "No alert rule you set is firing, and every consumer group Kafka named is stable. That is not a promise about anything Kavka did not measure.",
  "home.attention.partial":
    "No alert rule you set is firing. Kavka couldn't read this connection's group list, so this screen cannot say whether anything is reading — an empty list here is not an all-clear.",
  "home.attention.groupsUnread":
    "Kavka couldn't read this connection's group list, so nothing in this list is about who is reading.",
  "home.attention.alert.noDetail":
    "Kavka recorded this firing without the numbers behind it.",
  "home.attention.group.title": "{group} is not reading right now",
  "home.attention.group.sub":
    "Kafka reports this group as {state}, with {members, plural, one {# member} other {# members}}. A group that is not stable has stopped consuming until the rebalance finishes.",
  "home.attention.open.monitoring": "Open in Monitoring",
  "home.attention.open.alerts": "Open Alerts",
  "home.attention.open.groups": "Open Consumer groups",
  "home.attention.where": "on {screen}",
  "home.attention.foot":
    "Built from the newest {limit} entries in this connection's own alert log and the group list this screen read. Kavka evaluates nothing else here — a problem no rule watches for will not appear in this list.",
  "home.brokers.caption": "Brokers in this cluster",
  "home.brokers.none":
    "This cluster reported no brokers. That normally means the connection is up but metadata came back empty — try reconnecting.",
  "home.brokers.foot":
    "These are the brokers this cluster named in the metadata it answered with when you connected. Kavka has not contacted them one by one since, so a broker that stopped a minute ago is still listed here.",
  "home.brokers.details": "Details",
  "home.brokers.details.note":
    "protocol version, log directories, replication and retention, read from broker {id}",
  "home.brokers.details.reading": "Reading broker {id}'s configuration…",
  "home.brokers.details.unread":
    "Kavka couldn't read broker {id}'s configuration. The Brokers screen asks for the same settings and shows the error behind this.",
  "home.brokers.details.caveat":
    "Read from broker {id} only. Another broker in this cluster can be configured differently, and a cluster whose brokers disagree is a common and quiet misconfiguration.",
  "home.brokers.fact.protocol": "Protocol version",
  "home.brokers.fact.logDirs": "Log directories",
  "home.brokers.fact.replication": "Default replication",
  "home.brokers.fact.autoCreate": "Auto-create topics",
  "home.brokers.fact.retention": "Default retention (hours)",
  "home.brokers.fact.absent": "not set on this broker",

  // ── The Perch (Jackdaw) ─────────────────────────────────────────────────
  // Every screen opens with one of these. House rule: say what is true, say
  // what you don't know, and never be cheerful about numbers you didn't get.
  "perch.label": "{screen} — what Kavka can tell you",
  "perch.kicker": "{screen} · {state}",
  "perch.state.ok": "Looks healthy",
  "perch.state.watch": "Worth a look",
  "perch.state.problem": "Something's wrong",
  "perch.state.unknown": "Not sure yet",
  "perch.state.checking": "Still checking",
  "perch.checking":
    "Still checking — Kavka will say what it finds as soon as the cluster answers.",
  // The note can be put away for this screen. It comes back on its own the
  // moment the screen is loading or something failed, so none of these three
  // is ever the reason a user didn't hear about a problem.
  "perch.hide": "Hide",
  "perch.more": "Show the whole note",
  "perch.show": "Show the note for this screen",
  "perch.overview.counts":
    "Connected to {brokers, plural, one {# broker} other {# brokers}}, carrying {topics, plural, one {# topic} other {# topics}} across {partitions, plural, one {# partition} other {# partitions}}.",
  "perch.overview.firing":
    "{count, plural, one {# alert rule is} other {# alert rules are}} firing on this cluster right now. {counts}",
  "perch.overview.snapshot":
    "These counts came back when you connected and don't follow the cluster — reconnect to take them again.",
  "perch.overview.noBrokers":
    "This cluster answered, but it named no brokers at all.",
  "perch.overview.noBrokers.next":
    "That usually means you reached a load balancer rather than Kafka itself, or that metadata came back empty. Disconnect and connect again, and check the bootstrap address.",
  "perch.screen.messages": "Messages",
  "perch.screen.search": "Search",
  "perch.screen.sql": "SQL",
  "perch.screen.schemas": "Schemas",
  "perch.topics.unreadable": "Kavka has no list of this cluster's topics.",
  "perch.topics.unreadable.next":
    "The connection can be up while the account lacks Describe on the cluster. Refresh asks again.",
  "perch.topics.empty":
    "This cluster has no topics at all — none have been created on it yet.",
  "perch.topics.internalOnly":
    "Everything on this cluster is one of Kafka's own internal topics. Turn on Show internal to see them.",
  "perch.topics.counts":
    "{count, plural, one {# topic on this cluster} other {# topics on this cluster}}.",
  "perch.topics.countsHidden":
    "{count, plural, one {# topic shown} other {# topics shown}}.",
  "perch.topics.hiddenNote":
    "{count, plural, one {# more is one of Kafka's own internal topics and is hidden} other {# more are Kafka's own internal topics and are hidden}}.",
  "perch.topics.snapshot":
    "This list was read when you opened the screen and does not follow the cluster — Refresh reads it again.",
  "perch.topics.readOnly":
    "This connection is read-only, so nothing here can create, change or delete a topic.",
  "perch.topic.unreadable":
    "Kavka has no partition list for {topic}, so it cannot say what is in it.",
  "perch.topic.unreadable.next":
    "The topic may have been deleted, or the account may not have Describe on it.",
  "perch.topic.underReplicated":
    "{count, plural, one {# partition here is short of a copy} other {# partitions here are short of copies}} — Kafka is holding fewer replicas than this topic asks for.",
  "perch.topic.unpreferred":
    "{count, plural, one {# partition is} other {# partitions are}} led by a broker other than the first in its replica list. That is routine after a restart, and Elect preferred leaders puts them back.",
  "perch.topic.healthy":
    "{count, plural, one {# partition} other {# partitions}}, every copy in sync.",
  "perch.topic.records": "About {records} messages by the offsets.",
  "perch.topic.approx":
    "That message count is the gap between each partition's earliest and latest offset, so it still counts records retention or compaction has already removed.",
  "perch.messages.waiting":
    "Nothing read yet. Choose where to read from above and press Fetch.",
  "perch.messages.range":
    "{count, plural, one {# message} other {# messages}} from the range you asked for.",
  "perch.messages.none": "Nothing in the range you asked for.",
  "perch.messages.topicEmpty": "{topic} holds no messages yet.",
  "perch.messages.live":
    "Watching {topic} live — {count, plural, one {# message has} other {# messages have}} arrived since the tail started.",
  "perch.messages.liveQuiet":
    "Watching {topic} live. Nothing has been produced to it for at least thirty seconds.",
  "perch.messages.notWhole":
    "This is the slice you asked for, not the whole topic — {topic} holds about {total} messages.",
  "perch.messages.dropped":
    "{count, plural, one {# message arrived} other {# messages arrived}} faster than this window could take, and the session dropped it rather than fall behind — so the rows on screen are not everything the tail saw.",
  "perch.messages.trimmed":
    "Kavka keeps the last {cap} live rows; anything older has already left the buffer.",
  "perch.messages.masked":
    "Masking rules are switched on, so some values on screen are not the values on the topic. Copies and exports carry the replacements.",
  "perch.search.waiting":
    "Nothing scanned yet. Set the scope, say what you are looking for, and press Search.",
  "perch.search.running":
    "Scanning {topic} — {count, plural, one {# match} other {# matches}} so far.",
  "perch.search.running.note":
    "Partial. These numbers keep moving until the scan finishes.",
  "perch.search.matches":
    "{count, plural, one {# match} other {# matches}} in the {scanned} records this scan read.",
  "perch.search.none":
    "Nothing matched in the {scanned} records this scan read.",
  "perch.search.stopped":
    "You stopped this scan after {scanned} records, so it answers about part of the range and not all of it.",
  "perch.search.capped":
    "{matched} records matched but Kavka kept {kept}. Sorting, exporting or counting what is on screen answers about those, not about every match.",
  "perch.search.unevaluated":
    "{count, plural, one {# record could not be read} other {# records could not be read}} against your expression. They were skipped, not judged to be non-matching.",
  "perch.search.masked":
    "Masking rules are switched on, so some values on screen — and in anything you export — are not the values on the topic.",
  "perch.sql.waiting":
    "No query has run yet. The scope above decides which records the query can see.",
  "perch.sql.running": "Running — {scanned} records read so far.",
  "perch.sql.running.note":
    "Partial. Nothing below is the final answer until the scan finishes.",
  "perch.sql.rows":
    "{count, plural, one {# row} other {# rows}} from the {scanned} records this scan read.",
  "perch.sql.none":
    "The query returned no rows from the {scanned} records this scan read.",
  "perch.sql.scope":
    "This answers about the records the scan read, not about the whole topic — a different scope is a different answer.",
  "perch.sql.capped":
    "The scan stopped at its cap of {cap} records, so anything the query counted or summed is a count over that slice.",
  "perch.sql.stopped":
    "You stopped this scan after {scanned} records, so the answer covers part of the range.",
  "perch.sql.masked":
    "Masking rules were in force while this query ran, so some values here are not the values on the topic.",
  "perch.schemas.noRegistry":
    "This connection has no Schema Registry, so there is nothing here to read schemas from.",
  "perch.schemas.noRegistry.next":
    "A registry is a separate service with its own address. Add it under Schema Registry in this connection's settings.",
  "perch.schemas.missing": "The registry has no subject called {subject}.",
  "perch.schemas.missing.next":
    "Kavka looked under the topic-name strategy, which is what most producers use. A producer using a different strategy registers under a different name.",
  "perch.schemas.versions":
    "{count, plural, one {# version of this subject is registered} other {# versions of this subject are registered}}.",
  "perch.schemas.level": "New versions are checked as {level}.",
  "perch.schemas.levelUnknown":
    "Kavka could not read this subject's own compatibility setting, so it cannot say for certain which level the registry will apply.",
  "perch.groups.none":
    "No consumer groups on this cluster yet — nothing has read from it.",
  "perch.groups.counts":
    "{count, plural, one {# consumer group is} other {# consumer groups are}} reading from this cluster.",
  "perch.groups.rebalancing":
    "{unstable, plural, one {# group is} other {# groups are}} rebalancing right now, so their partitions are being handed around and consumption is paused while it happens. {counts}",
  "perch.groups.unread":
    "Kavka couldn't read this cluster's consumer groups, so it can't say anything about them. Until it can, nothing on this screen is a claim about the cluster.",
  "perch.groups.caveat":
    "This is the list as Kavka last read it. A group's state changes on every rebalance — press Refresh to take it again.",
  "perch.group.caughtUp":
    "{group} is caught up on every partition Kavka can see.",
  "perch.group.behind":
    "{group} is about {lag} messages behind across {partitions, plural, one {# partition} other {# partitions}}. The worst is {topic} partition {partition}, at {worst}.",
  "perch.group.noOffsets":
    "{group} has never committed an offset, so there is no position to report. It may only ever have produced, or it may have been created and never read anything.",
  "perch.group.noMembers":
    "Nothing is connected to {group} right now, so it is reading nothing. Its committed offsets are still here, and an application that starts will carry on from them.",
  "perch.group.caveat":
    "Kavka read these offsets once, when this screen opened. They do not follow the group — reopen it for a fresh reading.",
  "perch.brokers.counts":
    "{count, plural, one {# broker in this cluster} other {# brokers in this cluster}}. Open one to see every setting it is running with.",
  "perch.brokers.none":
    "This cluster answered, but it named no brokers at all.",
  "perch.brokers.noneNext":
    "That usually means metadata came back empty, or that you reached a load balancer rather than Kafka itself. Disconnect and connect again, and check the bootstrap address.",
  "perch.brokers.caveat":
    "The broker list came back when you connected and does not follow the cluster — reconnect to take it again.",
  "perch.broker.noOverrides":
    "Broker {broker} changes nothing from Kafka's defaults — every setting it has is one Kafka computes.",
  "perch.broker.overrides":
    "Broker {broker} overrides {count, plural, one {# setting} other {# settings}}; the other {rest} are whatever it computes right now.",
  "perch.broker.unread":
    "Kavka couldn't read this broker's settings, so it can't say what it is running with. The account normally needs DescribeConfigs on the cluster.",
  "perch.broker.caveat":
    "Only the rows marked with a + are set on this broker. A computed default can change under you when the cluster does, and Kafka reports some settings as read-only for clients — those keep their Edit button, disabled, with the reason on hover.",
  "perch.connect.noClusters":
    "This connection has no Kafka Connect workers, so there is nothing to drive from here.",
  "perch.connect.noClustersNext":
    "Connect runs as its own set of workers with their own REST address, usually on port 8083. Add one under Kafka Connect clusters in this connection's settings.",
  "perch.connect.empty":
    "No connectors on {cluster} yet, so nothing is being moved into or out of Kafka from here.",
  "perch.connect.allRunning":
    "{count, plural, one {# connector on {cluster}} other {# connectors on {cluster}}}, and every task is running.",
  "perch.connect.failed":
    "{failed, plural, one {# task has} other {# tasks have}} failed on {cluster}. A failed task moves no records at all until something restarts it — open the connector to read the worker's own trace first.",
  "perch.connect.paused":
    "{paused, plural, one {# connector is} other {# connectors are}} paused on {cluster}, so nothing is moving through {paused, plural, one {it} other {them}}. Their configs and their committed offsets are kept.",
  "perch.connect.unread":
    "Kavka couldn't reach the Connect workers, so it can't say what is running. That is a different address from the brokers' and it may be the only thing that is down.",
  "perch.connect.caveat":
    "These states came from the workers when Kavka last asked. Connect changes them on its own — press Refresh for a new reading.",
  "perch.connector.running":
    "{name} is running: {running} of {total} tasks are moving records.",
  "perch.connector.failed":
    "{name} has {failed, plural, one {# failed task} other {# failed tasks}}, moving nothing. Read why it stopped before restarting it — a restart with the cause still there just fails again.",
  "perch.connector.paused":
    "{name} is paused, so it is moving no records. Its config and its committed offsets are kept, and resuming picks up from them.",
  "perch.connector.noTasks":
    "{name} has no tasks at all, so nothing is moving. The workers create tasks from a connector's config, and a config they could not use leaves it with none.",
  "perch.connector.caveat":
    "This is one reading, taken when Kavka last asked the workers. Task states change on their own.",
  "perch.monitoring.origin":
    "Kafka doesn't remember lag — a broker can only say where a group stands right now. Everything on this screen is Kavka's own recording, taken while this connection was up.",
  "perch.monitoring.unread":
    "Kavka couldn't read its own lag history for this connection, so it can't say how far behind anything is — or whether it has any readings at all.",
  "perch.monitoring.noHistory":
    "Kavka has no lag readings for this connection yet. The first ones appear within {interval} of connecting, and a group only appears here once it has committed an offset at least once.",
  "perch.monitoring.noWindow":
    "Kavka has no readings for {group} in this window. Try a longer one, or check the sampler below.",
  "perch.monitoring.caughtUp":
    "{group} was caught up at the last reading — nothing was waiting to be read.",
  "perch.monitoring.rising":
    "{group} is about {lag} messages behind across {partitions, plural, one {# partition} other {# partitions}}, and rising. The worst is {topic} partition {partition}, which reached {peak}.",
  "perch.monitoring.steady":
    "{group} is about {lag} messages behind across {partitions, plural, one {# partition} other {# partitions}}, and steady since the start of this window.",
  "perch.monitoring.falling":
    "{group} is about {lag} messages behind across {partitions, plural, one {# partition} other {# partitions}}, and falling.",
  "perch.monitoring.caveat.sampled":
    "A point on these charts is the worst reading in its slice, never an average, and a break in a line is a stretch Kavka was not running — not an outage.",
  "perch.monitoring.caveat.stale":
    "The sampler is behind: its last reading was {ago}, more than three intervals ago. Everything below is older than it looks.",
  "perch.monitoring.caveat.stopped":
    "Nothing is being recorded for this connection at the moment, so this verdict is only as new as the last reading Kavka managed to take.",
  "perch.monitoring.caveat.unknownSampler":
    "Kavka can't say what its sampler is doing right now, so it can't promise these readings are current.",
  "perch.alerts.none":
    "No rules on this cluster, so Kavka is watching nothing here.",
  "perch.alerts.quiet":
    "{count, plural, one {# rule is} other {# rules are}} watching this cluster, and none of them is firing.",
  "perch.alerts.firingOne": "{rule} has been firing since {time}. {detail}",
  "perch.alerts.firingMany":
    "{count, plural, one {# rule is} other {# rules are}} firing on this cluster right now. The oldest is {rule}, since {time}.",
  "perch.alerts.unread":
    "Kavka couldn't read this connection's alert rules, so it can't say what is being watched — or whether anything is.",
  "perch.alerts.unreadHistory":
    "Kavka couldn't read this connection's alert log, so it can't say whether anything is firing right now — or whether anything ever has.",
  "perch.alerts.caveat.desktop":
    "Kavka has to be running to notice. Close the window and nothing is watched — this is a desktop app, not a service.",
  "perch.alerts.caveat.silent":
    "No channel is switched on, so a firing only reaches this window and the log below. Nothing will reach you when Kavka isn't in front of you.",
  "perch.masking.none":
    "No masking rules on this connection, so everything Kavka shows you is exactly what the producer sent.",
  "perch.masking.inForce":
    "{count, plural, one {# masking rule is} other {# masking rules are}} in force, so matching text is replaced before it ever reaches this window.",
  "perch.masking.off":
    "{count, plural, one {# masking rule exists} other {# masking rules exist}} and none of them is switched on, so nothing on screen is being hidden.",
  "perch.masking.unread":
    "Kavka couldn't read this connection's masking rules, so it can't promise what you are looking at is verbatim.",
  "perch.masking.caveat":
    "A rule you switch on now applies to the next fetch, tail batch, search or query — never to rows that are already on screen.",
  "perch.masking.caveat.sawMasked":
    "Something on screen in this session has already been masked, so at least one payload here is not what the producer sent.",
  "perch.streams.noGroups":
    "This cluster has no consumer groups yet, so there is nothing to work a topology out from.",
  "perch.streams.pick":
    "Pick an application above and Kavka will work out what it reads, what it writes and what it keeps in between.",
  "perch.streams.notStreams":
    "{group} doesn't look like a Kafka Streams application, so there is no topology to draw. An ordinary consumer group having none is not a fault.",
  "perch.streams.inferred":
    "This picture of {app} is a guess: {nodes, plural, one {# node} other {# nodes}} and {edges, plural, one {# link} other {# links}}, worked out from topic names.",
  "perch.streams.unread":
    "Kavka couldn't work out a topology for {group}, so it has nothing to show. The message below is what the cluster said.",
  "perch.streams.caveat":
    "Kafka does not publish a Streams topology anywhere a client can read it. Nothing here was read from the application itself, so a processor that leaves no topic behind does not appear at all.",
  "perch.acls.noAuthorizer":
    "This cluster has no authorizer, so it has no access rules to list and every request is decided by the brokers' own default.",
  "perch.acls.noAuthorizerNext":
    "That is a broker setting (authorizer.class.name), not a permission you are missing — Kafka refuses the request outright rather than answering with an empty list.",
  "perch.acls.none":
    "This cluster has an authorizer but no access rules yet, so what happens to a request is entirely the brokers' default.",
  "perch.acls.allAllow":
    "{count, plural, one {# access rule} other {# access rules}} on this cluster, and every one of them is an allow.",
  "perch.acls.someDeny":
    "{count, plural, one {# access rule} other {# access rules}} on this cluster. {denies, plural, one {# of them is a deny} other {# of them are denies}}, and a deny beats every allow that matches the same request.",
  "perch.acls.filtered":
    "Showing {count, plural, one {# rule} other {# rules}} that match this filter.",
  "perch.acls.unread":
    "Kavka couldn't read this cluster's access rules, so it can't say who is allowed to do what. The account normally needs Describe on the cluster to list them.",
  "perch.acls.caveat.filtered":
    "A filter is in force, so this counts the rules that match it — not the rules on the cluster.",
  "perch.acls.caveat.removing":
    "Removing a deny widens access rather than narrowing it. Kavka says so again before it removes one.",

  // ── Cluster screens (Jackdaw) ───────────────────────────────────────────
  // Furniture the deep views own outside their Perch: the ACL filter's
  // disclosure, the Alerts rule card and its log, the Monitoring tiles. A
  // tile's sub-line says what the number IS, so no figure sits unqualified.
  "topics.partitions.detail": "Show replica detail",
  "topics.partitions.detailTitle":
    "Adds the replica list, the in-sync list and each partition's earliest and latest offset. Health stays on screen either way.",
  "acls.filter.summary": "Filter these rules",
  "acls.filter.note": "by resource type, resource name and principal",
  "acls.filter.active": "a filter is in force",
  "alerts.state.firing": "Firing",
  "alerts.since": "since {time}",
  "alerts.details.summary": "Details",
  "alerts.details.note": "exactly what Kavka compares, and how often",
  "alerts.facts.kind": "Kind",
  "alerts.facts.waitsFor": "Waits for",
  "alerts.facts.noWait": "nothing — it fires the moment the condition is true",
  "alerts.facts.checked": "Checked",
  "alerts.facts.checkedValue":
    "on every reading Kavka takes, and only while Kavka is open",
  "alerts.facts.since": "Firing since",
  "alerts.history.started": "{rule} — started",
  "alerts.history.cleared": "{rule} — cleared",
  "alerts.history.lasted": "Cleared at {time}, after {duration}.",
  "alerts.history.stillFiring": "Still firing, {duration} so far.",
  "alerts.history.gap":
    "This log only covers time Kavka was open. A gap in it is a stretch nobody was watching, and Kavka will not guess what happened in one.",
  "alerts.toast.viewGroup": "View group {group}",
  "alerts.toast.viewAlerts": "View the alert",
  "alerts.preview.label": "The notification for {rule}",
  "alerts.preview.sent":
    "Kavka asked your operating system to show this at {time}. Asked, not showed — the notification centre can be off or permission withdrawn, and Kavka is not told when that happens. It carries these words and nothing else: there are no buttons on it. One per firing, and one more when it clears.",
  "alerts.preview.off":
    "Desktop notifications are off for this connection, so nothing was shown outside this window. This is what it would have said — the rule's name and the numbers that tripped it, and nothing else.",
  "monitoring.tile.lagNow": "Lag at the last reading",
  "monitoring.tile.lagNowSub":
    "messages waiting to be read when Kavka last sampled",
  "monitoring.tile.peak": "Peak in this window",
  "monitoring.tile.peakSub":
    "the worst single reading Kavka took, never an average",
  "monitoring.tile.trend": "Trend",
  "monitoring.tile.trendSub": "against the start of this window",
  "monitoring.tile.partitionsSub": "with at least one reading in this window",

  // ── Panel feet — what the table above each one cannot tell you ───────────
  "topics.list.foot":
    "Shape only. This is the cluster's own metadata, read when the screen opened: it says how each topic is laid out, not how much is in it, whether anything is reading it, or whether it is healthy. Open a topic for its partitions, its counts and its consumers.",
  "topic.partitions.foot":
    "Messages is the newest offset minus the oldest one the brokers still hold for that partition. Anything retention or compaction removed is not in it, and on a compacted topic it counts offsets rather than the records you would read back — so it is what this partition can still show you, never what it has been sent.",
  "topic.config.foot":
    "Read once, when this screen opened. A row without {plus} is whatever the brokers were defaulting to at that moment and can change under this topic without anything here changing; a value Kafka marks sensitive is withheld from every client, so the dash means the broker will not say rather than that nothing is set.",
  "schemas.versions.foot":
    "These are the versions the registry holds under {subject}. Subject naming is a producer-side convention, not something the topic records — so a short list, or none, is not evidence that nothing is writing to {topic} with a schema.",
  "alerts.rules.foot":
    "“Quiet” means nothing has tripped the rule, not that Kavka checked and liked the number — a rule whose reading is unavailable is quiet too. The state comes from the log below, so it is only as complete as that log is.",
  "alerts.channels.foot":
    "Kavka asks each of these once per firing and never retries. It is not told whether your operating system actually showed the notification, and a webhook that refuses goes to the diagnostics log — if you switched that on in About — rather than being shown here, so “on” means Kavka will ask, not that somebody was reached.",
  "alerts.channels.os.denied":
    "Your operating system is refusing notifications for Kavka, so this switch cannot deliver anything until that changes. On macOS: System Settings → Notifications → Kavka. On Windows: Settings → System → Notifications → Kavka. Kavka cannot change that setting for you.",
  "alerts.channels.os.confirmed":
    "Kavka just sent one notification to confirm this channel. If your operating system asked for permission instead, answer it now — that prompt is what would otherwise swallow your first real alert. If neither appeared, nothing is reaching this desktop yet.",
  "groups.list.foot":
    "Member counts and states are from the moment Kavka asked. A group that is rebalancing is handing its partitions around while you read this, so its count is already out of date — press Refresh for a new one.",
  "group.members.foot":
    "These are the members that were connected when Kavka asked. The partition counts are that instant's assignment, and a rebalance redraws them without anything on this screen changing.",
  "group.lag.foot":
    "Lag is the End column minus the Committed column, and both were read in the same call, so they agree with each other. ∅ means the group has never committed an offset for that partition, which is not the same as a lag of zero. A group that commits rarely reads as behind on work it has already done.",
  "brokers.list.foot":
    "This is the broker list Kafka answered with when this connection was made. A broker that has joined or left since then appears here only after you reconnect.",
  "broker.config.foot":
    "This is one broker's answer. Kafka keeps most settings per broker, so another broker in this cluster can be running with a different value for the same name, and nothing on this screen would show it.",
  "monitoring.foot.lag":
    "Lag is the partition's newest offset minus the group's committed offset, and both came out of the same reading, so they agree with each other. A group that commits rarely is drawn as behind on work it has already done, and nothing here can tell that apart from a group that is genuinely behind.",
  "monitoring.foot.health":
    "Both readings come from the metrics endpoint rather than from the brokers' own answers to Kavka, so they are only as fresh as the exporter is. They are cluster-wide totals: neither can tell you which partition.",
  "monitoring.foot.throughput":
    "These are the exporter's counters, kept in memory for this connection only — they start again from nothing every time it opens. A flat line and an exporter that quietly stopped answering look the same here, which is what the sampler line above is for.",
  "monitoring.foot.noEndpoint":
    "This is a fact about the connection Kavka was given, not about the cluster. The brokers may well be publishing JMX; Kavka has simply not been told where to find it.",
  "monitoring.foot.noSeries":
    "Kavka maps the metric names it recognises and ignores the rest, so a reading published under a name it does not know is missing here rather than wrong. It never invents a value to fill the gap.",
  "streams.topology.foot":
    "Kavka can only draw topics this connection is allowed to list. A repartition or changelog topic the account cannot describe is missing from the picture, and a missing box looks exactly like an application that never had one.",
  "acls.foot.authorizer":
    "This is the list the cluster's authorizer keeps. A cluster running without an authorizer allows everything and has no rules to list, which looks the same here as a cluster nobody has written any for.",
  "masking.rules.foot":
    "A rule matches the text Kavka is about to put on screen. A value split across fields, encoded, or spelled differently simply does not match, and nothing here reports a near miss — the only proof a rule works is seeing it work.",
  "connect.connectors.foot":
    "Connect reports a connector's state separately from its tasks, so a connector can say RUNNING while every task under it has failed. The task counts in each row are the reading to trust.",
  "connect.tasks.foot":
    "Restart asks the worker to restart the task; the worker decides when. This table only changes when Kavka reads the workers again.",
  "shareGroups.foot":
    "States and member counts are the coordinator's view at the moment Kavka asked. ∅ in the start-offset column means the broker reported nothing for that partition — a gap in the answer, not a zero.",

  // ── Settings (Jackdaw) ──────────────────────────────────────────────────
  "settings.title": "Settings",
  "settings.navLabel": "Settings sections",
  "settings.perch":
    "Everything here applies as you change it and is saved on this machine. Kavka is showing the {theme} theme right now.",
  "settings.section.appearance": "Appearance",
  "settings.section.appearance.sub":
    "How Kavka looks on this machine. None of this changes a cluster.",
  "settings.section.language": "Language",
  "settings.section.language.sub":
    "Kavka's own words — the rail, the palette, the forms and each screen's opening sentence. The tables underneath them are still English.",
  "settings.section.about": "About",
  "settings.section.about.sub":
    "Which build this is, what licence it carries, and the two files it can write about itself.",

  "settings.theme.title": "Theme",
  "settings.theme.help":
    "System follows your operating system and changes with it while Kavka is open.",
  "settings.theme.contrast":
    "Both themes are checked against the same contrast floor: 4.5:1 for anything you read, 3:1 for the edge of anything you can click. Nothing is dimmed to look calmer.",
  "settings.theme.system": "System",
  "settings.theme.light": "Light",
  "settings.theme.dark": "Dark",

  "settings.accent.title": "Accent",
  "settings.accent.note":
    "Accent: {name}. Used for the thing you are about to click and the row you have selected — never for status, so changing it cannot hide a warning.",
  "settings.accent.brass": "Brass",
  "settings.accent.moss": "Moss",
  "settings.accent.sky": "Sky",
  "settings.accent.plum": "Plum",

  "settings.density.title": "Density",
  "settings.density.help":
    "Comfortable gives every row room to breathe. Compact fits about a third more rows on screen — the row height Kavka shipped with.",
  "settings.density.comfortable": "Comfortable",
  "settings.density.compact": "Compact",

  "settings.font.title": "Text size",
  "settings.font.help":
    "Scales every size in the app together, so nothing crowds or overlaps at the largest step.",
  "settings.font.s": "Small",
  "settings.font.m": "Medium",
  "settings.font.l": "Large",

  "settings.motion.title": "Motion",
  "settings.motion.help":
    "System follows your operating system's reduced-motion setting. Reduced turns off every transition and animation in Kavka as well.",
  "settings.motion.system": "System",
  "settings.motion.reduce": "Reduced",

  "settings.env.title": "Environment colours",
  "settings.env.help":
    "{count, plural, one {# environment is} other {# environments are}} set up. Colour is identity; protected is the guardrail — so this is also where you decide which ones Kavka should be careful with. Unlike everything else on this screen, these travel with a connection you export.",
  "settings.env.manage": "Manage environments",

  "settings.perch.title": "The note on each screen",
  "settings.perch.help":
    "The warm note at the top of every screen, which says what Kavka can tell you there. One line keeps the verdict and drops the qualification; hidden turns it off on screens with nothing to report. A screen that is still loading, or that failed to read, shows the whole note in every setting.",
  "settings.perch.full": "Full",
  "settings.perch.line": "One line",
  "settings.perch.hidden": "Hidden",

  "settings.sample.title": "What this looks like",
  "settings.sample.sub": "a live sample of the parts you just changed",
  "settings.sample.note":
    "These three rows are invented so you can see what density and text size do before you go and find out. Nothing here came from a cluster.",
  "settings.sample.caption":
    "A sample of three invented message rows, shown so appearance changes are visible immediately.",
  "settings.sample.primary": "A primary button",
  "settings.sample.normal": "A normal one",
  "settings.sample.chip.ok": "Healthy",
  "settings.sample.chip.warn": "Falling behind",
  "settings.sample.focus":
    "Press {key} through these to see the focus ring in this theme.",

  "settings.language.title": "Language",
  "settings.language.help":
    "Covers Kavka's shell and the verdict every cluster screen opens with — the rail, the palette, this panel, the connection form and each screen's opening sentence. The tables and forms beneath those sentences are still English.",
  "settings.language.machine":
    "This catalog came out of a machine and no native speaker has been through it. Corrections are welcome.",

  "settings.about.title": "Version, licence and diagnostics",
  "settings.about.help":
    "The About panel carries Kavka's version and licence, the MCP server settings and the crash-diagnostics switch.",
  "settings.about.open": "Open About",

  // About and Support Kavka came here when the sidebar footer was deleted.
  "settings.support.title": "Support Kavka",
  "settings.support.help":
    "Kavka is free, open source, and paid for by the people who choose to chip in. Nothing in the app is held back from anyone who doesn't.",

  // ── Updates ─────────────────────────────────────────────────────────────
  //
  // THE HONESTY DOCTRINE, IN THE COPY ITSELF. Kavka now makes exactly one
  // request nobody asked for, and these sentences are the app's account of
  // it. Every one of them names something the reader can check: the host, the
  // frequency, what the request does not carry, where the switch is, and that
  // nothing is downloaded or installed without a press. Do not compress them
  // into "we check for updates" — that is the sentence this section exists
  // not to be.
  "settings.section.updates": "Updates",
  "settings.section.updates.sub":
    "Whether Kavka asks GitHub about new releases, and what that request does and does not carry. Nothing installs on its own.",

  "settings.updates.auto.title": "Check for updates",
  "settings.updates.auto.label": "Let Kavka look for new releases",
  "settings.updates.auto.hint":
    "On by default. Kavka asks github.com what the newest release is, at most once a day — the same question the public Releases page answers for anybody. It carries nothing that identifies you and nothing about your clusters, and it downloads and installs nothing on its own. This is the only request Kavka makes that you didn't ask for; turn this off and there is none.",

  "settings.updates.channel.title": "Which releases",
  "settings.updates.channel.help":
    "Stable follows the releases a person tagged on purpose. Every build follows the pre-release published by each merge to main — newer, and not held to the same bar.",
  "settings.updates.channel.stable": "Stable",
  "settings.updates.channel.builds": "Every build",
  "settings.updates.channel.warning":
    "Builds are published automatically from main. They compile and they pass the checks, but nobody has decided they are good. Take this only if you want the newest work and could reinstall a stable release if one misbehaves.",

  "settings.updates.check.title": "Check now",
  "settings.updates.check.help":
    "Asks github.com straight away, whatever the switch above says. Nothing is downloaded.",
  "settings.updates.check.button": "Check now",
  "settings.updates.check.checking": "Asking github.com…",

  "settings.updates.result.update":
    "Kavka {version} is available. The notice at the top of the window has the Install button.",
  "settings.updates.result.currentStable": "You are on the newest stable release.",
  "settings.updates.result.currentBuild": "You are on the newest build.",
  // The stable endpoint answers 404 because no stable release exists yet.
  // That is a fact about the project, not a failure, and it is said as one.
  //
  // TWO THINGS THIS SENTENCE DELIBERATELY DOES NOT DO. It is only ever shown
  // for an OBSERVED 404 — a rate limit or a GitHub outage is an error and gets
  // the error surface (see `stable_absence` in src-tauri/src/update.rs), never
  // this. And it says nothing about what the reader is running: a local
  // `cargo tauri dev` build carries no build number, so "you are on an
  // automated build" would have been a small false statement on the honesty
  // surface. What this install is, is the About panel's build line's job.
  "settings.updates.result.noStable":
    "No stable release has been published yet — so far there are only automated builds from main. Switch to Every build to follow those.",

  "settings.updates.lastChecked": "Kavka last checked {when}.",
  // Never "last checked" over a request that failed: the whole point of the
  // line is that it can be trusted.
  "settings.updates.lastCheckedFailed":
    "Kavka last tried {when} and couldn't reach github.com.",
  "settings.updates.never": "Kavka hasn't checked yet.",

  "updates.banner.label": "Update notice — Kavka {version}",
  "updates.banner.title": "Kavka {version} is available",
  "updates.banner.body":
    "Nothing has been downloaded. Kavka fetches the installer only when you press Install, and checks it against Kavka's own signing key before anything runs.",
  "updates.banner.bodyBuild":
    "This is an automated build from the newest merge to main, not a stable release — nobody has decided it is good. Nothing has been downloaded; Kavka fetches the installer only when you press Install, and checks it against Kavka's own signing key before anything runs.",
  "updates.banner.willClose":
    "Installing closes Kavka so the installer can replace it. Finish what you are doing first, then open Kavka again when the installer is done.",
  "updates.banner.willRestart":
    "Installing closes Kavka and opens it again once the update is in place. Finish what you are doing first.",
  "updates.banner.notes": "What changed",
  "updates.banner.releasePage": "Release page",
  "updates.banner.install": "Install…",
  "updates.banner.installing": "Downloading…",
  "updates.banner.notNow": "Not now",

  "updates.error.unreachable.title": "Kavka couldn't reach github.com",
  "updates.error.unreachable.detail":
    "Nothing was downloaded and nothing on this machine changed. Check the connection, or whether a proxy or a firewall sits between you and github.com, then try again.",
  "updates.error.title": "The update didn't finish",
  "updates.error.detail":
    "Nothing was installed and nothing on this machine changed. The full text is under Show details, and the release page has installers you can download yourself.",

  "unit.seconds": "{count, plural, one {# second} other {# seconds}}",
  "unit.minutes": "{count, plural, one {# minute} other {# minutes}}",
  "unit.hours": "{count, plural, one {# hour} other {# hours}}",
  "unit.days": "{count, plural, one {# day} other {# days}}",
} as const;

export type MessageKey = keyof typeof en;

/** Every catalog is checked against this shape. Partial is allowed; `en` fills the gaps. */
export type Catalog = Partial<Record<MessageKey, string>>;

export default en;
