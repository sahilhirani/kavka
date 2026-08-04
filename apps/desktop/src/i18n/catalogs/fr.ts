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
    "L'ossature de Kavka — la barre latérale, la palette de commandes, ces boîtes de dialogue et le formulaire de connexion. Les vues de cluster sont encore en anglais ; c'est la prochaine chose à traduire.",
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

  "unit.seconds": "{count, plural, one {# seconde} other {# secondes}}",
  "unit.minutes": "{count, plural, one {# minute} other {# minutes}}",
  "unit.hours": "{count, plural, one {# heure} other {# heures}}",
  "unit.days": "{count, plural, one {# jour} other {# jours}}",
};

export default fr;
