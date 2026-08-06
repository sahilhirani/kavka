// ─────────────────────────────────────────────────────────────────────────────
// Portuguese, Brazil (Português do Brasil) — MACHINE TRANSLATION —
// NATIVE REVIEW WELCOME.
//
// No native speaker has read this file. It was produced from `en.ts` and it is
// shipped honestly rather than quietly: the language picker in the About
// dialog says so next to the name, and `LOCALES` in ../index.ts carries
// `machine: true` for exactly this reason.
//
// If Brazilian Portuguese is your language, the highest-value contribution to
// Kavka is twenty minutes with this file. See docs/I18N.md — you need no
// build, no tooling and no account, and a partial fix is welcome: any key you
// delete falls back to English rather than breaking.
//
// This is also what a `pt-PT` speaker currently gets: `pt` is matched by
// primary subtag and Brazilian is the only Portuguese catalog that exists. A
// European Portuguese catalog would be a new entry in `LOCALES`, not an edit
// here — see docs/I18N.md, "Adding a language".
//
// Two things to keep while editing: the {placeholders} (they are values Kavka
// substitutes, and a renamed one silently disappears from the sentence), and
// the plural arms — CLDR Portuguese puts 0 and 1 in `one`, so
// `one {# conexão} other {# conexões}` is the shape.
// ─────────────────────────────────────────────────────────────────────────────

import type { Catalog } from "./en";

const ptBR: Catalog = {
  "common.close": "Fechar",
  "common.cancel": "Cancelar",
  "common.save": "Salvar",
  "common.connect": "Conectar",
  "common.tryAgain": "Tentar de novo",
  "common.remove": "Remover",
  "common.dismiss": "Dispensar",
  "common.showDetails": "Ver detalhes",
  "common.addConnection": "Adicionar conexão",
  "common.support": "Apoiar o Kavka ☕",
  "common.readingConnections": "Lendo suas conexões salvas…",
  "common.linkFailed":
    "O Kavka não conseguiu entregar esse link ao seu navegador. O endereço é {url} — copie daqui.",

  "confirm.kicker.destructive": "Destrutiva",
  "confirm.busy": "O Kavka está trabalhando nisso",
  "confirm.type.label": "Digite {name} para confirmar",
  "confirm.type.reason": "Digite exatamente {name} para confirmar isto",


  // ── The cluster switcher and the rail's cluster card ──────────────────
  // The sidebar is gone (DESIGN.md §5.1); its rows are this menu's rows and
  // its identity block is the rail's cluster card. Law 2 (§1) survives the
  // move: the status dot never carries the meaning alone, so `rowMeta` reads
  // "address · state" in EVERY state. Keep both slots.
  "switcher.trigger": "Trocar de cluster",
  "switcher.menuLabel": "Suas conexões",
  "switcher.empty": "Nenhuma conexão salva ainda.",
  "switcher.noEnvironment": "Sem ambiente",
  "switcher.rowMeta": "{address} · {status}",
  "switcher.status.disconnected": "não conectado",
  "switcher.status.connecting": "conectando…",
  "switcher.status.connected": "conectado",
  "switcher.connect": "Conectar",
  "switcher.connecting": "Conectando…",
  "switcher.disconnect": "Desconectar",
  "switcher.connectTitle": "Conectar a {name}",
  "switcher.disconnectTitle": "Desconectar de {name}",
  "switcher.draftName": "Nova conexão",
  "switcher.draftMeta": "ainda não salva",
  "switcher.protected": "Protegido",
  "brand.versionTitle": "Versão do núcleo do Kavka {version}",
  "card.none": "Nenhum cluster",
  "card.state.none": "Nada selecionado ainda",
  "card.state.connected":
    "Conectado · {count, plural, one {# broker} other {# brokers}}",
  "card.state.connecting": "Conectando…",
  "card.state.disconnected": "Não conectado",

  // ── App shell: status bar, empty states, global errors ─────────────────
  "app.status.disconnected": "Não conectado",
  "app.status.connecting": "Conectando…",
  "app.status.connected": "Conectado",
  "app.error.unknownProfile":
    "Essa conexão não está mais nesta máquina. Ela pode ter sido excluída em outra janela.",
  "app.profilesFailed.title":
    "O Kavka não conseguiu ler seu arquivo de conexões",
  "app.profilesFailed.hint":
    "Suas conexões continuam no disco — nada foi perdido. O Kavka as guarda na pasta de configuração, junto com as preferências deste aplicativo.",
  "app.firstRun.title": "Aponte o Kavka para um broker",
  "app.firstRun.what":
    "Uma conexão é um endereço salvo para um único cluster Kafka — um nome, um broker para começar e como se autenticar. O Kavka descobre o resto do cluster a partir daí.",
  "app.firstRun.example":
    "Um servidor bootstrap costuma ser parecido com {example}. Está rodando o cluster de desenvolvimento deste repositório? Use {local}.",
  "app.firstRun.footnote":
    "As senhas vão para o chaveiro do seu sistema operacional. Nada sobre seus clusters sai desta máquina.",
  "app.pick.title": "Escolha uma conexão",
  "app.pick.hint":
    "Abra o seletor de clusters no canto superior esquerdo para escolher um, ou adicione outra conexão.",
  "app.readonlyChip": "somente leitura",
  "app.readonlyTitle":
    "Esta conexão é somente leitura. Desative isso nas configurações da conexão para produzir ou editar.",
  "app.statusbar.draft": "Nova conexão — ainda não salva",
  "app.statusbar.none": "Nenhuma conexão selecionada",
  "app.statusbar.commands": "comandos",
  "app.cmd.search": "Buscar em {topic}",
  "app.cmd.search.kw":
    "find filter cel scan query messages grep buscar filtrar mensagens",
  "app.cmd.sql": "Consultar {topic} com SQL",
  "app.cmd.sql.kw":
    "sql select query aggregate count group datafusion analyse consulta contar analisar",
  "app.cmd.produce": "Produzir em {topic}",
  "app.cmd.produce.kw":
    "send write publish message record bulk producer enviar escrever publicar mensagem",
  "app.cmd.produce.confirmContext": "{cluster} · pede confirmação",

  "palette.label": "Comandos",
  "palette.searchLabel": "Buscar comandos e clusters",
  "palette.searchPlaceholder": "Buscar comandos e clusters…",
  "palette.empty":
    "Nada corresponde a “{query}”. Tente o nome de um cluster, ou limpe o campo para ver tudo o que o Kavka faz.",
  "palette.foot.move": "mover",
  "palette.foot.run": "executar",
  "palette.foot.close": "fechar",
  "palette.goTo": "Ir para {name}",
  "palette.connectTo": "Conectar em {name}",
  "palette.state.connected": "conectado",
  "palette.state.connecting": "conectando…",
  "palette.protectedCluster": "cluster protegido",
  "palette.profile.kw":
    "connect open switch cluster broker bootstrap conectar abrir trocar",
  "palette.add.context": "Um nome, um broker e como se autenticar",
  "palette.add.kw":
    "new connection profile cluster create bootstrap broker nova conexão criar",
  "palette.disconnect": "Desconectar",
  "palette.disconnect.kw": "close leave cluster session desconectar sair fechar",
  "palette.disconnect.none": "Nada está conectado no momento",
  "palette.disconnect.ambiguous":
    "Escolha primeiro no seletor de clusters aquele que você quer desconectar",
  "palette.refresh": "Recarregar topics",
  "palette.refresh.kw":
    "reload metadata list topics partitions cluster recarregar atualizar",
  "palette.export": "Exportar conexões…",
  "palette.export.context": "Todas as conexões desta máquina, em JSON",
  "palette.export.kw":
    "backup save copy share json profiles backup exportar copiar",
  "palette.import": "Importar conexões…",
  "palette.import.context": "Colar JSON de outra cópia do Kavka",
  "palette.import.kw":
    "restore paste load json profiles colar carregar restaurar",
  "palette.about.context": "Versão e licença",
  "palette.about.kw":
    "version licence license agpl source github help licença ajuda código fonte",
  "palette.support.context":
    "O Kavka é gratuito — doações mantêm ele assim",
  "palette.support.kw":
    "donate coffee sponsor fund open source doar café apoiar financiar",

  "about.title": "Sobre o Kavka",
  "about.body":
    "Um cliente de desktop para o Apache Kafka. O Kavka roda inteiramente nesta máquina: as senhas vão para o chaveiro do seu sistema operacional, e nada sobre seus clusters sai deste computador.",
  "about.coreVersion": "Versão do núcleo",
  "about.versionLoading": "Lendo agora…",
  "about.build": "Build {number}",
  "about.licence": "Licença",
  "about.licenceValue": "Livre e de código aberto sob a AGPL-3.0",
  "about.language": "Idioma",
  "about.language.hint":
    "A estrutura do Kavka e o veredito com que cada tela de cluster começa — a barra de navegação, a paleta de comandos, estas caixas de diálogo, o formulário de conexão e a frase de abertura de cada tela. As tabelas e os formulários abaixo continuam em inglês.",
  "about.language.machine":
    "{language} foi traduzido por máquina e não passou por revisão de um falante nativo. Correções são bem-vindas — docs/I18N.md explica como.",

  "transfer.title": "Conexões",
  "transfer.tablist": "Exportar ou importar",
  "transfer.tab.export": "Exportar",
  "transfer.tab.import": "Importar",
  "transfer.export.body":
    "Todas as conexões desta máquina, em JSON. Cole em outra cópia do Kavka para configurar os mesmos clusters lá.",
  "transfer.export.promise":
    "Senhas e chaves nunca saem desta máquina — exportações carregam referências, não segredos.",
  "transfer.export.failed":
    "O Kavka não conseguiu ler seu arquivo de conexões. Suas conexões continuam no disco — nada foi perdido.",
  "transfer.export.label": "Suas conexões, em JSON",
  "transfer.export.copied": "Copiado para a área de transferência.",
  "transfer.export.copyManual":
    "O Kavka não conseguiu acessar a área de transferência. O texto está selecionado — pressione {key} para copiar.",
  "transfer.export.copy": "Copiar para a área de transferência",
  "transfer.export.nothingToCopy":
    "Não há nada para copiar — o Kavka não conseguiu ler seu arquivo de conexões",
  "transfer.export.stillReading": "O Kavka ainda está lendo suas conexões",
  "transfer.import.body":
    "Cole uma exportação de outra cópia do Kavka. As senhas não estão nela — cada conexão importada pede a sua na primeira vez que você conectar.",
  "transfer.import.label": "JSON exportado",
  "transfer.import.kbd": "importar",
  "transfer.import.kbdClose": "fechar",
  "transfer.import.legend": "Se uma conexão já estiver aqui",
  "transfer.import.skip": "Manter a desta máquina",
  "transfer.import.skipHint":
    "As conexões já salvas aqui ficam exatamente como estão. Tudo o que for novo no JSON continua sendo adicionado.",
  "transfer.import.replace": "Substituir pela do JSON",
  "transfer.import.replaceHint":
    "A versão colada vence — nome, endereço, ambiente e método de autenticação. As senhas que já estão no seu chaveiro ficam onde estão.",
  "transfer.import.failed":
    "O Kavka não conseguiu ler isso como uma exportação. Verifique se você colou o arquivo inteiro, incluindo as chaves externas — o texto que o Kavka recebeu está abaixo.",
  "transfer.import.needsJson": "Cole primeiro o JSON de uma exportação",
  "transfer.import.busy": "O Kavka está importando essas conexões agora",
  "transfer.import.run": "Importar conexões",
  "transfer.import.running": "Importando…",
  "transfer.report.empty.title": "Esse JSON não tinha nenhuma conexão",
  "transfer.report.empty.detail":
    "Verifique se você colou a exportação inteira, incluindo as chaves externas — o Kavka leu tudo bem, só não havia nada a adicionar.",
  "transfer.report.added": "{count, plural, other {# adicionadas}}",
  "transfer.report.replaced": "{count, plural, other {# substituídas}}",
  "transfer.report.skipped":
    "{count, plural, other {# ignoradas — já estavam nesta máquina}}",
  "transfer.report.envAdded":
    "{count, plural, one {# ambiente adicionado} other {# ambientes adicionados}}",
  "transfer.report.envSkipped":
    "{count, plural, one {# ambiente já definido} other {# ambientes já definidos}}",
  "transfer.report.envOnly.title": "Nenhuma conexão nova — apenas ambientes",
  "transfer.report.unchanged.title":
    "{count, plural, one {Nada mudou — # conexão já estava aqui} other {Nada mudou — # conexões já estavam aqui}}",
  "transfer.report.unchanged.detail":
    "{bits}. Escolha “Substituir pela do JSON” acima se a intenção era sobrescrevê-las.",
  "transfer.report.imported.title":
    "{count, plural, one {# conexão importada} other {# conexões importadas}}",
  "transfer.report.imported.detail":
    "{bits}. As senhas não estão em uma exportação — abra cada conexão nova e digite a senha dela antes de conectar.",

  "editor.new.title": "Adicionar uma conexão",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "O que você vai reconhecer no seletor de clusters. Só o Kavka vê isso.",
  "editor.env.label": "Ambiente",
  "editor.env.hint.protected":
    "Este ambiente está marcado como protegido: a régua do razão leva a cor dele em toda tabela, o seletor de clusters marca este cluster, uma barra de aviso atravessa o topo da janela e toda ação destrutiva pede que você digite o nome antes. Ligue o modo somente leitura abaixo, a não ser que você realmente precise escrever.",
  "editor.env.hint.other":
    "O Kavka colore cada tela por ambiente, para você não confundir um cluster com outro.",
  "editor.env.manage": "Gerenciar ambientes…",
  "editor.env.hint.unknown":
    "Nada nesta máquina define {name}, então o Kavka mostra em cinza neutro e não aplica nenhuma proteção. Adicione em «Gerenciar ambientes» para dar uma cor a ele e decidir se é protegido.",
  "editor.bootstrap.hint":
    "Qualquer broker do seu cluster — o Kavka acha os demais a partir dele. Um por linha, ou separados por vírgula. Está rodando o cluster de desenvolvimento deste repositório? Use {local}.",

  "editor.auth.legend": "Autenticação",
  "editor.auth.kerberos":
    "Esta conexão se autentica com Kerberos ({service} como {principal}), o que o Kavka ainda não sabe configurar. Salvar mantém tudo exatamente como está; todos os outros campos aqui continuam funcionando.",
  "editor.auth.label": "Como este cluster verifica quem é você?",
  "editor.auth.plaintext":
    "Ele não verifica — qualquer um pode conectar (PLAINTEXT)",
  "editor.auth.saslPlain": "Usuário e senha — SASL/PLAIN",
  "editor.auth.saslScram": "Usuário e senha — SASL/SCRAM",
  "editor.auth.mtls": "Um certificado que esta máquina apresenta — mTLS",
  "editor.auth.mskIam": "As credenciais AWS desta máquina — MSK IAM",
  "editor.auth.oauth":
    "Um token do seu provedor de identidade — OAuth 2.0 / OIDC",
  "editor.auth.kerberosOption": "Um tíquete Kerberos — GSSAPI (ainda não)",
  "editor.auth.notYet":
    "O Kavka ainda não sabe configurar isso. Uma conexão que já usa esse método continua funcionando e é preservada exatamente como está quando você salva.",
  "editor.auth.hint":
    "Kafka gerenciado normalmente quer SASL/SCRAM com TLS ligado. Um broker local normalmente não quer nada. Kerberos é o único método que o Kavka ainda não sabe configurar.",
  "editor.mechanism.label": "Mecanismo SCRAM",
  "editor.mechanism.hint":
    "Se o broker recusar um, ele diz qual quer.",
  "editor.username.label": "Usuário",
  "editor.password.label": "Senha",
  "editor.password.placeholder": "Senha",
  "editor.secret.unchanged": "••••••••  (inalterada)",
  "editor.password.hint":
    "Vai para o chaveiro do seu sistema operacional — nunca para o arquivo de conexões, e nunca para fora desta máquina.",
  "editor.tls.label": "Criptografar a conexão (TLS)",
  "editor.tls.hint":
    "Kafka gerenciado quase sempre precisa disso ligado. Se o broker responde mas o handshake falha, é a primeira coisa a tentar.",

  "editor.mtls.hint":
    "O Kavka lê arquivos PEM exatamente como estão — não há keystore JKS ou PKCS#12 para converter antes.",
  "editor.caPath.label": "Certificado da AC",
  "editor.caPath.hint":
    "Caminho para o .pem da autoridade certificadora — deixe vazio para usar o repositório de confiança do sistema.",
  "editor.clientCert.label": "Certificado do cliente",
  "editor.clientCert.hint":
    "Caminho para o certificado que esta máquina mostra ao broker — deixe vazio se o broker não pedir nenhum.",
  "editor.clientKey.label": "Chave privada do cliente",
  "editor.clientKey.hint":
    "Cole a chave em si, não um caminho para ela. Ela vai para o chaveiro do seu sistema operacional — nunca para o arquivo de conexões, e nunca para fora desta máquina.",
  "editor.clientKey.storedHint":
    "Deixe vazio para manter a chave salva; limpar o caminho do certificado acima remove ela.",

  "editor.aws.hint":
    "O Kavka assina cada requisição com as credenciais AWS que já estão nesta máquina. Os servidores bootstrap acima precisam ser o endpoint IAM deste cluster — os hosts {host} do console do MSK, normalmente na porta 9098.",
  "editor.region.label": "Região",
  "editor.region.hint":
    "A região AWS em que o cluster roda. Ela precisa combinar com os hosts bootstrap, ou a assinatura não será aceita.",
  "editor.awsProfile.label": "Nome do perfil AWS",
  "editor.awsProfile.hint":
    "Um perfil nomeado de {config}. Deixe vazio para usar a cadeia de credenciais padrão — variáveis de ambiente, depois {dir}, depois SSO.",

  "editor.oauth.hint":
    "O Kavka pede um token ao seu provedor de identidade pelo fluxo de client credentials e depois apresenta ele ao broker como SASL/OAUTHBEARER.",
  "editor.tokenEndpoint.label": "Endpoint do token",
  "editor.tokenEndpoint.hint":
    "A URL que emite o token, não a página de login que um navegador usaria.",
  "editor.clientId.label": "ID do cliente",
  "editor.clientId.hint":
    "O aplicativo que seu provedor de identidade registrou para o Kafka — não a sua conta de usuário.",
  "editor.clientSecret.label": "Segredo do cliente",
  "editor.clientSecret.placeholder": "Segredo do cliente",
  "editor.clientSecret.hint":
    "Vai para o chaveiro do seu sistema operacional — nunca para o arquivo de conexões, e nunca para fora desta máquina.",

  "editor.sr.legend": "Schema Registry (opcional)",
  "editor.sr.hint":
    "Se as mensagens deste cluster forem Avro, Protobuf ou JSON Schema, o Kavka lê o schema daqui para decodificá-las — e mostra o subject, a versão e o id ao lado de cada mensagem. Sem isso, esses payloads aparecem como bytes brutos.",
  "editor.srUrl.label": "Endereço do registry",
  "editor.srUrl.hint":
    "A URL completa, incluindo o esquema. Confluent, Apicurio e Glue falam a mesma API de leitura aqui. Deixe vazio se este cluster não tiver registry.",
  "editor.srUsername.label": "Usuário do registry",
  "editor.srUsername.hint":
    "Só se o registry pedir um. Registries gerenciados costumam pedir; um registry dentro da sua própria rede normalmente não.",
  "editor.srPassword.label": "Senha do registry",
  "editor.srPassword.storedHint":
    "Deixe vazio para manter a senha salva; limpar o endereço acima remove ela.",

  "editor.connect.legend": "Clusters do Kafka Connect (opcional)",
  "editor.connect.hint":
    "O Kafka Connect roda conectores de origem e de destino, e responde na própria porta REST em vez de passar pelos brokers — então o Kavka precisa saber onde estão os workers. Adicione um por grupo de workers; o nome é como você escolhe entre eles na aba Connect.",
  "editor.connect.unnamed": "Cluster {number}",
  "editor.connect.removeLabel": "Remover {name}",
  "editor.connect.unnamedLong": "Cluster Connect {number}",
  "editor.connect.remove": "Remover este cluster Connect da conexão",
  "editor.connect.name.label": "Nome",
  "editor.connect.name.placeholder": "orders connect",
  "editor.connect.name.hint":
    "O que você reconhecer. Renomear depois mantém a senha salva.",
  "editor.connect.url.label": "Endereço dos workers",
  "editor.connect.url.hint":
    "O endpoint REST de qualquer worker do grupo — todos respondem pelo cluster inteiro. Normalmente a porta 8083, e não o mesmo host nem a mesma porta dos brokers.",
  "editor.connect.username.hint":
    "Só se os workers estiverem atrás de autenticação básica. A maioria não está.",
  "editor.connect.password.storedHint":
    "Deixe vazio para manter a senha salva; remover este cluster remove ela.",
  "editor.connect.add": "Adicionar um cluster Connect",

  "editor.monitoring.legend": "Monitoramento (opcional)",
  "editor.monitoring.hint":
    "Os brokers do Kafka não servem números de vazão, armazenamento ou replicação pelo protocolo Kafka — eles publicam isso como JMX, e quase todo mundo coloca um exporter do Prometheus na frente. Aponte o Kavka para o exporter e a aba Monitoramento se preenche. O histórico de lag não precisa de nada disso: o Kavka lê direto dos brokers.",
  "editor.metricsUrl.label": "Endereço das métricas",
  "editor.metricsUrl.hint":
    "A URL completa, incluindo o caminho. Se você opera os brokers, normalmente é o agente Java {agent} em um deles ({flag}). Um servidor Prometheus que já coleta desses brokers também serve — dê o endereço dele ao Kavka. Deixe vazio se este cluster não tiver exporter.",
  "editor.metricsUsername.label": "Usuário das métricas",
  "editor.metricsUsername.hint":
    "Só se o endpoint estiver atrás de autenticação básica. Um jmx_exporter normalmente não está; um Prometheus compartilhado normalmente está.",
  "editor.metricsPassword.label": "Senha das métricas",
  "editor.metricsPassword.storedHint":
    "Deixe vazio para manter a senha salva; limpar o endereço acima remove ela.",
  "editor.sampler.label": "Fazer uma leitura de lag a cada",
  "editor.sampler.hint":
    "Segundos. O Kafka não guarda o lag, então o Kavka faz a própria leitura nesse intervalo e mantém {days} dela em um arquivo nesta máquina. {warning} O mínimo é {floor}; o padrão é {default}, que custa uma requisição pequena por grupo a cada leitura.",
  "editor.sampler.warning":
    "As leituras só acontecem enquanto esta conexão está ativa — nada é coletado enquanto o Kavka está fechado ou este cluster está desconectado, e uma lacuna no gráfico significa exatamente isso.",

  "editor.readonly.label": "Conexão somente leitura",
  "editor.readonly.hint":
    "O Kavka continua navegando por tudo, mas não produz mensagens, não altera topics e não confirma offsets por esta conexão.",

  "editor.busy.connecting": "Espere a tentativa de conexão terminar",
  "editor.busy.saving": "O Kavka está salvando esta conexão",
  "editor.delete": "Excluir conexão",
  "editor.delete.confirm":
    "Remover {name} desta máquina? O cluster em si não é tocado.",
  "editor.kbd.connect": "conectar",
  "editor.kbd.cancel": "cancelar",
  "editor.kbd.undo": "desfazer edições",

  "editor.err.name":
    "Dê um nome a esta conexão para encontrá-la no seletor de clusters.",
  "editor.err.bootstrap":
    "Adicione pelo menos um broker, como host:porta — por ex. broker-1:9092",
  "editor.err.srUrl":
    "Use a URL completa, começando com http:// ou https:// — por ex. http://localhost:8081",
  "editor.err.srUserNoUrl":
    "Adicione o endereço do registry, ou limpe o usuário — uma autenticação sem destino não pode ser salva.",
  "editor.err.metricsUrl":
    "Use a URL completa, começando com http:// ou https:// — por ex. http://broker-1.internal:7071/metrics",
  "editor.err.metricsUserNoUrl":
    "Adicione o endereço das métricas, ou limpe o usuário — uma autenticação sem destino não pode ser salva.",
  "editor.err.sampler":
    "Colete no mínimo a cada {seconds, plural, one {# segundo} other {# segundos}}. Mais rápido que isso pede offsets aos brokers com mais frequência do que eles mudam.",
  "editor.err.connectName":
    "Dê um nome a este cluster Connect — toda ação que o Kavka envia nomeia o cluster de destino.",
  "editor.err.connectDuplicate":
    "Dois clusters Connect em uma conexão não podem ter o mesmo nome — o Kavka guarda as senhas deles sob ele.",
  "editor.err.connectUrlMissing":
    "Adicione o endereço REST dos workers — por ex. http://connect-1.internal:8083",
  "editor.err.connectUrl":
    "Use a URL completa, começando com http:// ou https:// — por ex. http://connect-1.internal:8083",
  "editor.err.username":
    "Este método de autenticação precisa do usuário pelo qual o broker conhece você.",
  "editor.err.password": "Este método de autenticação precisa de uma senha.",
  "editor.err.clientKey":
    "Cole a chave privada que acompanha esse certificado — o Kavka precisa das duas metades.",
  "editor.err.clientCert":
    "Adicione o caminho do certificado ao qual esta chave pertence — o Kavka precisa das duas metades.",
  "editor.err.region":
    "Informe a região em que o cluster roda — por ex. eu-west-1",
  "editor.err.tokenEndpoint":
    "Adicione a URL em que seu provedor de identidade emite tokens — por ex. https://login.example.com/oauth2/token",
  "editor.err.tokenEndpointUrl":
    "Use a URL completa, começando com https:// — por ex. https://login.example.com/oauth2/token",
  "editor.err.clientId":
    "Adicione o id de cliente que seu provedor de identidade emitiu para este aplicativo.",
  "editor.err.clientSecret":
    "Este método de autenticação precisa do segredo que acompanha esse id de cliente.",

  "editor.perch.screen": "Conexão",
  "editor.perch.new":
    "Nada foi salvo ainda — o Kavka não contatou nenhum broker, então nada nesta tela foi verificado.",
  "editor.perch.saved":
    "Salva, mas não conectada. O Kavka ainda não falou com {name}, então nenhum destes detalhes foi verificado contra o cluster.",
  "editor.perch.connected":
    "Conectada a {name}. O Kavka ainda está lendo o panorama do cluster.",
  "editor.perch.caveat.protected":
    "{name} está marcado como protegido: toda ação destrutiva neste cluster pede que você digite o nome antes.",
  "editor.perch.caveat.unknown":
    "Nada nesta máquina define {name}, então nenhuma proteção se aplica a esta conexão.",
  "editor.perch.caveat.readonly":
    "Somente leitura está ativado — o Kavka navegará neste cluster, mas nunca escreverá nele.",
  "editor.fold.set": "Configurado",
  "editor.fold.notSet": "Não configurado",
  "editor.fold.connectCount":
    "{count, plural, one {# cluster} other {# clusters}}",

  "editor.head.unsaved": "Ainda não salvo",
  "editor.step.name": "Como vamos chamá-lo?",
  "editor.step.env": "Qual é o ambiente?",
  "editor.step.env.why":
    "O ambiente define a cor que você verá para este cluster em todo o aplicativo — e se o Kavka o trata como protegido.",
  "editor.step.bootstrap": "Onde ele fica?",
  "editor.step.bootstrap.why":
    "Um {term} é suficiente. O Kavka pergunta a ele o resto do cluster.",
  "editor.bootstrap.term": "servidor de bootstrap",
  "editor.step.sr": "Existe um Schema Registry?",
  "editor.step.optional": "(opcional)",
  "editor.step.readonly": "O Kavka pode alterar alguma coisa aqui?",
  "editor.step.readonly.why":
    "Somente leitura é a forma mais segura de olhar o cluster de outra pessoa.",
  "editor.saveConnection": "Salvar conexão",
  "editor.state.connecting": "Conectando agora…",
  "editor.state.connected": "Conectado agora.",
  "editor.state.failed": "A última tentativa falhou — o motivo está acima.",
  "editor.state.draft": "Nada foi salvo ainda, então nada foi tentado.",
  "editor.state.idle":
    "Não conectado. O Kavka não guarda registro de quando esteve conectado pela última vez.",

  // ── A tela de conexões — seu cabeçalho e os clusters salvos ──────────────
  "connections.list.title": "Clusters salvos",
  "connections.list.empty":
    "Nenhuma conexão salva ainda. A que você está escrevendo agora será a primeira.",
  "connections.list.foot":
    "{protected} significa que o Kavka pede para você digitar o nome do cluster antes de qualquer ação destrutiva. A cor é identidade; protegido é a proteção.",
  "connections.list.footProtected": "Protegido",
  "connections.list.footSession":
    "“Conectado” e “não conectado” descrevem apenas esta sessão — o Kavka nunca contata um cluster ao qual não está conectado, então não consegue dizer se ele está no ar.",
  "connections.sub":
    "{count, plural, =0 {Nenhum cluster salvo ainda. Crie o primeiro.} one {Um cluster salvo. Selecione-o para editar, ou crie outro.} other {# clusters salvos. Selecione um para editar, ou crie um novo.}}",
  "connections.sub.unknown":
    "Selecione um cluster para editar, ou crie um novo.",
  "connections.manageEnvironments": "Gerenciar ambientes",


  // ── Ambientes ───────────────────────────────────────────────────────────
  "env.color.green": "verde",
  "env.color.amber": "âmbar",
  "env.color.red": "vermelho",
  "env.color.blue": "azul",
  "env.color.violet": "violeta",
  "env.color.cyan": "ciano",
  "env.color.slate": "ardósia",

  "env.mgr.title": "Ambientes",
  "env.mgr.intro":
    "Dê nome aos ambientes que sua organização realmente usa. A cor distingue um do outro num relance; «protegido» é a proteção.",
  "env.mgr.hint.title": "O que \"protegido\" realmente faz",
  "env.mgr.hint.detail":
    "O Kavka pede que você digite o nome do cluster antes de qualquer ação destrutiva, coloca o ambiente no título da janela e recusa comandos destrutivos da CLI e do servidor MCP sem uma flag explícita. Dois desses efeitos acontecem em outros processos, e é por isso que estão escritos aqui. A cor serve apenas para reconhecer — cada chip também escreve seu nome.",
  "env.mgr.failed": "Isso não foi concluído",
  "env.mgr.working": "O Kavka está trabalhando nisso",
  "env.mgr.namesAreYours":
    "Os nomes são seus. Adicione quantos sua organização realmente tiver — o Kavka não presume que existam apenas três.",
  "env.mgr.add": "Adicionar ambiente",
  "env.mgr.edit": "Editar",

  "env.mgr.row.protected": "protegido",
  "env.mgr.row.unprotected": "sem proteção",
  "env.mgr.row.used":
    "{count, plural, =0 {nenhuma conexão} one {# conexão} other {# conexões}}",

  "env.mgr.name.label": "Nome",
  "env.mgr.name.hint":
    "Como a sua equipe chama — dev, QA, UAT, produção. Mostrado exatamente como você digitar, e nunca traduzido.",
  "env.mgr.name.taken": "Já existe um ambiente com este nome.",
  "env.mgr.name.required": "Dê um nome ao ambiente primeiro",

  "env.mgr.color.label": "Cor",
  "env.mgr.color.hint":
    "Apenas identidade. A cor tinge a régua do razão e a etiqueta; ela nunca decide o que o Kavka deixa você fazer.",

  "env.mgr.protected.label": "Tratar este ambiente como protegido",
  "env.mgr.protected.hint":
    "O Kavka muda para o fundo de aviso, pede que você digite o nome do tópico ou do grupo antes de qualquer ação destrutiva, marca a janela e recusa escritas pela linha de comando e por assistentes de IA, a menos que sejam explicitamente autorizados.",
  "env.mgr.unprotect.prompt": "Digite {name} para remover a proteção",
  "env.mgr.unprotect.hint":
    "Todas as conexões em {name} perdem as proteções: sem confirmações digitadas, e a linha de comando e os assistentes de IA deixam de recusar escritas.",

  "env.mgr.delete.title": "Remover {name}?",
  "env.mgr.delete.unused": "Nenhuma conexão usa {name}, então nada mais muda.",
  "env.mgr.delete.used":
    "{count, plural, one {# conexão usa} other {# conexões usam}} {name}. Escolha para onde elas vão — o Kavka as move antes de remover.",
  "env.mgr.delete.moveTo": "Mover essas conexões para",
  "env.mgr.delete.moveHint": "Estas conexões serão movidas: {names}.",
  "env.mgr.delete.confirm": "Remover ambiente",
  "env.mgr.delete.needTarget":
    "Escolha um ambiente para onde mover essas conexões.",
  "env.mgr.delete.last":
    "É o único ambiente restante — adicione outro primeiro",

  // ── Navegação do cluster (Jackdaw) ──────────────────────────────────────
  // The two app-level groups. They render with NOTHING connected, which is
  // the whole reason Settings is a rail item: on first launch there is no
  // cluster, and the theme and the font size are what a new user needs first.
  // The read-only readout states its answer in BOTH directions — a guardrail
  // that is silent in its dangerous state is not a guardrail.
  "rail.navLabel": "Telas",
  "rail.group.setup": "Configurar",
  "rail.group.application": "Aplicativo",
  "rail.item.connections": "Conexões",
  "rail.item.settings": "Configurações",
  "rail.readonly.label": "Somente leitura: {state}",
  "rail.readonly.on": "ligado",
  "rail.readonly.off": "desligado",
  "rail.readonly.on.why": "O Kavka não vai produzir nem apagar nada aqui.",
  "rail.readonly.off.why": "O Kavka pode produzir e apagar aqui.",

  "rail.group.cluster": "Cluster",
  "rail.group.observe": "Observar",
  "rail.group.safety": "Segurança",
  "rail.group.integrations": "Integrações",
  "rail.item.overview": "Início",
  "rail.item.topics": "Tópicos",
  "rail.item.groups": "Grupos de consumo",
  "rail.item.brokers": "Brokers",
  "rail.item.monitoring": "Monitoramento",
  "rail.item.alerts": "Alertas",
  "rail.item.streams": "Streams",
  "rail.item.acls": "ACLs",
  "rail.item.masking": "Mascaramento",
  "rail.item.connect": "Connect",
  "rail.firing": "disparada",
  "rail.firingTitle":
    "{count, plural, one {# regra de alerta está disparada agora} other {# regras de alerta estão disparadas agora}}",

  // ── O cabeçalho da tela ─────────────────────────────────────────────────
  "stage.overview.title": "Início do cluster",
  "stage.overview.sub":
    "Do que este cluster é feito, segundo os metadados que ele respondeu na conexão.",
  "stage.overview.refresh": "Atualizar",
  "stage.overview.refresh.title":
    "Lê esta tela de novo — os grupos, o log de alertas, o quórum e as configurações dos brokers. Os blocos e a lista de brokers vieram com a conexão e só mudam quando você reconecta.",
  "stage.topics.sub":
    "Todos os tópicos que este cluster informou, com o que o Kavka consegue e não consegue dizer sobre cada um.",
  "stage.groups.sub":
    "Quem está lendo, o quanto está atrasado e o momento em que isso foi medido.",
  "stage.brokers.sub":
    "As máquinas deste cluster e as configurações com que cada uma está rodando.",
  "stage.monitoring.sub":
    "Desenhado só com leituras que o Kavka fez enquanto estava aberto — há uma lacuna para cada hora em que não estava.",
  "stage.alerts.sub":
    "Regras que o Kavka verifica por você enquanto está rodando, e tudo o que já disparou.",
  "stage.streams.sub":
    "Aplicações Kafka Streams, lidas a partir dos grupos de consumo por trás delas.",
  "stage.acls.sub":
    "Quem pode fazer o quê aqui, exatamente como o próprio cluster informa.",
  "stage.masking.sub":
    "As regras do próprio Kavka para esconder valores na tela. Nada aqui muda o cluster nem o que ele guarda.",
  "stage.connect.sub":
    "Workers do Kafka Connect que esta conexão conhece, e os conectores rodando neles.",

  // ── Início do cluster (Jackdaw) — blocos, triagem e as duas tabelas ──────
  "home.clusterId": "ID do cluster",
  "home.clusterId.absent": "Este cluster não informou um id.",
  "home.tile.reading": "ainda lendo",
  "home.tile.brokers.sub": "como o cluster os nomeou na conexão",
  "home.tile.brokers.none":
    "o cluster não informou nenhum — a conexão está ativa mas os metadados voltaram vazios",
  "home.tile.topics.sub":
    "{partitions, plural, one {# partição} other {# partições}} no total",
  "home.tile.partitions": "Partições",
  "home.tile.partitions.sub":
    "em todos os tópicos — cópias em outros brokers não são contadas duas vezes",
  "home.tile.groups.allStable": "todos eles estáveis",
  "home.tile.groups.unsettled":
    "{count, plural, one {# não está estável agora} other {# não estão estáveis agora}}",
  "home.tile.groups.idle":
    "{count, plural, one {# está sem ninguém conectado} other {# estão sem ninguém conectado}}",
  "home.tile.groups.none": "nada está lendo este cluster agora",
  "home.tile.groups.unread":
    "O Kavka não conseguiu ler a lista de grupos, então não pode dizer.",
  "home.attention.title": "Precisa de atenção",
  "home.attention.provenance":
    "O Kavka só lista o que consegue provar a partir deste instantâneo.",
  "home.attention.reading":
    "Lendo o registro de alertas desta conexão e sua lista de grupos…",
  "home.attention.unread":
    "O Kavka não conseguiu ler o registro de alertas desta conexão, então não pode dizer se algo está disparando. Uma lista vazia aqui não significaria que está tudo bem.",
  "home.attention.clear": "Nada neste instantâneo precisa de atenção.",
  "home.attention.clear.sub":
    "Nenhuma regra de alerta que você definiu está disparando, e todos os grupos de consumidores que o Kafka nomeou estão estáveis. Isso não é uma promessa sobre nada que o Kavka não mediu.",
  "home.attention.partial":
    "Nenhuma regra de alerta que você definiu está disparando. O Kavka não conseguiu ler a lista de grupos desta conexão, então esta tela não pode dizer se algo está lendo — uma lista vazia aqui não é um sinal de tudo certo.",
  "home.attention.groupsUnread":
    "O Kavka não conseguiu ler a lista de grupos desta conexão, então nada nesta lista trata de quem está lendo.",
  "home.attention.alert.noDetail":
    "O Kavka registrou este disparo sem os números por trás dele.",
  "home.attention.group.title": "{group} não está lendo agora",
  "home.attention.group.sub":
    "O Kafka informa este grupo como {state}, com {members, plural, one {# membro} other {# membros}}. Um grupo que não está estável para de consumir até o rebalanceamento terminar.",
  "home.attention.open.monitoring": "Abrir em Monitoramento",
  "home.attention.open.alerts": "Abrir Alertas",
  "home.attention.open.groups": "Abrir Grupos de consumidores",
  "home.attention.where": "em {screen}",
  "home.attention.foot":
    "Construída a partir das {limit} entradas mais recentes do registro de alertas desta conexão e da lista de grupos que esta tela leu. O Kavka não avalia mais nada aqui — um problema que nenhuma regra observa não aparecerá nesta lista.",
  "home.brokers.caption": "Brokers deste cluster",
  "home.brokers.none":
    "Este cluster não informou nenhum broker. Normalmente isso significa que a conexão está ativa mas os metadados voltaram vazios — tente reconectar.",
  "home.brokers.foot":
    "Estes são os brokers que este cluster nomeou nos metadados com que respondeu na conexão. O Kavka não os contatou um a um desde então, então um broker que parou há um minuto ainda aparece aqui.",
  "home.brokers.details": "Detalhes",
  "home.brokers.details.note":
    "versão de protocolo, diretórios de log, replicação e retenção, lidos do broker {id}",
  "home.brokers.details.reading": "Lendo a configuração do broker {id}…",
  "home.brokers.details.unread":
    "O Kavka não conseguiu ler a configuração do broker {id}. A tela Brokers pede as mesmas configurações e mostra o erro por trás disso.",
  "home.brokers.details.caveat":
    "Lido apenas do broker {id}. Outro broker deste cluster pode estar configurado de forma diferente, e um cluster cujos brokers divergem é um erro de configuração comum e silencioso.",
  "home.brokers.fact.protocol": "Versão de protocolo",
  "home.brokers.fact.logDirs": "Diretórios de log",
  "home.brokers.fact.replication": "Replicação padrão",
  "home.brokers.fact.autoCreate": "Criar tópicos automaticamente",
  "home.brokers.fact.retention": "Retenção padrão (horas)",
  "home.brokers.fact.absent": "não definido neste broker",

  // ── O poleiro (Jackdaw) ─────────────────────────────────────────────────
  "perch.label": "{screen} — o que o Kavka consegue dizer",
  "perch.kicker": "{screen} · {state}",
  "perch.state.ok": "Parece saudável",
  "perch.state.watch": "Vale uma olhada",
  "perch.state.problem": "Algo está errado",
  "perch.state.unknown": "Ainda não dá para dizer",
  "perch.state.checking": "Ainda verificando",
  "perch.checking":
    "Ainda verificando — o Kavka avisa o que encontrar assim que o cluster responder.",
  "perch.hide": "Ocultar",
  "perch.more": "Ver a nota inteira",
  "perch.show": "Ver a nota desta tela",
  "perch.overview.counts":
    "Conectado a {brokers, plural, one {# broker} other {# brokers}}, com {topics, plural, one {# tópico} other {# tópicos}} em {partitions, plural, one {# partição} other {# partições}}.",
  "perch.overview.firing":
    "{count, plural, one {# regra de alerta está disparada} other {# regras de alerta estão disparadas}} neste cluster agora. {counts}",
  "perch.overview.snapshot":
    "Estes números vieram no momento da conexão e não acompanham o cluster — reconecte para pegá-los de novo.",
  "perch.overview.noBrokers":
    "O cluster respondeu, mas não citou nenhum broker.",
  "perch.overview.noBrokers.next":
    "Isso normalmente quer dizer que você chegou a um balanceador de carga em vez do Kafka, ou que os metadados voltaram vazios. Desconecte, conecte de novo e confira o endereço de bootstrap.",
  "perch.screen.messages": "Mensagens",
  "perch.screen.search": "Busca",
  "perch.screen.sql": "SQL",
  "perch.screen.schemas": "Esquemas",
  "perch.topics.unreadable":
    "O Kavka não tem nenhuma lista dos topics deste cluster.",
  "perch.topics.unreadable.next":
    "A conexão pode estar ativa mesmo que a conta não tenha Describe no cluster. Atualizar pergunta de novo.",
  "perch.topics.empty":
    "Este cluster não tem nenhum topic — ainda não foi criado nenhum nele.",
  "perch.topics.internalOnly":
    "Tudo neste cluster é um topic interno do próprio Kafka. Ative Mostrar internos para vê-los.",
  "perch.topics.counts":
    "{count, plural, one {# topic neste cluster} other {# topics neste cluster}}.",
  "perch.topics.countsHidden":
    "{count, plural, one {# topic exibido} other {# topics exibidos}}.",
  "perch.topics.hiddenNote":
    "{count, plural, one {mais # é um topic interno do próprio Kafka e está oculto} other {mais # são topics internos do próprio Kafka e estão ocultos}}.",
  "perch.topics.snapshot":
    "Esta lista foi lida quando você abriu a tela e não acompanha o cluster — Atualizar a lê de novo.",
  "perch.topics.readOnly":
    "Esta conexão é somente leitura, então nada aqui pode criar, alterar ou excluir um topic.",
  "perch.topic.unreadable":
    "O Kavka não tem a lista de partições de {topic}, então não pode dizer o que há nele.",
  "perch.topic.unreadable.next":
    "O topic pode ter sido excluído, ou a conta pode não ter Describe sobre ele.",
  "perch.topic.underReplicated":
    "{count, plural, one {# partição aqui está sem uma cópia} other {# partições aqui estão sem cópias}} — o Kafka mantém menos réplicas do que este topic pede.",
  "perch.topic.unpreferred":
    "{count, plural, one {# partição é liderada} other {# partições são lideradas}} por um broker que não é o primeiro da sua lista de réplicas. Isso é rotina depois de um reinício, e Eleger líderes preferidos as devolve ao lugar.",
  "perch.topic.healthy":
    "{count, plural, one {# partição} other {# partições}}, com todas as cópias em sincronia.",
  "perch.topic.records": "Cerca de {records} mensagens pelos offsets.",
  "perch.topic.approx":
    "Essa contagem de mensagens é a diferença entre o offset mais antigo e o mais recente de cada partição, então ainda conta registros que a retenção ou a compactação já removeram.",
  "perch.messages.waiting":
    "Nada foi lido ainda. Escolha acima de onde ler e pressione Buscar.",
  "perch.messages.range":
    "{count, plural, one {# mensagem} other {# mensagens}} do intervalo que você pediu.",
  "perch.messages.none": "Nada no intervalo que você pediu.",
  "perch.messages.topicEmpty": "{topic} ainda não contém nenhuma mensagem.",
  "perch.messages.live":
    "Observando {topic} ao vivo — {count, plural, one {# mensagem chegou} other {# mensagens chegaram}} desde que o acompanhamento começou.",
  "perch.messages.liveQuiet":
    "Observando {topic} ao vivo. Nada foi produzido nele por pelo menos trinta segundos.",
  "perch.messages.notWhole":
    "Esta é a fatia que você pediu, não o topic inteiro — {topic} contém cerca de {total} mensagens.",
  "perch.messages.dropped":
    "{count, plural, one {# mensagem chegou} other {# mensagens chegaram}} mais rápido do que esta janela conseguia absorver, e a sessão a descartou em vez de ficar para trás — então as linhas na tela não são tudo o que o acompanhamento viu.",
  "perch.messages.trimmed":
    "O Kavka mantém as últimas {cap} linhas ao vivo; tudo mais antigo já saiu do buffer.",
  "perch.messages.masked":
    "Há regras de mascaramento ativas, então alguns valores na tela não são os valores do topic. Cópias e exportações levam as substituições.",
  "perch.search.waiting":
    "Nada foi varrido ainda. Defina o escopo, diga o que procura e pressione Buscar.",
  "perch.search.running":
    "Varrendo {topic} — {count, plural, one {# correspondência} other {# correspondências}} até agora.",
  "perch.search.running.note":
    "Parcial. Estes números continuam mudando até a varredura terminar.",
  "perch.search.matches":
    "{count, plural, one {# correspondência} other {# correspondências}} nos {scanned} registros que esta varredura leu.",
  "perch.search.none":
    "Nada correspondeu nos {scanned} registros que esta varredura leu.",
  "perch.search.stopped":
    "Você parou esta varredura após {scanned} registros, então ela responde sobre parte do intervalo e não sobre todo ele.",
  "perch.search.capped":
    "{matched} registros corresponderam, mas o Kavka guardou {kept}. Ordenar, exportar ou contar o que está na tela responde sobre esses, não sobre todas as correspondências.",
  "perch.search.unevaluated":
    "{count, plural, one {# registro não pôde ser lido} other {# registros não puderam ser lidos}} contra a sua expressão. Eles foram pulados, não julgados como não correspondentes.",
  "perch.search.masked":
    "Há regras de mascaramento ativas, então alguns valores na tela — e em tudo o que você exportar — não são os valores do topic.",
  "perch.sql.waiting":
    "Nenhuma consulta foi executada ainda. O escopo acima decide quais registros a consulta pode ver.",
  "perch.sql.running": "Executando — {scanned} registros lidos até agora.",
  "perch.sql.running.note":
    "Parcial. Nada abaixo é a resposta final até a varredura terminar.",
  "perch.sql.rows":
    "{count, plural, one {# linha} other {# linhas}} dos {scanned} registros que esta varredura leu.",
  "perch.sql.none":
    "A consulta não retornou nenhuma linha dos {scanned} registros que esta varredura leu.",
  "perch.sql.scope":
    "Isto responde sobre os registros que a varredura leu, não sobre o topic inteiro — outro escopo é outra resposta.",
  "perch.sql.capped":
    "A varredura parou no seu limite de {cap} registros, então tudo o que a consulta contou ou somou é um cálculo sobre essa fatia.",
  "perch.sql.stopped":
    "Você parou esta varredura após {scanned} registros, então a resposta cobre parte do intervalo.",
  "perch.sql.masked":
    "Havia regras de mascaramento em vigor enquanto esta consulta rodava, então alguns valores aqui não são os valores do topic.",
  "perch.schemas.noRegistry":
    "Esta conexão não tem Schema Registry, então não há de onde ler esquemas aqui.",
  "perch.schemas.noRegistry.next":
    "Um registry é um serviço separado com endereço próprio. Adicione-o em Schema Registry, nas configurações desta conexão.",
  "perch.schemas.missing":
    "O registry não tem nenhum subject chamado {subject}.",
  "perch.schemas.missing.next":
    "O Kavka procurou pela estratégia de nome de topic, que é a que a maioria dos produtores usa. Um produtor com outra estratégia registra sob outro nome.",
  "perch.schemas.versions":
    "{count, plural, one {# versão deste subject está registrada} other {# versões deste subject estão registradas}}.",
  "perch.schemas.level": "As novas versões são verificadas como {level}.",
  "perch.schemas.levelUnknown":
    "O Kavka não conseguiu ler a configuração de compatibilidade própria deste subject, então não pode dizer com certeza qual nível o registry vai aplicar.",
  "perch.groups.none":
    "Ainda não há grupos de consumidores neste cluster — nada leu dele.",
  "perch.groups.counts":
    "{count, plural, one {# grupo de consumidores está lendo} other {# grupos de consumidores estão lendo}} deste cluster.",
  "perch.groups.rebalancing":
    "{unstable, plural, one {# grupo está} other {# grupos estão}} rebalanceando agora, então suas partições estão sendo repassadas e o consumo fica pausado enquanto isso. {counts}",
  "perch.groups.unread":
    "O Kavka não conseguiu ler os grupos de consumidores deste cluster, então não pode dizer nada sobre eles. Até conseguir, nada nesta tela é uma afirmação sobre o cluster.",
  "perch.groups.caveat":
    "Esta é a lista como o Kavka a leu pela última vez. O estado de um grupo muda a cada rebalanceamento — pressione Atualizar para lê-la de novo.",
  "perch.group.caughtUp":
    "{group} está em dia em todas as partições que o Kavka consegue ver.",
  "perch.group.behind":
    "{group} está cerca de {lag} mensagens atrás em {partitions, plural, one {# partição} other {# partições}}. A pior é {topic} partição {partition}, com {worst}.",
  "perch.group.noOffsets":
    "{group} nunca confirmou um offset, então não há posição a relatar. Pode ser que só tenha produzido, ou que tenha sido criado e nunca tenha lido nada.",
  "perch.group.noMembers":
    "Nada está conectado a {group} no momento, então ele não está lendo nada. Seus offsets confirmados continuam aqui, e uma aplicação que iniciar seguirá a partir deles.",
  "perch.group.caveat":
    "O Kavka leu estes offsets uma vez, quando esta tela abriu. Eles não acompanham o grupo — reabra-a para uma leitura nova.",
  "perch.brokers.counts":
    "{count, plural, one {# broker neste cluster} other {# brokers neste cluster}}. Abra um para ver todas as configurações com que ele está rodando.",
  "perch.brokers.none": "Este cluster respondeu, mas não nomeou nenhum broker.",
  "perch.brokers.noneNext":
    "Isso normalmente significa que os metadados voltaram vazios, ou que você chegou a um balanceador de carga em vez do próprio Kafka. Desconecte, conecte de novo e confira o endereço de bootstrap.",
  "perch.brokers.caveat":
    "A lista de brokers voltou quando você conectou e não acompanha o cluster — reconecte para obtê-la de novo.",
  "perch.broker.noOverrides":
    "O broker {broker} não muda nada dos padrões do Kafka — cada configuração que ele tem é calculada pelo Kafka.",
  "perch.broker.overrides":
    "O broker {broker} sobrescreve {count, plural, one {# configuração} other {# configurações}}; as outras {rest} são o que ele calcula neste momento.",
  "perch.broker.unread":
    "O Kavka não conseguiu ler as configurações deste broker, então não pode dizer com o que ele está rodando. A conta normalmente precisa de DescribeConfigs no cluster.",
  "perch.broker.caveat":
    "Apenas as linhas marcadas com um + estão definidas neste broker. Um padrão calculado pode mudar sob seus pés quando o cluster muda, e o Kafka informa algumas configurações como somente leitura para clientes — essas mantêm o botão Editar, desativado, com o motivo ao passar o mouse.",
  "perch.connect.noClusters":
    "Esta conexão não tem workers do Kafka Connect, então não há nada a comandar daqui.",
  "perch.connect.noClustersNext":
    "O Connect roda como seu próprio conjunto de workers com endereço REST próprio, normalmente na porta 8083. Adicione um em Clusters do Kafka Connect, nas configurações desta conexão.",
  "perch.connect.empty":
    "Ainda não há conectores em {cluster}, então nada está sendo movido para dentro ou para fora do Kafka daqui.",
  "perch.connect.allRunning":
    "{count, plural, one {# conector em {cluster}} other {# conectores em {cluster}}}, e todas as tarefas estão rodando.",
  "perch.connect.failed":
    "{failed, plural, one {# tarefa falhou} other {# tarefas falharam}} em {cluster}. Uma tarefa com falha não move nenhum registro até que algo a reinicie — abra o conector e leia primeiro o rastreamento do próprio worker.",
  "perch.connect.paused":
    "{paused, plural, one {# conector está pausado} other {# conectores estão pausados}} em {cluster}, então nada passa por {paused, plural, one {ele} other {eles}}. Suas configurações e seus offsets confirmados são mantidos.",
  "perch.connect.unread":
    "O Kavka não conseguiu alcançar os workers do Connect, então não pode dizer o que está rodando. Esse é um endereço diferente do dos brokers e pode ser a única coisa fora do ar.",
  "perch.connect.caveat":
    "Estes estados vieram dos workers na última vez que o Kavka perguntou. O Connect os muda por conta própria — pressione Atualizar para uma leitura nova.",
  "perch.connector.running":
    "{name} está rodando: {running} de {total} tarefas estão movendo registros.",
  "perch.connector.failed":
    "{name} tem {failed, plural, one {# tarefa com falha} other {# tarefas com falha}} e não move nada. Leia por que ela parou antes de reiniciá-la — um reinício com a causa ainda ali simplesmente falha de novo.",
  "perch.connector.paused":
    "{name} está pausado, então não move nenhum registro. Sua configuração e seus offsets confirmados são mantidos, e retomar continua a partir deles.",
  "perch.connector.noTasks":
    "{name} não tem nenhuma tarefa, então nada se move. Os workers criam tarefas a partir da configuração de um conector, e uma configuração que eles não conseguiram usar o deixa sem nenhuma.",
  "perch.connector.caveat":
    "Esta é uma leitura única, tomada na última vez que o Kavka perguntou aos workers. Os estados das tarefas mudam sozinhos.",
  "perch.monitoring.origin":
    "O Kafka não lembra o atraso — um broker só pode dizer onde um grupo está agora. Tudo nesta tela é a gravação do próprio Kavka, feita enquanto esta conexão estava ativa.",
  "perch.monitoring.unread":
    "O Kavka não conseguiu ler sua própria gravação de atraso para esta conexão, então não pode dizer o quanto algo está atrasado — nem se tem alguma leitura.",
  "perch.monitoring.noHistory":
    "O Kavka ainda não tem leituras de atraso para esta conexão. As primeiras aparecem dentro de {interval} após conectar, e um grupo só aparece aqui depois de confirmar um offset pelo menos uma vez.",
  "perch.monitoring.noWindow":
    "O Kavka não tem leituras de {group} nesta janela. Tente uma mais longa, ou confira o amostrador abaixo.",
  "perch.monitoring.caughtUp":
    "{group} estava em dia na última leitura — nada esperava para ser lido.",
  "perch.monitoring.rising":
    "{group} está cerca de {lag} mensagens atrás em {partitions, plural, one {# partição} other {# partições}}, e subindo. A pior é {topic} partição {partition}, que chegou a {peak}.",
  "perch.monitoring.steady":
    "{group} está cerca de {lag} mensagens atrás em {partitions, plural, one {# partição} other {# partições}}, e estável desde o início desta janela.",
  "perch.monitoring.falling":
    "{group} está cerca de {lag} mensagens atrás em {partitions, plural, one {# partição} other {# partições}}, e caindo.",
  "perch.monitoring.caveat.sampled":
    "Um ponto nestes gráficos é a pior leitura da sua fatia, nunca uma média, e uma quebra numa linha é um trecho em que o Kavka não estava rodando — não uma queda.",
  "perch.monitoring.caveat.stale":
    "O amostrador está atrasado: sua última leitura foi {ago}, há mais de três intervalos. Tudo abaixo é mais antigo do que parece.",
  "perch.monitoring.caveat.stopped":
    "Nada está sendo gravado para esta conexão no momento, então este veredito é apenas tão novo quanto a última leitura que o Kavka conseguiu fazer.",
  "perch.monitoring.caveat.unknownSampler":
    "O Kavka não consegue dizer o que seu amostrador está fazendo agora, então não pode prometer que estas leituras sejam atuais.",
  "perch.alerts.none":
    "Não há regras neste cluster, então o Kavka não está vigiando nada aqui.",
  "perch.alerts.quiet":
    "{count, plural, one {# regra está vigiando} other {# regras estão vigiando}} este cluster, e nenhuma delas está disparando.",
  "perch.alerts.firingOne": "{rule} está disparando desde {time}. {detail}",
  "perch.alerts.firingMany":
    "{count, plural, one {# regra está disparando} other {# regras estão disparando}} neste cluster agora. A mais antiga é {rule}, desde {time}.",
  "perch.alerts.unread":
    "O Kavka não conseguiu ler as regras de alerta desta conexão, então não pode dizer o que está sendo vigiado — nem se algo está.",
  "perch.alerts.unreadHistory":
    "O Kavka não conseguiu ler o registro de alertas desta conexão, então não pode dizer se algo está disparando agora — nem se algo já disparou.",
  "perch.alerts.caveat.desktop":
    "O Kavka precisa estar rodando para perceber. Feche a janela e nada é vigiado — isto é um aplicativo de desktop, não um serviço.",
  "perch.alerts.caveat.silent":
    "Nenhum canal está ativado, então um disparo só chega a esta janela e ao registro abaixo. Nada chegará até você quando o Kavka não estiver à sua frente.",
  "perch.masking.none":
    "Não há regras de mascaramento nesta conexão, então tudo o que o Kavka mostra é exatamente o que o produtor enviou.",
  "perch.masking.inForce":
    "{count, plural, one {# regra de mascaramento está} other {# regras de mascaramento estão}} em vigor, então o texto correspondente é substituído antes de chegar a esta janela.",
  "perch.masking.off":
    "{count, plural, one {existe # regra de mascaramento} other {existem # regras de mascaramento}} e nenhuma está ativada, então nada na tela está sendo ocultado.",
  "perch.masking.unread":
    "O Kavka não conseguiu ler as regras de mascaramento desta conexão, então não pode prometer que o que você está vendo seja literal.",
  "perch.masking.caveat":
    "Uma regra que você ativa agora vale para a próxima busca, lote de acompanhamento, pesquisa ou consulta — nunca para linhas que já estão na tela.",
  "perch.masking.caveat.sawMasked":
    "Algo na tela nesta sessão já foi mascarado, então pelo menos um payload aqui não é o que o produtor enviou.",
  "perch.streams.noGroups":
    "Este cluster ainda não tem grupos de consumidores, então não há de onde deduzir uma topologia.",
  "perch.streams.pick":
    "Escolha uma aplicação acima e o Kavka vai deduzir o que ela lê, o que escreve e o que guarda no meio.",
  "perch.streams.notStreams":
    "{group} não parece uma aplicação Kafka Streams, então não há topologia a desenhar. Um grupo de consumidores comum não ter nenhuma não é uma falha.",
  "perch.streams.inferred":
    "Esta imagem de {app} é um palpite: {nodes, plural, one {# nó} other {# nós}} e {edges, plural, one {# ligação} other {# ligações}}, deduzidos dos nomes dos topics.",
  "perch.streams.unread":
    "O Kavka não conseguiu deduzir uma topologia para {group}, então não tem nada a mostrar. A mensagem abaixo é o que o cluster disse.",
  "perch.streams.caveat":
    "O Kafka não publica a topologia do Streams em nenhum lugar onde um cliente possa lê-la. Nada aqui foi lido da própria aplicação, então um processador que não deixa nenhum topic para trás simplesmente não aparece.",
  "perch.acls.noAuthorizer":
    "Este cluster não tem autorizador, então não há regras de acesso a listar e cada requisição é decidida pelo padrão dos próprios brokers.",
  "perch.acls.noAuthorizerNext":
    "Isso é uma configuração do broker (authorizer.class.name), não uma permissão que falta a você — o Kafka recusa a requisição de vez em vez de responder com uma lista vazia.",
  "perch.acls.none":
    "Este cluster tem autorizador, mas ainda não tem regras de acesso, então o que acontece com uma requisição depende inteiramente do padrão dos brokers.",
  "perch.acls.allAllow":
    "{count, plural, one {# regra de acesso} other {# regras de acesso}} neste cluster, e todas elas são permissões.",
  "perch.acls.someDeny":
    "{count, plural, one {# regra de acesso} other {# regras de acesso}} neste cluster. {denies, plural, one {# delas é uma negação} other {# delas são negações}}, e uma negação vence qualquer permissão que corresponda à mesma requisição.",
  "perch.acls.filtered":
    "Mostrando {count, plural, one {# regra} other {# regras}} que correspondem a este filtro.",
  "perch.acls.unread":
    "O Kavka não conseguiu ler as regras de acesso deste cluster, então não pode dizer quem tem permissão para quê. Para listá-las, a conta normalmente precisa de Describe no cluster.",
  "perch.acls.caveat.filtered":
    "Há um filtro em vigor, então isto conta as regras que correspondem a ele — não as regras do cluster.",
  "perch.acls.caveat.removing":
    "Remover uma negação amplia o acesso em vez de restringi-lo. O Kavka avisa de novo antes de remover uma.",

  // ── Telas do cluster (Jackdaw) ──────────────────────────────────────────
  "topics.partitions.detail": "Mostrar detalhe das réplicas",
  "topics.partitions.detailTitle":
    "Acrescenta a lista de réplicas, a lista de réplicas em sincronia e o offset mais antigo e mais recente de cada partição. O estado continua na tela de qualquer forma.",
  "acls.filter.summary": "Filtrar estas regras",
  "acls.filter.note": "por tipo de recurso, nome do recurso e principal",
  "acls.filter.active": "há um filtro em vigor",
  "alerts.state.firing": "Disparando",
  "alerts.since": "desde {time}",
  "alerts.details.summary": "Detalhes",
  "alerts.details.note":
    "o que exatamente o Kavka compara, e com que frequência",
  "alerts.facts.kind": "Tipo",
  "alerts.facts.waitsFor": "Espera",
  "alerts.facts.noWait":
    "nada — dispara no momento em que a condição é verdadeira",
  "alerts.facts.checked": "Verificada",
  "alerts.facts.checkedValue":
    "a cada leitura que o Kavka faz, e só enquanto o Kavka está aberto",
  "alerts.facts.since": "Disparando desde",
  "alerts.history.started": "{rule} — começou",
  "alerts.history.cleared": "{rule} — resolveu",
  "alerts.history.lasted": "Resolvida às {time}, após {duration}.",
  "alerts.history.stillFiring": "Ainda disparando, {duration} até agora.",
  "alerts.history.gap":
    "Este registro só cobre o tempo em que o Kavka esteve aberto. Uma lacuna nele é um trecho em que ninguém estava olhando, e o Kavka não vai adivinhar o que aconteceu ali.",
  "alerts.toast.viewGroup": "Ver o grupo {group}",
  "alerts.toast.viewAlerts": "Ver o alerta",
  "alerts.preview.label": "A notificação de {rule}",
  "alerts.preview.sent":
    "O Kavka pediu ao seu sistema operacional para exibir isto às {time}. Pediu, não exibiu — a central de notificações pode estar desligada ou a permissão revogada, e o Kavka não é avisado quando isso acontece. Ela carrega estas palavras e nada mais: não há botões nela. Uma por disparo, e mais uma quando o alerta se resolve.",
  "alerts.preview.off":
    "As notificações da área de trabalho estão desligadas para esta conexão, então nada foi exibido fora desta janela. Isto é o que ela teria dito — o nome da regra e os números que a dispararam, e nada mais.",
  "monitoring.tile.lagNow": "Atraso na última leitura",
  "monitoring.tile.lagNowSub":
    "mensagens esperando para serem lidas quando o Kavka amostrou pela última vez",
  "monitoring.tile.peak": "Pico nesta janela",
  "monitoring.tile.peakSub":
    "a pior leitura isolada que o Kavka fez, nunca uma média",
  "monitoring.tile.trend": "Tendência",
  "monitoring.tile.trendSub": "em relação ao início desta janela",
  "monitoring.tile.partitionsSub": "com pelo menos uma leitura nesta janela",

  // ── Rodapés de painel — o que a tabela acima não pode dizer ──────────────
  "topics.list.foot":
    "Só o formato. Estes são os metadados do próprio cluster, lidos quando esta tela abriu: eles dizem como cada tópico está organizado, não quanto há nele, se algo o está lendo, nem se ele está saudável. Abra um tópico para ver suas partições, suas contagens e seus consumidores.",
  "topic.partitions.foot":
    "“Mensagens” é o offset mais novo menos o mais antigo que os brokers ainda guardam para aquela partição. O que a retenção ou a compactação removeu não está aí, e num tópico compactado ela conta offsets em vez dos registros que você leria de volta — então é o que esta partição ainda pode lhe mostrar, nunca o que ela recebeu.",
  "topic.config.foot":
    "Lido uma vez, quando esta tela abriu. Uma linha sem {plus} é o que os brokers estavam usando como padrão naquele instante e pode mudar sob este tópico sem que nada aqui mude; um valor que o Kafka marca como sensível é omitido de todo cliente, então o traço significa que o broker não vai dizer, e não que nada esteja definido.",
  "schemas.versions.foot":
    "Estas são as versões que o registro mantém sob {subject}. Nomear subjects é uma convenção do lado do produtor, não algo que o tópico registre — então uma lista curta, ou nenhuma, não é prova de que nada esteja escrevendo em {topic} com um esquema.",
  "alerts.rules.foot":
    "“Quieta” significa que nada disparou a regra, não que o Kavka verificou o número e o achou bom — uma regra cuja leitura está indisponível também fica quieta. O estado vem do log abaixo, então ele só é tão completo quanto aquele log.",
  "alerts.channels.foot":
    "O Kavka pede a cada um deles uma vez por disparo e nunca tenta de novo. Ele não é informado se o seu sistema operacional realmente exibiu a notificação, e um webhook que recusa vai para o log do Kavka em vez de aparecer aqui — então “ligado” significa que o Kavka vai pedir, não que alguém foi alcançado.",
  "groups.list.foot":
    "As contagens de membros e os estados são do instante em que o Kavka perguntou. Um grupo que está rebalanceando está repassando suas partições enquanto você lê isto, então a contagem dele já está desatualizada — pressione Atualizar para obter uma nova.",
  "group.members.foot":
    "Estes são os membros que estavam conectados quando o Kavka perguntou. As contagens de partições são a atribuição daquele instante, e um rebalanceamento as redesenha sem que nada nesta tela mude.",
  "group.lag.foot":
    "O lag é a coluna Fim menos a coluna Confirmado, e as duas foram lidas na mesma chamada, então elas concordam entre si. ∅ significa que o grupo nunca confirmou um offset para aquela partição, o que não é o mesmo que lag zero. Um grupo que confirma raramente aparece como atrasado num trabalho que já fez.",
  "brokers.list.foot":
    "Esta é a lista de brokers com que o Kafka respondeu quando esta conexão foi feita. Um broker que entrou ou saiu desde então só aparece aqui depois que você reconectar.",
  "broker.config.foot":
    "Esta é a resposta de UM broker. O Kafka mantém a maioria das configurações por broker, então outro broker deste cluster pode estar rodando com um valor diferente para o mesmo nome, e nada nesta tela mostraria isso.",
  "monitoring.foot.lag":
    "O lag é o offset mais novo da partição menos o offset confirmado do grupo, e os dois vieram da mesma leitura, então eles concordam entre si. Um grupo que confirma raramente é desenhado como atrasado num trabalho que já fez, e nada aqui consegue distinguir isso de um grupo genuinamente atrasado.",
  "monitoring.foot.health":
    "As duas leituras vêm do endpoint de métricas e não das próprias respostas dos brokers ao Kavka, então elas só são tão recentes quanto o exportador for. São totais de todo o cluster: nenhuma das duas pode dizer qual partição.",
  "monitoring.foot.throughput":
    "Estes são os contadores do exportador, mantidos em memória apenas para esta conexão — eles recomeçam do zero toda vez que ela abre. Uma linha plana e um exportador que parou de responder em silêncio parecem iguais aqui, que é justamente para o que serve a linha do amostrador acima.",
  "monitoring.foot.noEndpoint":
    "Isto é um fato sobre a conexão que o Kavka recebeu, não sobre o cluster. Os brokers podem muito bem estar publicando JMX; o Kavka simplesmente não foi informado de onde encontrá-lo.",
  "monitoring.foot.noSeries":
    "O Kavka mapeia os nomes de métrica que reconhece e ignora o resto, então uma leitura publicada sob um nome que ele não conhece fica ausente aqui em vez de aparecer errada. Ele nunca inventa um valor para preencher a lacuna.",
  "streams.topology.foot":
    "O Kavka só consegue desenhar os tópicos que esta conexão tem permissão para listar. Um tópico de reparticionamento ou de changelog que a conta não consegue descrever fica de fora do desenho, e uma caixa ausente parece exatamente uma aplicação que nunca teve uma.",
  "acls.foot.authorizer":
    "Esta é a lista que o autorizador do cluster mantém. Um cluster rodando sem autorizador permite tudo e não tem regras para listar, o que aqui parece igual a um cluster para o qual ninguém escreveu nenhuma.",
  "masking.rules.foot":
    "Uma regra casa com o texto que o Kavka está prestes a colocar na tela. Um valor dividido entre campos, codificado ou escrito de outro jeito simplesmente não casa, e nada aqui relata um quase-acerto — a única prova de que uma regra funciona é vê-la funcionar.",
  "connect.connectors.foot":
    "O Connect informa o estado de um conector separadamente do estado das suas tarefas, então um conector pode dizer RUNNING enquanto todas as tarefas abaixo dele falharam. As contagens de tarefas em cada linha são a leitura em que confiar.",
  "connect.tasks.foot":
    "Reiniciar pede ao worker que reinicie a tarefa; o worker decide quando. Esta tabela só muda quando o Kavka lê os workers de novo.",
  "shareGroups.foot":
    "Os estados e as contagens de membros são a visão do coordenador no momento em que o Kavka perguntou. ∅ na coluna do offset inicial significa que o broker não informou nada para aquela partição — uma lacuna na resposta, não um zero.",

  // ── Configurações (Jackdaw) ─────────────────────────────────────────────
  "settings.title": "Configurações",
  "settings.navLabel": "Seções das configurações",
  "settings.perch":
    "Tudo aqui vale na hora e fica salvo nesta máquina. O Kavka está mostrando o tema {theme} agora.",
  "settings.section.appearance": "Aparência",
  "settings.section.appearance.sub":
    "Como o Kavka aparece nesta máquina. Nada aqui altera um cluster.",
  "settings.section.language": "Idioma",
  "settings.section.language.sub":
    "As palavras do próprio Kavka — o trilho, a paleta, os formulários e a frase de abertura de cada tela. As tabelas abaixo delas continuam em inglês.",
  "settings.section.about": "Sobre",
  "settings.section.about.sub":
    "Qual build é esta, qual licença ela carrega e os dois arquivos que ela pode escrever sobre si mesma.",

  "settings.theme.title": "Tema",
  "settings.theme.help":
    "Sistema segue o seu sistema operacional e muda junto enquanto o Kavka está aberto.",
  "settings.theme.contrast":
    "Ambos os temas são verificados contra o mesmo piso de contraste: 4,5:1 para tudo o que você lê, 3:1 para a borda de tudo em que você pode clicar. Nada é esmaecido para parecer mais calmo.",
  "settings.theme.system": "Sistema",
  "settings.theme.light": "Claro",
  "settings.theme.dark": "Escuro",

  "settings.accent.title": "Cor de destaque",
  "settings.accent.note":
    "Destaque: {name}. Usado para aquilo que você está prestes a clicar e para a linha selecionada — nunca para status, então mudá-lo não pode esconder um aviso.",
  "settings.accent.brass": "Latão",
  "settings.accent.moss": "Musgo",
  "settings.accent.sky": "Céu",
  "settings.accent.plum": "Ameixa",

  "settings.density.title": "Densidade",
  "settings.density.help":
    "Confortável dá espaço a cada linha. Compacta mostra cerca de um terço a mais de linhas — a altura com que o Kavka saiu.",
  "settings.density.comfortable": "Confortável",
  "settings.density.compact": "Compacta",

  "settings.font.title": "Tamanho do texto",
  "settings.font.help":
    "Escala todos os tamanhos juntos, para que nada se sobreponha no maior passo.",
  "settings.font.s": "Pequeno",
  "settings.font.m": "Médio",
  "settings.font.l": "Grande",

  "settings.motion.title": "Movimento",
  "settings.motion.help":
    "Sistema segue a preferência de movimento reduzido do seu sistema operacional. Reduzido também desliga todas as animações do Kavka.",
  "settings.motion.system": "Sistema",
  "settings.motion.reduce": "Reduzido",

  "settings.env.title": "Cores de ambiente",
  "settings.env.help":
    "{count, plural, one {# ambiente está} other {# ambientes estão}} configurado(s). Cor é identidade; protegido é a proteção — então aqui você também decide com quais o Kavka deve ter cuidado. Diferente de tudo o mais nesta tela, estes viajam junto com uma conexão que você exporta.",
  "settings.env.manage": "Gerenciar ambientes",

  "settings.perch.title": "A nota de cada tela",
  "settings.perch.help":
    "A nota quente no topo de cada tela, que diz o que o Kavka consegue contar ali. “Uma linha” mantém o veredito e dispensa a ressalva; “Oculta” desliga a nota nas telas que não têm nada a relatar. Uma tela ainda carregando, ou cuja leitura falhou, mostra a nota inteira em qualquer ajuste.",
  "settings.perch.full": "Completa",
  "settings.perch.line": "Uma linha",
  "settings.perch.hidden": "Oculta",

  "settings.sample.title": "Como isso fica",
  "settings.sample.sub":
    "uma amostra ao vivo das partes que você acabou de mudar",
  "settings.sample.note":
    "Estas três linhas são inventadas para você ver o que densidade e tamanho de texto fazem antes de descobrir na prática. Nada aqui veio de um cluster.",
  "settings.sample.caption":
    "Uma amostra de três linhas de mensagens inventadas, exibida para que mudanças de aparência sejam visíveis imediatamente.",
  "settings.sample.primary": "Um botão primário",
  "settings.sample.normal": "Um normal",
  "settings.sample.chip.ok": "Saudável",
  "settings.sample.chip.warn": "Ficando para trás",
  "settings.sample.focus":
    "Pressione {key} através destes para ver o anel de foco neste tema.",

  "settings.language.title": "Idioma",
  "settings.language.help":
    "Cobre a estrutura do Kavka e o veredito com que cada tela de cluster começa — a barra de navegação, a paleta, este painel, o formulário de conexão e a frase de abertura de cada tela. As tabelas e os formulários abaixo dessas frases continuam em inglês.",
  "settings.language.machine":
    "Este catálogo saiu de uma máquina e nenhum falante nativo o revisou. Correções são bem-vindas.",

  "settings.about.title": "Versão, licença e diagnóstico",
  "settings.about.help":
    "O painel Sobre traz a versão e a licença do Kavka, as configurações do servidor MCP e a chave do diagnóstico de falhas.",
  "settings.about.open": "Abrir Sobre",

  // About and Support Kavka came here when the sidebar footer was deleted.
  "settings.support.title": "Apoiar o Kavka",
  "settings.support.help":
    "O Kavka é gratuito, de código aberto e mantido por quem escolhe contribuir. Quem não contribui não fica sem nada.",

  // ── Updates ─────────────────────────────────────────────────────────────
  "settings.section.updates": "Atualizações",
  "settings.section.updates.sub":
    "Se o Kavka pergunta ao GitHub sobre novas versões, e o que essa requisição leva e não leva. Nada se instala sozinho.",

  "settings.updates.auto.title": "Procurar atualizações",
  "settings.updates.auto.label": "Deixar o Kavka procurar novas versões",
  "settings.updates.auto.hint":
    "Ligado por padrão. O Kavka pergunta ao github.com qual é a versão mais recente, no máximo uma vez por dia — a mesma pergunta que a página pública de Releases responde para qualquer pessoa. A requisição não leva nada que identifique você nem nada sobre seus clusters, e não baixa nem instala nada por conta própria. É a única requisição que o Kavka faz sem que você peça; desligue isto e não há nenhuma.",

  "settings.updates.channel.title": "Quais versões",
  "settings.updates.channel.help":
    "Estável segue as versões que uma pessoa marcou de propósito. Cada build segue a pré-versão publicada por cada merge na main — mais nova, e sem o mesmo critério.",
  "settings.updates.channel.stable": "Estável",
  "settings.updates.channel.builds": "Cada build",
  "settings.updates.channel.warning":
    "Builds são publicados automaticamente a partir da main. Eles compilam e passam nas verificações, mas ninguém decidiu que são bons. Escolha isto só se quiser o trabalho mais recente e puder reinstalar uma versão estável caso algum se comporte mal.",

  "settings.updates.check.title": "Verificar agora",
  "settings.updates.check.help":
    "Pergunta ao github.com na hora, independente do interruptor acima. Nada é baixado.",
  "settings.updates.check.button": "Verificar agora",
  "settings.updates.check.checking": "Perguntando ao github.com…",

  "settings.updates.result.update":
    "O Kavka {version} está disponível. O aviso no topo da janela tem o botão de instalar.",
  "settings.updates.result.currentStable":
    "Você está na versão estável mais recente.",
  "settings.updates.result.currentBuild": "Você está no build mais recente.",
  "settings.updates.result.noStable":
    "Nenhuma versão estável foi publicada ainda — por enquanto só existem builds automáticos de main. Mude para Cada build para acompanhá-los.",

  "settings.updates.lastChecked": "O Kavka verificou pela última vez em {when}.",
  "settings.updates.lastCheckedFailed":
    "O Kavka tentou pela última vez em {when} e não conseguiu alcançar o github.com.",
  "settings.updates.never": "O Kavka ainda não verificou.",

  "updates.banner.label": "Aviso de atualização — Kavka {version}",
  "updates.banner.title": "O Kavka {version} está disponível",
  "updates.banner.body":
    "Nada foi baixado. O Kavka busca o instalador só quando você aperta Instalar, e o confere com a própria chave de assinatura do Kavka antes de executar qualquer coisa.",
  "updates.banner.bodyBuild":
    "Este é um build automático do merge mais recente na main, não uma versão estável — ninguém decidiu que ele é bom. Nada foi baixado; o Kavka busca o instalador só quando você aperta Instalar, e o confere com a própria chave de assinatura do Kavka antes de executar qualquer coisa.",
  "updates.banner.willClose":
    "Instalar fecha o Kavka para que o instalador possa substituí-lo. Termine antes o que estiver fazendo e abra o Kavka de novo quando o instalador acabar.",
  "updates.banner.willRestart":
    "Instalar fecha o Kavka e o abre de novo assim que a atualização estiver no lugar. Termine antes o que estiver fazendo.",
  "updates.banner.notes": "O que mudou",
  "updates.banner.releasePage": "Página da versão",
  "updates.banner.install": "Instalar…",
  "updates.banner.installing": "Baixando…",
  "updates.banner.notNow": "Agora não",

  "updates.error.unreachable.title":
    "O Kavka não conseguiu alcançar o github.com",
  "updates.error.unreachable.detail":
    "Nada foi baixado e nada mudou nesta máquina. Verifique a conexão, ou se há um proxy ou firewall entre você e o github.com, e tente de novo.",
  "updates.error.title": "A atualização não terminou",
  "updates.error.detail":
    "Nada foi instalado e nada mudou nesta máquina. O texto completo está em «Ver detalhes», e a página da versão tem instaladores que você mesmo pode baixar.",

  "unit.seconds": "{count, plural, one {# segundo} other {# segundos}}",
  "unit.minutes": "{count, plural, one {# minuto} other {# minutos}}",
  "unit.hours": "{count, plural, one {# hora} other {# horas}}",
  "unit.days": "{count, plural, one {# dia} other {# dias}}",
};

export default ptBR;
