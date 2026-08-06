// ─────────────────────────────────────────────────────────────────────────────
// German (Deutsch) — MACHINE TRANSLATION — NATIVE REVIEW WELCOME.
//
// No native speaker has read this file. It was produced from `en.ts` and it is
// shipped honestly rather than quietly: the language picker in the About
// dialog says so next to the name, and `LOCALES` in ../index.ts carries
// `machine: true` for exactly this reason.
//
// If German is your language, the highest-value contribution to Kavka is
// twenty minutes with this file. See docs/I18N.md — you need no build, no
// tooling and no account, and a partial fix is welcome: any key you delete
// falls back to English rather than breaking.
//
// Two things to keep while editing: the {placeholders} (they are values Kavka
// substitutes, and a renamed one silently disappears from the sentence), and
// the plural arms — German is one/other, so `one {# Verbindung} other
// {# Verbindungen}` is the shape.
// ─────────────────────────────────────────────────────────────────────────────

import type { Catalog } from "./en";

const de: Catalog = {
  "common.close": "Schließen",
  "common.cancel": "Abbrechen",
  "common.save": "Speichern",
  "common.connect": "Verbinden",
  "common.tryAgain": "Erneut versuchen",
  "common.remove": "Entfernen",
  "common.dismiss": "Ausblenden",
  "common.showDetails": "Details anzeigen",
  "common.addConnection": "Verbindung hinzufügen",
  "common.support": "Kavka unterstützen ☕",
  "common.readingConnections": "Gespeicherte Verbindungen werden gelesen…",
  "common.linkFailed":
    "Kavka konnte diesen Link nicht an Ihren Browser übergeben. Die Adresse lautet {url} — kopieren Sie sie von hier.",

  "confirm.kicker.destructive": "Zerstörend",
  "confirm.busy": "Kavka arbeitet daran",
  "confirm.type.label": "Zum Bestätigen {name} eingeben",
  "confirm.type.reason": "Geben Sie genau {name} ein, um dies zu bestätigen",


  // ── The cluster switcher and the rail's cluster card ──────────────────
  // The sidebar is gone (DESIGN.md §5.1); its rows are this menu's rows and
  // its identity block is the rail's cluster card. Law 2 (§1) survives the
  // move: the status dot never carries the meaning alone, so `rowMeta` reads
  // "address · state" in EVERY state. Keep both slots.
  "switcher.trigger": "Cluster wechseln",
  "switcher.menuLabel": "Ihre Verbindungen",
  "switcher.empty": "Noch keine Verbindungen gespeichert.",
  "switcher.noEnvironment": "Keine Umgebung",
  "switcher.rowMeta": "{address} · {status}",
  "switcher.status.disconnected": "nicht verbunden",
  "switcher.status.connecting": "verbindet…",
  "switcher.status.connected": "verbunden",
  "switcher.connect": "Verbinden",
  "switcher.connecting": "Verbindet…",
  "switcher.disconnect": "Trennen",
  "switcher.connectTitle": "Mit {name} verbinden",
  "switcher.disconnectTitle": "Verbindung zu {name} trennen",
  "switcher.draftName": "Neue Verbindung",
  "switcher.draftMeta": "noch nicht gespeichert",
  "switcher.protected": "Geschützt",
  "brand.versionTitle": "Kavka Core-Version {version}",
  "card.none": "Kein Cluster",
  "card.state.none": "Noch nichts ausgewählt",
  "card.state.connected":
    "Verbunden · {count, plural, one {# Broker} other {# Broker}}",
  "card.state.connecting": "Verbindet…",
  "card.state.disconnected": "Nicht verbunden",

  // ── App shell: status bar, empty states, global errors ─────────────────
  "app.status.disconnected": "Nicht verbunden",
  "app.status.connecting": "Wird verbunden…",
  "app.status.connected": "Verbunden",
  "app.error.unknownProfile":
    "Diese Verbindung ist nicht mehr auf diesem Rechner. Sie wurde möglicherweise in einem anderen Fenster gelöscht.",
  "app.profilesFailed.title": "Kavka konnte seine Verbindungsdatei nicht lesen",
  "app.profilesFailed.hint":
    "Ihre Verbindungen liegen weiterhin auf der Festplatte — nichts ist verloren gegangen. Kavka speichert sie im Konfigurationsverzeichnis, neben den Einstellungen dieser App.",
  "app.firstRun.title": "Richten Sie Kavka auf einen Broker",
  "app.firstRun.what":
    "Eine Verbindung ist eine gespeicherte Adresse für genau ein Kafka-Cluster — ein Name, ein Broker als Ausgangspunkt und die Art der Anmeldung. Den Rest des Clusters findet Kavka von dort aus.",
  "app.firstRun.example":
    "Ein Bootstrap-Server sieht üblicherweise aus wie {example}. Sie betreiben das Dev-Cluster aus diesem Repository? Dann nehmen Sie {local}.",
  "app.firstRun.footnote":
    "Passwörter gehen in den Schlüsselbund Ihres Betriebssystems. Nichts über Ihre Cluster verlässt diesen Rechner.",
  "app.pick.title": "Wählen Sie eine Verbindung",
  "app.pick.hint":
    "Öffnen Sie oben links den Cluster-Umschalter, um einen auszuwählen, oder fügen Sie eine weitere Verbindung hinzu.",
  "app.readonlyChip": "schreibgeschützt",
  "app.readonlyTitle":
    "Diese Verbindung ist schreibgeschützt. Schalten Sie das in den Einstellungen der Verbindung aus, um zu schreiben oder zu bearbeiten.",
  "app.statusbar.draft": "Neue Verbindung — noch nicht gespeichert",
  "app.statusbar.none": "Keine Verbindung ausgewählt",
  "app.statusbar.commands": "Befehle",
  "app.cmd.search": "In {topic} suchen",
  "app.cmd.search.kw":
    "find filter cel scan query messages grep suchen filtern nachrichten",
  "app.cmd.sql": "{topic} mit SQL abfragen",
  "app.cmd.sql.kw":
    "sql select query aggregate count group datafusion analyse abfrage zählen auswerten",
  "app.cmd.produce": "An {topic} senden",
  "app.cmd.produce.kw":
    "send write publish message record bulk producer senden schreiben veröffentlichen nachricht",
  "app.cmd.produce.confirmContext": "{cluster} · fragt nach einer Bestätigung",

  "palette.label": "Befehle",
  "palette.searchLabel": "Befehle und Cluster durchsuchen",
  "palette.searchPlaceholder": "Befehle und Cluster durchsuchen…",
  "palette.empty":
    "Nichts passt zu „{query}“. Versuchen Sie einen Cluster-Namen, oder leeren Sie das Feld, um alles zu sehen, was Kavka kann.",
  "palette.foot.move": "bewegen",
  "palette.foot.run": "ausführen",
  "palette.foot.close": "schließen",
  "palette.goTo": "Zu {name} wechseln",
  "palette.connectTo": "Mit {name} verbinden",
  "palette.state.connected": "verbunden",
  "palette.state.connecting": "wird verbunden…",
  "palette.protectedCluster": "geschütztes Cluster",
  "palette.profile.kw":
    "connect open switch cluster broker bootstrap verbinden wechseln öffnen",
  "palette.add.context": "Ein Name, ein Broker und die Art der Anmeldung",
  "palette.add.kw":
    "new connection profile cluster create bootstrap broker neu anlegen verbindung",
  "palette.disconnect": "Verbindung trennen",
  "palette.disconnect.kw": "close leave cluster session trennen beenden",
  "palette.disconnect.none": "Zurzeit ist nichts verbunden",
  "palette.disconnect.ambiguous":
    "Wählen Sie zuerst im Cluster-Umschalter den Cluster aus, den Sie trennen möchten",
  "palette.refresh": "Topics neu laden",
  "palette.refresh.kw":
    "reload metadata list topics partitions cluster neu laden aktualisieren",
  "palette.export": "Verbindungen exportieren…",
  "palette.export.context": "Alle Verbindungen auf diesem Rechner, als JSON",
  "palette.export.kw":
    "backup save copy share json profiles sicherung exportieren kopieren",
  "palette.import": "Verbindungen importieren…",
  "palette.import.context":
    "JSON aus einer anderen Kavka-Installation einfügen",
  "palette.import.kw":
    "restore paste load json profiles einfügen laden wiederherstellen",
  "palette.about.context": "Version und Lizenz",
  "palette.about.kw":
    "version licence license agpl source github help lizenz hilfe quelltext",
  "palette.support.context":
    "Kavka ist kostenlos — Spenden halten das so",
  "palette.support.kw":
    "donate coffee sponsor fund open source spenden kaffee unterstützen",

  "about.title": "Über Kavka",
  "about.body":
    "Ein Desktop-Client für Apache Kafka. Kavka läuft vollständig auf diesem Rechner: Passwörter gehen in den Schlüsselbund Ihres Betriebssystems, und nichts über Ihre Cluster verlässt diesen Computer.",
  "about.coreVersion": "Core-Version",
  "about.versionLoading": "Wird gerade gelesen…",
  "about.build": "Build {number}",
  "about.licence": "Lizenz",
  "about.licenceValue": "Freie und quelloffene Software unter AGPL-3.0",
  "about.language": "Sprache",
  "about.language.hint":
    "Kavkas Rahmen und das Urteil, mit dem jeder Cluster-Bildschirm beginnt — die Leiste, die Befehlspalette, diese Dialoge, das Verbindungsformular und der Eröffnungssatz jedes Bildschirms. Die Tabellen und Formulare darunter sind weiterhin auf Englisch.",
  "about.language.machine":
    "{language} wurde maschinell übersetzt und von keinem Muttersprachler geprüft. Korrekturen sind willkommen — docs/I18N.md erklärt, wie.",

  "transfer.title": "Verbindungen",
  "transfer.tablist": "Exportieren oder importieren",
  "transfer.tab.export": "Exportieren",
  "transfer.tab.import": "Importieren",
  "transfer.export.body":
    "Alle Verbindungen auf diesem Rechner, als JSON. Fügen Sie es in eine andere Kavka-Installation ein, um dort dieselben Cluster einzurichten.",
  "transfer.export.promise":
    "Passwörter und Schlüssel verlassen diesen Rechner nie — Exporte enthalten Verweise, keine Geheimnisse.",
  "transfer.export.failed":
    "Kavka konnte seine Verbindungsdatei nicht lesen. Ihre Verbindungen liegen weiterhin auf der Festplatte — nichts ist verloren gegangen.",
  "transfer.export.label": "Ihre Verbindungen, als JSON",
  "transfer.export.copied": "In die Zwischenablage kopiert.",
  "transfer.export.copyManual":
    "Kavka konnte die Zwischenablage nicht erreichen. Der Text ist markiert — drücken Sie {key}, um ihn zu kopieren.",
  "transfer.export.copy": "In die Zwischenablage kopieren",
  "transfer.export.nothingToCopy":
    "Es gibt nichts zu kopieren — Kavka konnte seine Verbindungsdatei nicht lesen",
  "transfer.export.stillReading": "Kavka liest Ihre Verbindungen noch",
  "transfer.import.body":
    "Fügen Sie einen Export aus einer anderen Kavka-Installation ein. Passwörter sind darin nicht enthalten — jede importierte Verbindung fragt beim ersten Verbinden nach ihrem eigenen.",
  "transfer.import.label": "Exportiertes JSON",
  "transfer.import.kbd": "importieren",
  "transfer.import.kbdClose": "schließen",
  "transfer.import.legend": "Wenn eine Verbindung bereits vorhanden ist",
  "transfer.import.skip": "Die auf diesem Rechner behalten",
  "transfer.import.skipHint":
    "Bereits hier gespeicherte Verbindungen bleiben genau so, wie sie sind. Alles Neue aus dem JSON wird trotzdem hinzugefügt.",
  "transfer.import.replace": "Durch die aus dem JSON ersetzen",
  "transfer.import.replaceHint":
    "Die eingefügte Fassung gewinnt — Name, Adresse, Umgebung und Anmeldeverfahren. Passwörter, die bereits in Ihrem Schlüsselbund liegen, bleiben unangetastet.",
  "transfer.import.failed":
    "Kavka konnte das nicht als Export lesen. Prüfen Sie, ob Sie die ganze Datei eingefügt haben, einschließlich der äußeren geschweiften Klammern — der Text, den Kavka bekommen hat, steht unten.",
  "transfer.import.needsJson":
    "Fügen Sie zuerst das JSON aus einem Export ein",
  "transfer.import.busy": "Kavka importiert diese Verbindungen gerade",
  "transfer.import.run": "Verbindungen importieren",
  "transfer.import.running": "Wird importiert…",
  "transfer.report.empty.title": "In diesem JSON waren keine Verbindungen",
  "transfer.report.empty.detail":
    "Prüfen Sie, ob Sie den ganzen Export eingefügt haben, einschließlich der äußeren geschweiften Klammern — Kavka konnte ihn lesen, es war nur nichts hinzuzufügen.",
  "transfer.report.added": "{count, plural, other {# hinzugefügt}}",
  "transfer.report.replaced": "{count, plural, other {# ersetzt}}",
  "transfer.report.skipped":
    "{count, plural, other {# übersprungen — schon auf diesem Rechner}}",
  "transfer.report.envAdded":
    "{count, plural, one {# Umgebung hinzugefügt} other {# Umgebungen hinzugefügt}}",
  "transfer.report.envSkipped":
    "{count, plural, one {# Umgebung bereits definiert} other {# Umgebungen bereits definiert}}",
  "transfer.report.envOnly.title": "Keine neuen Verbindungen — nur Umgebungen",
  "transfer.report.unchanged.title":
    "{count, plural, one {Nichts geändert — # Verbindung war schon hier} other {Nichts geändert — # Verbindungen waren schon hier}}",
  "transfer.report.unchanged.detail":
    "{bits}. Wählen Sie oben „Durch die aus dem JSON ersetzen“, wenn Sie sie überschreiben wollten.",
  "transfer.report.imported.title":
    "{count, plural, one {# Verbindung importiert} other {# Verbindungen importiert}}",
  "transfer.report.imported.detail":
    "{bits}. Passwörter stehen nicht in einem Export — öffnen Sie jede neue Verbindung und tragen Sie ihr Passwort ein, bevor Sie sich verbinden.",

  "editor.new.title": "Verbindung hinzufügen",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "Was auch immer Sie im Cluster-Umschalter wiedererkennen. Nur Kavka sieht es.",
  "editor.env.label": "Umgebung",
  "editor.env.hint.protected":
    "Diese Umgebung ist als geschützt markiert: Die Ledger-Linie trägt ihre Farbe in jeder Tabelle, der Cluster-Umschalter markiert diesen Cluster, ein Warnbalken liegt oben über dem Fenster, und jede zerstörerische Aktion verlangt vorher die Eingabe des Namens. Schalten Sie unten Nur-Lesen ein, sofern Sie nicht wirklich schreiben müssen.",
  "editor.env.hint.other":
    "Kavka färbt jede Ansicht nach Umgebung, damit Sie kein Cluster mit einem anderen verwechseln.",
  "editor.env.manage": "Umgebungen verwalten…",
  "editor.env.hint.unknown":
    "Auf diesem Rechner ist {name} nirgends definiert, deshalb zeigt Kavka die Umgebung neutral grau an und wendet keine Schutzmechanismen an. Legen Sie sie unter „Umgebungen verwalten“ an, um ihr eine Farbe zu geben und zu entscheiden, ob sie geschützt ist.",
  "editor.bootstrap.hint":
    "Ein beliebiger einzelner Broker in Ihrem Cluster — den Rest findet Kavka von dort aus. Einer pro Zeile, oder durch Komma getrennt. Sie betreiben das Dev-Cluster aus diesem Repository? Dann nehmen Sie {local}.",

  "editor.auth.legend": "Anmeldung",
  "editor.auth.kerberos":
    "Diese Verbindung meldet sich mit Kerberos an ({service} als {principal}), was Kavka noch nicht einrichten kann. Beim Speichern bleibt sie genau so erhalten; alle anderen Felder hier funktionieren weiterhin.",
  "editor.auth.label": "Wie prüft dieses Cluster, wer Sie sind?",
  "editor.auth.plaintext": "Gar nicht — jeder darf sich verbinden (PLAINTEXT)",
  "editor.auth.saslPlain": "Benutzername und Passwort — SASL/PLAIN",
  "editor.auth.saslScram": "Benutzername und Passwort — SASL/SCRAM",
  "editor.auth.mtls": "Ein Zertifikat, das dieser Rechner vorlegt — mTLS",
  "editor.auth.mskIam":
    "Die AWS-Anmeldedaten auf diesem Rechner — MSK IAM",
  "editor.auth.oauth":
    "Ein Token von Ihrem Identitätsanbieter — OAuth 2.0 / OIDC",
  "editor.auth.kerberosOption": "Ein Kerberos-Ticket — GSSAPI (noch nicht)",
  "editor.auth.notYet":
    "Kavka kann das noch nicht einrichten. Eine Verbindung, die es bereits nutzt, funktioniert weiter und bleibt beim Speichern genau so erhalten.",
  "editor.auth.hint":
    "Verwaltetes Kafka verlangt meist SASL/SCRAM mit eingeschaltetem TLS. Ein lokaler Broker verlangt meist gar nichts. Kerberos ist das einzige Verfahren, das Kavka noch nicht einrichten kann.",
  "editor.mechanism.label": "SCRAM-Verfahren",
  "editor.mechanism.hint":
    "Wenn der Broker eines ablehnt, sagt er Ihnen, welches er will.",
  "editor.username.label": "Benutzername",
  "editor.password.label": "Passwort",
  "editor.password.placeholder": "Passwort",
  "editor.secret.unchanged": "••••••••  (unverändert)",
  "editor.password.hint":
    "Geht in den Schlüsselbund Ihres Betriebssystems — nie in die Verbindungsdatei, und nie von diesem Rechner herunter.",
  "editor.tls.label": "Verbindung verschlüsseln (TLS)",
  "editor.tls.hint":
    "Verwaltetes Kafka braucht das fast immer eingeschaltet. Wenn der Broker antwortet, der Handshake aber scheitert, ist das die erste Stellschraube.",

  "editor.tls.hintPlainCleartext":
    "Ist dies aus und SASL/PLAIN gewählt, geht Ihr Passwort unverschlüsselt an den Broker — alles auf dem Netzwerkweg kann es mitlesen. Verwaltetes Kafka braucht das fast immer eingeschaltet.",
  "editor.tls.hintScramCleartext":
    "Ist dies aus, sendet SCRAM zwar nicht das Passwort selbst, aber alles Gesendete lässt sich mitschneiden und offline angreifen — und der übrige Verkehr ist ebenfalls unverschlüsselt. Verwaltetes Kafka braucht das fast immer eingeschaltet.",

  "editor.mtls.hint":
    "Kavka liest PEM-Dateien genau so, wie sie sind — es gibt keinen JKS- oder PKCS#12-Keystore, den Sie vorher umwandeln müssten.",
  "editor.caPath.label": "CA-Zertifikat",
  "editor.caPath.hint":
    "Pfad zur CA-.pem-Datei — leer lassen, um den Vertrauensspeicher des Systems zu verwenden.",
  "editor.clientCert.label": "Client-Zertifikat",
  "editor.clientCert.hint":
    "Pfad zu dem Zertifikat, das dieser Rechner dem Broker zeigt — leer lassen, wenn der Broker keines verlangt.",
  "editor.clientKey.label": "Privater Client-Schlüssel",
  "editor.clientKey.hint":
    "Fügen Sie den Schlüssel selbst ein, nicht einen Pfad dorthin. Er geht in den Schlüsselbund Ihres Betriebssystems — nie in die Verbindungsdatei, und nie von diesem Rechner herunter.",
  "editor.clientKey.storedHint":
    "Lassen Sie es leer, um den gespeicherten Schlüssel zu behalten; wird der Zertifikatspfad oben geleert, wird er entfernt.",

  "editor.aws.hint":
    "Kavka signiert jede Anfrage mit den AWS-Anmeldedaten, die bereits auf diesem Rechner liegen. Die Bootstrap-Server oben müssen der IAM-Endpunkt dieses Clusters sein — die {host}-Hosts aus der MSK-Konsole, üblicherweise auf Port 9098.",
  "editor.region.label": "Region",
  "editor.region.hint":
    "Die AWS-Region, in der das Cluster läuft. Sie muss zu den Bootstrap-Hosts passen, sonst wird die Signatur nicht akzeptiert.",
  "editor.awsProfile.label": "Name des AWS-Profils",
  "editor.awsProfile.hint":
    "Ein benanntes Profil aus {config}. Leer lassen, um die Standard-Anmeldekette zu verwenden — Umgebungsvariablen, dann {dir}, dann SSO.",

  "editor.oauth.hint":
    "Kavka fordert bei Ihrem Identitätsanbieter ein Token über den Client-Credentials-Grant an und legt es dem Broker als SASL/OAUTHBEARER vor.",
  "editor.tokenEndpoint.label": "Token-Endpunkt",
  "editor.tokenEndpoint.hint":
    "Die URL, die das Token ausstellt, nicht die Anmeldeseite, die ein Browser aufrufen würde.",
  "editor.clientId.label": "Client-ID",
  "editor.clientId.hint":
    "Die Anwendung, die Ihr Identitätsanbieter für Kafka registriert hat — nicht Ihr eigenes Benutzerkonto.",
  "editor.clientSecret.label": "Client-Secret",
  "editor.clientSecret.placeholder": "Client-Secret",
  "editor.clientSecret.hint":
    "Geht in den Schlüsselbund Ihres Betriebssystems — nie in die Verbindungsdatei, und nie von diesem Rechner herunter.",

  "editor.sr.legend": "Schema Registry (optional)",
  "editor.sr.hint":
    "Wenn die Nachrichten dieses Clusters Avro, Protobuf oder JSON Schema sind, liest Kavka das Schema von hier, um sie zu dekodieren — und zeigt Subject, Version und ID neben jeder Nachricht. Ohne sie werden diese Payloads als Rohbytes angezeigt.",
  "editor.srUrl.label": "Adresse der Registry",
  "editor.srUrl.hint":
    "Die vollständige URL, einschließlich Schema. Confluent, Apicurio und Glue sprechen hier dieselbe Lese-API. Leer lassen, wenn dieses Cluster keine Registry hat.",
  "editor.srUsername.label": "Benutzername der Registry",
  "editor.srUsername.hint":
    "Nur wenn die Registry einen verlangt. Verwaltete Registries tun das meist; eine Registry im eigenen Netz meist nicht.",
  "editor.srPassword.label": "Passwort der Registry",
  "editor.srPassword.storedHint":
    "Lassen Sie es leer, um das gespeicherte zu behalten; wird die Adresse oben geleert, wird es entfernt.",

  "editor.connect.legend": "Kafka-Connect-Cluster (optional)",
  "editor.connect.hint":
    "Kafka Connect betreibt Source- und Sink-Konnektoren und antwortet über einen eigenen REST-Port statt über die Broker — Kavka muss also erfahren, wo die Worker sind. Fügen Sie einen pro Worker-Gruppe hinzu; über den Namen wählen Sie im Connect-Tab zwischen ihnen.",
  "editor.connect.unnamed": "Cluster {number}",
  "editor.connect.removeLabel": "{name} entfernen",
  "editor.connect.unnamedLong": "Connect-Cluster {number}",
  "editor.connect.remove":
    "Dieses Connect-Cluster aus der Verbindung entfernen",
  "editor.connect.name.label": "Name",
  "editor.connect.name.placeholder": "orders connect",
  "editor.connect.name.hint":
    "Was immer Sie wiedererkennen. Ein späteres Umbenennen behält das gespeicherte Passwort.",
  "editor.connect.url.label": "Adresse der Worker",
  "editor.connect.url.hint":
    "Der REST-Endpunkt eines beliebigen Workers der Gruppe — sie antworten alle für das ganze Cluster. Üblicherweise Port 8083, und nicht derselbe Host oder Port wie die Broker.",
  "editor.connect.username.hint":
    "Nur wenn die Worker hinter Basic Auth stehen. Die meisten tun das nicht.",
  "editor.connect.password.storedHint":
    "Lassen Sie es leer, um das gespeicherte zu behalten; wird dieses Cluster entfernt, wird es mit entfernt.",
  "editor.connect.add": "Ein Connect-Cluster hinzufügen",

  "editor.monitoring.legend": "Überwachung (optional)",
  "editor.monitoring.hint":
    "Kafkas Broker liefern Durchsatz-, Speicher- und Replikationszahlen nicht über das Kafka-Protokoll — sie veröffentlichen sie als JMX, und fast alle stellen einen Prometheus-Exporter davor. Richten Sie Kavka auf den Exporter, und der Überwachungs-Tab füllt sich. Der Lag-Verlauf braucht davon nichts: den liest Kavka selbst von den Brokern.",
  "editor.metricsUrl.label": "Adresse der Metriken",
  "editor.metricsUrl.hint":
    "Die vollständige URL, einschließlich Pfad. Wenn Sie die Broker selbst betreiben, ist das meist der Java-Agent {agent} auf einem von ihnen ({flag}). Ein Prometheus-Server, der diese Broker bereits abfragt, geht auch — geben Sie Kavka dann dessen Adresse. Leer lassen, wenn dieses Cluster keinen Exporter hat.",
  "editor.metricsUsername.label": "Benutzername für Metriken",
  "editor.metricsUsername.hint":
    "Nur wenn der Endpunkt hinter Basic Auth steht. Ein jmx_exporter meist nicht; ein gemeinsam genutztes Prometheus meist schon.",
  "editor.metricsPassword.label": "Passwort für Metriken",
  "editor.metricsPassword.storedHint":
    "Lassen Sie es leer, um das gespeicherte zu behalten; wird die Adresse oben geleert, wird es entfernt.",
  "editor.sampler.label": "Lag-Messung alle",
  "editor.sampler.hint":
    "Sekunden. Kafka merkt sich Lag nicht, also nimmt Kavka in diesem Takt eigene Messwerte auf und hält {days} davon in einer Datei auf diesem Rechner vor. {warning} Die Untergrenze liegt bei {floor}; die Voreinstellung ist {default} und kostet eine kleine Anfrage pro Gruppe und Messung.",
  "editor.sampler.warning":
    "Messwerte entstehen nur, solange diese Verbindung besteht — während Kavka geschlossen oder dieses Cluster getrennt ist, wird nichts erfasst, und eine Lücke im Diagramm bedeutet genau das.",

  "editor.readonly.label": "Schreibgeschützte Verbindung",
  "editor.readonly.hint":
    "Kavka zeigt weiterhin alles an, sendet über diese Verbindung aber keine Nachrichten, ändert keine Topics und schreibt keine Offsets fest.",

  "editor.busy.connecting": "Warten Sie, bis der Verbindungsversuch fertig ist",
  "editor.busy.saving": "Kavka speichert diese Verbindung",
  "editor.delete": "Verbindung löschen",
  "editor.delete.confirm":
    "{name} von diesem Rechner entfernen? Das Cluster selbst bleibt unangetastet.",
  "editor.kbd.connect": "verbinden",
  "editor.kbd.cancel": "abbrechen",
  "editor.kbd.undo": "Änderungen verwerfen",

  "editor.err.name":
    "Geben Sie dieser Verbindung einen Namen, damit Sie sie im Cluster-Umschalter wiederfinden.",
  "editor.err.bootstrap":
    "Fügen Sie mindestens einen Broker hinzu, als host:port — z. B. broker-1:9092",
  "editor.err.srUrl":
    "Nehmen Sie die vollständige URL, beginnend mit http:// oder https:// — z. B. http://localhost:8081",
  "editor.err.srUserNoUrl":
    "Ergänzen Sie die Adresse der Registry, oder leeren Sie den Benutzernamen — eine Anmeldung ohne Ziel lässt sich nicht speichern.",
  "editor.err.metricsUrl":
    "Nehmen Sie die vollständige URL, beginnend mit http:// oder https:// — z. B. http://broker-1.internal:7071/metrics",
  "editor.err.metricsUserNoUrl":
    "Ergänzen Sie die Adresse der Metriken, oder leeren Sie den Benutzernamen — eine Anmeldung ohne Ziel lässt sich nicht speichern.",
  "editor.err.sampler":
    "Messen Sie höchstens alle {seconds, plural, one {# Sekunde} other {# Sekunden}}. Schneller fragt die Broker öfter nach Offsets, als diese sich ändern.",
  "editor.err.connectName":
    "Geben Sie diesem Connect-Cluster einen Namen — jede Aktion, die Kavka sendet, benennt das Cluster, an das sie geht.",
  "editor.err.connectDuplicate":
    "Zwei Connect-Cluster an einer Verbindung können sich keinen Namen teilen — Kavka speichert ihre Passwörter darunter.",
  "editor.err.connectUrlMissing":
    "Ergänzen Sie die REST-Adresse der Worker — z. B. http://connect-1.internal:8083",
  "editor.err.connectUrl":
    "Nehmen Sie die vollständige URL, beginnend mit http:// oder https:// — z. B. http://connect-1.internal:8083",
  "editor.err.username":
    "Dieses Anmeldeverfahren braucht den Benutzernamen, unter dem der Broker Sie kennt.",
  "editor.err.password": "Dieses Anmeldeverfahren braucht ein Passwort.",
  "editor.err.clientKey":
    "Fügen Sie den privaten Schlüssel ein, der zu diesem Zertifikat gehört — Kavka braucht beide Hälften.",
  "editor.err.clientCert":
    "Ergänzen Sie den Pfad zu dem Zertifikat, zu dem dieser Schlüssel gehört — Kavka braucht beide Hälften.",
  "editor.err.region":
    "Nennen Sie die Region, in der das Cluster läuft — z. B. eu-west-1",
  "editor.err.tokenEndpoint":
    "Ergänzen Sie die URL, unter der Ihr Identitätsanbieter Token ausstellt — z. B. https://login.example.com/oauth2/token",
  "editor.err.tokenEndpointUrl":
    "Nehmen Sie die vollständige URL, beginnend mit https:// — z. B. https://login.example.com/oauth2/token",
  "editor.err.clientId":
    "Ergänzen Sie die Client-ID, die Ihr Identitätsanbieter für diese Anwendung ausgestellt hat.",
  "editor.err.clientSecret":
    "Dieses Anmeldeverfahren braucht das Secret, das zu dieser Client-ID gehört.",

  "editor.perch.screen": "Verbindung",
  "editor.perch.new":
    "Noch nichts gespeichert — Kavka hat keinen Broker kontaktiert, also wurde auf diesem Bildschirm nichts geprüft.",
  "editor.perch.saved":
    "Gespeichert, aber nicht verbunden. Kavka hat noch nicht mit {name} gesprochen, daher wurde keines dieser Details gegen den Cluster geprüft.",
  "editor.perch.connected":
    "Mit {name} verbunden. Kavka liest noch die Übersicht des Clusters.",
  "editor.perch.caveat.protected":
    "{name} ist als geschützt markiert: Jede zerstörende Aktion auf diesem Cluster verlangt zuerst die Eingabe des Namens.",
  "editor.perch.caveat.unknown":
    "Auf diesem Rechner definiert nichts {name}, daher gelten für diese Verbindung keine Schutzmechanismen.",
  "editor.perch.caveat.readonly":
    "Nur-Lesen ist aktiv — Kavka durchsucht diesen Cluster, schreibt aber nie hinein.",
  "editor.fold.set": "Eingerichtet",
  "editor.fold.notSet": "Nicht eingerichtet",
  "editor.fold.connectCount":
    "{count, plural, one {# Cluster} other {# Cluster}}",

  "editor.head.unsaved": "Noch nicht gespeichert",
  "editor.step.name": "Wie sollen wir ihn nennen?",
  "editor.step.env": "Um welche Umgebung handelt es sich?",
  "editor.step.env.why":
    "Die Umgebung bestimmt die Farbe, die Sie für diesen Cluster überall in der App sehen — und ob Kavka ihn als geschützt behandelt.",
  "editor.step.bootstrap": "Wo ist er erreichbar?",
  "editor.step.bootstrap.why":
    "Ein {term} genügt. Kavka fragt ihn nach dem Rest des Clusters.",
  "editor.bootstrap.term": "Bootstrap-Server",
  "editor.step.sr": "Gibt es eine Schema Registry?",
  "editor.step.optional": "(optional)",
  "editor.step.readonly": "Soll Kavka hier etwas ändern dürfen?",
  "editor.step.readonly.why":
    "Nur-Lesen ist der sicherste Weg, sich den Cluster einer anderen Person anzusehen.",
  "editor.saveConnection": "Verbindung speichern",
  "editor.state.connecting": "Verbindung wird aufgebaut…",
  "editor.state.connected": "Gerade verbunden.",
  "editor.state.failed":
    "Der letzte Versuch ist fehlgeschlagen — der Grund steht oben.",
  "editor.state.draft":
    "Noch nichts gespeichert, also wurde noch nichts versucht.",
  "editor.state.idle":
    "Nicht verbunden. Kavka führt kein Protokoll darüber, wann es das zuletzt war.",

  // ── Der Verbindungen-Bildschirm — Kopf und Liste gespeicherter Cluster ───
  "connections.list.title": "Gespeicherte Cluster",
  "connections.list.empty":
    "Noch keine Verbindungen gespeichert. Die, die Sie gerade schreiben, wird die erste sein.",
  "connections.list.foot":
    "{protected} bedeutet, dass Kavka Sie den Namen des Clusters eintippen lässt, bevor etwas Zerstörerisches passiert. Farbe ist Identität; geschützt ist die Schutzvorrichtung.",
  "connections.list.footProtected": "Geschützt",
  "connections.list.footSession":
    "„Verbunden“ und „nicht verbunden“ beschreiben nur diese Sitzung — Kavka kontaktiert nie einen Cluster, mit dem es nicht verbunden ist, und kann daher nicht sagen, ob einer läuft.",
  "connections.sub":
    "{count, plural, =0 {Noch keine gespeicherten Cluster. Legen Sie den ersten an.} one {Ein gespeicherter Cluster. Wählen Sie ihn zum Bearbeiten oder legen Sie einen weiteren an.} other {# gespeicherte Cluster. Wählen Sie einen zum Bearbeiten oder legen Sie einen neuen an.}}",
  "connections.sub.unknown":
    "Wählen Sie einen Cluster zum Bearbeiten oder legen Sie einen neuen an.",
  "connections.manageEnvironments": "Umgebungen verwalten",


  // ── Umgebungen ──────────────────────────────────────────────────────────
  "env.color.green": "Grün",
  "env.color.amber": "Bernstein",
  "env.color.red": "Rot",
  "env.color.blue": "Blau",
  "env.color.violet": "Violett",
  "env.color.cyan": "Türkis",
  "env.color.slate": "Schiefer",

  "env.mgr.title": "Umgebungen",
  "env.mgr.intro":
    "Benennen Sie die Umgebungen, die Ihre Organisation tatsächlich betreibt. Die Farbe unterscheidet sie auf einen Blick; „geschützt“ ist der Schutzmechanismus.",
  "env.mgr.hint.title": "Was „geschützt“ tatsächlich bewirkt",
  "env.mgr.hint.detail":
    "Kavka verlangt vor jeder zerstörerischen Aktion die Eingabe des Clusternamens, zeigt die Umgebung im Fenstertitel an und verweigert zerstörerische Befehle aus der CLI und dem MCP-Server ohne ausdrückliches Flag. Zwei davon geschehen in anderen Prozessen — deshalb stehen sie hier. Die Farbe dient nur der Wiedererkennung; jeder Chip nennt zusätzlich seinen Namen.",
  "env.mgr.failed": "Das hat nicht geklappt",
  "env.mgr.working": "Kavka arbeitet daran",
  "env.mgr.namesAreYours":
    "Die Namen gehören Ihnen. Legen Sie so viele an, wie Ihre Organisation tatsächlich hat — Kavka geht nicht davon aus, dass es nur drei gibt.",
  "env.mgr.add": "Umgebung hinzufügen",
  "env.mgr.edit": "Bearbeiten",

  "env.mgr.row.protected": "geschützt",
  "env.mgr.row.unprotected": "nicht geschützt",
  "env.mgr.row.used":
    "{count, plural, =0 {keine Verbindungen} one {# Verbindung} other {# Verbindungen}}",

  "env.mgr.name.label": "Name",
  "env.mgr.name.hint":
    "Wie auch immer Ihr Team sie nennt — dev, QA, UAT, Produktion. Wird genau so angezeigt, wie Sie sie eintippen, und nie übersetzt.",
  "env.mgr.name.taken": "Eine Umgebung mit diesem Namen gibt es bereits.",
  "env.mgr.name.required": "Geben Sie der Umgebung zuerst einen Namen",

  "env.mgr.color.label": "Farbe",
  "env.mgr.color.hint":
    "Nur zur Identität. Die Farbe färbt die Ledger-Linie und den Chip; sie entscheidet nie darüber, was Kavka Ihnen erlaubt.",

  "env.mgr.protected.label": "Diese Umgebung als geschützt behandeln",
  "env.mgr.protected.hint":
    "Kavka wechselt zum Warn-Untergrund, verlangt vor jeder zerstörenden Aktion das Eintippen des Topic- oder Gruppennamens, markiert das Fenster und verweigert Schreibvorgänge über die Kommandozeile und über KI-Assistenten, sofern es ihnen nicht ausdrücklich anders gesagt wurde.",
  "env.mgr.unprotect.prompt": "Tippen Sie {name}, um den Schutz zu entfernen",
  "env.mgr.unprotect.hint":
    "Jede Verbindung in {name} verliert ihre Schutzmechanismen: keine eingetippten Bestätigungen mehr, und Kommandozeile und KI-Assistenten verweigern Schreibvorgänge nicht länger.",

  "env.mgr.delete.title": "{name} entfernen?",
  "env.mgr.delete.unused":
    "Keine Verbindung nutzt {name}, sonst ändert sich nichts.",
  "env.mgr.delete.used":
    "{count, plural, one {# Verbindung nutzt} other {# Verbindungen nutzen}} {name}. Wählen Sie, wohin sie gehen — Kavka verschiebt sie vor dem Entfernen.",
  "env.mgr.delete.moveTo": "Diese Verbindungen verschieben nach",
  "env.mgr.delete.moveHint": "Diese Verbindungen werden verschoben: {names}.",
  "env.mgr.delete.confirm": "Umgebung entfernen",
  "env.mgr.delete.needTarget":
    "Wählen Sie eine Umgebung, in die diese Verbindungen verschoben werden.",
  "env.mgr.delete.last":
    "Das ist die einzige verbliebene Umgebung — legen Sie zuerst eine weitere an",

  // ── Cluster-Navigation (Jackdaw) ────────────────────────────────────────
  // The two app-level groups. They render with NOTHING connected, which is
  // the whole reason Settings is a rail item: on first launch there is no
  // cluster, and the theme and the font size are what a new user needs first.
  // The read-only readout states its answer in BOTH directions — a guardrail
  // that is silent in its dangerous state is not a guardrail.
  "rail.navLabel": "Ansichten",
  "rail.group.setup": "Einrichten",
  "rail.group.application": "Anwendung",
  "rail.item.connections": "Verbindungen",
  "rail.item.settings": "Einstellungen",
  "rail.readonly.label": "Nur-Lesen: {state}",
  "rail.readonly.on": "ein",
  "rail.readonly.off": "aus",
  "rail.readonly.on.why": "Kavka wird hier nichts schreiben oder löschen.",
  "rail.readonly.off.why": "Kavka kann hier schreiben und löschen.",

  "rail.group.cluster": "Cluster",
  "rail.group.observe": "Beobachten",
  "rail.group.safety": "Sicherheit",
  "rail.group.integrations": "Integrationen",
  "rail.item.overview": "Start",
  "rail.item.topics": "Topics",
  "rail.item.groups": "Consumer-Gruppen",
  "rail.item.brokers": "Broker",
  "rail.item.monitoring": "Überwachung",
  "rail.item.alerts": "Warnungen",
  "rail.item.streams": "Streams",
  "rail.item.acls": "ACLs",
  "rail.item.masking": "Maskierung",
  "rail.item.connect": "Connect",
  "rail.firing": "aktiv",
  "rail.firingTitle":
    "{count, plural, one {# Warnregel ist gerade aktiv} other {# Warnregeln sind gerade aktiv}}",

  // ── Der Bildschirmkopf ──────────────────────────────────────────────────
  "stage.overview.title": "Cluster-Übersicht",
  "stage.overview.sub":
    "Woraus dieser Cluster besteht — aus den Metadaten, mit denen er beim Verbinden geantwortet hat.",
  "stage.overview.refresh": "Aktualisieren",
  "stage.overview.refresh.title":
    "Liest diesen Bildschirm erneut — die Gruppen, das Warnprotokoll, das Quorum und die Broker-Einstellungen. Die Kacheln und die Broker-Liste kamen mit der Verbindung und ändern sich erst beim erneuten Verbinden.",
  "stage.topics.sub":
    "Jedes Topic, das dieser Cluster gemeldet hat, samt dem, was Kavka darüber sagen kann und was nicht.",
  "stage.groups.sub":
    "Wer liest, wie weit zurück er ist und wann das gemessen wurde.",
  "stage.brokers.sub":
    "Die Maschinen in diesem Cluster und die Einstellungen, mit denen jede läuft.",
  "stage.monitoring.sub":
    "Gezeichnet nur aus Messungen, die Kavka bei geöffnetem Fenster genommen hat — für jede Stunde ohne Kavka bleibt eine Lücke.",
  "stage.alerts.sub":
    "Regeln, die Kavka für dich prüft, solange es läuft, und alles, was ausgelöst hat.",
  "stage.streams.sub":
    "Kafka-Streams-Anwendungen, gelesen aus den Consumer-Gruppen dahinter.",
  "stage.acls.sub":
    "Wer hier was darf — genau so, wie der Cluster es selbst meldet.",
  "stage.masking.sub":
    "Kavkas eigene Regeln zum Verbergen von Werten auf dem Bildschirm. Nichts davon ändert den Cluster oder das, was er speichert.",
  "stage.connect.sub":
    "Kafka-Connect-Worker, die diese Verbindung kennt, und die Connectors, die darauf laufen.",

  // ── Cluster-Start (Jackdaw) — Kacheln, Triage-Liste, die beiden Tabellen ─
  "home.clusterId": "Cluster-ID",
  "home.clusterId.absent": "Dieser Cluster hat keine ID gemeldet.",
  "home.tile.reading": "wird noch gelesen",
  "home.tile.brokers.sub": "so wie der Cluster sie beim Verbinden genannt hat",
  "home.tile.brokers.none":
    "der Cluster hat keine gemeldet — die Verbindung steht, aber die Metadaten kamen leer zurück",
  "home.tile.topics.sub":
    "insgesamt {partitions, plural, one {# Partition} other {# Partitionen}}",
  "home.tile.partitions": "Partitionen",
  "home.tile.partitions.sub":
    "über alle Topics hinweg — Kopien auf anderen Brokern werden nicht doppelt gezählt",
  "home.tile.groups.allStable": "alle stabil",
  "home.tile.groups.unsettled":
    "{count, plural, one {# gerade nicht stabil} other {# gerade nicht stabil}}",
  "home.tile.groups.idle":
    "{count, plural, one {bei # ist niemand verbunden} other {bei # ist niemand verbunden}}",
  "home.tile.groups.none": "derzeit liest nichts diesen Cluster",
  "home.tile.groups.unread":
    "Kavka konnte die Gruppenliste nicht lesen und kann es daher nicht sagen.",
  "home.attention.title": "Sollte angesehen werden",
  "home.attention.provenance":
    "Kavka listet nur auf, was sich aus dieser Momentaufnahme belegen lässt.",
  "home.attention.reading":
    "Das Alarmprotokoll und die Gruppenliste dieser Verbindung werden gelesen…",
  "home.attention.unread":
    "Kavka konnte das Alarmprotokoll dieser Verbindung nicht lesen und kann daher nicht sagen, ob etwas ausgelöst hat. Eine leere Liste würde hier nicht bedeuten, dass alles in Ordnung ist.",
  "home.attention.clear":
    "In dieser Momentaufnahme muss nichts angesehen werden.",
  "home.attention.clear.sub":
    "Keine von Ihnen gesetzte Alarmregel löst aus, und jede von Kafka genannte Consumer-Gruppe ist stabil. Das ist keine Zusage zu allem, was Kavka nicht gemessen hat.",
  "home.attention.partial":
    "Keine von Ihnen gesetzte Alarmregel löst aus. Kavka konnte die Gruppenliste dieser Verbindung nicht lesen und kann daher nicht sagen, ob etwas liest — eine leere Liste ist hier keine Entwarnung.",
  "home.attention.groupsUnread":
    "Kavka konnte die Gruppenliste dieser Verbindung nicht lesen — nichts in dieser Liste betrifft daher, wer liest.",
  "home.attention.alert.noDetail":
    "Kavka hat dieses Auslösen ohne die zugehörigen Zahlen aufgezeichnet.",
  "home.attention.group.title": "{group} liest derzeit nicht",
  "home.attention.group.sub":
    "Kafka meldet diese Gruppe als {state}, mit {members, plural, one {# Mitglied} other {# Mitgliedern}}. Eine Gruppe, die nicht stabil ist, konsumiert nicht weiter, bis das Rebalancing abgeschlossen ist.",
  "home.attention.open.monitoring": "In Überwachung öffnen",
  "home.attention.open.alerts": "Alarme öffnen",
  "home.attention.open.groups": "Consumer-Gruppen öffnen",
  "home.attention.where": "unter {screen}",
  "home.attention.foot":
    "Erstellt aus den neuesten {limit} Einträgen im Alarmprotokoll dieser Verbindung und der Gruppenliste, die dieser Bildschirm gelesen hat. Kavka wertet hier nichts anderes aus — ein Problem, das keine Regel beobachtet, erscheint in dieser Liste nicht.",
  "home.brokers.caption": "Broker in diesem Cluster",
  "home.brokers.none":
    "Dieser Cluster hat keine Broker gemeldet. Das bedeutet normalerweise, dass die Verbindung steht, die Metadaten aber leer zurückkamen — versuchen Sie, neu zu verbinden.",
  "home.brokers.foot":
    "Das sind die Broker, die dieser Cluster in den Metadaten beim Verbinden genannt hat. Kavka hat sie seitdem nicht einzeln kontaktiert — ein Broker, der vor einer Minute ausgefallen ist, steht hier also noch.",
  "home.brokers.details": "Details",
  "home.brokers.details.note":
    "Protokollversion, Log-Verzeichnisse, Replikation und Aufbewahrung, gelesen von Broker {id}",
  "home.brokers.details.reading": "Konfiguration von Broker {id} wird gelesen…",
  "home.brokers.details.unread":
    "Kavka konnte die Konfiguration von Broker {id} nicht lesen. Der Broker-Bildschirm fragt dieselben Einstellungen ab und zeigt den Fehler dahinter.",
  "home.brokers.details.caveat":
    "Nur von Broker {id} gelesen. Ein anderer Broker in diesem Cluster kann anders konfiguriert sein, und ein Cluster mit uneinigen Brokern ist eine häufige und stille Fehlkonfiguration.",
  "home.brokers.fact.protocol": "Protokollversion",
  "home.brokers.fact.logDirs": "Log-Verzeichnisse",
  "home.brokers.fact.replication": "Standard-Replikation",
  "home.brokers.fact.autoCreate": "Topics automatisch anlegen",
  "home.brokers.fact.retention": "Standard-Aufbewahrung (Stunden)",
  "home.brokers.fact.absent": "auf diesem Broker nicht gesetzt",

  // ── Die Sitzstange (Jackdaw) ────────────────────────────────────────────
  "perch.label": "{screen} — was Kavka dazu sagen kann",
  "perch.kicker": "{screen} · {state}",
  "perch.state.ok": "Sieht gesund aus",
  "perch.state.watch": "Einen Blick wert",
  "perch.state.problem": "Etwas stimmt nicht",
  "perch.state.unknown": "Noch unklar",
  "perch.state.checking": "Wird noch geprüft",
  "perch.checking":
    "Wird noch geprüft — Kavka sagt Bescheid, sobald der Cluster antwortet.",
  "perch.hide": "Ausblenden",
  "perch.more": "Ganzen Hinweis anzeigen",
  "perch.show": "Hinweis für diesen Bildschirm anzeigen",
  "perch.overview.counts":
    "Verbunden mit {brokers, plural, one {# Broker} other {# Brokern}}, mit {topics, plural, one {# Topic} other {# Topics}} über {partitions, plural, one {# Partition} other {# Partitionen}}.",
  "perch.overview.firing":
    "{count, plural, one {# Warnregel ist} other {# Warnregeln sind}} auf diesem Cluster gerade aktiv. {counts}",
  "perch.overview.snapshot":
    "Diese Zahlen stammen vom Verbindungsaufbau und folgen dem Cluster nicht — für neue Werte bitte neu verbinden.",
  "perch.overview.noBrokers":
    "Der Cluster hat geantwortet, aber überhaupt keinen Broker genannt.",
  "perch.overview.noBrokers.next":
    "Meist heißt das, dass ein Load Balancer statt Kafka selbst geantwortet hat oder die Metadaten leer zurückkamen. Trennen, neu verbinden und die Bootstrap-Adresse prüfen.",
  "perch.screen.messages": "Nachrichten",
  "perch.screen.search": "Suche",
  "perch.screen.sql": "SQL",
  "perch.screen.schemas": "Schemas",
  "perch.topics.unreadable":
    "Kavka hat keine Liste der Topics dieses Clusters.",
  "perch.topics.unreadable.next":
    "Die Verbindung kann stehen, während dem Konto Describe auf dem Cluster fehlt. Aktualisieren fragt erneut nach.",
  "perch.topics.empty":
    "Dieser Cluster hat überhaupt keine Topics — es wurde noch keines darauf angelegt.",
  "perch.topics.internalOnly":
    "Alles auf diesem Cluster ist ein Kafka-eigenes internes Topic. Schalten Sie Interne anzeigen ein, um sie zu sehen.",
  "perch.topics.counts":
    "{count, plural, one {# Topic auf diesem Cluster} other {# Topics auf diesem Cluster}}.",
  "perch.topics.countsHidden":
    "{count, plural, one {# Topic angezeigt} other {# Topics angezeigt}}.",
  "perch.topics.hiddenNote":
    "{count, plural, one {# weiteres ist ein Kafka-eigenes internes Topic und ist ausgeblendet} other {# weitere sind Kafka-eigene interne Topics und sind ausgeblendet}}.",
  "perch.topics.snapshot":
    "Diese Liste wurde beim Öffnen des Bildschirms gelesen und folgt dem Cluster nicht — Aktualisieren liest sie erneut.",
  "perch.topics.readOnly":
    "Diese Verbindung ist nur lesend, hier kann also nichts ein Topic anlegen, ändern oder löschen.",
  "perch.topic.unreadable":
    "Kavka hat keine Partitionsliste für {topic} und kann daher nicht sagen, was darin ist.",
  "perch.topic.unreadable.next":
    "Das Topic wurde vielleicht gelöscht, oder dem Konto fehlt Describe darauf.",
  "perch.topic.underReplicated":
    "{count, plural, one {# Partition hier fehlt eine Kopie} other {# Partitionen hier fehlen Kopien}} — Kafka hält weniger Replikate, als dieses Topic verlangt.",
  "perch.topic.unpreferred":
    "{count, plural, one {# Partition wird} other {# Partitionen werden}} von einem anderen Broker geführt als dem ersten in der Replikatliste. Nach einem Neustart ist das normal, und Bevorzugte Leader wählen setzt sie zurück.",
  "perch.topic.healthy":
    "{count, plural, one {# Partition} other {# Partitionen}}, jede Kopie synchron.",
  "perch.topic.records": "Nach den Offsets rund {records} Nachrichten.",
  "perch.topic.approx":
    "Diese Nachrichtenzahl ist der Abstand zwischen dem frühesten und dem spätesten Offset jeder Partition und zählt daher auch Datensätze mit, die Aufbewahrung oder Verdichtung längst entfernt haben.",
  "perch.messages.waiting":
    "Noch nichts gelesen. Wählen Sie oben, wo gelesen werden soll, und drücken Sie Abrufen.",
  "perch.messages.range":
    "{count, plural, one {# Nachricht} other {# Nachrichten}} aus dem angeforderten Bereich.",
  "perch.messages.none": "Nichts im angeforderten Bereich.",
  "perch.messages.topicEmpty": "{topic} enthält noch keine Nachrichten.",
  "perch.messages.live":
    "{topic} wird live beobachtet — seit dem Start des Mitlesens {count, plural, one {ist # Nachricht} other {sind # Nachrichten}} eingetroffen.",
  "perch.messages.liveQuiet":
    "{topic} wird live beobachtet. Seit mindestens dreißig Sekunden wurde nichts dorthin geschrieben.",
  "perch.messages.notWhole":
    "Das ist der angeforderte Ausschnitt, nicht das ganze Topic — {topic} enthält rund {total} Nachrichten.",
  "perch.messages.dropped":
    "{count, plural, one {# Nachricht traf} other {# Nachrichten trafen}} schneller ein, als dieses Fenster sie aufnehmen konnte, und die Sitzung hat sie verworfen, statt zurückzufallen — die Zeilen auf dem Bildschirm sind also nicht alles, was das Mitlesen gesehen hat.",
  "perch.messages.trimmed":
    "Kavka behält die letzten {cap} Live-Zeilen; alles Ältere hat den Puffer bereits verlassen.",
  "perch.messages.masked":
    "Maskierungsregeln sind eingeschaltet, daher sind manche Werte auf dem Bildschirm nicht die Werte im Topic. Kopien und Exporte enthalten die Ersetzungen.",
  "perch.search.waiting":
    "Noch nichts durchsucht. Legen Sie den Bereich fest, sagen Sie, wonach Sie suchen, und drücken Sie Suchen.",
  "perch.search.running":
    "{topic} wird durchsucht — bisher {count, plural, one {# Treffer} other {# Treffer}}.",
  "perch.search.running.note":
    "Unvollständig. Diese Zahlen ändern sich, bis der Durchlauf fertig ist.",
  "perch.search.matches":
    "{count, plural, one {# Treffer} other {# Treffer}} in den {scanned} Datensätzen, die dieser Durchlauf gelesen hat.",
  "perch.search.none":
    "In den {scanned} Datensätzen, die dieser Durchlauf gelesen hat, passte nichts.",
  "perch.search.stopped":
    "Sie haben diesen Durchlauf nach {scanned} Datensätzen gestoppt, er beantwortet also einen Teil des Bereichs und nicht den ganzen.",
  "perch.search.capped":
    "{matched} Datensätze passten, Kavka hat {kept} behalten. Sortieren, Exportieren oder Zählen des Bildschirminhalts beantwortet Fragen zu diesen, nicht zu allen Treffern.",
  "perch.search.unevaluated":
    "{count, plural, one {# Datensatz konnte} other {# Datensätze konnten}} nicht gegen Ihren Ausdruck gelesen werden. Sie wurden übersprungen, nicht als nicht passend bewertet.",
  "perch.search.masked":
    "Maskierungsregeln sind eingeschaltet, daher sind manche Werte auf dem Bildschirm — und in allem, was Sie exportieren — nicht die Werte im Topic.",
  "perch.sql.waiting":
    "Es wurde noch keine Abfrage ausgeführt. Der Bereich oben entscheidet, welche Datensätze die Abfrage sehen kann.",
  "perch.sql.running": "Läuft — bisher {scanned} Datensätze gelesen.",
  "perch.sql.running.note":
    "Unvollständig. Nichts darunter ist die endgültige Antwort, bis der Durchlauf fertig ist.",
  "perch.sql.rows":
    "{count, plural, one {# Zeile} other {# Zeilen}} aus den {scanned} Datensätzen, die dieser Durchlauf gelesen hat.",
  "perch.sql.none":
    "Die Abfrage lieferte aus den {scanned} gelesenen Datensätzen keine Zeilen.",
  "perch.sql.scope":
    "Das beantwortet Fragen zu den gelesenen Datensätzen, nicht zum ganzen Topic — ein anderer Bereich ist eine andere Antwort.",
  "perch.sql.capped":
    "Der Durchlauf endete bei seiner Obergrenze von {cap} Datensätzen; was die Abfrage gezählt oder summiert hat, gilt daher nur für diesen Ausschnitt.",
  "perch.sql.stopped":
    "Sie haben diesen Durchlauf nach {scanned} Datensätzen gestoppt, die Antwort deckt also einen Teil des Bereichs ab.",
  "perch.sql.masked":
    "Während dieser Abfrage galten Maskierungsregeln, daher sind manche Werte hier nicht die Werte im Topic.",
  "perch.schemas.noRegistry":
    "Diese Verbindung hat keine Schema Registry, daher gibt es hier nichts, woraus Schemas gelesen werden könnten.",
  "perch.schemas.noRegistry.next":
    "Eine Registry ist ein eigener Dienst mit eigener Adresse. Fügen Sie sie unter Schema Registry in den Einstellungen dieser Verbindung hinzu.",
  "perch.schemas.missing": "Die Registry kennt kein Subject namens {subject}.",
  "perch.schemas.missing.next":
    "Kavka hat unter der Topic-Name-Strategie gesucht, die die meisten Producer verwenden. Ein Producer mit einer anderen Strategie registriert unter einem anderen Namen.",
  "perch.schemas.versions":
    "{count, plural, one {# Version dieses Subjects ist registriert} other {# Versionen dieses Subjects sind registriert}}.",
  "perch.schemas.level": "Neue Versionen werden als {level} geprüft.",
  "perch.schemas.levelUnknown":
    "Kavka konnte die eigene Kompatibilitätseinstellung dieses Subjects nicht lesen und kann daher nicht sicher sagen, welche Stufe die Registry anwenden wird.",
  "perch.groups.none":
    "Noch keine Consumer-Gruppen auf diesem Cluster — es hat noch nichts daraus gelesen.",
  "perch.groups.counts":
    "{count, plural, one {# Consumer-Gruppe liest} other {# Consumer-Gruppen lesen}} aus diesem Cluster.",
  "perch.groups.rebalancing":
    "{unstable, plural, one {# Gruppe verteilt} other {# Gruppen verteilen}} gerade neu, ihre Partitionen werden also herumgereicht und der Verbrauch pausiert währenddessen. {counts}",
  "perch.groups.unread":
    "Kavka konnte die Consumer-Gruppen dieses Clusters nicht lesen und kann daher nichts über sie sagen. Bis das gelingt, ist nichts auf diesem Bildschirm eine Aussage über den Cluster.",
  "perch.groups.caveat":
    "Das ist die Liste, wie Kavka sie zuletzt gelesen hat. Der Zustand einer Gruppe ändert sich bei jedem Rebalance — drücken Sie Aktualisieren für eine neue Aufnahme.",
  "perch.group.caughtUp":
    "{group} ist auf jeder Partition, die Kavka sieht, auf dem aktuellen Stand.",
  "perch.group.behind":
    "{group} liegt über {partitions, plural, one {# Partition} other {# Partitionen}} rund {lag} Nachrichten zurück. Am schlimmsten ist {topic} Partition {partition} mit {worst}.",
  "perch.group.noOffsets":
    "{group} hat nie einen Offset bestätigt, es gibt also keine Position zu melden. Vielleicht hat sie nur geschrieben, oder sie wurde angelegt und hat nie etwas gelesen.",
  "perch.group.noMembers":
    "Derzeit ist nichts mit {group} verbunden, sie liest also nichts. Ihre bestätigten Offsets sind weiterhin da, und eine startende Anwendung macht dort weiter.",
  "perch.group.caveat":
    "Kavka hat diese Offsets einmal gelesen, beim Öffnen dieses Bildschirms. Sie folgen der Gruppe nicht — öffnen Sie sie erneut für eine frische Messung.",
  "perch.brokers.counts":
    "{count, plural, one {# Broker in diesem Cluster} other {# Broker in diesem Cluster}}. Öffnen Sie einen, um jede Einstellung zu sehen, mit der er läuft.",
  "perch.brokers.none":
    "Der Cluster hat geantwortet, aber überhaupt keinen Broker genannt.",
  "perch.brokers.noneNext":
    "Meist heißt das, dass die Metadaten leer zurückkamen oder dass Sie einen Load Balancer statt Kafka selbst erreicht haben. Trennen, neu verbinden und die Bootstrap-Adresse prüfen.",
  "perch.brokers.caveat":
    "Die Broker-Liste kam beim Verbinden zurück und folgt dem Cluster nicht — für eine neue Aufnahme bitte neu verbinden.",
  "perch.broker.noOverrides":
    "Broker {broker} weicht in nichts von Kafkas Standardwerten ab — jede Einstellung, die er hat, berechnet Kafka selbst.",
  "perch.broker.overrides":
    "Broker {broker} überschreibt {count, plural, one {# Einstellung} other {# Einstellungen}}; die übrigen {rest} sind das, was er gerade berechnet.",
  "perch.broker.unread":
    "Kavka konnte die Einstellungen dieses Brokers nicht lesen und kann daher nicht sagen, womit er läuft. Das Konto braucht dafür in der Regel DescribeConfigs auf dem Cluster.",
  "perch.broker.caveat":
    "Nur die mit + markierten Zeilen sind auf diesem Broker gesetzt. Ein berechneter Standardwert kann sich mit dem Cluster ändern, und Kafka meldet manche Einstellungen für Clients als schreibgeschützt — diese behalten ihre Schaltfläche Bearbeiten, deaktiviert, mit dem Grund beim Überfahren.",
  "perch.connect.noClusters":
    "Diese Verbindung hat keine Kafka-Connect-Worker, hier gibt es also nichts zu steuern.",
  "perch.connect.noClustersNext":
    "Connect läuft als eigener Satz Worker mit eigener REST-Adresse, meist auf Port 8083. Fügen Sie einen unter Kafka-Connect-Cluster in den Einstellungen dieser Verbindung hinzu.",
  "perch.connect.empty":
    "Noch keine Konnektoren auf {cluster}, hier wird also nichts nach Kafka hinein oder heraus bewegt.",
  "perch.connect.allRunning":
    "{count, plural, one {# Konnektor auf {cluster}} other {# Konnektoren auf {cluster}}}, und jede Task läuft.",
  "perch.connect.failed":
    "Auf {cluster} {failed, plural, one {ist # Task} other {sind # Tasks}} fehlgeschlagen. Eine fehlgeschlagene Task bewegt überhaupt keine Datensätze, bis sie neu gestartet wird — öffnen Sie den Konnektor und lesen Sie zuerst den Trace des Workers.",
  "perch.connect.paused":
    "Auf {cluster} {paused, plural, one {ist # Konnektor} other {sind # Konnektoren}} pausiert, es bewegt sich also nichts hindurch. Ihre Konfigurationen und ihre bestätigten Offsets bleiben erhalten.",
  "perch.connect.unread":
    "Kavka konnte die Connect-Worker nicht erreichen und kann daher nicht sagen, was läuft. Das ist eine andere Adresse als die der Broker, und vielleicht ist nur sie ausgefallen.",
  "perch.connect.caveat":
    "Diese Zustände kamen von den Workern, als Kavka zuletzt gefragt hat. Connect ändert sie von selbst — drücken Sie Aktualisieren für eine neue Messung.",
  "perch.connector.running":
    "{name} läuft: {running} von {total} Tasks bewegen Datensätze.",
  "perch.connector.failed":
    "{name} hat {failed, plural, one {# fehlgeschlagene Task} other {# fehlgeschlagene Tasks}} und bewegt nichts. Lesen Sie erst, warum sie gestoppt ist — ein Neustart bei unveränderter Ursache schlägt einfach wieder fehl.",
  "perch.connector.paused":
    "{name} ist pausiert und bewegt daher keine Datensätze. Konfiguration und bestätigte Offsets bleiben erhalten, und das Fortsetzen macht dort weiter.",
  "perch.connector.noTasks":
    "{name} hat überhaupt keine Tasks, es bewegt sich also nichts. Die Worker erzeugen Tasks aus der Konfiguration eines Konnektors; eine unbrauchbare Konfiguration lässt ihn ohne zurück.",
  "perch.connector.caveat":
    "Das ist eine Messung, aufgenommen, als Kavka die Worker zuletzt gefragt hat. Task-Zustände ändern sich von selbst.",
  "perch.monitoring.origin":
    "Kafka merkt sich keinen Rückstand — ein Broker kann nur sagen, wo eine Gruppe gerade steht. Alles auf diesem Bildschirm ist Kavkas eigene Aufzeichnung, entstanden, während diese Verbindung stand.",
  "perch.monitoring.unread":
    "Kavka konnte seine eigene Rückstandsaufzeichnung für diese Verbindung nicht lesen und kann daher nicht sagen, wie weit etwas zurückliegt — oder ob es überhaupt Messungen gibt.",
  "perch.monitoring.noHistory":
    "Kavka hat für diese Verbindung noch keine Rückstandsmessungen. Die ersten erscheinen innerhalb von {interval} nach dem Verbinden, und eine Gruppe taucht hier erst auf, wenn sie mindestens einmal einen Offset bestätigt hat.",
  "perch.monitoring.noWindow":
    "Kavka hat für {group} in diesem Zeitfenster keine Messungen. Wählen Sie ein längeres, oder prüfen Sie den Sampler unten.",
  "perch.monitoring.caughtUp":
    "{group} war bei der letzten Messung auf dem aktuellen Stand — nichts wartete auf das Gelesenwerden.",
  "perch.monitoring.rising":
    "{group} liegt über {partitions, plural, one {# Partition} other {# Partitionen}} rund {lag} Nachrichten zurück, Tendenz steigend. Am schlimmsten ist {topic} Partition {partition} mit einem Höchstwert von {peak}.",
  "perch.monitoring.steady":
    "{group} liegt über {partitions, plural, one {# Partition} other {# Partitionen}} rund {lag} Nachrichten zurück und ist seit Beginn dieses Zeitfensters stabil.",
  "perch.monitoring.falling":
    "{group} liegt über {partitions, plural, one {# Partition} other {# Partitionen}} rund {lag} Nachrichten zurück, Tendenz fallend.",
  "perch.monitoring.caveat.sampled":
    "Ein Punkt in diesen Diagrammen ist die schlechteste Messung seines Abschnitts, nie ein Mittelwert, und eine Lücke in einer Linie ist eine Zeit, in der Kavka nicht lief — kein Ausfall.",
  "perch.monitoring.caveat.stale":
    "Der Sampler hinkt hinterher: Seine letzte Messung war {ago}, mehr als drei Intervalle her. Alles darunter ist älter, als es aussieht.",
  "perch.monitoring.caveat.stopped":
    "Für diese Verbindung wird derzeit nichts aufgezeichnet, dieses Urteil ist also nur so neu wie die letzte Messung, die Kavka nehmen konnte.",
  "perch.monitoring.caveat.unknownSampler":
    "Kavka kann nicht sagen, was sein Sampler gerade tut, und daher nicht zusichern, dass diese Messungen aktuell sind.",
  "perch.alerts.none":
    "Keine Regeln auf diesem Cluster, Kavka beobachtet hier also nichts.",
  "perch.alerts.quiet":
    "{count, plural, one {# Regel beobachtet} other {# Regeln beobachten}} diesen Cluster, und keine davon ist ausgelöst.",
  "perch.alerts.firingOne": "{rule} ist seit {time} ausgelöst. {detail}",
  "perch.alerts.firingMany":
    "{count, plural, one {# Regel ist} other {# Regeln sind}} auf diesem Cluster gerade ausgelöst. Die älteste ist {rule}, seit {time}.",
  "perch.alerts.unread":
    "Kavka konnte die Warnregeln dieser Verbindung nicht lesen und kann daher nicht sagen, was beobachtet wird — oder ob überhaupt etwas beobachtet wird.",
  "perch.alerts.unreadHistory":
    "Kavka konnte das Warnprotokoll dieser Verbindung nicht lesen und kann daher nicht sagen, ob gerade etwas ausgelöst ist — oder ob jemals etwas ausgelöst wurde.",
  "perch.alerts.caveat.desktop":
    "Kavka muss laufen, um etwas zu bemerken. Schließen Sie das Fenster, und nichts wird beobachtet — das ist eine Desktop-Anwendung, kein Dienst.",
  "perch.alerts.caveat.silent":
    "Kein Kanal ist eingeschaltet, ein Auslösen erreicht also nur dieses Fenster und das Protokoll unten. Wenn Kavka nicht vor Ihnen steht, erreicht Sie nichts.",
  "perch.masking.none":
    "Keine Maskierungsregeln auf dieser Verbindung, alles, was Kavka Ihnen zeigt, ist also genau das, was der Producer gesendet hat.",
  "perch.masking.inForce":
    "{count, plural, one {# Maskierungsregel ist} other {# Maskierungsregeln sind}} in Kraft, passender Text wird also ersetzt, bevor er dieses Fenster überhaupt erreicht.",
  "perch.masking.off":
    "Es {count, plural, one {gibt # Maskierungsregel} other {gibt # Maskierungsregeln}}, und keine davon ist eingeschaltet, es wird also nichts auf dem Bildschirm verborgen.",
  "perch.masking.unread":
    "Kavka konnte die Maskierungsregeln dieser Verbindung nicht lesen und kann daher nicht zusichern, dass das Angezeigte wortgetreu ist.",
  "perch.masking.caveat":
    "Eine Regel, die Sie jetzt einschalten, gilt für den nächsten Abruf, den nächsten Mitlese-Block, die nächste Suche oder Abfrage — nie für Zeilen, die bereits auf dem Bildschirm stehen.",
  "perch.masking.caveat.sawMasked":
    "In dieser Sitzung wurde bereits etwas auf dem Bildschirm maskiert, mindestens eine Nutzlast hier ist also nicht das, was der Producer gesendet hat.",
  "perch.streams.noGroups":
    "Dieser Cluster hat noch keine Consumer-Gruppen, es gibt also nichts, woraus sich eine Topologie ableiten ließe.",
  "perch.streams.pick":
    "Wählen Sie oben eine Anwendung, und Kavka ermittelt, was sie liest, was sie schreibt und was sie dazwischen behält.",
  "perch.streams.notStreams":
    "{group} sieht nicht nach einer Kafka-Streams-Anwendung aus, es gibt also keine Topologie zu zeichnen. Dass eine gewöhnliche Consumer-Gruppe keine hat, ist kein Fehler.",
  "perch.streams.inferred":
    "Dieses Bild von {app} ist eine Vermutung: {nodes, plural, one {# Knoten} other {# Knoten}} und {edges, plural, one {# Verbindung} other {# Verbindungen}}, abgeleitet aus Topic-Namen.",
  "perch.streams.unread":
    "Kavka konnte für {group} keine Topologie ermitteln und hat daher nichts zu zeigen. Die Meldung unten ist das, was der Cluster gesagt hat.",
  "perch.streams.caveat":
    "Kafka veröffentlicht eine Streams-Topologie nirgends, wo ein Client sie lesen könnte. Nichts hiervon stammt aus der Anwendung selbst, ein Prozessor, der kein Topic hinterlässt, taucht daher überhaupt nicht auf.",
  "perch.acls.noAuthorizer":
    "Dieser Cluster hat keinen Authorizer, es gibt also keine Zugriffsregeln aufzulisten, und jede Anfrage entscheidet der Standardwert der Broker.",
  "perch.acls.noAuthorizerNext":
    "Das ist eine Broker-Einstellung (authorizer.class.name), keine fehlende Berechtigung — Kafka weist die Anfrage rundweg ab, statt mit einer leeren Liste zu antworten.",
  "perch.acls.none":
    "Dieser Cluster hat einen Authorizer, aber noch keine Zugriffsregeln; was mit einer Anfrage geschieht, entscheidet also allein der Standardwert der Broker.",
  "perch.acls.allAllow":
    "{count, plural, one {# Zugriffsregel} other {# Zugriffsregeln}} auf diesem Cluster, und jede davon erlaubt.",
  "perch.acls.someDeny":
    "{count, plural, one {# Zugriffsregel} other {# Zugriffsregeln}} auf diesem Cluster. {denies, plural, one {# davon verbietet} other {# davon verbieten}}, und ein Verbot schlägt jede Erlaubnis, die auf dieselbe Anfrage passt.",
  "perch.acls.filtered":
    "Es {count, plural, one {wird # Regel} other {werden # Regeln}} angezeigt, die zu diesem Filter passen.",
  "perch.acls.unread":
    "Kavka konnte die Zugriffsregeln dieses Clusters nicht lesen und kann daher nicht sagen, wer was darf. Zum Auflisten braucht das Konto in der Regel Describe auf dem Cluster.",
  "perch.acls.caveat.filtered":
    "Ein Filter ist aktiv, das zählt also die dazu passenden Regeln — nicht die Regeln auf dem Cluster.",
  "perch.acls.caveat.removing":
    "Ein Verbot zu entfernen erweitert den Zugriff, statt ihn einzuschränken. Kavka sagt das noch einmal, bevor es eines entfernt.",

  // ── Cluster-Ansichten (Jackdaw) ─────────────────────────────────────────
  "topics.partitions.detail": "Replikat-Details anzeigen",
  "topics.partitions.detailTitle":
    "Ergänzt die Replikatliste, die Liste der synchronen Replikate sowie den frühesten und spätesten Offset jeder Partition. Der Zustand bleibt so oder so sichtbar.",
  "acls.filter.summary": "Diese Regeln filtern",
  "acls.filter.note": "nach Ressourcentyp, Ressourcenname und Principal",
  "acls.filter.active": "ein Filter ist aktiv",
  "alerts.state.firing": "Ausgelöst",
  "alerts.since": "seit {time}",
  "alerts.details.summary": "Details",
  "alerts.details.note": "was Kavka genau vergleicht, und wie oft",
  "alerts.facts.kind": "Art",
  "alerts.facts.waitsFor": "Wartet",
  "alerts.facts.noWait": "nicht — sie löst aus, sobald die Bedingung zutrifft",
  "alerts.facts.checked": "Geprüft",
  "alerts.facts.checkedValue":
    "bei jeder Messung, die Kavka nimmt, und nur solange Kavka geöffnet ist",
  "alerts.facts.since": "Ausgelöst seit",
  "alerts.history.started": "{rule} — begonnen",
  "alerts.history.cleared": "{rule} — beendet",
  "alerts.history.lasted": "Beendet um {time}, nach {duration}.",
  "alerts.history.stillFiring": "Weiterhin ausgelöst, bisher {duration}.",
  "alerts.history.gap":
    "Dieses Protokoll deckt nur die Zeit ab, in der Kavka geöffnet war. Eine Lücke darin ist eine Zeit, in der niemand hingesehen hat, und Kavka rät nicht, was darin geschah.",
  "alerts.toast.viewGroup": "Gruppe {group} ansehen",
  "alerts.toast.viewAlerts": "Warnung ansehen",
  "alerts.preview.label": "Die Benachrichtigung für {rule}",
  "alerts.preview.sent":
    "Kavka hat Ihr Betriebssystem um {time} gebeten, dies anzuzeigen. Gebeten, nicht angezeigt — das Benachrichtigungscenter kann aus sein oder die Berechtigung entzogen, und Kavka erfährt davon nichts. Sie trägt diese Worte und nichts sonst: Schaltflächen gibt es darauf keine. Eine pro Auslösung, und eine weitere, wenn sie sich auflöst.",
  "alerts.preview.off":
    "Desktop-Benachrichtigungen sind für diese Verbindung aus, außerhalb dieses Fensters wurde also nichts angezeigt. So hätte es gelautet — der Name der Regel und die Zahlen, die sie ausgelöst haben, und sonst nichts.",
  "monitoring.tile.lagNow": "Rückstand bei der letzten Messung",
  "monitoring.tile.lagNowSub":
    "Nachrichten, die bei Kavkas letzter Messung auf das Gelesenwerden warteten",
  "monitoring.tile.peak": "Höchstwert in diesem Zeitfenster",
  "monitoring.tile.peakSub":
    "die schlechteste Einzelmessung, die Kavka genommen hat, nie ein Mittelwert",
  "monitoring.tile.trend": "Tendenz",
  "monitoring.tile.trendSub": "gegenüber dem Beginn dieses Zeitfensters",
  "monitoring.tile.partitionsSub":
    "mit mindestens einer Messung in diesem Zeitfenster",

  // ── Panel-Fußnoten — was die Tabelle darüber nicht sagen kann ────────────
  "topics.list.foot":
    "Nur die Form. Das sind die Metadaten des Clusters selbst, gelesen beim Öffnen dieses Bildschirms: Sie sagen, wie jedes Topic aufgebaut ist, nicht wie viel darin steckt, ob etwas daraus liest oder ob es gesund ist. Öffnen Sie ein Topic für seine Partitionen, seine Zahlen und seine Consumer.",
  "topic.partitions.foot":
    "„Nachrichten“ ist der neueste Offset minus dem ältesten, den die Broker für diese Partition noch vorhalten. Was Retention oder Compaction entfernt hat, steckt nicht darin, und auf einem komprimierten Topic zählt es Offsets statt der Datensätze, die Sie zurücklesen würden — es ist also, was diese Partition Ihnen noch zeigen kann, nie, was sie je empfangen hat.",
  "topic.config.foot":
    "Einmal gelesen, beim Öffnen dieses Bildschirms. Eine Zeile ohne {plus} ist das, worauf die Broker in diesem Moment zurückgefallen sind, und kann sich unter diesem Topic ändern, ohne dass sich hier etwas ändert; einen Wert, den Kafka als vertraulich kennzeichnet, hält es vor jedem Client zurück — der Strich heißt also, dass der Broker es nicht sagt, nicht, dass nichts gesetzt ist.",
  "schemas.versions.foot":
    "Das sind die Versionen, die die Registry unter {subject} führt. Die Benennung von Subjects ist eine Konvention der Producer-Seite und nichts, was das Topic festhält — eine kurze Liste, oder gar keine, ist also kein Beleg dafür, dass nichts mit einem Schema nach {topic} schreibt.",
  "alerts.rules.foot":
    "„Ruhig“ heißt, dass nichts die Regel ausgelöst hat, nicht, dass Kavka nachgesehen und die Zahl in Ordnung gefunden hat — eine Regel, deren Messwert nicht verfügbar ist, ist ebenfalls ruhig. Der Zustand stammt aus dem Protokoll unten und ist damit nur so vollständig wie dieses Protokoll.",
  "alerts.channels.foot":
    "Kavka fragt jeden davon einmal pro Auslösung und versucht es nie erneut. Es erfährt nicht, ob Ihr Betriebssystem die Benachrichtigung tatsächlich angezeigt hat, und ein Webhook, der ablehnt, landet in Kavkas Protokoll statt hier — „ein“ heißt also, dass Kavka fragen wird, nicht, dass jemand erreicht wurde.",
  "groups.list.foot":
    "Mitgliederzahlen und Zustände stammen aus dem Moment, in dem Kavka gefragt hat. Eine Gruppe, die gerade rebalanciert, verteilt ihre Partitionen um, während Sie das lesen — ihre Zahl ist also bereits veraltet. Drücken Sie Aktualisieren für eine neue.",
  "group.members.foot":
    "Das sind die Mitglieder, die verbunden waren, als Kavka gefragt hat. Die Partitionszahlen sind die Zuordnung dieses Augenblicks, und ein Rebalance zeichnet sie neu, ohne dass sich auf diesem Bildschirm etwas ändert.",
  "group.lag.foot":
    "Lag ist die Spalte „Ende“ minus der Spalte „Committet“, und beide wurden im selben Aufruf gelesen, stimmen also miteinander überein. ∅ heißt, dass die Gruppe für diese Partition noch nie einen Offset committet hat, was nicht dasselbe ist wie ein Lag von null. Eine Gruppe, die selten committet, liest sich als im Rückstand bei Arbeit, die sie längst erledigt hat.",
  "brokers.list.foot":
    "Das ist die Broker-Liste, mit der Kafka geantwortet hat, als diese Verbindung hergestellt wurde. Ein Broker, der seither hinzugekommen oder gegangen ist, taucht hier erst nach einem erneuten Verbinden auf.",
  "broker.config.foot":
    "Das ist die Antwort EINES Brokers. Kafka hält die meisten Einstellungen pro Broker, ein anderer Broker in diesem Cluster kann also mit einem anderen Wert für denselben Namen laufen, und auf diesem Bildschirm wäre davon nichts zu sehen.",
  "monitoring.foot.lag":
    "Lag ist der neueste Offset der Partition minus dem committeten Offset der Gruppe, und beide stammen aus derselben Messung, stimmen also miteinander überein. Eine Gruppe, die selten committet, wird als im Rückstand bei bereits erledigter Arbeit gezeichnet, und nichts hier kann das von einer Gruppe unterscheiden, die wirklich hinterherhängt.",
  "monitoring.foot.health":
    "Beide Messwerte kommen vom Metrik-Endpunkt und nicht aus den eigenen Antworten der Broker an Kavka, sie sind also nur so frisch wie der Exporter. Es sind clusterweite Summen: keiner von beiden kann Ihnen sagen, welche Partition.",
  "monitoring.foot.throughput":
    "Das sind die Zähler des Exporters, nur für diese Verbindung im Speicher gehalten — sie beginnen bei jedem Öffnen wieder von vorn. Eine flache Linie und ein Exporter, der stillschweigend aufgehört hat zu antworten, sehen hier gleich aus; genau dafür ist die Sampler-Zeile darüber da.",
  "monitoring.foot.noEndpoint":
    "Das ist eine Tatsache über die Verbindung, die Kavka bekommen hat, nicht über den Cluster. Die Broker veröffentlichen womöglich sehr wohl JMX; Kavka wurde nur nicht gesagt, wo es das findet.",
  "monitoring.foot.noSeries":
    "Kavka ordnet die Metriknamen zu, die es kennt, und ignoriert den Rest — ein Messwert, der unter einem ihm unbekannten Namen veröffentlicht wird, fehlt hier also, statt falsch zu sein. Kavka erfindet nie einen Wert, um die Lücke zu füllen.",
  "streams.topology.foot":
    "Kavka kann nur Topics zeichnen, die diese Verbindung auflisten darf. Ein Repartition- oder Changelog-Topic, das das Konto nicht beschreiben kann, fehlt im Bild — und ein fehlendes Kästchen sieht genauso aus wie eine Anwendung, die nie eines hatte.",
  "acls.foot.authorizer":
    "Das ist die Liste, die der Authorizer des Clusters führt. Ein Cluster ohne Authorizer erlaubt alles und hat keine Regeln aufzulisten — was hier genauso aussieht wie ein Cluster, für den niemand welche geschrieben hat.",
  "masking.rules.foot":
    "Eine Regel greift auf den Text, den Kavka gleich auf den Bildschirm bringt. Ein Wert, der über Felder verteilt, kodiert oder anders geschrieben ist, greift schlicht nicht, und nichts hier meldet einen Beinahe-Treffer — der einzige Beweis, dass eine Regel wirkt, ist, sie wirken zu sehen.",
  "connect.connectors.foot":
    "Connect meldet den Zustand eines Konnektors getrennt von dem seiner Tasks, ein Konnektor kann also RUNNING sagen, während jede Task darunter fehlgeschlagen ist. Die Task-Zahlen in jeder Zeile sind der Messwert, dem zu trauen ist.",
  "connect.tasks.foot":
    "Neu starten bittet den Worker, die Task neu zu starten; wann, entscheidet der Worker. Diese Tabelle ändert sich nur, wenn Kavka die Worker erneut liest.",
  "shareGroups.foot":
    "Zustände und Mitgliederzahlen sind die Sicht des Koordinators in dem Moment, in dem Kavka gefragt hat. ∅ in der Spalte für den Start-Offset heißt, dass der Broker für diese Partition nichts gemeldet hat — eine Lücke in der Antwort, keine Null.",

  // ── Einstellungen (Jackdaw) ─────────────────────────────────────────────
  "settings.title": "Einstellungen",
  "settings.navLabel": "Bereiche der Einstellungen",
  "settings.perch":
    "Alles hier wirkt sofort und wird auf diesem Rechner gespeichert. Kavka zeigt gerade das Design {theme}.",
  "settings.section.appearance": "Darstellung",
  "settings.section.appearance.sub":
    "Wie Kavka auf diesem Rechner aussieht. Nichts davon verändert einen Cluster.",
  "settings.section.language": "Sprache",
  "settings.section.language.sub":
    "Kavkas eigene Texte — die Leiste, die Palette, die Formulare und der Einleitungssatz jedes Bildschirms. Die Tabellen darunter sind weiterhin auf Englisch.",
  "settings.section.about": "Über",
  "settings.section.about.sub":
    "Um welchen Build es sich handelt, unter welcher Lizenz er steht und welche zwei Dateien er über sich selbst schreiben kann.",

  "settings.theme.title": "Design",
  "settings.theme.help":
    "System folgt dem Betriebssystem und wechselt mit, während Kavka geöffnet ist.",
  "settings.theme.contrast":
    "Beide Themes werden gegen dieselbe Kontrast-Untergrenze geprüft: 4,5:1 für alles, was Sie lesen, 3:1 für die Kante von allem, was Sie anklicken können. Nichts wird abgedunkelt, damit es ruhiger wirkt.",
  "settings.theme.system": "System",
  "settings.theme.light": "Hell",
  "settings.theme.dark": "Dunkel",

  "settings.accent.title": "Akzentfarbe",
  "settings.accent.note":
    "Akzent: {name}. Wird für das verwendet, was Sie gleich anklicken, und für die ausgewählte Zeile — nie für Status, sodass eine Änderung keine Warnung verbergen kann.",
  "settings.accent.brass": "Messing",
  "settings.accent.moss": "Moos",
  "settings.accent.sky": "Himmel",
  "settings.accent.plum": "Pflaume",

  "settings.density.title": "Dichte",
  "settings.density.help":
    "Komfortabel gibt jeder Zeile Luft. Kompakt zeigt rund ein Drittel mehr Zeilen — die Zeilenhöhe, mit der Kavka ausgeliefert wurde.",
  "settings.density.comfortable": "Komfortabel",
  "settings.density.compact": "Kompakt",

  "settings.font.title": "Schriftgröße",
  "settings.font.help":
    "Skaliert alle Größen gemeinsam, damit auch in der größten Stufe nichts überlappt.",
  "settings.font.s": "Klein",
  "settings.font.m": "Mittel",
  "settings.font.l": "Groß",

  "settings.motion.title": "Bewegung",
  "settings.motion.help":
    "System folgt der Einstellung „Bewegung reduzieren“ des Betriebssystems. Reduziert schaltet zusätzlich jede Animation in Kavka ab.",
  "settings.motion.system": "System",
  "settings.motion.reduce": "Reduziert",

  "settings.env.title": "Umgebungsfarben",
  "settings.env.help":
    "{count, plural, one {# Umgebung ist} other {# Umgebungen sind}} eingerichtet. Farbe ist Identität, geschützt ist die Schutzvorrichtung — hier entscheiden Sie also auch, bei welchen Kavka vorsichtig sein soll. Anders als alles andere auf diesem Bildschirm reisen diese mit einer exportierten Verbindung mit.",
  "settings.env.manage": "Umgebungen verwalten",

  "settings.perch.title": "Der Hinweis auf jedem Bildschirm",
  "settings.perch.help":
    "Der warme Hinweis oben auf jedem Bildschirm, der sagt, was Kavka dort mitteilen kann. „Eine Zeile“ behält das Urteil und lässt die Einschränkung weg; „Ausgeblendet“ schaltet ihn auf Bildschirmen ohne Befund ab. Ein Bildschirm, der noch lädt oder dessen Abfrage fehlgeschlagen ist, zeigt in jeder Einstellung den ganzen Hinweis.",
  "settings.perch.full": "Vollständig",
  "settings.perch.line": "Eine Zeile",
  "settings.perch.hidden": "Ausgeblendet",

  "settings.sample.title": "So sieht das aus",
  "settings.sample.sub":
    "eine lebende Probe der Teile, die Sie gerade geändert haben",
  "settings.sample.note":
    "Diese drei Zeilen sind erfunden, damit Sie sehen können, was Dichte und Textgröße bewirken, bevor Sie es herausfinden müssen. Nichts hiervon stammt aus einem Cluster.",
  "settings.sample.caption":
    "Eine Probe aus drei erfundenen Nachrichtenzeilen, damit Änderungen am Erscheinungsbild sofort sichtbar sind.",
  "settings.sample.primary": "Eine primäre Schaltfläche",
  "settings.sample.normal": "Eine normale",
  "settings.sample.chip.ok": "Gesund",
  "settings.sample.chip.warn": "Fällt zurück",
  "settings.sample.focus":
    "Drücken Sie {key} durch diese hindurch, um den Fokusrahmen in diesem Theme zu sehen.",

  "settings.language.title": "Sprache",
  "settings.language.help":
    "Umfasst Kavkas Rahmen und das Urteil, mit dem jeder Cluster-Bildschirm beginnt — die Leiste, die Palette, dieses Panel, das Verbindungsformular und den Eröffnungssatz jedes Bildschirms. Die Tabellen und Formulare unter diesen Sätzen sind weiterhin auf Englisch.",
  "settings.language.machine":
    "Dieser Katalog stammt aus einer Maschine und wurde von keinem Muttersprachler geprüft. Korrekturen sind willkommen.",

  "settings.about.title": "Version, Lizenz und Diagnose",
  "settings.about.help":
    "Das Über-Panel enthält Kavkas Version und Lizenz, die MCP-Servereinstellungen und den Schalter für Absturzdiagnose.",
  "settings.about.open": "Über öffnen",

  // About and Support Kavka came here when the sidebar footer was deleted.
  "settings.support.title": "Kavka unterstützen",
  "settings.support.help":
    "Kavka ist kostenlos, quelloffen und wird von Menschen finanziert, die freiwillig etwas beitragen. Wer nichts beiträgt, dem wird nichts vorenthalten.",

  // ── Updates ─────────────────────────────────────────────────────────────
  "settings.section.updates": "Updates",
  "settings.section.updates.sub":
    "Ob Kavka GitHub nach neuen Releases fragt und was diese Anfrage enthält und was nicht. Es installiert sich nichts von selbst.",

  "settings.updates.auto.title": "Nach Updates suchen",
  "settings.updates.auto.label": "Kavka nach neuen Releases sehen lassen",
  "settings.updates.auto.hint":
    "Standardmäßig an. Kavka fragt github.com höchstens einmal am Tag, welches das neueste Release ist — dieselbe Frage, die die öffentliche Releases-Seite jedem beantwortet. Die Anfrage enthält nichts, was Sie identifiziert, und nichts über Ihre Cluster, und sie lädt und installiert von sich aus nichts. Das ist die einzige Anfrage, die Kavka ungefragt stellt; schalten Sie sie ab, gibt es keine.",

  "settings.updates.channel.title": "Welche Releases",
  "settings.updates.channel.help":
    "Stabil folgt den Releases, die ein Mensch bewusst getaggt hat. Jeder Build folgt der Vorabversion, die jeder Merge nach main veröffentlicht — neuer, und nicht an denselben Maßstab gebunden.",
  "settings.updates.channel.stable": "Stabil",
  "settings.updates.channel.builds": "Jeder Build",
  "settings.updates.channel.warning":
    "Builds werden automatisch aus main veröffentlicht. Sie kompilieren und bestehen die Prüfungen, aber niemand hat entschieden, dass sie gut sind. Nehmen Sie das nur, wenn Sie die neueste Arbeit wollen und ein stabiles Release neu installieren könnten, falls sich ein Build danebenbenimmt.",

  "settings.updates.check.title": "Jetzt prüfen",
  "settings.updates.check.help":
    "Fragt sofort bei github.com nach, unabhängig vom Schalter oben. Es wird nichts heruntergeladen.",
  "settings.updates.check.button": "Jetzt prüfen",
  "settings.updates.check.checking": "github.com wird gefragt…",

  "settings.updates.result.update":
    "Kavka {version} ist verfügbar. Der Hinweis oben im Fenster hat die Schaltfläche zum Installieren.",
  "settings.updates.result.currentStable":
    "Sie haben das neueste stabile Release.",
  "settings.updates.result.currentBuild": "Sie haben den neuesten Build.",
  "settings.updates.result.noStable":
    "Es wurde noch kein stabiles Release veröffentlicht — bisher gibt es nur automatische Builds aus main. Wechseln Sie zu „Jeder Build“, um ihnen zu folgen.",

  "settings.updates.lastChecked": "Kavka hat zuletzt {when} geprüft.",
  "settings.updates.lastCheckedFailed":
    "Kavka hat es zuletzt {when} versucht und github.com nicht erreicht.",
  "settings.updates.never": "Kavka hat noch nicht geprüft.",

  "updates.banner.label": "Update-Hinweis — Kavka {version}",
  "updates.banner.title": "Kavka {version} ist verfügbar",
  "updates.banner.body":
    "Es wurde nichts heruntergeladen. Kavka holt das Installationsprogramm erst, wenn Sie auf Installieren drücken, und prüft es gegen Kavkas eigenen Signaturschlüssel, bevor irgendetwas ausgeführt wird.",
  "updates.banner.bodyBuild":
    "Das ist ein automatischer Build vom neuesten Merge nach main, kein stabiles Release — niemand hat entschieden, dass er gut ist. Es wurde nichts heruntergeladen; Kavka holt das Installationsprogramm erst, wenn Sie auf Installieren drücken, und prüft es gegen Kavkas eigenen Signaturschlüssel, bevor irgendetwas ausgeführt wird.",
  "updates.banner.willClose":
    "Beim Installieren schließt sich Kavka, damit das Installationsprogramm es ersetzen kann. Beenden Sie vorher, was Sie gerade tun, und öffnen Sie Kavka wieder, sobald das Installationsprogramm fertig ist.",
  "updates.banner.willRestart":
    "Beim Installieren schließt sich Kavka und öffnet sich wieder, sobald das Update eingespielt ist. Beenden Sie vorher, was Sie gerade tun.",
  "updates.banner.notes": "Was sich geändert hat",
  "updates.banner.releasePage": "Release-Seite",
  "updates.banner.install": "Installieren…",
  "updates.banner.installing": "Wird heruntergeladen…",
  "updates.banner.notNow": "Jetzt nicht",

  "updates.error.unreachable.title": "Kavka konnte github.com nicht erreichen",
  "updates.error.unreachable.detail":
    "Es wurde nichts heruntergeladen und auf diesem Rechner hat sich nichts geändert. Prüfen Sie die Verbindung oder ob ein Proxy oder eine Firewall zwischen Ihnen und github.com steht, und versuchen Sie es dann erneut.",
  "updates.error.title": "Das Update wurde nicht abgeschlossen",
  "updates.error.detail":
    "Es wurde nichts installiert und auf diesem Rechner hat sich nichts geändert. Der vollständige Text steht unter „Details anzeigen“, und auf der Release-Seite liegen Installationsprogramme, die Sie selbst herunterladen können.",

  "unit.seconds": "{count, plural, one {# Sekunde} other {# Sekunden}}",
  "unit.minutes": "{count, plural, one {# Minute} other {# Minuten}}",
  "unit.hours": "{count, plural, one {# Stunde} other {# Stunden}}",
  "unit.days": "{count, plural, one {# Tag} other {# Tage}}",
};

export default de;
