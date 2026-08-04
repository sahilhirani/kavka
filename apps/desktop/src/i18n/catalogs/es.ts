// ─────────────────────────────────────────────────────────────────────────────
// Spanish (Español) — MACHINE TRANSLATION — NATIVE REVIEW WELCOME.
//
// No native speaker has read this file. It was produced from `en.ts` and it is
// shipped honestly rather than quietly: the language picker in the About
// dialog says so next to the name, and `LOCALES` in ../index.ts carries
// `machine: true` for exactly this reason.
//
// If Spanish is your language, the highest-value contribution to Kavka is
// twenty minutes with this file. See docs/I18N.md — you need no build, no
// tooling and no account, and a partial fix is welcome: any key you delete
// falls back to English rather than breaking.
//
// Two things to keep while editing: the {placeholders} (they are values Kavka
// substitutes, and a renamed one silently disappears from the sentence), and
// the plural arms — Spanish is one/other, so `one {# conexión} other
// {# conexiones}` is the shape.
//
// This catalog uses neutral (non-regional) Spanish and the formal "usted".
// ─────────────────────────────────────────────────────────────────────────────

import type { Catalog } from "./en";

const es: Catalog = {
  "common.close": "Cerrar",
  "common.cancel": "Cancelar",
  "common.save": "Guardar",
  "common.connect": "Conectar",
  "common.tryAgain": "Intentar de nuevo",
  "common.remove": "Quitar",
  "common.dismiss": "Descartar",
  "common.showDetails": "Ver detalles",
  "common.addConnection": "Añadir conexión",
  "common.support": "Apoyar a Kavka ☕",
  "common.readingConnections": "Leyendo sus conexiones guardadas…",
  "common.linkFailed":
    "Kavka no pudo pasar ese enlace a su navegador. La dirección es {url} — cópiela desde aquí.",

  "sidebar.navLabel": "Conexiones guardadas",
  "sidebar.title": "Clústeres",
  "sidebar.loading": "Leyendo sus conexiones…",
  "sidebar.empty": "Todavía no hay nada. Añada su primera conexión abajo.",
  "sidebar.profileMeta": "{address} · {status}",
  "sidebar.status.disconnected": "sin conexión",
  "sidebar.status.connecting": "conectando…",
  "sidebar.status.connected": "conectado",
  "sidebar.draftName": "Nueva conexión",
  "sidebar.draftMeta": "todavía sin guardar",
  "sidebar.about": "Acerca de",

  "app.status.disconnected": "Sin conexión",
  "app.status.connecting": "Conectando…",
  "app.status.connected": "Conectado",
  "app.error.unknownProfile":
    "Esa conexión ya no está en este equipo. Es posible que se haya eliminado en otra ventana.",
  "app.profilesFailed.title": "Kavka no pudo leer su archivo de conexiones",
  "app.profilesFailed.hint":
    "Sus conexiones siguen en el disco — no se ha perdido nada. Kavka las guarda en su carpeta de configuración, junto a los ajustes de esta aplicación.",
  "app.firstRun.title": "Apunte Kavka a un broker",
  "app.firstRun.what":
    "Una conexión es una dirección guardada para un único clúster de Kafka — un nombre, un broker desde el que empezar y cómo autenticarse. Kavka descubre el resto del clúster a partir de ahí.",
  "app.firstRun.example":
    "Un servidor bootstrap suele tener este aspecto: {example}. ¿Está ejecutando el clúster de desarrollo de este repositorio? Use {local}.",
  "app.firstRun.footnote":
    "Las contraseñas van al llavero de su sistema operativo. Nada sobre sus clústeres sale de este equipo.",
  "app.pick.title": "Elija una conexión",
  "app.pick.hint":
    "Elija un clúster a la izquierda para ver sus brokers y topics, o añada otra conexión.",
  "app.readonlyChip": "solo lectura",
  "app.readonlyTitle":
    "Esta conexión es de solo lectura. Desactívelo en los ajustes de la conexión para producir o editar.",
  "app.statusbar.draft": "Nueva conexión — todavía sin guardar",
  "app.statusbar.none": "Ninguna conexión seleccionada",
  "app.statusbar.commands": "comandos",
  "app.statusbar.coreVersion": "core v{version}",
  "app.cmd.search": "Buscar en {topic}",
  "app.cmd.search.kw":
    "find filter cel scan query messages grep buscar filtrar mensajes",
  "app.cmd.sql": "Consultar {topic} con SQL",
  "app.cmd.sql.kw":
    "sql select query aggregate count group datafusion analyse consulta contar analizar",
  "app.cmd.produce": "Producir en {topic}",
  "app.cmd.produce.kw":
    "send write publish message record bulk producer enviar escribir publicar mensaje",
  "app.cmd.produce.confirmContext": "{cluster} · pide confirmación",

  "palette.label": "Comandos",
  "palette.searchLabel": "Buscar comandos y clústeres",
  "palette.searchPlaceholder": "Buscar comandos y clústeres…",
  "palette.empty":
    "Nada coincide con «{query}». Pruebe con el nombre de un clúster, o vacíe el campo para ver todo lo que Kavka puede hacer.",
  "palette.foot.move": "moverse",
  "palette.foot.run": "ejecutar",
  "palette.foot.close": "cerrar",
  "palette.goTo": "Ir a {name}",
  "palette.connectTo": "Conectar con {name}",
  "palette.state.connected": "conectado",
  "palette.state.connecting": "conectando…",
  "palette.protectedCluster": "clúster protegido",
  "palette.profile.kw":
    "connect open switch cluster broker bootstrap conectar abrir cambiar",
  "palette.add.context": "Un nombre, un broker y cómo autenticarse",
  "palette.add.kw":
    "new connection profile cluster create bootstrap broker nueva conexión crear",
  "palette.disconnect": "Desconectar",
  "palette.disconnect.kw": "close leave cluster session desconectar salir cerrar",
  "palette.disconnect.none": "Ahora mismo no hay nada conectado",
  "palette.disconnect.ambiguous":
    "Elija primero en la barra lateral el clúster que quiere desconectar",
  "palette.refresh": "Recargar topics",
  "palette.refresh.kw":
    "reload metadata list topics partitions cluster recargar actualizar",
  "palette.export": "Exportar conexiones…",
  "palette.export.context": "Todas las conexiones de este equipo, en JSON",
  "palette.export.kw":
    "backup save copy share json profiles copia exportar copiar",
  "palette.import": "Importar conexiones…",
  "palette.import.context": "Pegar JSON de otra copia de Kavka",
  "palette.import.kw":
    "restore paste load json profiles pegar cargar restaurar",
  "palette.about.context": "Versión y licencia",
  "palette.about.kw":
    "version licence license agpl source github help licencia ayuda código fuente",
  "palette.support.context":
    "Kavka es gratuito — las donaciones lo mantienen así",
  "palette.support.kw":
    "donate coffee sponsor fund open source donar café apoyar financiar",

  "about.title": "Acerca de Kavka",
  "about.body":
    "Un cliente de escritorio para Apache Kafka. Kavka se ejecuta por completo en este equipo: las contraseñas van al llavero de su sistema operativo, y nada sobre sus clústeres sale de este ordenador.",
  "about.coreVersion": "Versión del núcleo",
  "about.versionLoading": "Leyéndola ahora…",
  "about.licence": "Licencia",
  "about.licenceValue": "Software libre y de código abierto bajo AGPL-3.0",
  "about.language": "Idioma",
  "about.language.hint":
    "El armazón de Kavka — la barra lateral, la paleta de comandos, estos diálogos y el formulario de conexión. Las vistas de clúster siguen en inglés; son lo siguiente que se traducirá.",
  "about.language.machine":
    "{language} se tradujo automáticamente y ningún hablante nativo lo ha revisado. Las correcciones son bienvenidas — docs/I18N.md explica cómo.",

  "transfer.title": "Conexiones",
  "transfer.tablist": "Exportar o importar",
  "transfer.tab.export": "Exportar",
  "transfer.tab.import": "Importar",
  "transfer.export.body":
    "Todas las conexiones de este equipo, en JSON. Péguelo en otra copia de Kavka para configurar allí los mismos clústeres.",
  "transfer.export.promise":
    "Las contraseñas y las claves nunca salen de este equipo — las exportaciones llevan referencias, no secretos.",
  "transfer.export.failed":
    "Kavka no pudo leer su archivo de conexiones. Sus conexiones siguen en el disco — no se ha perdido nada.",
  "transfer.export.label": "Sus conexiones, en JSON",
  "transfer.export.copied": "Copiado al portapapeles.",
  "transfer.export.copyManual":
    "Kavka no pudo acceder al portapapeles. El texto está seleccionado — pulse {key} para copiarlo.",
  "transfer.export.copy": "Copiar al portapapeles",
  "transfer.export.nothingToCopy":
    "No hay nada que copiar — Kavka no pudo leer su archivo de conexiones",
  "transfer.export.stillReading": "Kavka todavía está leyendo sus conexiones",
  "transfer.import.body":
    "Pegue una exportación de otra copia de Kavka. Las contraseñas no están en ella — cada conexión importada pedirá la suya la primera vez que se conecte.",
  "transfer.import.label": "JSON exportado",
  "transfer.import.kbd": "importar",
  "transfer.import.kbdClose": "cerrar",
  "transfer.import.legend": "Si una conexión ya está aquí",
  "transfer.import.skip": "Conservar la de este equipo",
  "transfer.import.skipHint":
    "Las conexiones ya guardadas aquí se dejan exactamente como están. Todo lo nuevo del JSON se añade igualmente.",
  "transfer.import.replace": "Sustituirla por la del JSON",
  "transfer.import.replaceHint":
    "Gana la versión pegada — nombre, dirección, entorno y método de autenticación. Las contraseñas que ya están en su llavero se quedan donde están.",
  "transfer.import.failed":
    "Kavka no pudo leer eso como una exportación. Compruebe que pegó el archivo completo, incluidas las llaves exteriores — el texto que recibió Kavka está abajo.",
  "transfer.import.needsJson": "Pegue primero el JSON de una exportación",
  "transfer.import.busy": "Kavka está importando esas conexiones ahora",
  "transfer.import.run": "Importar conexiones",
  "transfer.import.running": "Importando…",
  "transfer.report.empty.title": "Ese JSON no tenía ninguna conexión",
  "transfer.report.empty.detail":
    "Compruebe que pegó la exportación completa, incluidas las llaves exteriores — Kavka la leyó bien, simplemente no había nada que añadir.",
  "transfer.report.added": "{count, plural, other {# añadidas}}",
  "transfer.report.replaced": "{count, plural, other {# sustituidas}}",
  "transfer.report.skipped":
    "{count, plural, other {# omitidas — ya estaban en este equipo}}",
  "transfer.report.envAdded":
    "{count, plural, one {# entorno añadido} other {# entornos añadidos}}",
  "transfer.report.envSkipped":
    "{count, plural, one {# entorno ya definido} other {# entornos ya definidos}}",
  "transfer.report.envOnly.title": "Ninguna conexión nueva: solo entornos",
  "transfer.report.unchanged.title":
    "{count, plural, one {No cambió nada — # conexión ya estaba aquí} other {No cambió nada — # conexiones ya estaban aquí}}",
  "transfer.report.unchanged.detail":
    "{bits}. Elija «Sustituirla por la del JSON» arriba si quería sobrescribirlas.",
  "transfer.report.imported.title":
    "{count, plural, one {# conexión importada} other {# conexiones importadas}}",
  "transfer.report.imported.detail":
    "{bits}. Las contraseñas no están en una exportación — abra cada conexión nueva e introduzca su contraseña antes de conectarse.",

  "editor.new.title": "Añadir una conexión",
  "editor.new.subtitle":
    "Basta con un broker para empezar — Kavka descubre el resto del clúster a partir de ahí.",
  "editor.saved.subtitle":
    "Sin conexión. Revise los datos de abajo y después conéctese.",
  "editor.name.label": "Nombre de la conexión",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "Lo que usted reconozca en la barra lateral. Solo lo ve Kavka.",
  "editor.env.label": "Entorno",
  "editor.env.hint.protected":
    "Este entorno está marcado como protegido: la regla del registro lleva su color en todas las tablas, la barra lateral marca este clúster, una barra de aviso ocupa la parte superior de la ventana y cada acción destructiva le pide escribir el nombre antes. Active abajo el modo de solo lectura salvo que realmente necesite escribir.",
  "editor.env.hint.other":
    "Kavka colorea cada vista según el entorno, para que no confunda un clúster con otro.",
  "editor.env.manage": "Gestionar entornos…",
  "editor.env.hint.unknown":
    "En este equipo nada define {name}, así que Kavka lo muestra en gris neutro y no aplica ninguna protección. Añádalo en «Gestionar entornos» para darle un color y decidir si está protegido.",
  "editor.bootstrap.label": "Servidores bootstrap",
  "editor.bootstrap.hint":
    "Cualquier broker de su clúster — Kavka encuentra el resto a partir de ahí. Uno por línea, o separados por comas. ¿Está ejecutando el clúster de desarrollo de este repositorio? Use {local}.",

  "editor.auth.legend": "Autenticación",
  "editor.auth.kerberos":
    "Esta conexión se autentica con Kerberos ({service} como {principal}), algo que Kavka todavía no sabe configurar. Al guardar se conserva exactamente igual; todos los demás campos siguen funcionando.",
  "editor.auth.label": "¿Cómo comprueba este clúster quién es usted?",
  "editor.auth.plaintext":
    "No lo comprueba — cualquiera puede conectarse (PLAINTEXT)",
  "editor.auth.saslPlain": "Usuario y contraseña — SASL/PLAIN",
  "editor.auth.saslScram": "Usuario y contraseña — SASL/SCRAM",
  "editor.auth.mtls": "Un certificado que presenta este equipo — mTLS",
  "editor.auth.mskIam": "Las credenciales de AWS de este equipo — MSK IAM",
  "editor.auth.oauth":
    "Un token de su proveedor de identidad — OAuth 2.0 / OIDC",
  "editor.auth.kerberosOption": "Un tique Kerberos — GSSAPI (todavía no)",
  "editor.auth.notYet":
    "Kavka todavía no sabe configurar esto. Una conexión que ya lo use seguirá funcionando y se conserva exactamente igual al guardar.",
  "editor.auth.hint":
    "Un Kafka gestionado suele pedir SASL/SCRAM con TLS activado. Un broker local no suele pedir nada. Kerberos es el único método que Kavka todavía no sabe configurar.",
  "editor.mechanism.label": "Mecanismo SCRAM",
  "editor.mechanism.hint":
    "Si el broker rechaza uno, le dirá cuál quiere.",
  "editor.username.label": "Usuario",
  "editor.password.label": "Contraseña",
  "editor.password.placeholder": "Contraseña",
  "editor.secret.unchanged": "••••••••  (sin cambios)",
  "editor.password.hint":
    "Va al llavero de su sistema operativo — nunca al archivo de conexiones, y nunca fuera de este equipo.",
  "editor.tls.label": "Cifrar la conexión (TLS)",
  "editor.tls.hint":
    "Un Kafka gestionado casi siempre lo necesita activado. Si el broker responde pero falla el saludo TLS, esto es lo primero que hay que probar.",

  "editor.mtls.hint":
    "Kavka lee los archivos PEM tal cual — no hay ningún almacén JKS o PKCS#12 que convertir antes.",
  "editor.caPath.label": "Certificado de la CA",
  "editor.caPath.hint":
    "Ruta al .pem de la CA — déjelo vacío para usar el almacén de confianza del sistema.",
  "editor.clientCert.label": "Certificado de cliente",
  "editor.clientCert.hint":
    "Ruta al certificado que este equipo muestra al broker — déjelo vacío si el broker no pide ninguno.",
  "editor.clientKey.label": "Clave privada del cliente",
  "editor.clientKey.hint":
    "Pegue la clave en sí, no una ruta a ella. Va al llavero de su sistema operativo — nunca al archivo de conexiones, y nunca fuera de este equipo.",
  "editor.clientKey.storedHint":
    "Déjelo vacío para conservar la clave guardada; si borra arriba la ruta del certificado, se elimina.",

  "editor.aws.hint":
    "Kavka firma cada petición con las credenciales de AWS que ya están en este equipo. Los servidores bootstrap de arriba tienen que ser el punto de acceso IAM de este clúster — los hosts {host} de la consola de MSK, normalmente en el puerto 9098.",
  "editor.region.label": "Región",
  "editor.region.hint":
    "La región de AWS en la que se ejecuta el clúster. Tiene que coincidir con los hosts bootstrap, o la firma no se aceptará.",
  "editor.awsProfile.label": "Nombre del perfil de AWS",
  "editor.awsProfile.hint":
    "Un perfil con nombre de {config}. Déjelo vacío para usar la cadena de credenciales por defecto — variables de entorno, luego {dir}, luego SSO.",

  "editor.oauth.hint":
    "Kavka pide un token a su proveedor de identidad con el flujo de credenciales de cliente y luego se lo presenta al broker como SASL/OAUTHBEARER.",
  "editor.tokenEndpoint.label": "Punto de acceso del token",
  "editor.tokenEndpoint.hint":
    "La URL que emite el token, no la página de inicio de sesión que usaría un navegador.",
  "editor.clientId.label": "ID de cliente",
  "editor.clientId.hint":
    "La aplicación que su proveedor de identidad registró para Kafka — no su propia cuenta de usuario.",
  "editor.clientSecret.label": "Secreto de cliente",
  "editor.clientSecret.placeholder": "Secreto de cliente",
  "editor.clientSecret.hint":
    "Va al llavero de su sistema operativo — nunca al archivo de conexiones, y nunca fuera de este equipo.",

  "editor.sr.legend": "Schema Registry (opcional)",
  "editor.sr.hint":
    "Si los mensajes de este clúster son Avro, Protobuf o JSON Schema, Kavka lee el esquema desde aquí para decodificarlos — y muestra el sujeto, la versión y el id junto a cada mensaje. Sin él, esas cargas se muestran como bytes en bruto.",
  "editor.srUrl.label": "Dirección del registro",
  "editor.srUrl.hint":
    "La URL completa, incluido el esquema. Confluent, Apicurio y Glue hablan aquí la misma API de lectura. Déjelo vacío si este clúster no tiene registro.",
  "editor.srUsername.label": "Usuario del registro",
  "editor.srUsername.hint":
    "Solo si el registro lo pide. Los registros gestionados suelen pedirlo; uno dentro de su propia red normalmente no.",
  "editor.srPassword.label": "Contraseña del registro",
  "editor.srPassword.storedHint":
    "Déjelo vacío para conservar la guardada; si borra arriba la dirección, se elimina.",

  "editor.connect.legend": "Clústeres de Kafka Connect (opcional)",
  "editor.connect.hint":
    "Kafka Connect ejecuta conectores de origen y de destino, y responde en su propio puerto REST en lugar de a través de los brokers — así que hay que decirle a Kavka dónde están los workers. Añada uno por grupo de workers; el nombre es como elegirá entre ellos en la pestaña Connect.",
  "editor.connect.unnamed": "Clúster {number}",
  "editor.connect.removeLabel": "Quitar {name}",
  "editor.connect.unnamedLong": "Clúster de Connect {number}",
  "editor.connect.remove": "Quitar este clúster de Connect de la conexión",
  "editor.connect.name.label": "Nombre",
  "editor.connect.name.placeholder": "orders connect",
  "editor.connect.name.hint":
    "Lo que usted reconozca. Cambiarle el nombre más adelante conserva su contraseña guardada.",
  "editor.connect.url.label": "Dirección de los workers",
  "editor.connect.url.hint":
    "El punto de acceso REST de cualquier worker del grupo — todos responden por el clúster entero. Normalmente el puerto 8083, y no el mismo host ni el mismo puerto que los brokers.",
  "editor.connect.username.hint":
    "Solo si los workers están detrás de autenticación básica. La mayoría no lo están.",
  "editor.connect.password.storedHint":
    "Déjelo vacío para conservar la guardada; si quita este clúster, se elimina.",
  "editor.connect.add": "Añadir un clúster de Connect",

  "editor.monitoring.legend": "Supervisión (opcional)",
  "editor.monitoring.hint":
    "Los brokers de Kafka no sirven las cifras de rendimiento, almacenamiento o replicación por el protocolo de Kafka — las publican como JMX, y casi todo el mundo pone delante un exportador de Prometheus. Apunte Kavka al exportador y la pestaña Supervisión se rellena. El histórico de lag no necesita nada de esto: Kavka lo lee él mismo de los brokers.",
  "editor.metricsUrl.label": "Dirección de las métricas",
  "editor.metricsUrl.hint":
    "La URL completa, incluida la ruta. Si usted opera los brokers, esto suele ser el agente Java {agent} en uno de ellos ({flag}). También sirve un servidor Prometheus que ya recopile de esos brokers — dele a Kavka su dirección. Déjelo vacío si este clúster no tiene exportador.",
  "editor.metricsUsername.label": "Usuario de las métricas",
  "editor.metricsUsername.hint":
    "Solo si el punto de acceso está detrás de autenticación básica. Un jmx_exporter normalmente no; un Prometheus compartido normalmente sí.",
  "editor.metricsPassword.label": "Contraseña de las métricas",
  "editor.metricsPassword.storedHint":
    "Déjelo vacío para conservar la guardada; si borra arriba la dirección, se elimina.",
  "editor.sampler.label": "Tomar una lectura de lag cada",
  "editor.sampler.hint":
    "Segundos. Kafka no recuerda el lag, así que Kavka toma su propia lectura en este intervalo y guarda {days} en un archivo de este equipo. {warning} El mínimo es {floor}; el valor por defecto es {default}, que cuesta una petición pequeña por grupo y lectura.",
  "editor.sampler.warning":
    "Las lecturas solo se producen mientras esta conexión está activa — no se recopila nada mientras Kavka está cerrado o este clúster está desconectado, y un hueco en el gráfico significa exactamente eso.",

  "editor.readonly.label": "Conexión de solo lectura",
  "editor.readonly.hint":
    "Kavka seguirá mostrándolo todo, pero no producirá mensajes, no cambiará topics ni confirmará offsets por esta conexión.",

  "editor.busy.connecting": "Espere a que termine el intento de conexión",
  "editor.busy.saving": "Kavka está guardando esta conexión",
  "editor.delete": "Eliminar conexión",
  "editor.delete.confirm":
    "¿Quitar {name} de este equipo? El clúster en sí no se toca.",
  "editor.kbd.connect": "conectar",
  "editor.kbd.cancel": "cancelar",
  "editor.kbd.undo": "deshacer cambios",

  "editor.err.name":
    "Dele un nombre a esta conexión para poder encontrarla en la barra lateral.",
  "editor.err.bootstrap":
    "Añada al menos un broker, como host:puerto — p. ej. broker-1:9092",
  "editor.err.srUrl":
    "Use la URL completa, empezando por http:// o https:// — p. ej. http://localhost:8081",
  "editor.err.srUserNoUrl":
    "Añada la dirección del registro, o borre el usuario — no se puede guardar una autenticación sin destino.",
  "editor.err.metricsUrl":
    "Use la URL completa, empezando por http:// o https:// — p. ej. http://broker-1.internal:7071/metrics",
  "editor.err.metricsUserNoUrl":
    "Añada la dirección de las métricas, o borre el usuario — no se puede guardar una autenticación sin destino.",
  "editor.err.sampler":
    "Muestree como mucho cada {seconds, plural, one {# segundo} other {# segundos}}. Más rápido pide offsets a los brokers más a menudo de lo que cambian.",
  "editor.err.connectName":
    "Dele un nombre a este clúster de Connect — cada acción que envía Kavka nombra el clúster al que va.",
  "editor.err.connectDuplicate":
    "Dos clústeres de Connect en una conexión no pueden compartir nombre — Kavka guarda sus contraseñas bajo él.",
  "editor.err.connectUrlMissing":
    "Añada la dirección REST de los workers — p. ej. http://connect-1.internal:8083",
  "editor.err.connectUrl":
    "Use la URL completa, empezando por http:// o https:// — p. ej. http://connect-1.internal:8083",
  "editor.err.username":
    "Este método de autenticación necesita el usuario con el que le conoce el broker.",
  "editor.err.password": "Este método de autenticación necesita una contraseña.",
  "editor.err.clientKey":
    "Pegue la clave privada que va con ese certificado — Kavka necesita las dos mitades.",
  "editor.err.clientCert":
    "Añada la ruta al certificado al que pertenece esta clave — Kavka necesita las dos mitades.",
  "editor.err.region":
    "Indique la región en la que se ejecuta el clúster — p. ej. eu-west-1",
  "editor.err.tokenEndpoint":
    "Añada la URL en la que su proveedor de identidad emite los tokens — p. ej. https://login.example.com/oauth2/token",
  "editor.err.tokenEndpointUrl":
    "Use la URL completa, empezando por https:// — p. ej. https://login.example.com/oauth2/token",
  "editor.err.clientId":
    "Añada el id de cliente que su proveedor de identidad emitió para esta aplicación.",
  "editor.err.clientSecret":
    "Este método de autenticación necesita el secreto que va con ese id de cliente.",


  // ── Entornos ────────────────────────────────────────────────────────────
  "env.color.green": "verde",
  "env.color.amber": "ámbar",
  "env.color.red": "rojo",
  "env.color.blue": "azul",
  "env.color.violet": "violeta",
  "env.color.cyan": "cian",
  "env.color.slate": "pizarra",

  "env.mgr.title": "Entornos",
  "env.mgr.intro":
    "Nombre los entornos que su organización realmente utiliza. El color los distingue de un vistazo; «protegido» es la barrera de seguridad.",
  "env.mgr.failed": "Eso no se completó",
  "env.mgr.working": "Kavka está trabajando en ello",
  "env.mgr.add": "Añadir entorno",
  "env.mgr.edit": "Editar",

  "env.mgr.row.protected": "protegido",
  "env.mgr.row.unprotected": "sin proteger",
  "env.mgr.row.used":
    "{count, plural, =0 {ninguna conexión} one {# conexión} other {# conexiones}}",

  "env.mgr.name.label": "Nombre",
  "env.mgr.name.hint":
    "Como lo llame su equipo: dev, QA, UAT, producción. Se muestra exactamente como lo escriba, y nunca se traduce.",
  "env.mgr.name.taken": "Ya existe un entorno con este nombre.",
  "env.mgr.name.required": "Primero dé un nombre al entorno",

  "env.mgr.color.label": "Color",
  "env.mgr.color.hint":
    "Solo identidad. El color tiñe la regla del registro y la etiqueta; nunca decide lo que Kavka le permite hacer.",

  "env.mgr.protected.label": "Tratar este entorno como protegido",
  "env.mgr.protected.hint":
    "Kavka cambia al sustrato de aviso, le pide escribir el nombre del tema o del grupo antes de cualquier acción destructiva, marca la ventana y rechaza las escrituras desde la línea de comandos y desde los asistentes de IA salvo que se les indique explícitamente lo contrario.",
  "env.mgr.unprotect.prompt": "Escriba {name} para quitarle la protección",
  "env.mgr.unprotect.hint":
    "Todas las conexiones de {name} pierden sus barreras: sin confirmaciones escritas, y la línea de comandos y los asistentes de IA dejan de rechazar escrituras.",

  "env.mgr.delete.title": "¿Eliminar {name}?",
  "env.mgr.delete.unused":
    "Ninguna conexión usa {name}, así que no cambia nada más.",
  "env.mgr.delete.used":
    "{count, plural, one {# conexión usa} other {# conexiones usan}} {name}. Elija adónde van: Kavka las mueve antes de eliminarlo.",
  "env.mgr.delete.moveTo": "Mover esas conexiones a",
  "env.mgr.delete.moveHint": "Estas conexiones se moverán: {names}.",
  "env.mgr.delete.confirm": "Eliminar entorno",
  "env.mgr.delete.needTarget": "Elija un entorno al que mover esas conexiones.",
  "env.mgr.delete.last": "Es el único entorno que queda: añada otro primero",

  "unit.seconds": "{count, plural, one {# segundo} other {# segundos}}",
  "unit.minutes": "{count, plural, one {# minuto} other {# minutos}}",
  "unit.hours": "{count, plural, one {# hora} other {# horas}}",
  "unit.days": "{count, plural, one {# día} other {# días}}",
};

export default es;
