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

  "sidebar.navLabel": "Conexões salvas",
  "sidebar.title": "Clusters",
  "sidebar.loading": "Lendo suas conexões…",
  "sidebar.empty": "Ainda não há nada aqui. Adicione sua primeira conexão abaixo.",
  "sidebar.profileMeta": "{address} · {status}",
  "sidebar.status.disconnected": "não conectado",
  "sidebar.status.connecting": "conectando…",
  "sidebar.status.connected": "conectado",
  "sidebar.draftName": "Nova conexão",
  "sidebar.draftMeta": "ainda não salva",
  "sidebar.about": "Sobre",

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
    "Escolha um cluster à esquerda para ver seus brokers e topics, ou adicione outra conexão.",
  "app.readonlyChip": "somente leitura",
  "app.readonlyTitle":
    "Esta conexão é somente leitura. Desative isso nas configurações da conexão para produzir ou editar.",
  "app.statusbar.draft": "Nova conexão — ainda não salva",
  "app.statusbar.none": "Nenhuma conexão selecionada",
  "app.statusbar.commands": "comandos",
  "app.statusbar.coreVersion": "core v{version}",
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
  "palette.prodCluster": "cluster de produção",
  "palette.profile.kw":
    "connect open switch cluster broker bootstrap conectar abrir trocar",
  "palette.add.context": "Um nome, um broker e como se autenticar",
  "palette.add.kw":
    "new connection profile cluster create bootstrap broker nova conexão criar",
  "palette.disconnect": "Desconectar",
  "palette.disconnect.kw": "close leave cluster session desconectar sair fechar",
  "palette.disconnect.none": "Nada está conectado no momento",
  "palette.disconnect.ambiguous":
    "Escolha primeiro na barra lateral o cluster que quer desconectar",
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
  "about.licence": "Licença",
  "about.licenceValue": "Livre e de código aberto sob a AGPL-3.0",
  "about.language": "Idioma",
  "about.language.hint":
    "A moldura do Kavka — a barra lateral, a paleta de comandos, estas caixas de diálogo e o formulário de conexão. As telas de cluster ainda estão em inglês; elas são a próxima coisa a ser traduzida.",
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
  "transfer.report.unchanged.title":
    "{count, plural, one {Nada mudou — # conexão já estava aqui} other {Nada mudou — # conexões já estavam aqui}}",
  "transfer.report.unchanged.detail":
    "{bits}. Escolha “Substituir pela do JSON” acima se a intenção era sobrescrevê-las.",
  "transfer.report.imported.title":
    "{count, plural, one {# conexão importada} other {# conexões importadas}}",
  "transfer.report.imported.detail":
    "{bits}. As senhas não estão em uma exportação — abra cada conexão nova e digite a senha dela antes de conectar.",

  "editor.new.title": "Adicionar uma conexão",
  "editor.new.subtitle":
    "Um broker já basta para começar — o Kavka descobre o resto do cluster a partir daí.",
  "editor.saved.subtitle":
    "Não conectado. Confira os dados abaixo e depois conecte.",
  "editor.name.label": "Nome da conexão",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "O que você reconhecer na barra lateral. Só o Kavka vê isso.",
  "editor.env.label": "Ambiente",
  "editor.env.hint.prod":
    "Produção deixa a régua do razão em coral em todas as tabelas, marca este cluster na barra lateral e coloca uma faixa de aviso no topo da janela. Ative o modo somente leitura abaixo, a menos que você realmente precise escrever.",
  "editor.env.hint.other":
    "O Kavka colore cada tela por ambiente, para você não confundir um cluster com outro.",
  "editor.bootstrap.label": "Servidores bootstrap",
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
    "Dê um nome a esta conexão para você encontrá-la na barra lateral.",
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

  "unit.seconds": "{count, plural, one {# segundo} other {# segundos}}",
  "unit.minutes": "{count, plural, one {# minuto} other {# minutos}}",
  "unit.hours": "{count, plural, one {# hora} other {# horas}}",
  "unit.days": "{count, plural, one {# dia} other {# dias}}",
};

export default ptBR;
