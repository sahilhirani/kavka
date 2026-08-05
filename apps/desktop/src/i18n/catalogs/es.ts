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

  "confirm.kicker.destructive": "Destructiva",
  "confirm.busy": "Kavka está trabajando en ello",
  "confirm.type.label": "Escribe {name} para confirmar",
  "confirm.type.reason": "Escribe {name} exactamente para confirmar esto",

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
  "sidebar.settings": "Ajustes",

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
    "El armazón de Kavka y el veredicto con el que abre cada pantalla del clúster: la barra lateral, la paleta de comandos, estos diálogos, el formulario de conexión y la frase inicial de cada pantalla. Las tablas y formularios que hay debajo siguen en inglés.",
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

  "editor.perch.screen": "Conexión",
  "editor.perch.new":
    "Todavía no hay nada guardado: Kavka no ha contactado con ningún broker, así que no se ha comprobado nada de esta pantalla.",
  "editor.perch.saved":
    "Guardada, pero sin conectar. Kavka aún no ha hablado con {name}, así que ninguno de estos datos se ha comprobado contra el clúster.",
  "editor.perch.connected":
    "Conectada a {name}. Kavka todavía está leyendo el resumen del clúster.",
  "editor.perch.caveat.protected":
    "{name} está marcado como protegido: cada acción destructiva en este clúster te pide escribir su nombre primero.",
  "editor.perch.caveat.unknown":
    "Nada en esta máquina define {name}, así que a esta conexión no se le aplica ninguna protección.",
  "editor.perch.caveat.readonly":
    "Solo lectura está activado: Kavka explorará este clúster pero nunca escribirá en él.",
  "editor.cluster.legend": "El clúster",
  "editor.guardrails.legend": "Protecciones",
  "editor.fold.set": "Configurado",
  "editor.fold.notSet": "Sin configurar",
  "editor.fold.connectCount":
    "{count, plural, one {# clúster} other {# clústeres}}",


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

  // ── Navegación del clúster (Jackdaw) ────────────────────────────────────
  "rail.label": "Pantallas del clúster",
  "rail.group.cluster": "Clúster",
  "rail.group.observe": "Observar",
  "rail.group.safety": "Seguridad",
  "rail.group.integrations": "Integraciones",
  "rail.item.overview": "Inicio",
  "rail.item.topics": "Topics",
  "rail.item.groups": "Grupos de consumo",
  "rail.item.brokers": "Brokers",
  "rail.item.monitoring": "Monitorización",
  "rail.item.alerts": "Alertas",
  "rail.item.streams": "Streams",
  "rail.item.acls": "ACL",
  "rail.item.masking": "Enmascarado",
  "rail.item.connect": "Connect",
  "rail.firing": "activa",
  "rail.firingTitle":
    "{count, plural, one {# regla de alerta está activa ahora mismo} other {# reglas de alerta están activas ahora mismo}}",
  "rail.disconnect": "Desconectar",

  // ── La percha (Jackdaw) ─────────────────────────────────────────────────
  "perch.label": "{screen} — lo que Kavka puede decirte",
  "perch.kicker": "{screen} · {state}",
  "perch.state.ok": "Parece sano",
  "perch.state.watch": "Merece un vistazo",
  "perch.state.problem": "Algo va mal",
  "perch.state.unknown": "Aún no está claro",
  "perch.state.checking": "Comprobando",
  "perch.checking":
    "Aún comprobando — Kavka dirá lo que encuentre en cuanto responda el clúster.",
  "perch.overview.counts":
    "Conectado a {brokers, plural, one {# broker} other {# brokers}}, con {topics, plural, one {# topic} other {# topics}} repartidos en {partitions, plural, one {# partición} other {# particiones}}.",
  "perch.overview.firing":
    "{count, plural, one {# regla de alerta está activa} other {# reglas de alerta están activas}} en este clúster ahora mismo. {counts}",
  "perch.overview.snapshot":
    "Estas cifras llegaron al conectar y no siguen al clúster: vuelve a conectar para tomarlas de nuevo.",
  "perch.overview.noBrokers":
    "El clúster respondió, pero no nombró ningún broker.",
  "perch.overview.noBrokers.next":
    "Suele significar que has llegado a un balanceador en vez de a Kafka, o que los metadatos volvieron vacíos. Desconecta, vuelve a conectar y revisa la dirección de bootstrap.",
  "perch.screen.messages": "Mensajes",
  "perch.screen.search": "Búsqueda",
  "perch.screen.sql": "SQL",
  "perch.screen.schemas": "Esquemas",
  "perch.topics.unreadable":
    "Kavka no tiene ninguna lista de los topics de este clúster.",
  "perch.topics.unreadable.next":
    "La conexión puede estar activa aunque la cuenta no tenga Describe sobre el clúster. Actualizar lo vuelve a pedir.",
  "perch.topics.empty":
    "Este clúster no tiene ningún topic: todavía no se ha creado ninguno en él.",
  "perch.topics.internalOnly":
    "Todo lo que hay en este clúster es un topic interno del propio Kafka. Activa Mostrar internos para verlos.",
  "perch.topics.counts":
    "{count, plural, one {# topic en este clúster} other {# topics en este clúster}}.",
  "perch.topics.countsHidden":
    "{count, plural, one {# topic mostrado} other {# topics mostrados}}.",
  "perch.topics.hiddenNote":
    "{count, plural, one {# más es un topic interno del propio Kafka y está oculto} other {# más son topics internos del propio Kafka y están ocultos}}.",
  "perch.topics.snapshot":
    "Esta lista se leyó al abrir la pantalla y no sigue al clúster: Actualizar la vuelve a leer.",
  "perch.topics.readOnly":
    "Esta conexión es de solo lectura, así que nada de aquí puede crear, cambiar ni borrar un topic.",
  "perch.topic.unreadable":
    "Kavka no tiene la lista de particiones de {topic}, así que no puede decir qué contiene.",
  "perch.topic.unreadable.next":
    "Puede que el topic se haya borrado, o que la cuenta no tenga Describe sobre él.",
  "perch.topic.underReplicated":
    "{count, plural, one {a # partición de aquí le falta una copia} other {a # particiones de aquí les faltan copias}}: Kafka mantiene menos réplicas de las que pide este topic.",
  "perch.topic.unpreferred":
    "{count, plural, one {# partición está liderada} other {# particiones están lideradas}} por un broker que no es el primero de su lista de réplicas. Es lo normal tras un reinicio, y Elegir líderes preferidos las devuelve a su sitio.",
  "perch.topic.healthy":
    "{count, plural, one {# partición} other {# particiones}}, con todas las copias sincronizadas.",
  "perch.topic.records": "Unos {records} mensajes según los offsets.",
  "perch.topic.approx":
    "Ese número de mensajes es la diferencia entre el offset más antiguo y el más reciente de cada partición, así que aún cuenta registros que la retención o la compactación ya han eliminado.",
  "perch.messages.waiting":
    "Todavía no se ha leído nada. Elige arriba desde dónde leer y pulsa Obtener.",
  "perch.messages.range":
    "{count, plural, one {# mensaje} other {# mensajes}} del rango que has pedido.",
  "perch.messages.none": "No hay nada en el rango que has pedido.",
  "perch.messages.topicEmpty": "{topic} todavía no contiene ningún mensaje.",
  "perch.messages.live":
    "Observando {topic} en vivo: {count, plural, one {ha llegado # mensaje} other {han llegado # mensajes}} desde que empezó el seguimiento.",
  "perch.messages.liveQuiet":
    "Observando {topic} en vivo. No se ha producido nada en él durante al menos treinta segundos.",
  "perch.messages.notWhole":
    "Esto es el trozo que has pedido, no el topic entero: {topic} contiene unos {total} mensajes.",
  "perch.messages.dropped":
    "{count, plural, one {# mensaje llegó} other {# mensajes llegaron}} más rápido de lo que esta ventana podía asimilar, y la sesión lo descartó en vez de quedarse atrás: las filas en pantalla no son todo lo que vio el seguimiento.",
  "perch.messages.trimmed":
    "Kavka conserva las últimas {cap} filas en vivo; todo lo anterior ya ha salido del búfer.",
  "perch.messages.masked":
    "Hay reglas de enmascarado activadas, así que algunos valores en pantalla no son los del topic. Las copias y las exportaciones llevan los reemplazos.",
  "perch.search.waiting":
    "Todavía no se ha explorado nada. Fija el alcance, di qué buscas y pulsa Buscar.",
  "perch.search.running":
    "Explorando {topic}: {count, plural, one {# coincidencia} other {# coincidencias}} hasta ahora.",
  "perch.search.running.note":
    "Parcial. Estos números seguirán cambiando hasta que termine la exploración.",
  "perch.search.matches":
    "{count, plural, one {# coincidencia} other {# coincidencias}} en los {scanned} registros que ha leído esta exploración.",
  "perch.search.none":
    "No ha coincidido nada en los {scanned} registros que ha leído esta exploración.",
  "perch.search.stopped":
    "Has detenido esta exploración tras {scanned} registros, así que responde sobre una parte del rango y no sobre todo él.",
  "perch.search.capped":
    "Han coincidido {matched} registros pero Kavka ha conservado {kept}. Ordenar, exportar o contar lo que hay en pantalla responde sobre esos, no sobre todas las coincidencias.",
  "perch.search.unevaluated":
    "{count, plural, one {# registro no se ha podido leer} other {# registros no se han podido leer}} contra tu expresión. Se han omitido, no se han considerado no coincidentes.",
  "perch.search.masked":
    "Hay reglas de enmascarado activadas, así que algunos valores en pantalla —y en todo lo que exportes— no son los del topic.",
  "perch.sql.waiting":
    "Todavía no se ha ejecutado ninguna consulta. El alcance de arriba decide qué registros puede ver la consulta.",
  "perch.sql.running": "En ejecución: {scanned} registros leídos hasta ahora.",
  "perch.sql.running.note":
    "Parcial. Nada de lo de abajo es la respuesta definitiva hasta que termine la exploración.",
  "perch.sql.rows":
    "{count, plural, one {# fila} other {# filas}} de los {scanned} registros que ha leído esta exploración.",
  "perch.sql.none":
    "La consulta no ha devuelto ninguna fila de los {scanned} registros que ha leído esta exploración.",
  "perch.sql.scope":
    "Esto responde sobre los registros que ha leído la exploración, no sobre el topic entero: otro alcance es otra respuesta.",
  "perch.sql.capped":
    "La exploración se ha detenido en su tope de {cap} registros, así que lo que la consulta haya contado o sumado es un cálculo sobre ese trozo.",
  "perch.sql.stopped":
    "Has detenido esta exploración tras {scanned} registros, así que la respuesta cubre una parte del rango.",
  "perch.sql.masked":
    "Había reglas de enmascarado en vigor mientras se ejecutaba esta consulta, así que algunos valores de aquí no son los del topic.",
  "perch.schemas.noRegistry":
    "Esta conexión no tiene Schema Registry, así que aquí no hay de dónde leer esquemas.",
  "perch.schemas.noRegistry.next":
    "Un registry es un servicio aparte con su propia dirección. Añádelo en Schema Registry, dentro de los ajustes de esta conexión.",
  "perch.schemas.missing":
    "El registry no tiene ningún subject llamado {subject}.",
  "perch.schemas.missing.next":
    "Kavka ha buscado con la estrategia de nombre de topic, que es la que usa la mayoría de productores. Un productor con otra estrategia se registra con otro nombre.",
  "perch.schemas.versions":
    "{count, plural, one {hay # versión de este subject registrada} other {hay # versiones de este subject registradas}}.",
  "perch.schemas.level": "Las versiones nuevas se comprueban como {level}.",
  "perch.schemas.levelUnknown":
    "Kavka no ha podido leer el ajuste de compatibilidad propio de este subject, así que no puede decir con certeza qué nivel aplicará el registry.",
  "perch.groups.none":
    "Todavía no hay grupos de consumidores en este clúster: nada ha leído de él.",
  "perch.groups.counts":
    "{count, plural, one {# grupo de consumidores está leyendo} other {# grupos de consumidores están leyendo}} de este clúster.",
  "perch.groups.rebalancing":
    "{unstable, plural, one {# grupo está reequilibrándose} other {# grupos están reequilibrándose}} ahora mismo, así que sus particiones se están repartiendo y el consumo se detiene mientras tanto. {counts}",
  "perch.groups.unread":
    "Kavka no ha podido leer los grupos de consumidores de este clúster, así que no puede decir nada sobre ellos. Hasta que pueda, nada de esta pantalla es una afirmación sobre el clúster.",
  "perch.groups.caveat":
    "Esta es la lista tal como Kavka la leyó por última vez. El estado de un grupo cambia en cada reequilibrio: pulsa Actualizar para volver a tomarla.",
  "perch.group.caughtUp":
    "{group} está al día en todas las particiones que Kavka puede ver.",
  "perch.group.behind":
    "{group} va unos {lag} mensajes por detrás en {partitions, plural, one {# partición} other {# particiones}}. La peor es {topic} partición {partition}, con {worst}.",
  "perch.group.noOffsets":
    "{group} nunca ha confirmado un offset, así que no hay ninguna posición que informar. Puede que solo haya producido, o que se creara y nunca leyera nada.",
  "perch.group.noMembers":
    "Ahora mismo no hay nada conectado a {group}, así que no está leyendo nada. Sus offsets confirmados siguen aquí, y una aplicación que arranque continuará desde ellos.",
  "perch.group.caveat":
    "Kavka leyó estos offsets una vez, al abrir esta pantalla. No siguen al grupo: vuelve a abrirla para una lectura nueva.",
  "perch.brokers.counts":
    "{count, plural, one {# broker en este clúster} other {# brokers en este clúster}}. Abre uno para ver todos los ajustes con los que se está ejecutando.",
  "perch.brokers.none":
    "Este clúster ha respondido, pero no ha nombrado ningún broker.",
  "perch.brokers.noneNext":
    "Eso suele significar que los metadatos han vuelto vacíos, o que has llegado a un balanceador de carga en vez de a Kafka. Desconecta y vuelve a conectar, y comprueba la dirección de bootstrap.",
  "perch.brokers.caveat":
    "La lista de brokers llegó al conectar y no sigue al clúster: vuelve a conectar para tomarla de nuevo.",
  "perch.broker.noOverrides":
    "El broker {broker} no cambia nada respecto a los valores por defecto de Kafka: todos sus ajustes son los que Kafka calcula.",
  "perch.broker.overrides":
    "El broker {broker} sobrescribe {count, plural, one {# ajuste} other {# ajustes}}; los otros {rest} son los que calcula en este momento.",
  "perch.broker.unread":
    "Kavka no ha podido leer los ajustes de este broker, así que no puede decir con qué se está ejecutando. La cuenta suele necesitar DescribeConfigs sobre el clúster.",
  "perch.broker.caveat":
    "Solo las filas marcadas con un + están fijadas en este broker. Un valor por defecto calculado puede cambiar bajo tus pies cuando cambia el clúster, y Kafka declara algunos ajustes como de solo lectura para los clientes: esos conservan su botón Editar, desactivado, con el motivo al pasar por encima.",
  "perch.connect.noClusters":
    "Esta conexión no tiene workers de Kafka Connect, así que desde aquí no hay nada que gobernar.",
  "perch.connect.noClustersNext":
    "Connect funciona como su propio conjunto de workers con su propia dirección REST, normalmente en el puerto 8083. Añade uno en Clústeres de Kafka Connect, dentro de los ajustes de esta conexión.",
  "perch.connect.empty":
    "Todavía no hay conectores en {cluster}, así que desde aquí no se está moviendo nada hacia Kafka ni desde Kafka.",
  "perch.connect.allRunning":
    "{count, plural, one {# conector en {cluster}} other {# conectores en {cluster}}}, y todas las tareas están en marcha.",
  "perch.connect.failed":
    "{failed, plural, one {# tarea ha fallado} other {# tareas han fallado}} en {cluster}. Una tarea fallida no mueve ningún registro hasta que algo la reinicie: abre el conector y lee primero la traza del propio worker.",
  "perch.connect.paused":
    "{paused, plural, one {# conector está pausado} other {# conectores están pausados}} en {cluster}, así que no se mueve nada a través de {paused, plural, one {él} other {ellos}}. Sus configuraciones y sus offsets confirmados se conservan.",
  "perch.connect.unread":
    "Kavka no ha podido llegar a los workers de Connect, así que no puede decir qué se está ejecutando. Esa es una dirección distinta de la de los brokers y puede ser lo único que está caído.",
  "perch.connect.caveat":
    "Estos estados vinieron de los workers la última vez que Kavka preguntó. Connect los cambia por su cuenta: pulsa Actualizar para una lectura nueva.",
  "perch.connector.running":
    "{name} está en marcha: {running} de {total} tareas están moviendo registros.",
  "perch.connector.failed":
    "{name} tiene {failed, plural, one {# tarea fallida} other {# tareas fallidas}} y no mueve nada. Lee por qué se paró antes de reiniciarla: un reinicio con la causa todavía ahí vuelve a fallar.",
  "perch.connector.paused":
    "{name} está pausado, así que no mueve ningún registro. Su configuración y sus offsets confirmados se conservan, y al reanudar continúa desde ahí.",
  "perch.connector.noTasks":
    "{name} no tiene ninguna tarea, así que no se mueve nada. Los workers crean las tareas a partir de la configuración de un conector, y una configuración que no han podido usar lo deja sin ninguna.",
  "perch.connector.caveat":
    "Esta es una única lectura, tomada la última vez que Kavka preguntó a los workers. Los estados de las tareas cambian por su cuenta.",
  "perch.monitoring.origin":
    "Kafka no recuerda el retraso: un broker solo puede decir dónde está un grupo ahora mismo. Todo lo de esta pantalla es la grabación propia de Kavka, tomada mientras esta conexión estaba activa.",
  "perch.monitoring.unread":
    "Kavka no ha podido leer su propia grabación de retraso para esta conexión, así que no puede decir cuánto va por detrás nada, ni si tiene alguna lectura.",
  "perch.monitoring.noHistory":
    "Kavka todavía no tiene lecturas de retraso para esta conexión. Las primeras aparecen dentro de {interval} tras conectar, y un grupo solo aparece aquí cuando ha confirmado un offset al menos una vez.",
  "perch.monitoring.noWindow":
    "Kavka no tiene lecturas de {group} en esta ventana. Prueba con una más larga, o revisa el muestreador de abajo.",
  "perch.monitoring.caughtUp":
    "{group} estaba al día en la última lectura: no había nada esperando a ser leído.",
  "perch.monitoring.rising":
    "{group} va unos {lag} mensajes por detrás en {partitions, plural, one {# partición} other {# particiones}}, y va a más. La peor es {topic} partición {partition}, que llegó a {peak}.",
  "perch.monitoring.steady":
    "{group} va unos {lag} mensajes por detrás en {partitions, plural, one {# partición} other {# particiones}}, y se mantiene estable desde el inicio de esta ventana.",
  "perch.monitoring.falling":
    "{group} va unos {lag} mensajes por detrás en {partitions, plural, one {# partición} other {# particiones}}, y va a menos.",
  "perch.monitoring.caveat.sampled":
    "Un punto de estas gráficas es la peor lectura de su tramo, nunca una media, y un hueco en una línea es un rato en que Kavka no estaba en marcha, no una caída.",
  "perch.monitoring.caveat.stale":
    "El muestreador va atrasado: su última lectura fue {ago}, hace más de tres intervalos. Todo lo de abajo es más antiguo de lo que parece.",
  "perch.monitoring.caveat.stopped":
    "Ahora mismo no se está grabando nada de esta conexión, así que este veredicto es tan reciente como la última lectura que Kavka consiguió tomar.",
  "perch.monitoring.caveat.unknownSampler":
    "Kavka no puede decir qué está haciendo su muestreador ahora mismo, así que no puede prometer que estas lecturas sean actuales.",
  "perch.alerts.none":
    "No hay reglas en este clúster, así que Kavka no está vigilando nada aquí.",
  "perch.alerts.quiet":
    "{count, plural, one {# regla está vigilando} other {# reglas están vigilando}} este clúster, y ninguna se está disparando.",
  "perch.alerts.firingOne": "{rule} lleva disparándose desde {time}. {detail}",
  "perch.alerts.firingMany":
    "{count, plural, one {# regla se está disparando} other {# reglas se están disparando}} en este clúster ahora mismo. La más antigua es {rule}, desde {time}.",
  "perch.alerts.unread":
    "Kavka no ha podido leer las reglas de aviso de esta conexión, así que no puede decir qué se está vigilando, ni si se está vigilando algo.",
  "perch.alerts.unreadHistory":
    "Kavka no ha podido leer el registro de avisos de esta conexión, así que no puede decir si algo se está disparando ahora mismo, ni si alguna vez se ha disparado.",
  "perch.alerts.caveat.desktop":
    "Kavka tiene que estar en marcha para darse cuenta. Cierra la ventana y no se vigila nada: esto es una aplicación de escritorio, no un servicio.",
  "perch.alerts.caveat.silent":
    "No hay ningún canal activado, así que un disparo solo llega a esta ventana y al registro de abajo. No te llegará nada cuando Kavka no esté delante de ti.",
  "perch.masking.none":
    "No hay reglas de enmascarado en esta conexión, así que todo lo que Kavka te muestra es exactamente lo que envió el productor.",
  "perch.masking.inForce":
    "{count, plural, one {# regla de enmascarado está} other {# reglas de enmascarado están}} en vigor, así que el texto coincidente se sustituye antes de llegar a esta ventana.",
  "perch.masking.off":
    "{count, plural, one {existe # regla de enmascarado} other {existen # reglas de enmascarado}} y ninguna está activada, así que no se está ocultando nada en pantalla.",
  "perch.masking.unread":
    "Kavka no ha podido leer las reglas de enmascarado de esta conexión, así que no puede prometer que lo que estás viendo sea literal.",
  "perch.masking.caveat":
    "Una regla que actives ahora se aplica a la siguiente obtención, lote de seguimiento, búsqueda o consulta, nunca a las filas que ya están en pantalla.",
  "perch.masking.caveat.sawMasked":
    "En esta sesión ya se ha enmascarado algo en pantalla, así que al menos una carga útil de aquí no es lo que envió el productor.",
  "perch.streams.noGroups":
    "Este clúster todavía no tiene grupos de consumidores, así que no hay nada de donde deducir una topología.",
  "perch.streams.pick":
    "Elige arriba una aplicación y Kavka deducirá qué lee, qué escribe y qué guarda entre medias.",
  "perch.streams.notStreams":
    "{group} no parece una aplicación de Kafka Streams, así que no hay ninguna topología que dibujar. Que un grupo de consumidores normal no tenga ninguna no es un fallo.",
  "perch.streams.inferred":
    "Esta imagen de {app} es una conjetura: {nodes, plural, one {# nodo} other {# nodos}} y {edges, plural, one {# enlace} other {# enlaces}}, deducidos de los nombres de los topics.",
  "perch.streams.unread":
    "Kavka no ha podido deducir una topología para {group}, así que no tiene nada que mostrar. El mensaje de abajo es lo que dijo el clúster.",
  "perch.streams.caveat":
    "Kafka no publica la topología de Streams en ningún sitio donde un cliente pueda leerla. Nada de esto se ha leído de la propia aplicación, así que un procesador que no deja ningún topic detrás no aparece en absoluto.",
  "perch.acls.noAuthorizer":
    "Este clúster no tiene autorizador, así que no hay reglas de acceso que listar y cada petición la decide el valor por defecto de los brokers.",
  "perch.acls.noAuthorizerNext":
    "Eso es un ajuste del broker (authorizer.class.name), no un permiso que te falte: Kafka rechaza la petición de plano en vez de responder con una lista vacía.",
  "perch.acls.none":
    "Este clúster tiene autorizador pero todavía no tiene reglas de acceso, así que lo que le pase a una petición depende por completo del valor por defecto de los brokers.",
  "perch.acls.allAllow":
    "{count, plural, one {# regla de acceso} other {# reglas de acceso}} en este clúster, y todas ellas son permisos.",
  "perch.acls.someDeny":
    "{count, plural, one {# regla de acceso} other {# reglas de acceso}} en este clúster. {denies, plural, one {# de ellas es una denegación} other {# de ellas son denegaciones}}, y una denegación gana a cualquier permiso que coincida con la misma petición.",
  "perch.acls.filtered":
    "Se muestran {count, plural, one {# regla} other {# reglas}} que coinciden con este filtro.",
  "perch.acls.unread":
    "Kavka no ha podido leer las reglas de acceso de este clúster, así que no puede decir quién tiene permiso para qué. Para listarlas, la cuenta suele necesitar Describe sobre el clúster.",
  "perch.acls.caveat.filtered":
    "Hay un filtro en vigor, así que esto cuenta las reglas que coinciden con él, no las reglas del clúster.",
  "perch.acls.caveat.removing":
    "Quitar una denegación amplía el acceso en vez de reducirlo. Kavka lo vuelve a decir antes de quitar ninguna.",

  // ── Pantallas del clúster (Jackdaw) ─────────────────────────────────────
  "topics.partitions.detail": "Mostrar detalle de réplicas",
  "topics.partitions.detailTitle":
    "Añade la lista de réplicas, la lista de réplicas sincronizadas y el offset más antiguo y más reciente de cada partición. El estado sigue en pantalla en cualquier caso.",
  "acls.filter.summary": "Filtrar estas reglas",
  "acls.filter.note": "por tipo de recurso, nombre de recurso y principal",
  "acls.filter.active": "hay un filtro en vigor",
  "alerts.state.firing": "Disparándose",
  "alerts.since": "desde {time}",
  "alerts.details.summary": "Detalles",
  "alerts.details.note": "qué compara Kavka exactamente, y con qué frecuencia",
  "alerts.facts.kind": "Tipo",
  "alerts.facts.waitsFor": "Espera",
  "alerts.facts.noWait": "nada: se dispara en cuanto la condición se cumple",
  "alerts.facts.checked": "Se comprueba",
  "alerts.facts.checkedValue":
    "en cada lectura que toma Kavka, y solo mientras Kavka está abierto",
  "alerts.facts.since": "Disparándose desde",
  "alerts.history.started": "{rule} — ha empezado",
  "alerts.history.cleared": "{rule} — se ha resuelto",
  "alerts.history.lasted": "Resuelta a las {time}, tras {duration}.",
  "alerts.history.stillFiring": "Sigue disparándose, {duration} hasta ahora.",
  "alerts.history.gap":
    "Este registro solo cubre el tiempo en que Kavka estuvo abierto. Un hueco en él es un rato en que nadie estaba mirando, y Kavka no va a adivinar qué pasó ahí.",
  "monitoring.tile.lagNow": "Retraso en la última lectura",
  "monitoring.tile.lagNowSub":
    "mensajes esperando a ser leídos cuando Kavka tomó la última muestra",
  "monitoring.tile.peak": "Pico en esta ventana",
  "monitoring.tile.peakSub":
    "la peor lectura individual que tomó Kavka, nunca una media",
  "monitoring.tile.trend": "Tendencia",
  "monitoring.tile.trendSub": "respecto al inicio de esta ventana",
  "monitoring.tile.partitionsSub": "con al menos una lectura en esta ventana",

  // ── Ajustes (Jackdaw) ───────────────────────────────────────────────────
  "settings.title": "Ajustes",
  "settings.navLabel": "Secciones de ajustes",
  "settings.perch":
    "Todo lo de aquí se aplica al momento y se guarda en este equipo. Kavka está mostrando el tema {theme}.",
  "settings.section.appearance": "Apariencia",
  "settings.section.language": "Idioma",
  "settings.section.about": "Acerca de",

  "settings.theme.title": "Tema",
  "settings.theme.help":
    "Sistema sigue a tu sistema operativo y cambia con él mientras Kavka está abierto.",
  "settings.theme.system": "Sistema",
  "settings.theme.light": "Claro",
  "settings.theme.dark": "Oscuro",

  "settings.accent.title": "Color de acento",
  "settings.accent.help":
    "El color de los botones, los enlaces y la pantalla en la que estás. No significa nada por sí mismo, así que cambiarlo no puede ocultar un aviso.",
  "settings.accent.brass": "Latón",
  "settings.accent.moss": "Musgo",
  "settings.accent.sky": "Cielo",
  "settings.accent.plum": "Ciruela",

  "settings.density.title": "Densidad",
  "settings.density.help":
    "Cómoda da aire a cada fila. Compacta muestra alrededor de un tercio más de filas: la altura con la que Kavka salió.",
  "settings.density.comfortable": "Cómoda",
  "settings.density.compact": "Compacta",

  "settings.font.title": "Tamaño del texto",
  "settings.font.help":
    "Escala todos los tamaños a la vez, para que nada se solape en el paso más grande.",
  "settings.font.s": "Pequeño",
  "settings.font.m": "Mediano",
  "settings.font.l": "Grande",

  "settings.motion.title": "Movimiento",
  "settings.motion.help":
    "Sistema sigue el ajuste de movimiento reducido de tu sistema operativo. Reducido apaga además todas las animaciones de Kavka.",
  "settings.motion.system": "Sistema",
  "settings.motion.reduce": "Reducido",

  "settings.language.title": "Idioma",
  "settings.language.help":
    "Cubre el armazón de Kavka y el veredicto con el que abre cada pantalla del clúster: la barra lateral, la paleta, este panel, el formulario de conexión y la frase inicial de cada pantalla. Las tablas y formularios que hay debajo siguen en inglés.",
  "settings.language.machine":
    "Este catálogo salió de una máquina y ningún hablante nativo lo ha revisado. Las correcciones son bienvenidas.",

  "settings.about.title": "Versión, licencia y diagnóstico",
  "settings.about.help":
    "El panel Acerca de tiene la versión y la licencia de Kavka, los ajustes del servidor MCP y el interruptor de diagnóstico de fallos.",
  "settings.about.open": "Abrir Acerca de",

  "unit.seconds": "{count, plural, one {# segundo} other {# segundos}}",
  "unit.minutes": "{count, plural, one {# minuto} other {# minutos}}",
  "unit.hours": "{count, plural, one {# hora} other {# horas}}",
  "unit.days": "{count, plural, one {# día} other {# días}}",
};

export default es;
