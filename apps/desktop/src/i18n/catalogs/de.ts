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

  "sidebar.navLabel": "Gespeicherte Verbindungen",
  "sidebar.title": "Cluster",
  "sidebar.loading": "Ihre Verbindungen werden gelesen…",
  "sidebar.empty":
    "Hier ist noch nichts. Fügen Sie unten Ihre erste Verbindung hinzu.",
  "sidebar.profileMeta": "{address} · {status}",
  "sidebar.status.disconnected": "nicht verbunden",
  "sidebar.status.connecting": "wird verbunden…",
  "sidebar.status.connected": "verbunden",
  "sidebar.draftName": "Neue Verbindung",
  "sidebar.draftMeta": "noch nicht gespeichert",
  "sidebar.about": "Über",
  "sidebar.settings": "Einstellungen",

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
    "Wählen Sie links ein Cluster, um seine Broker und Topics zu sehen, oder fügen Sie eine weitere Verbindung hinzu.",
  "app.readonlyChip": "schreibgeschützt",
  "app.readonlyTitle":
    "Diese Verbindung ist schreibgeschützt. Schalten Sie das in den Einstellungen der Verbindung aus, um zu schreiben oder zu bearbeiten.",
  "app.statusbar.draft": "Neue Verbindung — noch nicht gespeichert",
  "app.statusbar.none": "Keine Verbindung ausgewählt",
  "app.statusbar.commands": "Befehle",
  "app.statusbar.coreVersion": "core v{version}",
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
    "Wählen Sie zuerst in der Seitenleiste das Cluster, das getrennt werden soll",
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
  "about.licence": "Lizenz",
  "about.licenceValue": "Freie und quelloffene Software unter AGPL-3.0",
  "about.language": "Sprache",
  "about.language.hint":
    "Kavkas Rahmen und das Urteil, mit dem jede Cluster-Ansicht beginnt — Seitenleiste, Befehlspalette, diese Dialoge, das Verbindungsformular und der Eröffnungssatz jeder Ansicht. Die Tabellen und Formulare darunter sind noch englisch.",
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
  "editor.new.subtitle":
    "Ein Broker genügt für den Anfang — den Rest des Clusters findet Kavka von dort aus.",
  "editor.saved.subtitle":
    "Nicht verbunden. Prüfen Sie die Angaben unten und verbinden Sie sich dann.",
  "editor.name.label": "Name der Verbindung",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "Was immer Sie in der Seitenleiste wiedererkennen. Nur Kavka sieht es.",
  "editor.env.label": "Umgebung",
  "editor.env.hint.protected":
    "Diese Umgebung ist als geschützt markiert: Die Ledger-Linie trägt ihre Farbe in jeder Tabelle, die Seitenleiste markiert dieses Cluster, ein Warnbalken liegt über dem oberen Fensterrand, und vor jeder zerstörenden Aktion müssen Sie den Namen eintippen. Schalten Sie unten den Schreibschutz ein, sofern Sie nicht wirklich schreiben müssen.",
  "editor.env.hint.other":
    "Kavka färbt jede Ansicht nach Umgebung, damit Sie kein Cluster mit einem anderen verwechseln.",
  "editor.env.manage": "Umgebungen verwalten…",
  "editor.env.hint.unknown":
    "Auf diesem Rechner ist {name} nirgends definiert, deshalb zeigt Kavka die Umgebung neutral grau an und wendet keine Schutzmechanismen an. Legen Sie sie unter „Umgebungen verwalten“ an, um ihr eine Farbe zu geben und zu entscheiden, ob sie geschützt ist.",
  "editor.bootstrap.label": "Bootstrap-Server",
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
    "Geben Sie dieser Verbindung einen Namen, damit Sie sie in der Seitenleiste wiederfinden.",
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
  "editor.cluster.legend": "Der Cluster",
  "editor.guardrails.legend": "Schutzmechanismen",
  "editor.fold.set": "Eingerichtet",
  "editor.fold.notSet": "Nicht eingerichtet",
  "editor.fold.connectCount":
    "{count, plural, one {# Cluster} other {# Cluster}}",


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
  "env.mgr.failed": "Das hat nicht geklappt",
  "env.mgr.working": "Kavka arbeitet daran",
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
  "rail.label": "Cluster-Ansichten",
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
  "rail.disconnect": "Trennen",

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

  // ── Einstellungen (Jackdaw) ─────────────────────────────────────────────
  "settings.title": "Einstellungen",
  "settings.navLabel": "Bereiche der Einstellungen",
  "settings.perch":
    "Alles hier wirkt sofort und wird auf diesem Rechner gespeichert. Kavka zeigt gerade das Design {theme}.",
  "settings.section.appearance": "Darstellung",
  "settings.section.language": "Sprache",
  "settings.section.about": "Über",

  "settings.theme.title": "Design",
  "settings.theme.help":
    "System folgt dem Betriebssystem und wechselt mit, während Kavka geöffnet ist.",
  "settings.theme.system": "System",
  "settings.theme.light": "Hell",
  "settings.theme.dark": "Dunkel",

  "settings.accent.title": "Akzentfarbe",
  "settings.accent.help":
    "Die Farbe auf Schaltflächen, Links und der aktuellen Ansicht. Sie trägt keine eigene Bedeutung, deshalb kann eine Änderung keine Warnung verdecken.",
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

  "settings.language.title": "Sprache",
  "settings.language.help":
    "Umfasst Kavkas Rahmen und das Urteil, mit dem jede Cluster-Ansicht beginnt — Seitenleiste, Befehlspalette, dieses Panel, das Verbindungsformular und den Eröffnungssatz jeder Ansicht. Die Tabellen und Formulare darunter sind noch englisch.",
  "settings.language.machine":
    "Dieser Katalog stammt aus einer Maschine und wurde von keinem Muttersprachler geprüft. Korrekturen sind willkommen.",

  "settings.about.title": "Version, Lizenz und Diagnose",
  "settings.about.help":
    "Das Über-Panel enthält Kavkas Version und Lizenz, die MCP-Servereinstellungen und den Schalter für Absturzdiagnose.",
  "settings.about.open": "Über öffnen",

  "unit.seconds": "{count, plural, one {# Sekunde} other {# Sekunden}}",
  "unit.minutes": "{count, plural, one {# Minute} other {# Minuten}}",
  "unit.hours": "{count, plural, one {# Stunde} other {# Stunden}}",
  "unit.days": "{count, plural, one {# Tag} other {# Tage}}",
};

export default de;
