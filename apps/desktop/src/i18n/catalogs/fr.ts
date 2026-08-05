// ─────────────────────────────────────────────────────────────────────────────
// French (Français) — MACHINE TRANSLATION — NATIVE REVIEW WELCOME.
//
// No native speaker has read this file. It was produced from `en.ts` and it is
// shipped honestly rather than quietly: the language picker in the About
// dialog says so next to the name, and `LOCALES` in ../index.ts carries
// `machine: true` for exactly this reason.
//
// If French is your language, the highest-value contribution to Kavka is
// twenty minutes with this file. See docs/I18N.md — you need no build, no
// tooling and no account, and a partial fix is welcome: any key you delete
// falls back to English rather than breaking.
//
// Two things to keep while editing: the {placeholders} (they are values Kavka
// substitutes, and a renamed one silently disappears from the sentence), and
// the plural arms — CLDR French puts 0 and 1 in `one`, so `one {# connexion}
// other {# connexions}` is the shape.
// ─────────────────────────────────────────────────────────────────────────────

import type { Catalog } from "./en";

const fr: Catalog = {
  "common.close": "Fermer",
  "common.cancel": "Annuler",
  "common.save": "Enregistrer",
  "common.connect": "Se connecter",
  "common.tryAgain": "Réessayer",
  "common.remove": "Retirer",
  "common.dismiss": "Masquer",
  "common.showDetails": "Afficher les détails",
  "common.addConnection": "Ajouter une connexion",
  "common.support": "Soutenir Kavka ☕",
  "common.readingConnections": "Lecture de vos connexions enregistrées…",
  "common.linkFailed":
    "Kavka n'a pas pu transmettre ce lien à votre navigateur. L'adresse est {url} — copiez-la d'ici.",

  "confirm.kicker.destructive": "Destructrice",
  "confirm.busy": "Kavka s'en occupe",
  "confirm.type.label": "Saisissez {name} pour confirmer",
  "confirm.type.reason": "Saisissez exactement {name} pour confirmer",

  "sidebar.navLabel": "Connexions enregistrées",
  "sidebar.title": "Clusters",
  "sidebar.loading": "Lecture de vos connexions…",
  "sidebar.empty":
    "Rien pour l'instant. Ajoutez votre première connexion ci-dessous.",
  "sidebar.profileMeta": "{address} · {status}",
  "sidebar.status.disconnected": "non connecté",
  "sidebar.status.connecting": "connexion…",
  "sidebar.status.connected": "connecté",
  "sidebar.draftName": "Nouvelle connexion",
  "sidebar.draftMeta": "pas encore enregistrée",
  "sidebar.about": "À propos",
  "sidebar.settings": "Réglages",

  "app.status.disconnected": "Non connecté",
  "app.status.connecting": "Connexion…",
  "app.status.connected": "Connecté",
  "app.error.unknownProfile":
    "Cette connexion n'est plus sur cette machine. Elle a peut-être été supprimée dans une autre fenêtre.",
  "app.profilesFailed.title":
    "Kavka n'a pas pu lire son fichier de connexions",
  "app.profilesFailed.hint":
    "Vos connexions sont toujours sur le disque — rien n'a été perdu. Kavka les range dans son dossier de configuration, à côté des réglages de l'application.",
  "app.firstRun.title": "Indiquez un broker à Kavka",
  "app.firstRun.what":
    "Une connexion est une adresse enregistrée pour un seul cluster Kafka — un nom, un broker de départ et la façon de s'authentifier. Kavka découvre le reste du cluster à partir de là.",
  "app.firstRun.example":
    "Un serveur bootstrap ressemble généralement à {example}. Vous faites tourner le cluster de dev de ce dépôt ? Utilisez {local}.",
  "app.firstRun.footnote":
    "Les mots de passe vont dans le trousseau de votre système d'exploitation. Rien de ce qui concerne vos clusters ne quitte cette machine.",
  "app.pick.title": "Choisissez une connexion",
  "app.pick.hint":
    "Choisissez un cluster à gauche pour voir ses brokers et ses topics, ou ajoutez une autre connexion.",
  "app.readonlyChip": "lecture seule",
  "app.readonlyTitle":
    "Cette connexion est en lecture seule. Désactivez-le dans les réglages de la connexion pour produire ou modifier.",
  "app.statusbar.draft": "Nouvelle connexion — pas encore enregistrée",
  "app.statusbar.none": "Aucune connexion sélectionnée",
  "app.statusbar.commands": "commandes",
  "app.statusbar.coreVersion": "core v{version}",
  "app.cmd.search": "Rechercher dans {topic}",
  "app.cmd.search.kw":
    "find filter cel scan query messages grep rechercher filtrer messages",
  "app.cmd.sql": "Interroger {topic} en SQL",
  "app.cmd.sql.kw":
    "sql select query aggregate count group datafusion analyse requête compter analyser",
  "app.cmd.produce": "Produire vers {topic}",
  "app.cmd.produce.kw":
    "send write publish message record bulk producer envoyer écrire publier message",
  "app.cmd.produce.confirmContext": "{cluster} · demande une confirmation",

  "palette.label": "Commandes",
  "palette.searchLabel": "Rechercher des commandes et des clusters",
  "palette.searchPlaceholder": "Rechercher des commandes et des clusters…",
  "palette.empty":
    "Rien ne correspond à « {query} ». Essayez un nom de cluster, ou videz le champ pour voir tout ce que Kavka sait faire.",
  "palette.foot.move": "se déplacer",
  "palette.foot.run": "exécuter",
  "palette.foot.close": "fermer",
  "palette.goTo": "Aller à {name}",
  "palette.connectTo": "Se connecter à {name}",
  "palette.state.connected": "connecté",
  "palette.state.connecting": "connexion…",
  "palette.protectedCluster": "cluster protégé",
  "palette.profile.kw":
    "connect open switch cluster broker bootstrap connecter ouvrir basculer",
  "palette.add.context":
    "Un nom, un broker et la façon de s'authentifier",
  "palette.add.kw":
    "new connection profile cluster create bootstrap broker nouvelle connexion créer",
  "palette.disconnect": "Se déconnecter",
  "palette.disconnect.kw":
    "close leave cluster session déconnecter quitter fermer",
  "palette.disconnect.none": "Rien n'est connecté pour l'instant",
  "palette.disconnect.ambiguous":
    "Choisissez d'abord dans la barre latérale le cluster à déconnecter",
  "palette.refresh": "Recharger les topics",
  "palette.refresh.kw":
    "reload metadata list topics partitions cluster recharger actualiser",
  "palette.export": "Exporter les connexions…",
  "palette.export.context":
    "Toutes les connexions de cette machine, en JSON",
  "palette.export.kw":
    "backup save copy share json profiles sauvegarde exporter copier",
  "palette.import": "Importer des connexions…",
  "palette.import.context": "Coller du JSON d'une autre copie de Kavka",
  "palette.import.kw":
    "restore paste load json profiles coller charger restaurer",
  "palette.about.context": "Version et licence",
  "palette.about.kw":
    "version licence license agpl source github help aide code source",
  "palette.support.context":
    "Kavka est gratuit — les dons le maintiennent ainsi",
  "palette.support.kw":
    "donate coffee sponsor fund open source don café soutenir financer",

  "about.title": "À propos de Kavka",
  "about.body":
    "Un client de bureau pour Apache Kafka. Kavka fonctionne entièrement sur cette machine : les mots de passe vont dans le trousseau de votre système d'exploitation, et rien de ce qui concerne vos clusters ne quitte cet ordinateur.",
  "about.coreVersion": "Version du cœur",
  "about.versionLoading": "Lecture en cours…",
  "about.licence": "Licence",
  "about.licenceValue": "Libre et open source sous AGPL-3.0",
  "about.language": "Langue",
  "about.language.hint":
    "L'ossature de Kavka et le verdict par lequel s'ouvre chaque écran du cluster — la barre latérale, la palette de commandes, ces boîtes de dialogue, le formulaire de connexion et la phrase d'ouverture de chaque écran. Les tableaux et formulaires en dessous sont encore en anglais.",
  "about.language.machine":
    "{language} a été traduit par une machine et n'a été relu par aucun locuteur natif. Les corrections sont les bienvenues — docs/I18N.md explique comment.",

  "transfer.title": "Connexions",
  "transfer.tablist": "Exporter ou importer",
  "transfer.tab.export": "Exporter",
  "transfer.tab.import": "Importer",
  "transfer.export.body":
    "Toutes les connexions de cette machine, en JSON. Collez-le dans une autre copie de Kavka pour y installer les mêmes clusters.",
  "transfer.export.promise":
    "Les mots de passe et les clés ne quittent jamais cette machine — un export porte des références, pas des secrets.",
  "transfer.export.failed":
    "Kavka n'a pas pu lire son fichier de connexions. Vos connexions sont toujours sur le disque — rien n'a été perdu.",
  "transfer.export.label": "Vos connexions, en JSON",
  "transfer.export.copied": "Copié dans le presse-papiers.",
  "transfer.export.copyManual":
    "Kavka n'a pas pu atteindre le presse-papiers. Le texte est sélectionné — appuyez sur {key} pour le copier.",
  "transfer.export.copy": "Copier dans le presse-papiers",
  "transfer.export.nothingToCopy":
    "Il n'y a rien à copier — Kavka n'a pas pu lire son fichier de connexions",
  "transfer.export.stillReading": "Kavka lit encore vos connexions",
  "transfer.import.body":
    "Collez un export d'une autre copie de Kavka. Les mots de passe n'y sont pas — chaque connexion importée demandera le sien à la première connexion.",
  "transfer.import.label": "JSON exporté",
  "transfer.import.kbd": "importer",
  "transfer.import.kbdClose": "fermer",
  "transfer.import.legend": "Si une connexion est déjà là",
  "transfer.import.skip": "Garder celle de cette machine",
  "transfer.import.skipHint":
    "Les connexions déjà enregistrées ici restent exactement telles quelles. Tout ce qui est nouveau dans le JSON est quand même ajouté.",
  "transfer.import.replace": "La remplacer par celle du JSON",
  "transfer.import.replaceHint":
    "La version collée l'emporte — nom, adresse, environnement et méthode d'authentification. Les mots de passe déjà dans votre trousseau ne bougent pas.",
  "transfer.import.failed":
    "Kavka n'a pas pu lire cela comme un export. Vérifiez que vous avez collé le fichier entier, accolades extérieures comprises — le texte reçu par Kavka est ci-dessous.",
  "transfer.import.needsJson": "Collez d'abord le JSON d'un export",
  "transfer.import.busy": "Kavka importe ces connexions en ce moment",
  "transfer.import.run": "Importer les connexions",
  "transfer.import.running": "Import en cours…",
  "transfer.report.empty.title": "Ce JSON ne contenait aucune connexion",
  "transfer.report.empty.detail":
    "Vérifiez que vous avez collé l'export entier, accolades extérieures comprises — Kavka l'a bien lu, il n'y avait simplement rien à ajouter.",
  "transfer.report.added": "{count, plural, other {# ajoutée(s)}}",
  "transfer.report.replaced": "{count, plural, other {# remplacée(s)}}",
  "transfer.report.skipped":
    "{count, plural, other {# ignorée(s) — déjà sur cette machine}}",
  "transfer.report.envAdded":
    "{count, plural, one {# environnement ajouté} other {# environnements ajoutés}}",
  "transfer.report.envSkipped":
    "{count, plural, one {# environnement déjà défini} other {# environnements déjà définis}}",
  "transfer.report.envOnly.title":
    "Aucune nouvelle connexion — seulement des environnements",
  "transfer.report.unchanged.title":
    "{count, plural, one {Rien n'a changé — # connexion était déjà là} other {Rien n'a changé — # connexions étaient déjà là}}",
  "transfer.report.unchanged.detail":
    "{bits}. Choisissez « La remplacer par celle du JSON » ci-dessus si vous vouliez les écraser.",
  "transfer.report.imported.title":
    "{count, plural, one {# connexion importée} other {# connexions importées}}",
  "transfer.report.imported.detail":
    "{bits}. Les mots de passe ne sont pas dans un export — ouvrez chaque nouvelle connexion et saisissez son mot de passe avant de vous connecter.",

  "editor.new.title": "Ajouter une connexion",
  "editor.new.subtitle":
    "Un seul broker suffit pour démarrer — Kavka découvre le reste du cluster à partir de là.",
  "editor.saved.subtitle":
    "Non connecté. Vérifiez les informations ci-dessous, puis connectez-vous.",
  "editor.name.label": "Nom de la connexion",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "Ce que vous reconnaîtrez dans la barre latérale. Kavka est le seul à le voir.",
  "editor.env.label": "Environnement",
  "editor.env.hint.protected":
    "Cet environnement est marqué comme protégé : la règle du registre prend sa couleur dans toutes les tables, la barre latérale marque ce cluster, un bandeau d'avertissement occupe le haut de la fenêtre, et chaque action destructrice vous demande de saisir le nom d'abord. Activez la lecture seule ci-dessous, sauf si vous avez vraiment besoin d'écrire.",
  "editor.env.hint.other":
    "Kavka colore chaque vue selon l'environnement, pour que vous ne confondiez pas deux clusters.",
  "editor.env.manage": "Gérer les environnements…",
  "editor.env.hint.unknown":
    "Rien sur cette machine ne définit {name} : Kavka l'affiche en gris neutre et n'applique aucun garde-fou. Ajoutez-le dans « Gérer les environnements » pour lui donner une couleur et décider s'il est protégé.",
  "editor.bootstrap.label": "Serveurs bootstrap",
  "editor.bootstrap.hint":
    "N'importe quel broker de votre cluster — Kavka trouve les autres à partir de là. Un par ligne, ou séparés par des virgules. Vous faites tourner le cluster de dev de ce dépôt ? Utilisez {local}.",

  "editor.auth.legend": "Authentification",
  "editor.auth.kerberos":
    "Cette connexion s'authentifie avec Kerberos ({service} en tant que {principal}), ce que Kavka ne sait pas encore configurer. L'enregistrement la conserve exactement telle quelle ; tous les autres champs fonctionnent normalement.",
  "editor.auth.label": "Comment ce cluster vérifie-t-il qui vous êtes ?",
  "editor.auth.plaintext":
    "Il ne le fait pas — tout le monde peut se connecter (PLAINTEXT)",
  "editor.auth.saslPlain": "Nom d'utilisateur et mot de passe — SASL/PLAIN",
  "editor.auth.saslScram": "Nom d'utilisateur et mot de passe — SASL/SCRAM",
  "editor.auth.mtls": "Un certificat présenté par cette machine — mTLS",
  "editor.auth.mskIam":
    "Les identifiants AWS présents sur cette machine — MSK IAM",
  "editor.auth.oauth":
    "Un jeton de votre fournisseur d'identité — OAuth 2.0 / OIDC",
  "editor.auth.kerberosOption":
    "Un ticket Kerberos — GSSAPI (pas encore disponible)",
  "editor.auth.notYet":
    "Kavka ne sait pas encore configurer cela. Une connexion qui l'utilise déjà continue de fonctionner et est conservée exactement telle quelle à l'enregistrement.",
  "editor.auth.hint":
    "Un Kafka managé demande généralement SASL/SCRAM avec TLS activé. Un broker local ne demande généralement rien. Kerberos est la seule méthode que Kavka ne sait pas encore configurer.",
  "editor.mechanism.label": "Mécanisme SCRAM",
  "editor.mechanism.hint":
    "Si le broker en refuse un, il vous dira lequel il attend.",
  "editor.username.label": "Nom d'utilisateur",
  "editor.password.label": "Mot de passe",
  "editor.password.placeholder": "Mot de passe",
  "editor.secret.unchanged": "••••••••  (inchangé)",
  "editor.password.hint":
    "Va dans le trousseau de votre système d'exploitation — jamais dans le fichier de connexions, et jamais hors de cette machine.",
  "editor.tls.label": "Chiffrer la connexion (TLS)",
  "editor.tls.hint":
    "Un Kafka managé en a presque toujours besoin. Si le broker répond mais que la poignée de main échoue, c'est la première chose à essayer.",

  "editor.mtls.hint":
    "Kavka lit les fichiers PEM tels quels — il n'y a aucun keystore JKS ou PKCS#12 à convertir au préalable.",
  "editor.caPath.label": "Certificat de l'AC",
  "editor.caPath.hint":
    "Chemin vers le .pem de l'autorité de certification — laissez vide pour utiliser le magasin de confiance du système.",
  "editor.clientCert.label": "Certificat client",
  "editor.clientCert.hint":
    "Chemin vers le certificat que cette machine présente au broker — laissez vide si le broker n'en demande pas.",
  "editor.clientKey.label": "Clé privée du client",
  "editor.clientKey.hint":
    "Collez la clé elle-même, pas un chemin vers elle. Elle va dans le trousseau de votre système d'exploitation — jamais dans le fichier de connexions, et jamais hors de cette machine.",
  "editor.clientKey.storedHint":
    "Laissez vide pour conserver la clé enregistrée ; vider le chemin du certificat ci-dessus la supprime.",

  "editor.aws.hint":
    "Kavka signe chaque requête avec les identifiants AWS déjà présents sur cette machine. Les serveurs bootstrap ci-dessus doivent être le point d'entrée IAM de ce cluster — les hôtes {host} de la console MSK, généralement sur le port 9098.",
  "editor.region.label": "Région",
  "editor.region.hint":
    "La région AWS où tourne le cluster. Elle doit correspondre aux hôtes bootstrap, sinon la signature sera refusée.",
  "editor.awsProfile.label": "Nom du profil AWS",
  "editor.awsProfile.hint":
    "Un profil nommé de {config}. Laissez vide pour utiliser la chaîne d'identifiants par défaut — variables d'environnement, puis {dir}, puis SSO.",

  "editor.oauth.hint":
    "Kavka demande un jeton à votre fournisseur d'identité via le flux client credentials, puis le présente au broker en SASL/OAUTHBEARER.",
  "editor.tokenEndpoint.label": "Point d'accès du jeton",
  "editor.tokenEndpoint.hint":
    "L'URL qui délivre le jeton, pas la page de connexion qu'un navigateur utiliserait.",
  "editor.clientId.label": "Identifiant client",
  "editor.clientId.hint":
    "L'application que votre fournisseur d'identité a enregistrée pour Kafka — pas votre propre compte utilisateur.",
  "editor.clientSecret.label": "Secret client",
  "editor.clientSecret.placeholder": "Secret client",
  "editor.clientSecret.hint":
    "Va dans le trousseau de votre système d'exploitation — jamais dans le fichier de connexions, et jamais hors de cette machine.",

  "editor.sr.legend": "Schema Registry (facultatif)",
  "editor.sr.hint":
    "Si les messages de ce cluster sont en Avro, Protobuf ou JSON Schema, Kavka lit le schéma ici pour les décoder — et affiche le sujet, la version et l'identifiant à côté de chaque message. Sans cela, ces charges utiles sont affichées en octets bruts.",
  "editor.srUrl.label": "Adresse du registre",
  "editor.srUrl.hint":
    "L'URL complète, schéma compris. Confluent, Apicurio et Glue parlent ici la même API de lecture. Laissez vide si ce cluster n'a pas de registre.",
  "editor.srUsername.label": "Nom d'utilisateur du registre",
  "editor.srUsername.hint":
    "Seulement si le registre en demande un. Les registres managés le font en général ; un registre dans votre propre réseau, en général non.",
  "editor.srPassword.label": "Mot de passe du registre",
  "editor.srPassword.storedHint":
    "Laissez vide pour conserver celui qui est enregistré ; vider l'adresse ci-dessus le supprime.",

  "editor.connect.legend": "Clusters Kafka Connect (facultatif)",
  "editor.connect.hint":
    "Kafka Connect fait tourner des connecteurs source et sink, et il répond sur son propre port REST plutôt qu'à travers les brokers — Kavka doit donc savoir où sont les workers. Ajoutez-en un par groupe de workers ; c'est par le nom que vous choisirez entre eux dans l'onglet Connect.",
  "editor.connect.unnamed": "Cluster {number}",
  "editor.connect.removeLabel": "Retirer {name}",
  "editor.connect.unnamedLong": "Cluster Connect {number}",
  "editor.connect.remove": "Retirer ce cluster Connect de la connexion",
  "editor.connect.name.label": "Nom",
  "editor.connect.name.placeholder": "orders connect",
  "editor.connect.name.hint":
    "Ce que vous reconnaîtrez. Le renommer plus tard conserve son mot de passe enregistré.",
  "editor.connect.url.label": "Adresse des workers",
  "editor.connect.url.hint":
    "Le point d'accès REST de n'importe quel worker du groupe — ils répondent tous pour le cluster entier. Généralement le port 8083, et pas le même hôte ni le même port que les brokers.",
  "editor.connect.username.hint":
    "Seulement si les workers sont derrière une authentification basique. La plupart ne le sont pas.",
  "editor.connect.password.storedHint":
    "Laissez vide pour conserver celui qui est enregistré ; retirer ce cluster le supprime.",
  "editor.connect.add": "Ajouter un cluster Connect",

  "editor.monitoring.legend": "Supervision (facultatif)",
  "editor.monitoring.hint":
    "Les brokers Kafka ne servent pas les chiffres de débit, de stockage ou de réplication via le protocole Kafka — ils les publient en JMX, et presque tout le monde place un exportateur Prometheus devant. Indiquez l'exportateur à Kavka et l'onglet Supervision se remplit. L'historique du lag n'a besoin de rien de tout cela : Kavka le lit lui-même auprès des brokers.",
  "editor.metricsUrl.label": "Adresse des métriques",
  "editor.metricsUrl.hint":
    "L'URL complète, chemin compris. Si vous exploitez les brokers, c'est généralement l'agent Java {agent} sur l'un d'eux ({flag}). Un serveur Prometheus qui interroge déjà ces brokers convient aussi — donnez son adresse à Kavka. Laissez vide si ce cluster n'a pas d'exportateur.",
  "editor.metricsUsername.label": "Nom d'utilisateur des métriques",
  "editor.metricsUsername.hint":
    "Seulement si le point d'accès est derrière une authentification basique. Un jmx_exporter, en général non ; un Prometheus partagé, en général oui.",
  "editor.metricsPassword.label": "Mot de passe des métriques",
  "editor.metricsPassword.storedHint":
    "Laissez vide pour conserver celui qui est enregistré ; vider l'adresse ci-dessus le supprime.",
  "editor.sampler.label": "Prendre une mesure du lag toutes les",
  "editor.sampler.hint":
    "Secondes. Kafka ne mémorise pas le lag, donc Kavka prend sa propre mesure à cet intervalle et en garde {days} dans un fichier sur cette machine. {warning} Le plancher est de {floor} ; la valeur par défaut est {default}, ce qui coûte une petite requête par groupe et par mesure.",
  "editor.sampler.warning":
    "Les mesures n'ont lieu que tant que cette connexion est ouverte — rien n'est collecté pendant que Kavka est fermé ou que ce cluster est déconnecté, et un trou dans le graphique signifie exactement cela.",

  "editor.readonly.label": "Connexion en lecture seule",
  "editor.readonly.hint":
    "Kavka continuera de tout parcourir, mais il ne produira pas de messages, ne modifiera pas de topics et ne validera pas d'offsets sur cette connexion.",

  "editor.busy.connecting":
    "Attendez la fin de la tentative de connexion",
  "editor.busy.saving": "Kavka enregistre cette connexion",
  "editor.delete": "Supprimer la connexion",
  "editor.delete.confirm":
    "Retirer {name} de cette machine ? Le cluster lui-même n'est pas touché.",
  "editor.kbd.connect": "se connecter",
  "editor.kbd.cancel": "annuler",
  "editor.kbd.undo": "annuler les modifications",

  "editor.err.name":
    "Donnez un nom à cette connexion pour la retrouver dans la barre latérale.",
  "editor.err.bootstrap":
    "Ajoutez au moins un broker, sous la forme hôte:port — par ex. broker-1:9092",
  "editor.err.srUrl":
    "Utilisez l'URL complète, commençant par http:// ou https:// — par ex. http://localhost:8081",
  "editor.err.srUserNoUrl":
    "Ajoutez l'adresse du registre, ou videz le nom d'utilisateur — une authentification sans destination ne peut pas être enregistrée.",
  "editor.err.metricsUrl":
    "Utilisez l'URL complète, commençant par http:// ou https:// — par ex. http://broker-1.internal:7071/metrics",
  "editor.err.metricsUserNoUrl":
    "Ajoutez l'adresse des métriques, ou videz le nom d'utilisateur — une authentification sans destination ne peut pas être enregistrée.",
  "editor.err.sampler":
    "Mesurez au plus toutes les {seconds, plural, one {# seconde} other {# secondes}}. Plus rapide, cela demande les offsets aux brokers plus souvent qu'ils ne changent.",
  "editor.err.connectName":
    "Donnez un nom à ce cluster Connect — chaque action envoyée par Kavka nomme le cluster auquel elle s'adresse.",
  "editor.err.connectDuplicate":
    "Deux clusters Connect d'une même connexion ne peuvent pas partager un nom — Kavka enregistre leurs mots de passe sous celui-ci.",
  "editor.err.connectUrlMissing":
    "Ajoutez l'adresse REST des workers — par ex. http://connect-1.internal:8083",
  "editor.err.connectUrl":
    "Utilisez l'URL complète, commençant par http:// ou https:// — par ex. http://connect-1.internal:8083",
  "editor.err.username":
    "Cette méthode d'authentification a besoin du nom d'utilisateur sous lequel le broker vous connaît.",
  "editor.err.password":
    "Cette méthode d'authentification a besoin d'un mot de passe.",
  "editor.err.clientKey":
    "Collez la clé privée qui va avec ce certificat — Kavka a besoin des deux moitiés.",
  "editor.err.clientCert":
    "Ajoutez le chemin du certificat auquel appartient cette clé — Kavka a besoin des deux moitiés.",
  "editor.err.region":
    "Indiquez la région où tourne le cluster — par ex. eu-west-1",
  "editor.err.tokenEndpoint":
    "Ajoutez l'URL à laquelle votre fournisseur d'identité délivre les jetons — par ex. https://login.example.com/oauth2/token",
  "editor.err.tokenEndpointUrl":
    "Utilisez l'URL complète, commençant par https:// — par ex. https://login.example.com/oauth2/token",
  "editor.err.clientId":
    "Ajoutez l'identifiant client que votre fournisseur d'identité a émis pour cette application.",
  "editor.err.clientSecret":
    "Cette méthode d'authentification a besoin du secret qui va avec cet identifiant client.",

  "editor.perch.screen": "Connexion",
  "editor.perch.new":
    "Rien n'est encore enregistré — Kavka n'a contacté aucun broker, donc rien sur cet écran n'a été vérifié.",
  "editor.perch.saved":
    "Enregistrée, mais non connectée. Kavka n'a pas encore parlé à {name}, donc aucun de ces détails n'a été vérifié auprès du cluster.",
  "editor.perch.connected":
    "Connectée à {name}. Kavka lit encore la vue d'ensemble du cluster.",
  "editor.perch.caveat.protected":
    "{name} est marqué comme protégé : chaque action destructrice sur ce cluster vous demande d'abord d'en saisir le nom.",
  "editor.perch.caveat.unknown":
    "Rien sur cette machine ne définit {name}, donc aucun garde-fou ne s'applique à cette connexion.",
  "editor.perch.caveat.readonly":
    "La lecture seule est activée — Kavka parcourra ce cluster mais n'y écrira jamais.",
  "editor.cluster.legend": "Le cluster",
  "editor.guardrails.legend": "Garde-fous",
  "editor.fold.set": "Configuré",
  "editor.fold.notSet": "Non configuré",
  "editor.fold.connectCount":
    "{count, plural, one {# cluster} other {# clusters}}",


  // ── Environnements ──────────────────────────────────────────────────────
  "env.color.green": "vert",
  "env.color.amber": "ambre",
  "env.color.red": "rouge",
  "env.color.blue": "bleu",
  "env.color.violet": "violet",
  "env.color.cyan": "cyan",
  "env.color.slate": "ardoise",

  "env.mgr.title": "Environnements",
  "env.mgr.intro":
    "Nommez les environnements que votre organisation utilise réellement. La couleur les distingue d'un coup d'œil ; « protégé » est le garde-fou.",
  "env.mgr.failed": "Ça n'est pas passé",
  "env.mgr.working": "Kavka s'en occupe",
  "env.mgr.add": "Ajouter un environnement",
  "env.mgr.edit": "Modifier",

  "env.mgr.row.protected": "protégé",
  "env.mgr.row.unprotected": "non protégé",
  "env.mgr.row.used":
    "{count, plural, =0 {aucune connexion} one {# connexion} other {# connexions}}",

  "env.mgr.name.label": "Nom",
  "env.mgr.name.hint":
    "Comme votre équipe l'appelle — dev, QA, UAT, production. Affiché exactement comme vous le saisissez, et jamais traduit.",
  "env.mgr.name.taken": "Un environnement portant ce nom existe déjà.",
  "env.mgr.name.required": "Donnez d'abord un nom à l'environnement",

  "env.mgr.color.label": "Couleur",
  "env.mgr.color.hint":
    "Identité uniquement. La couleur teinte la règle du registre et la pastille ; elle ne décide jamais de ce que Kavka vous laisse faire.",

  "env.mgr.protected.label": "Traiter cet environnement comme protégé",
  "env.mgr.protected.hint":
    "Kavka passe au fond d'avertissement, vous demande de saisir le nom du topic ou du groupe avant toute action destructrice, marque la fenêtre, et refuse les écritures depuis la ligne de commande et depuis les assistants IA sauf indication explicite du contraire.",
  "env.mgr.unprotect.prompt": "Saisissez {name} pour retirer sa protection",
  "env.mgr.unprotect.hint":
    "Toutes les connexions de {name} perdent leurs garde-fous : plus de confirmations saisies, et la ligne de commande comme les assistants IA cessent de refuser les écritures.",

  "env.mgr.delete.title": "Supprimer {name} ?",
  "env.mgr.delete.unused":
    "Aucune connexion n'utilise {name}, rien d'autre ne change.",
  "env.mgr.delete.used":
    "{count, plural, one {# connexion utilise} other {# connexions utilisent}} {name}. Choisissez où elles vont — Kavka les déplace avant de le supprimer.",
  "env.mgr.delete.moveTo": "Déplacer ces connexions vers",
  "env.mgr.delete.moveHint": "Ces connexions seront déplacées : {names}.",
  "env.mgr.delete.confirm": "Supprimer l'environnement",
  "env.mgr.delete.needTarget":
    "Choisissez un environnement où déplacer ces connexions.",
  "env.mgr.delete.last":
    "C'est le seul environnement restant — ajoutez-en un autre d'abord",

  // ── Navigation du cluster (Jackdaw) ─────────────────────────────────────
  "rail.label": "Écrans du cluster",
  "rail.group.cluster": "Cluster",
  "rail.group.observe": "Observer",
  "rail.group.safety": "Sécurité",
  "rail.group.integrations": "Intégrations",
  "rail.item.overview": "Accueil",
  "rail.item.topics": "Topics",
  "rail.item.groups": "Groupes de consommateurs",
  "rail.item.brokers": "Brokers",
  "rail.item.monitoring": "Supervision",
  "rail.item.alerts": "Alertes",
  "rail.item.streams": "Streams",
  "rail.item.acls": "ACL",
  "rail.item.masking": "Masquage",
  "rail.item.connect": "Connect",
  "rail.firing": "active",
  "rail.firingTitle":
    "{count, plural, one {# règle d’alerte est active en ce moment} other {# règles d’alerte sont actives en ce moment}}",
  "rail.disconnect": "Se déconnecter",

  // ── Le perchoir (Jackdaw) ───────────────────────────────────────────────
  "perch.label": "{screen} — ce que Kavka peut en dire",
  "perch.kicker": "{screen} · {state}",
  "perch.state.ok": "A l’air en bonne santé",
  "perch.state.watch": "Mérite un coup d’œil",
  "perch.state.problem": "Quelque chose ne va pas",
  "perch.state.unknown": "Pas encore sûr",
  "perch.state.checking": "Vérification en cours",
  "perch.checking":
    "Vérification en cours — Kavka dira ce qu’il trouve dès que le cluster répond.",
  "perch.overview.counts":
    "Connecté à {brokers, plural, one {# broker} other {# brokers}}, portant {topics, plural, one {# topic} other {# topics}} répartis sur {partitions, plural, one {# partition} other {# partitions}}.",
  "perch.overview.firing":
    "{count, plural, one {# règle d’alerte est active} other {# règles d’alerte sont actives}} sur ce cluster en ce moment. {counts}",
  "perch.overview.snapshot":
    "Ces chiffres datent de la connexion et ne suivent pas le cluster — reconnectez-vous pour les reprendre.",
  "perch.overview.noBrokers":
    "Le cluster a répondu, mais n’a nommé aucun broker.",
  "perch.overview.noBrokers.next":
    "Cela veut souvent dire que vous avez atteint un répartiteur de charge plutôt que Kafka, ou que les métadonnées sont revenues vides. Déconnectez-vous, reconnectez-vous et vérifiez l’adresse bootstrap.",
  "perch.screen.messages": "Messages",
  "perch.screen.search": "Recherche",
  "perch.screen.sql": "SQL",
  "perch.screen.schemas": "Schémas",
  "perch.topics.unreadable": "Kavka n'a aucune liste des topics de ce cluster.",
  "perch.topics.unreadable.next":
    "La connexion peut être établie alors que le compte n'a pas Describe sur le cluster. Actualiser redemande.",
  "perch.topics.empty":
    "Ce cluster n'a aucun topic — aucun n'y a encore été créé.",
  "perch.topics.internalOnly":
    "Tout ce qui se trouve sur ce cluster est un topic interne propre à Kafka. Activez Afficher les internes pour les voir.",
  "perch.topics.counts":
    "{count, plural, one {# topic sur ce cluster} other {# topics sur ce cluster}}.",
  "perch.topics.countsHidden":
    "{count, plural, one {# topic affiché} other {# topics affichés}}.",
  "perch.topics.hiddenNote":
    "{count, plural, one {# autre est un topic interne propre à Kafka et reste masqué} other {# autres sont des topics internes propres à Kafka et restent masqués}}.",
  "perch.topics.snapshot":
    "Cette liste a été lue à l'ouverture de l'écran et ne suit pas le cluster — Actualiser la relit.",
  "perch.topics.readOnly":
    "Cette connexion est en lecture seule : rien ici ne peut créer, modifier ou supprimer un topic.",
  "perch.topic.unreadable":
    "Kavka n'a pas la liste des partitions de {topic} et ne peut donc pas dire ce qu'il contient.",
  "perch.topic.unreadable.next":
    "Le topic a peut-être été supprimé, ou le compte n'a pas Describe dessus.",
  "perch.topic.underReplicated":
    "{count, plural, one {# partition ici manque d'une copie} other {# partitions ici manquent de copies}} — Kafka conserve moins de réplicas que ce topic ne le demande.",
  "perch.topic.unpreferred":
    "{count, plural, one {# partition est dirigée} other {# partitions sont dirigées}} par un broker autre que le premier de sa liste de réplicas. C'est courant après un redémarrage, et Élire les leaders préférés les remet en place.",
  "perch.topic.healthy":
    "{count, plural, one {# partition} other {# partitions}}, toutes les copies synchronisées.",
  "perch.topic.records": "Environ {records} messages d'après les offsets.",
  "perch.topic.approx":
    "Ce nombre de messages est l'écart entre le premier et le dernier offset de chaque partition : il compte donc encore des enregistrements que la rétention ou le compactage ont déjà supprimés.",
  "perch.messages.waiting":
    "Rien n'a encore été lu. Choisissez ci-dessus d'où lire, puis appuyez sur Récupérer.",
  "perch.messages.range":
    "{count, plural, one {# message} other {# messages}} dans la plage demandée.",
  "perch.messages.none": "Rien dans la plage demandée.",
  "perch.messages.topicEmpty": "{topic} ne contient encore aucun message.",
  "perch.messages.live":
    "Observation de {topic} en direct — {count, plural, one {# message est arrivé} other {# messages sont arrivés}} depuis le début du suivi.",
  "perch.messages.liveQuiet":
    "Observation de {topic} en direct. Rien n'y a été produit depuis au moins trente secondes.",
  "perch.messages.notWhole":
    "Ceci est la tranche demandée, pas le topic entier — {topic} contient environ {total} messages.",
  "perch.messages.dropped":
    "{count, plural, one {# message est arrivé} other {# messages sont arrivés}} plus vite que cette fenêtre ne pouvait l'absorber, et la session l'a abandonné plutôt que de prendre du retard — les lignes à l'écran ne sont donc pas tout ce que le suivi a vu.",
  "perch.messages.trimmed":
    "Kavka conserve les {cap} dernières lignes en direct ; tout ce qui est plus ancien a déjà quitté le tampon.",
  "perch.messages.masked":
    "Des règles de masquage sont actives : certaines valeurs à l'écran ne sont pas celles du topic. Les copies et les exports contiennent les remplacements.",
  "perch.search.waiting":
    "Rien n'a encore été parcouru. Définissez la portée, dites ce que vous cherchez et appuyez sur Rechercher.",
  "perch.search.running":
    "Analyse de {topic} — {count, plural, one {# correspondance} other {# correspondances}} pour l'instant.",
  "perch.search.running.note":
    "Partiel. Ces chiffres continuent de bouger jusqu'à la fin de l'analyse.",
  "perch.search.matches":
    "{count, plural, one {# correspondance} other {# correspondances}} parmi les {scanned} enregistrements lus par cette analyse.",
  "perch.search.none":
    "Rien ne correspond parmi les {scanned} enregistrements lus par cette analyse.",
  "perch.search.stopped":
    "Vous avez arrêté cette analyse après {scanned} enregistrements : elle répond donc sur une partie de la plage et non sur sa totalité.",
  "perch.search.capped":
    "{matched} enregistrements correspondent mais Kavka en a gardé {kept}. Trier, exporter ou compter ce qui est à l'écran répond sur ceux-là, pas sur toutes les correspondances.",
  "perch.search.unevaluated":
    "{count, plural, one {# enregistrement n'a pas pu être lu} other {# enregistrements n'ont pas pu être lus}} au regard de votre expression. Ils ont été ignorés, pas jugés non correspondants.",
  "perch.search.masked":
    "Des règles de masquage sont actives : certaines valeurs à l'écran — et dans tout ce que vous exportez — ne sont pas celles du topic.",
  "perch.sql.waiting":
    "Aucune requête n'a encore été exécutée. La portée ci-dessus décide quels enregistrements la requête peut voir.",
  "perch.sql.running":
    "En cours — {scanned} enregistrements lus pour l'instant.",
  "perch.sql.running.note":
    "Partiel. Rien ci-dessous n'est la réponse définitive tant que l'analyse n'est pas terminée.",
  "perch.sql.rows":
    "{count, plural, one {# ligne} other {# lignes}} issues des {scanned} enregistrements lus par cette analyse.",
  "perch.sql.none":
    "La requête n'a renvoyé aucune ligne à partir des {scanned} enregistrements lus par cette analyse.",
  "perch.sql.scope":
    "Cela répond sur les enregistrements lus par l'analyse, pas sur le topic entier — une autre portée donne une autre réponse.",
  "perch.sql.capped":
    "L'analyse s'est arrêtée à son plafond de {cap} enregistrements : tout ce que la requête a compté ou additionné porte sur cette tranche.",
  "perch.sql.stopped":
    "Vous avez arrêté cette analyse après {scanned} enregistrements : la réponse ne couvre donc qu'une partie de la plage.",
  "perch.sql.masked":
    "Des règles de masquage étaient en vigueur pendant l'exécution de cette requête : certaines valeurs ici ne sont pas celles du topic.",
  "perch.schemas.noRegistry":
    "Cette connexion n'a pas de Schema Registry : il n'y a donc rien ici d'où lire des schémas.",
  "perch.schemas.noRegistry.next":
    "Un registry est un service distinct avec sa propre adresse. Ajoutez-le sous Schema Registry dans les réglages de cette connexion.",
  "perch.schemas.missing": "Le registry n'a aucun subject nommé {subject}.",
  "perch.schemas.missing.next":
    "Kavka a cherché selon la stratégie du nom de topic, celle qu'utilisent la plupart des producteurs. Un producteur employant une autre stratégie s'enregistre sous un autre nom.",
  "perch.schemas.versions":
    "{count, plural, one {# version de ce subject est enregistrée} other {# versions de ce subject sont enregistrées}}.",
  "perch.schemas.level": "Les nouvelles versions sont vérifiées en {level}.",
  "perch.schemas.levelUnknown":
    "Kavka n'a pas pu lire le réglage de compatibilité propre à ce subject : il ne peut donc pas dire avec certitude quel niveau le registry appliquera.",
  "perch.groups.none":
    "Aucun groupe de consommateurs sur ce cluster pour l'instant — rien n'y a encore lu.",
  "perch.groups.counts":
    "{count, plural, one {# groupe de consommateurs lit} other {# groupes de consommateurs lisent}} depuis ce cluster.",
  "perch.groups.rebalancing":
    "{unstable, plural, one {# groupe est} other {# groupes sont}} en cours de rééquilibrage : leurs partitions changent de mains et la consommation est suspendue pendant ce temps. {counts}",
  "perch.groups.unread":
    "Kavka n'a pas pu lire les groupes de consommateurs de ce cluster et ne peut donc rien en dire. D'ici là, rien sur cet écran n'est une affirmation sur le cluster.",
  "perch.groups.caveat":
    "Voici la liste telle que Kavka l'a lue la dernière fois. L'état d'un groupe change à chaque rééquilibrage — appuyez sur Actualiser pour la reprendre.",
  "perch.group.caughtUp":
    "{group} est à jour sur chaque partition que Kavka peut voir.",
  "perch.group.behind":
    "{group} accuse environ {lag} messages de retard sur {partitions, plural, one {# partition} other {# partitions}}. La pire est {topic} partition {partition}, à {worst}.",
  "perch.group.noOffsets":
    "{group} n'a jamais validé d'offset : il n'y a donc aucune position à signaler. Ce groupe n'a peut-être que produit, ou il a été créé sans jamais rien lire.",
  "perch.group.noMembers":
    "Rien n'est connecté à {group} pour le moment : il ne lit donc rien. Ses offsets validés sont toujours là, et une application qui démarre repartira de ceux-ci.",
  "perch.group.caveat":
    "Kavka a lu ces offsets une seule fois, à l'ouverture de cet écran. Ils ne suivent pas le groupe — rouvrez-le pour une nouvelle lecture.",
  "perch.brokers.counts":
    "{count, plural, one {# broker dans ce cluster} other {# brokers dans ce cluster}}. Ouvrez-en un pour voir tous les réglages avec lesquels il tourne.",
  "perch.brokers.none": "Ce cluster a répondu, mais il n'a nommé aucun broker.",
  "perch.brokers.noneNext":
    "Cela signifie généralement que les métadonnées sont revenues vides, ou que vous avez atteint un répartiteur de charge plutôt que Kafka lui-même. Déconnectez-vous, reconnectez-vous et vérifiez l'adresse de bootstrap.",
  "perch.brokers.caveat":
    "La liste des brokers est revenue lors de la connexion et ne suit pas le cluster — reconnectez-vous pour la reprendre.",
  "perch.broker.noOverrides":
    "Le broker {broker} ne change rien aux valeurs par défaut de Kafka — chacun de ses réglages est calculé par Kafka.",
  "perch.broker.overrides":
    "Le broker {broker} remplace {count, plural, one {# réglage} other {# réglages}} ; les {rest} autres sont ce qu'il calcule à cet instant.",
  "perch.broker.unread":
    "Kavka n'a pas pu lire les réglages de ce broker et ne peut donc pas dire avec quoi il tourne. Le compte a normalement besoin de DescribeConfigs sur le cluster.",
  "perch.broker.caveat":
    "Seules les lignes marquées d'un + sont définies sur ce broker. Une valeur par défaut calculée peut changer en même temps que le cluster, et Kafka signale certains réglages comme en lecture seule pour les clients — ceux-là gardent leur bouton Modifier, désactivé, avec la raison au survol.",
  "perch.connect.noClusters":
    "Cette connexion n'a aucun worker Kafka Connect : il n'y a donc rien à piloter d'ici.",
  "perch.connect.noClustersNext":
    "Connect s'exécute comme son propre ensemble de workers avec sa propre adresse REST, généralement sur le port 8083. Ajoutez-en un sous Clusters Kafka Connect dans les réglages de cette connexion.",
  "perch.connect.empty":
    "Aucun connecteur sur {cluster} pour l'instant : rien n'est donc déplacé vers Kafka ou depuis Kafka d'ici.",
  "perch.connect.allRunning":
    "{count, plural, one {# connecteur sur {cluster}} other {# connecteurs sur {cluster}}}, et toutes les tâches tournent.",
  "perch.connect.failed":
    "{failed, plural, one {# tâche a échoué} other {# tâches ont échoué}} sur {cluster}. Une tâche en échec ne déplace aucun enregistrement tant que rien ne la redémarre — ouvrez le connecteur et lisez d'abord la trace du worker.",
  "perch.connect.paused":
    "{paused, plural, one {# connecteur est} other {# connecteurs sont}} en pause sur {cluster} : plus rien ne passe par {paused, plural, one {lui} other {eux}}. Leurs configurations et leurs offsets validés sont conservés.",
  "perch.connect.unread":
    "Kavka n'a pas pu joindre les workers Connect et ne peut donc pas dire ce qui tourne. C'est une adresse différente de celle des brokers, et c'est peut-être la seule chose en panne.",
  "perch.connect.caveat":
    "Ces états proviennent des workers lors de la dernière demande de Kavka. Connect les modifie de lui-même — appuyez sur Actualiser pour une nouvelle lecture.",
  "perch.connector.running":
    "{name} tourne : {running} tâches sur {total} déplacent des enregistrements.",
  "perch.connector.failed":
    "{name} a {failed, plural, one {# tâche en échec} other {# tâches en échec}} et ne déplace rien. Lisez pourquoi elle s'est arrêtée avant de la redémarrer — un redémarrage avec la cause toujours présente échoue à nouveau.",
  "perch.connector.paused":
    "{name} est en pause : il ne déplace aucun enregistrement. Sa configuration et ses offsets validés sont conservés, et la reprise repart de là.",
  "perch.connector.noTasks":
    "{name} n'a aucune tâche : rien ne bouge donc. Les workers créent les tâches à partir de la configuration d'un connecteur, et une configuration inutilisable le laisse sans aucune.",
  "perch.connector.caveat":
    "Ceci est une lecture unique, prise lors de la dernière demande de Kavka aux workers. Les états des tâches évoluent d'eux-mêmes.",
  "perch.monitoring.origin":
    "Kafka ne mémorise pas le retard — un broker ne peut dire que la position actuelle d'un groupe. Tout ce qui est sur cet écran est l'enregistrement propre à Kavka, pris pendant que cette connexion était établie.",
  "perch.monitoring.unread":
    "Kavka n'a pas pu lire son propre enregistrement de retard pour cette connexion et ne peut donc pas dire l'ampleur du retard de quoi que ce soit — ni s'il dispose du moindre relevé.",
  "perch.monitoring.noHistory":
    "Kavka n'a pas encore de relevés de retard pour cette connexion. Les premiers apparaissent dans les {interval} suivant la connexion, et un groupe n'apparaît ici qu'après avoir validé un offset au moins une fois.",
  "perch.monitoring.noWindow":
    "Kavka n'a aucun relevé pour {group} dans cette fenêtre. Essayez-en une plus longue, ou vérifiez l'échantillonneur ci-dessous.",
  "perch.monitoring.caughtUp":
    "{group} était à jour au dernier relevé — rien n'attendait d'être lu.",
  "perch.monitoring.rising":
    "{group} accuse environ {lag} messages de retard sur {partitions, plural, one {# partition} other {# partitions}}, et cela augmente. La pire est {topic} partition {partition}, qui a atteint {peak}.",
  "perch.monitoring.steady":
    "{group} accuse environ {lag} messages de retard sur {partitions, plural, one {# partition} other {# partitions}}, et cela reste stable depuis le début de cette fenêtre.",
  "perch.monitoring.falling":
    "{group} accuse environ {lag} messages de retard sur {partitions, plural, one {# partition} other {# partitions}}, et cela diminue.",
  "perch.monitoring.caveat.sampled":
    "Un point sur ces graphiques est le pire relevé de sa tranche, jamais une moyenne, et une rupture dans une courbe correspond à une période où Kavka ne tournait pas — pas à une panne.",
  "perch.monitoring.caveat.stale":
    "L'échantillonneur est en retard : son dernier relevé date de {ago}, soit plus de trois intervalles. Tout ce qui est en dessous est plus ancien qu'il n'y paraît.",
  "perch.monitoring.caveat.stopped":
    "Rien n'est enregistré pour cette connexion en ce moment : ce verdict n'est donc pas plus récent que le dernier relevé que Kavka a pu prendre.",
  "perch.monitoring.caveat.unknownSampler":
    "Kavka ne peut pas dire ce que fait son échantillonneur en ce moment : il ne peut donc pas garantir que ces relevés sont à jour.",
  "perch.alerts.none":
    "Aucune règle sur ce cluster : Kavka ne surveille donc rien ici.",
  "perch.alerts.quiet":
    "{count, plural, one {# règle surveille} other {# règles surveillent}} ce cluster, et aucune ne se déclenche.",
  "perch.alerts.firingOne": "{rule} se déclenche depuis {time}. {detail}",
  "perch.alerts.firingMany":
    "{count, plural, one {# règle se déclenche} other {# règles se déclenchent}} sur ce cluster en ce moment. La plus ancienne est {rule}, depuis {time}.",
  "perch.alerts.unread":
    "Kavka n'a pas pu lire les règles d'alerte de cette connexion et ne peut donc pas dire ce qui est surveillé — ni si quoi que ce soit l'est.",
  "perch.alerts.unreadHistory":
    "Kavka n'a pas pu lire le journal d'alertes de cette connexion et ne peut donc pas dire si quelque chose se déclenche en ce moment — ni si quoi que ce soit s'est jamais déclenché.",
  "perch.alerts.caveat.desktop":
    "Kavka doit être en cours d'exécution pour remarquer quoi que ce soit. Fermez la fenêtre et plus rien n'est surveillé — c'est une application de bureau, pas un service.",
  "perch.alerts.caveat.silent":
    "Aucun canal n'est activé : un déclenchement n'atteint donc que cette fenêtre et le journal ci-dessous. Rien ne vous parviendra quand Kavka n'est pas devant vous.",
  "perch.masking.none":
    "Aucune règle de masquage sur cette connexion : tout ce que Kavka vous montre est exactement ce que le producteur a envoyé.",
  "perch.masking.inForce":
    "{count, plural, one {# règle de masquage est} other {# règles de masquage sont}} en vigueur : le texte correspondant est remplacé avant même d'atteindre cette fenêtre.",
  "perch.masking.off":
    "{count, plural, one {# règle de masquage existe} other {# règles de masquage existent}} et aucune n'est activée : rien n'est donc masqué à l'écran.",
  "perch.masking.unread":
    "Kavka n'a pas pu lire les règles de masquage de cette connexion : il ne peut donc pas garantir que ce que vous voyez est verbatim.",
  "perch.masking.caveat":
    "Une règle que vous activez maintenant s'applique à la prochaine récupération, au prochain lot de suivi, à la prochaine recherche ou requête — jamais aux lignes déjà affichées.",
  "perch.masking.caveat.sawMasked":
    "Quelque chose à l'écran a déjà été masqué durant cette session : au moins une charge utile ici n'est pas ce que le producteur a envoyé.",
  "perch.streams.noGroups":
    "Ce cluster n'a pas encore de groupes de consommateurs : il n'y a donc rien dont déduire une topologie.",
  "perch.streams.pick":
    "Choisissez une application ci-dessus et Kavka déduira ce qu'elle lit, ce qu'elle écrit et ce qu'elle conserve entre les deux.",
  "perch.streams.notStreams":
    "{group} ne ressemble pas à une application Kafka Streams : il n'y a donc aucune topologie à dessiner. Qu'un groupe de consommateurs ordinaire n'en ait pas n'est pas une anomalie.",
  "perch.streams.inferred":
    "Cette image de {app} est une supposition : {nodes, plural, one {# nœud} other {# nœuds}} et {edges, plural, one {# lien} other {# liens}}, déduits des noms de topics.",
  "perch.streams.unread":
    "Kavka n'a pas pu déduire de topologie pour {group} et n'a donc rien à montrer. Le message ci-dessous est ce qu'a répondu le cluster.",
  "perch.streams.caveat":
    "Kafka ne publie nulle part une topologie Streams qu'un client pourrait lire. Rien ici n'a été lu depuis l'application elle-même : un processeur qui ne laisse aucun topic derrière lui n'apparaît donc pas du tout.",
  "perch.acls.noAuthorizer":
    "Ce cluster n'a pas d'autorisateur : il n'y a donc aucune règle d'accès à lister et chaque requête est tranchée par la valeur par défaut des brokers.",
  "perch.acls.noAuthorizerNext":
    "C'est un réglage de broker (authorizer.class.name), pas une permission qui vous manque — Kafka refuse la requête purement et simplement au lieu de répondre par une liste vide.",
  "perch.acls.none":
    "Ce cluster a un autorisateur mais pas encore de règles d'accès : le sort d'une requête dépend donc entièrement de la valeur par défaut des brokers.",
  "perch.acls.allAllow":
    "{count, plural, one {# règle d'accès} other {# règles d'accès}} sur ce cluster, et chacune est une autorisation.",
  "perch.acls.someDeny":
    "{count, plural, one {# règle d'accès} other {# règles d'accès}} sur ce cluster. {denies, plural, one {# d'entre elles est un refus} other {# d'entre elles sont des refus}}, et un refus l'emporte sur toute autorisation correspondant à la même requête.",
  "perch.acls.filtered":
    "Affichage de {count, plural, one {# règle} other {# règles}} correspondant à ce filtre.",
  "perch.acls.unread":
    "Kavka n'a pas pu lire les règles d'accès de ce cluster et ne peut donc pas dire qui a le droit de faire quoi. Pour les lister, le compte a normalement besoin de Describe sur le cluster.",
  "perch.acls.caveat.filtered":
    "Un filtre est actif : ceci compte donc les règles qui y correspondent — pas les règles du cluster.",
  "perch.acls.caveat.removing":
    "Supprimer un refus élargit l'accès au lieu de le restreindre. Kavka le redit avant d'en supprimer un.",

  // ── Écrans du cluster (Jackdaw) ─────────────────────────────────────────
  "topics.partitions.detail": "Afficher le détail des réplicas",
  "topics.partitions.detailTitle":
    "Ajoute la liste des réplicas, la liste des réplicas synchronisés et le premier et le dernier offset de chaque partition. L'état reste affiché dans les deux cas.",
  "acls.filter.summary": "Filtrer ces règles",
  "acls.filter.note": "par type de ressource, nom de ressource et principal",
  "acls.filter.active": "un filtre est actif",
  "alerts.state.firing": "Déclenchée",
  "alerts.since": "depuis {time}",
  "alerts.details.summary": "Détails",
  "alerts.details.note":
    "ce que Kavka compare exactement, et à quelle fréquence",
  "alerts.facts.kind": "Type",
  "alerts.facts.waitsFor": "Attend",
  "alerts.facts.noWait":
    "rien — elle se déclenche dès que la condition est vraie",
  "alerts.facts.checked": "Vérifiée",
  "alerts.facts.checkedValue":
    "à chaque relevé pris par Kavka, et uniquement tant que Kavka est ouvert",
  "alerts.facts.since": "Déclenchée depuis",
  "alerts.history.started": "{rule} — a commencé",
  "alerts.history.cleared": "{rule} — terminée",
  "alerts.history.lasted": "Terminée à {time}, après {duration}.",
  "alerts.history.stillFiring":
    "Toujours déclenchée, {duration} pour l'instant.",
  "alerts.history.gap":
    "Ce journal ne couvre que le temps où Kavka était ouvert. Un trou dedans correspond à une période où personne ne regardait, et Kavka ne devinera pas ce qui s'y est passé.",
  "monitoring.tile.lagNow": "Retard au dernier relevé",
  "monitoring.tile.lagNowSub":
    "messages en attente de lecture lors du dernier échantillonnage de Kavka",
  "monitoring.tile.peak": "Pic dans cette fenêtre",
  "monitoring.tile.peakSub":
    "le pire relevé unique pris par Kavka, jamais une moyenne",
  "monitoring.tile.trend": "Tendance",
  "monitoring.tile.trendSub": "par rapport au début de cette fenêtre",
  "monitoring.tile.partitionsSub": "avec au moins un relevé dans cette fenêtre",

  // ── Réglages (Jackdaw) ──────────────────────────────────────────────────
  "settings.title": "Réglages",
  "settings.navLabel": "Sections des réglages",
  "settings.perch":
    "Tout ici s’applique au fur et à mesure et reste sur cette machine. Kavka affiche actuellement le thème {theme}.",
  "settings.section.appearance": "Apparence",
  "settings.section.language": "Langue",
  "settings.section.about": "À propos",

  "settings.theme.title": "Thème",
  "settings.theme.help":
    "Système suit votre système d’exploitation et change avec lui tant que Kavka est ouvert.",
  "settings.theme.system": "Système",
  "settings.theme.light": "Clair",
  "settings.theme.dark": "Sombre",

  "settings.accent.title": "Couleur d’accent",
  "settings.accent.help":
    "La couleur des boutons, des liens et de l’écran où vous êtes. Elle ne porte aucun sens propre : la changer ne peut donc pas masquer un avertissement.",
  "settings.accent.brass": "Laiton",
  "settings.accent.moss": "Mousse",
  "settings.accent.sky": "Ciel",
  "settings.accent.plum": "Prune",

  "settings.density.title": "Densité",
  "settings.density.help":
    "Confortable donne de l’air à chaque ligne. Compacte affiche environ un tiers de lignes en plus — la hauteur livrée à l’origine.",
  "settings.density.comfortable": "Confortable",
  "settings.density.compact": "Compacte",

  "settings.font.title": "Taille du texte",
  "settings.font.help":
    "Met toutes les tailles à l’échelle ensemble, pour que rien ne se chevauche au plus grand cran.",
  "settings.font.s": "Petite",
  "settings.font.m": "Moyenne",
  "settings.font.l": "Grande",

  "settings.motion.title": "Animations",
  "settings.motion.help":
    "Système suit le réglage « réduire les animations » de votre système. Réduites coupe en plus toutes les transitions de Kavka.",
  "settings.motion.system": "Système",
  "settings.motion.reduce": "Réduites",

  "settings.language.title": "Langue",
  "settings.language.help":
    "Couvre l'ossature de Kavka et le verdict par lequel s'ouvre chaque écran du cluster — la barre latérale, la palette, ce panneau, le formulaire de connexion et la phrase d'ouverture de chaque écran. Les tableaux et formulaires en dessous sont encore en anglais.",
  "settings.language.machine":
    "Ce catalogue sort d’une machine et aucun locuteur natif ne l’a relu. Les corrections sont bienvenues.",

  "settings.about.title": "Version, licence et diagnostics",
  "settings.about.help":
    "Le panneau À propos contient la version et la licence de Kavka, les réglages du serveur MCP et l’interrupteur de diagnostic de plantage.",
  "settings.about.open": "Ouvrir À propos",

  "unit.seconds": "{count, plural, one {# seconde} other {# secondes}}",
  "unit.minutes": "{count, plural, one {# minute} other {# minutes}}",
  "unit.hours": "{count, plural, one {# heure} other {# heures}}",
  "unit.days": "{count, plural, one {# jour} other {# jours}}",
};

export default fr;
