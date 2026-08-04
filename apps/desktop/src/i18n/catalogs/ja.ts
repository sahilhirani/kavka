// ─────────────────────────────────────────────────────────────────────────────
// Japanese (日本語) — MACHINE TRANSLATION — NATIVE REVIEW WELCOME.
//
// No native speaker has read this file. It was produced from `en.ts` and it is
// shipped honestly rather than quietly: the language picker in the About
// dialog says so next to the name, and `LOCALES` in ../index.ts carries
// `machine: true` for exactly this reason.
//
// If Japanese is your language, the highest-value contribution to Kavka is
// twenty minutes with this file. See docs/I18N.md — you need no build, no
// tooling and no account, and a partial fix is welcome: any key you delete
// falls back to English rather than breaking.
//
// Japanese is the catalog most likely to read as machine output, and the one
// where a reviewer's judgement matters most: this file is long-form
// explanatory prose, not labels, and the register (です・ます) was chosen
// without a human ear on it. Line length also matters — DESIGN.md budgets
// ~40% growth for translated toolbar labels, and Japanese usually runs
// SHORTER, which is its own layout problem.
//
// Two things to keep while editing: the {placeholders} (they are values Kavka
// substitutes, and a renamed one silently disappears from the sentence), and
// the plural arms — Japanese has only `other`, so `{count, plural, other
// {#件}}` is the whole shape; there is no `one` arm to add.
// ─────────────────────────────────────────────────────────────────────────────

import type { Catalog } from "./en";

const ja: Catalog = {
  "common.close": "閉じる",
  "common.cancel": "キャンセル",
  "common.save": "保存",
  "common.connect": "接続",
  "common.tryAgain": "再試行",
  "common.remove": "削除",
  "common.dismiss": "閉じる",
  "common.showDetails": "詳細を表示",
  "common.addConnection": "接続を追加",
  "common.support": "Kavka を支援する ☕",
  "common.readingConnections": "保存された接続を読み込んでいます…",
  "common.linkFailed":
    "Kavka はそのリンクをブラウザーに渡せませんでした。アドレスは {url} です — ここからコピーしてください。",

  "sidebar.navLabel": "保存された接続",
  "sidebar.title": "クラスター",
  "sidebar.loading": "接続を読み込んでいます…",
  "sidebar.empty": "まだ何もありません。下から最初の接続を追加してください。",
  "sidebar.profileMeta": "{address} · {status}",
  "sidebar.status.disconnected": "未接続",
  "sidebar.status.connecting": "接続中…",
  "sidebar.status.connected": "接続済み",
  "sidebar.draftName": "新しい接続",
  "sidebar.draftMeta": "未保存",
  "sidebar.about": "情報",

  "app.status.disconnected": "未接続",
  "app.status.connecting": "接続中…",
  "app.status.connected": "接続済み",
  "app.error.unknownProfile":
    "その接続はもうこのマシンにありません。別のウィンドウで削除された可能性があります。",
  "app.profilesFailed.title": "Kavka は接続ファイルを読み込めませんでした",
  "app.profilesFailed.hint":
    "接続はディスク上にそのまま残っています — 失われたものはありません。Kavka はこのアプリの設定と並べて、設定ディレクトリに保存しています。",
  "app.firstRun.title": "Kavka にブローカーを教えてください",
  "app.firstRun.what":
    "接続とは、1 つの Kafka クラスターの保存されたアドレスです — 名前、出発点となるブローカー 1 つ、そしてサインインの方法。クラスターの残りは Kavka がそこから見つけます。",
  "app.firstRun.example":
    "ブートストラップサーバーは通常 {example} のような形です。このリポジトリの開発用クラスターを動かしていますか？ その場合は {local} を使ってください。",
  "app.firstRun.footnote":
    "パスワードはお使いの OS のキーチェーンに保存されます。クラスターに関する情報がこのマシンから出ることはありません。",
  "app.pick.title": "接続を選んでください",
  "app.pick.hint":
    "左でクラスターを選ぶとブローカーとトピックが表示されます。別の接続を追加することもできます。",
  "app.readonlyChip": "読み取り専用",
  "app.readonlyTitle":
    "この接続は読み取り専用です。書き込みや編集を行うには、接続の設定でオフにしてください。",
  "app.statusbar.draft": "新しい接続 — 未保存",
  "app.statusbar.none": "接続が選択されていません",
  "app.statusbar.commands": "コマンド",
  "app.statusbar.coreVersion": "core v{version}",
  "app.cmd.search": "{topic} 内を検索",
  "app.cmd.search.kw":
    "find filter cel scan query messages grep 検索 フィルター メッセージ",
  "app.cmd.sql": "{topic} を SQL で照会",
  "app.cmd.sql.kw":
    "sql select query aggregate count group datafusion analyse 照会 集計 分析",
  "app.cmd.produce": "{topic} に送信",
  "app.cmd.produce.kw":
    "send write publish message record bulk producer 送信 書き込み 発行 メッセージ",
  "app.cmd.produce.confirmContext": "{cluster} · 確認を求めます",

  "palette.label": "コマンド",
  "palette.searchLabel": "コマンドとクラスターを検索",
  "palette.searchPlaceholder": "コマンドとクラスターを検索…",
  "palette.empty":
    "「{query}」に一致するものはありません。クラスター名を試すか、入力欄を空にすると Kavka にできることがすべて表示されます。",
  "palette.foot.move": "移動",
  "palette.foot.run": "実行",
  "palette.foot.close": "閉じる",
  "palette.goTo": "{name} へ移動",
  "palette.connectTo": "{name} に接続",
  "palette.state.connected": "接続済み",
  "palette.state.connecting": "接続中…",
  "palette.protectedCluster": "保護されたクラスター",
  "palette.profile.kw":
    "connect open switch cluster broker bootstrap 接続 開く 切り替え",
  "palette.add.context": "名前、ブローカー 1 つ、そしてサインインの方法",
  "palette.add.kw":
    "new connection profile cluster create bootstrap broker 新規 接続 作成",
  "palette.disconnect": "切断",
  "palette.disconnect.kw": "close leave cluster session 切断 終了 閉じる",
  "palette.disconnect.none": "現在、接続されているものはありません",
  "palette.disconnect.ambiguous":
    "先にサイドバーで切断したいクラスターを選んでください",
  "palette.refresh": "トピックを再読み込み",
  "palette.refresh.kw":
    "reload metadata list topics partitions cluster 再読み込み 更新",
  "palette.export": "接続をエクスポート…",
  "palette.export.context": "このマシン上のすべての接続を JSON として",
  "palette.export.kw":
    "backup save copy share json profiles バックアップ エクスポート コピー",
  "palette.import": "接続をインポート…",
  "palette.import.context": "別の Kavka からの JSON を貼り付け",
  "palette.import.kw":
    "restore paste load json profiles 貼り付け 読み込み 復元",
  "palette.about.context": "バージョンとライセンス",
  "palette.about.kw":
    "version licence license agpl source github help バージョン ライセンス ヘルプ",
  "palette.support.context": "Kavka は無料です — 寄付がそれを支えます",
  "palette.support.kw":
    "donate coffee sponsor fund open source 寄付 コーヒー 支援",

  "about.title": "Kavka について",
  "about.body":
    "Apache Kafka のためのデスクトップクライアントです。Kavka は完全にこのマシン上で動作します。パスワードはお使いの OS のキーチェーンに保存され、クラスターに関する情報がこのコンピューターから出ることはありません。",
  "about.coreVersion": "コアのバージョン",
  "about.versionLoading": "読み込んでいます…",
  "about.licence": "ライセンス",
  "about.licenceValue": "AGPL-3.0 のもとで自由に使えるオープンソース",
  "about.language": "言語",
  "about.language.hint":
    "Kavka の外枠 — サイドバー、コマンドパレット、これらのダイアログ、接続フォーム。クラスターの画面はまだ英語です。次に翻訳される予定です。",
  "about.language.machine":
    "{language} は機械翻訳であり、ネイティブスピーカーによる確認を受けていません。修正を歓迎します — 方法は docs/I18N.md にあります。",

  "transfer.title": "接続",
  "transfer.tablist": "エクスポートまたはインポート",
  "transfer.tab.export": "エクスポート",
  "transfer.tab.import": "インポート",
  "transfer.export.body":
    "このマシン上のすべての接続を JSON として出力します。別の Kavka に貼り付ければ、同じクラスターをそちらで設定できます。",
  "transfer.export.promise":
    "パスワードと鍵がこのマシンから出ることはありません — エクスポートに含まれるのは参照であって、秘密情報ではありません。",
  "transfer.export.failed":
    "Kavka は接続ファイルを読み込めませんでした。接続はディスク上にそのまま残っています — 失われたものはありません。",
  "transfer.export.label": "あなたの接続、JSON 形式",
  "transfer.export.copied": "クリップボードにコピーしました。",
  "transfer.export.copyManual":
    "Kavka はクリップボードにアクセスできませんでした。テキストは選択済みです — {key} を押してコピーしてください。",
  "transfer.export.copy": "クリップボードにコピー",
  "transfer.export.nothingToCopy":
    "コピーするものがありません — Kavka は接続ファイルを読み込めませんでした",
  "transfer.export.stillReading": "Kavka はまだ接続を読み込んでいます",
  "transfer.import.body":
    "別の Kavka からのエクスポートを貼り付けてください。パスワードは含まれていません — インポートされた各接続は、最初に接続するときに自分のパスワードを尋ねます。",
  "transfer.import.label": "エクスポートされた JSON",
  "transfer.import.kbd": "インポート",
  "transfer.import.kbdClose": "閉じる",
  "transfer.import.legend": "同じ接続がすでにある場合",
  "transfer.import.skip": "このマシンのものを残す",
  "transfer.import.skipHint":
    "すでにここに保存されている接続はそのまま残ります。JSON に含まれる新しいものは追加されます。",
  "transfer.import.replace": "JSON のもので置き換える",
  "transfer.import.replaceHint":
    "貼り付けた側が優先されます — 名前、アドレス、環境、サインイン方法。すでにキーチェーンにあるパスワードはそのまま残ります。",
  "transfer.import.failed":
    "Kavka はそれをエクスポートとして読み込めませんでした。外側の波かっこを含めてファイル全体を貼り付けたか確認してください — Kavka が受け取ったテキストは下にあります。",
  "transfer.import.needsJson":
    "先にエクスポートの JSON を貼り付けてください",
  "transfer.import.busy": "Kavka はいまその接続をインポートしています",
  "transfer.import.run": "接続をインポート",
  "transfer.import.running": "インポート中…",
  "transfer.report.empty.title": "その JSON に接続は含まれていませんでした",
  "transfer.report.empty.detail":
    "外側の波かっこを含めてエクスポート全体を貼り付けたか確認してください — Kavka は問題なく読み込めましたが、追加するものがありませんでした。",
  "transfer.report.added": "{count, plural, other {#件を追加}}",
  "transfer.report.replaced": "{count, plural, other {#件を置き換え}}",
  "transfer.report.skipped":
    "{count, plural, other {#件をスキップ — すでにこのマシンにあります}}",
  "transfer.report.envAdded": "{count, plural, other {# 件の環境を追加}}",
  "transfer.report.envSkipped": "{count, plural, other {# 件の環境は定義済み}}",
  "transfer.report.envOnly.title": "新しい接続はありません — 環境のみ",
  "transfer.report.unchanged.title":
    "{count, plural, other {変更はありません — #件の接続はすでにここにありました}}",
  "transfer.report.unchanged.detail":
    "{bits}。上書きするつもりだった場合は、上の「JSON のもので置き換える」を選んでください。",
  "transfer.report.imported.title":
    "{count, plural, other {#件の接続をインポートしました}}",
  "transfer.report.imported.detail":
    "{bits}。パスワードはエクスポートに含まれていません — 新しい接続をそれぞれ開き、接続する前にパスワードを入力してください。",

  "editor.new.title": "接続を追加",
  "editor.new.subtitle":
    "始めるにはブローカー 1 つで十分です — クラスターの残りは Kavka がそこから見つけます。",
  "editor.saved.subtitle": "未接続です。下の内容を確認してから接続してください。",
  "editor.name.label": "接続の名前",
  "editor.name.placeholder": "orders — local",
  "editor.name.hint":
    "サイドバーで見分けられる名前なら何でも構いません。見えるのは Kavka だけです。",
  "editor.env.label": "環境",
  "editor.env.hint.protected":
    "この環境は保護対象として設定されています。すべての表で台帳の罫線がこの色になり、サイドバーでこのクラスターが目印付きになり、ウィンドウ上部に警告バーが表示され、破壊的な操作のたびに名前の入力を求められます。実際に書き込む必要がなければ、下の読み取り専用をオンにしてください。",
  "editor.env.hint.other":
    "Kavka はすべての画面を環境ごとに色分けするので、クラスターを取り違えることがありません。",
  "editor.env.manage": "環境を管理…",
  "editor.env.hint.unknown":
    "このマシンには {name} の定義がないため、Kavka は中立的なグレーで表示し、ガードレールを適用しません。「環境を管理」で追加すると、色を割り当て、保護するかどうかを決められます。",
  "editor.bootstrap.label": "ブートストラップサーバー",
  "editor.bootstrap.hint":
    "クラスター内のどれか 1 つのブローカー — 残りは Kavka がそこから見つけます。1 行に 1 つ、またはカンマ区切りで。このリポジトリの開発用クラスターを動かしていますか？ その場合は {local} を使ってください。",

  "editor.auth.legend": "サインイン",
  "editor.auth.kerberos":
    "この接続は Kerberos でサインインします（{principal} として {service}）。Kavka はまだこれを設定できません。保存してもそのまま維持され、ここにある他の項目はすべて通常どおり使えます。",
  "editor.auth.label": "このクラスターはどうやって本人確認をしますか？",
  "editor.auth.plaintext": "確認しない — 誰でも接続できる（PLAINTEXT）",
  "editor.auth.saslPlain": "ユーザー名とパスワード — SASL/PLAIN",
  "editor.auth.saslScram": "ユーザー名とパスワード — SASL/SCRAM",
  "editor.auth.mtls": "このマシンが提示する証明書 — mTLS",
  "editor.auth.mskIam": "このマシンにある AWS 認証情報 — MSK IAM",
  "editor.auth.oauth": "ID プロバイダーからのトークン — OAuth 2.0 / OIDC",
  "editor.auth.kerberosOption": "Kerberos チケット — GSSAPI（未対応）",
  "editor.auth.notYet":
    "Kavka はまだこれを設定できません。すでにこれを使っている接続はそのまま動作し、保存してもそのまま維持されます。",
  "editor.auth.hint":
    "マネージド Kafka はたいてい SASL/SCRAM と TLS のオンを求めます。ローカルのブローカーはたいてい何も求めません。Kerberos だけは Kavka がまだ設定できません。",
  "editor.mechanism.label": "SCRAM の方式",
  "editor.mechanism.hint":
    "ブローカーが一方を拒否した場合、どちらを求めているか教えてくれます。",
  "editor.username.label": "ユーザー名",
  "editor.password.label": "パスワード",
  "editor.password.placeholder": "パスワード",
  "editor.secret.unchanged": "••••••••（変更なし）",
  "editor.password.hint":
    "お使いの OS のキーチェーンに保存されます — 接続ファイルに書き込まれることはなく、このマシンから出ることもありません。",
  "editor.tls.label": "接続を暗号化する（TLS）",
  "editor.tls.hint":
    "マネージド Kafka ではほぼ常にオンが必要です。ブローカーは応答するのにハンドシェイクが失敗する場合、まず試すべきはここです。",

  "editor.mtls.hint":
    "Kavka は PEM ファイルをそのまま読み込みます — 事前に JKS や PKCS#12 のキーストアへ変換する必要はありません。",
  "editor.caPath.label": "CA 証明書",
  "editor.caPath.hint":
    "CA の .pem へのパス — 空のままにするとシステムのトラストストアを使います。",
  "editor.clientCert.label": "クライアント証明書",
  "editor.clientCert.hint":
    "このマシンがブローカーに提示する証明書へのパス — ブローカーが求めない場合は空のままで構いません。",
  "editor.clientKey.label": "クライアント秘密鍵",
  "editor.clientKey.hint":
    "パスではなく鍵そのものを貼り付けてください。お使いの OS のキーチェーンに保存されます — 接続ファイルに書き込まれることはなく、このマシンから出ることもありません。",
  "editor.clientKey.storedHint":
    "保存済みの鍵をそのまま使うには空のままにしてください。上の証明書のパスを消すと鍵も削除されます。",

  "editor.aws.hint":
    "Kavka は、このマシンにすでにある AWS 認証情報で各リクエストに署名します。上のブートストラップサーバーは、このクラスターの IAM エンドポイントである必要があります — MSK コンソールにある {host} のホストで、通常はポート 9098 です。",
  "editor.region.label": "リージョン",
  "editor.region.hint":
    "クラスターが動作している AWS リージョン。ブートストラップのホストと一致していないと、署名が受け付けられません。",
  "editor.awsProfile.label": "AWS プロファイル名",
  "editor.awsProfile.hint":
    "{config} にある名前付きプロファイル。空のままにすると既定の認証情報チェーンを使います — 環境変数、次に {dir}、次に SSO。",

  "editor.oauth.hint":
    "Kavka はクライアントクレデンシャルグラントで ID プロバイダーからトークンを取得し、それを SASL/OAUTHBEARER としてブローカーに提示します。",
  "editor.tokenEndpoint.label": "トークンエンドポイント",
  "editor.tokenEndpoint.hint":
    "トークンを発行する URL であって、ブラウザーが使うサインインページではありません。",
  "editor.clientId.label": "クライアント ID",
  "editor.clientId.hint":
    "ID プロバイダーが Kafka のために登録したアプリケーション — あなた自身のユーザーアカウントではありません。",
  "editor.clientSecret.label": "クライアントシークレット",
  "editor.clientSecret.placeholder": "クライアントシークレット",
  "editor.clientSecret.hint":
    "お使いの OS のキーチェーンに保存されます — 接続ファイルに書き込まれることはなく、このマシンから出ることもありません。",

  "editor.sr.legend": "Schema Registry（任意）",
  "editor.sr.hint":
    "このクラスターのメッセージが Avro、Protobuf、JSON Schema の場合、Kavka はここからスキーマを読み取ってデコードし、各メッセージの横にサブジェクト、バージョン、ID を表示します。指定がない場合、それらのペイロードは生のバイト列として表示されます。",
  "editor.srUrl.label": "レジストリのアドレス",
  "editor.srUrl.hint":
    "スキームを含む完全な URL。Confluent、Apicurio、Glue はここで同じ読み取り API を話します。このクラスターにレジストリがない場合は空のままにしてください。",
  "editor.srUsername.label": "レジストリのユーザー名",
  "editor.srUsername.hint":
    "レジストリが求める場合のみ。マネージドのレジストリはたいてい求めます。自社ネットワーク内のレジストリはたいてい求めません。",
  "editor.srPassword.label": "レジストリのパスワード",
  "editor.srPassword.storedHint":
    "保存済みのものをそのまま使うには空のままにしてください。上のアドレスを消すとパスワードも削除されます。",

  "editor.connect.legend": "Kafka Connect クラスター（任意）",
  "editor.connect.hint":
    "Kafka Connect はソースコネクターとシンクコネクターを動かし、ブローカー経由ではなく独自の REST ポートで応答します — そのため Kavka にワーカーの場所を教える必要があります。ワーカーグループごとに 1 つ追加してください。Connect タブではこの名前で選び分けます。",
  "editor.connect.unnamed": "クラスター {number}",
  "editor.connect.removeLabel": "{name} を削除",
  "editor.connect.unnamedLong": "Connect クラスター {number}",
  "editor.connect.remove": "この Connect クラスターを接続から削除",
  "editor.connect.name.label": "名前",
  "editor.connect.name.placeholder": "orders connect",
  "editor.connect.name.hint":
    "見分けられる名前なら何でも構いません。あとで名前を変えても、保存済みのパスワードはそのまま残ります。",
  "editor.connect.url.label": "ワーカーのアドレス",
  "editor.connect.url.hint":
    "グループ内のどれか 1 つのワーカーの REST エンドポイント — どれもクラスター全体について応答します。通常はポート 8083 で、ブローカーとは別のホストとポートです。",
  "editor.connect.username.hint":
    "ワーカーが Basic 認証の内側にある場合のみ。多くはそうではありません。",
  "editor.connect.password.storedHint":
    "保存済みのものをそのまま使うには空のままにしてください。このクラスターを削除するとパスワードも削除されます。",
  "editor.connect.add": "Connect クラスターを追加",

  "editor.monitoring.legend": "モニタリング（任意）",
  "editor.monitoring.hint":
    "Kafka のブローカーは、スループット、ストレージ、レプリケーションの数値を Kafka プロトコルでは提供しません — JMX として公開し、ほとんどの場合その前段に Prometheus エクスポーターを置きます。Kavka にエクスポーターを指定すると、モニタリングタブが埋まります。ラグの履歴にこれは不要です。Kavka がブローカーから直接読み取ります。",
  "editor.metricsUrl.label": "メトリクスのアドレス",
  "editor.metricsUrl.hint":
    "パスを含む完全な URL。ブローカーを自分で運用している場合、通常はそのいずれかで動く Java エージェント {agent} です（{flag}）。それらのブローカーをすでにスクレイプしている Prometheus サーバーでも構いません — その場合は Kavka にそのアドレスを教えてください。このクラスターにエクスポーターがない場合は空のままにしてください。",
  "editor.metricsUsername.label": "メトリクスのユーザー名",
  "editor.metricsUsername.hint":
    "エンドポイントが Basic 認証の内側にある場合のみ。jmx_exporter はたいてい不要で、共有の Prometheus はたいてい必要です。",
  "editor.metricsPassword.label": "メトリクスのパスワード",
  "editor.metricsPassword.storedHint":
    "保存済みのものをそのまま使うには空のままにしてください。上のアドレスを消すとパスワードも削除されます。",
  "editor.sampler.label": "ラグを測定する間隔",
  "editor.sampler.hint":
    "秒。Kafka はラグを記録しないため、Kavka がこの間隔で独自に測定し、このマシン上のファイルに {days} 分を保持します。{warning} 下限は {floor}、既定値は {default} で、1 回の測定につきグループごとに小さなリクエストが 1 つかかります。",
  "editor.sampler.warning":
    "測定はこの接続が有効な間だけ行われます — Kavka が閉じている間やこのクラスターが切断されている間は何も収集されず、グラフの空白はまさにそれを意味します。",

  "editor.readonly.label": "読み取り専用の接続",
  "editor.readonly.hint":
    "Kavka はすべてを閲覧できますが、この接続でメッセージを送信したり、トピックを変更したり、オフセットをコミットしたりはしません。",

  "editor.busy.connecting": "接続の試行が終わるまでお待ちください",
  "editor.busy.saving": "Kavka はこの接続を保存しています",
  "editor.delete": "接続を削除",
  "editor.delete.confirm":
    "{name} をこのマシンから削除しますか？ クラスター自体には手を触れません。",
  "editor.kbd.connect": "接続",
  "editor.kbd.cancel": "キャンセル",
  "editor.kbd.undo": "編集を元に戻す",

  "editor.err.name":
    "サイドバーで見つけられるように、この接続に名前を付けてください。",
  "editor.err.bootstrap":
    "ブローカーを少なくとも 1 つ、host:port の形式で追加してください — 例: broker-1:9092",
  "editor.err.srUrl":
    "http:// または https:// で始まる完全な URL を入力してください — 例: http://localhost:8081",
  "editor.err.srUserNoUrl":
    "レジストリのアドレスを追加するか、ユーザー名を消してください — 接続先のないサインインは保存できません。",
  "editor.err.metricsUrl":
    "http:// または https:// で始まる完全な URL を入力してください — 例: http://broker-1.internal:7071/metrics",
  "editor.err.metricsUserNoUrl":
    "メトリクスのアドレスを追加するか、ユーザー名を消してください — 接続先のないサインインは保存できません。",
  "editor.err.sampler":
    "測定は最短でも {seconds, plural, other {#秒}} ごとにしてください。これより速いと、オフセットが変化する頻度を超えてブローカーに問い合わせることになります。",
  "editor.err.connectName":
    "この Connect クラスターに名前を付けてください — Kavka が送るすべての操作は、宛先のクラスター名を伴います。",
  "editor.err.connectDuplicate":
    "1 つの接続に属する 2 つの Connect クラスターが同じ名前を使うことはできません — Kavka はその名前でパスワードを保存します。",
  "editor.err.connectUrlMissing":
    "ワーカーの REST アドレスを追加してください — 例: http://connect-1.internal:8083",
  "editor.err.connectUrl":
    "http:// または https:// で始まる完全な URL を入力してください — 例: http://connect-1.internal:8083",
  "editor.err.username":
    "このサインイン方法には、ブローカーがあなたを識別するユーザー名が必要です。",
  "editor.err.password": "このサインイン方法にはパスワードが必要です。",
  "editor.err.clientKey":
    "その証明書に対応する秘密鍵を貼り付けてください — Kavka には両方が必要です。",
  "editor.err.clientCert":
    "この鍵が属する証明書のパスを追加してください — Kavka には両方が必要です。",
  "editor.err.region":
    "クラスターが動作しているリージョンを指定してください — 例: eu-west-1",
  "editor.err.tokenEndpoint":
    "ID プロバイダーがトークンを発行する URL を追加してください — 例: https://login.example.com/oauth2/token",
  "editor.err.tokenEndpointUrl":
    "https:// で始まる完全な URL を入力してください — 例: https://login.example.com/oauth2/token",
  "editor.err.clientId":
    "ID プロバイダーがこのアプリケーション用に発行したクライアント ID を追加してください。",
  "editor.err.clientSecret":
    "このサインイン方法には、そのクライアント ID に対応するシークレットが必要です。",


  // ── 環境 ────────────────────────────────────────────────────────────────
  "env.color.green": "グリーン",
  "env.color.amber": "アンバー",
  "env.color.red": "レッド",
  "env.color.blue": "ブルー",
  "env.color.violet": "バイオレット",
  "env.color.cyan": "シアン",
  "env.color.slate": "スレート",

  "env.mgr.title": "環境",
  "env.mgr.intro": "組織が実際に運用している環境に名前を付けてください。色はひと目で見分けるためのもので、保護がガードレールです。",
  "env.mgr.failed": "処理できませんでした",
  "env.mgr.working": "Kavka が処理中です",
  "env.mgr.add": "環境を追加",
  "env.mgr.edit": "編集",

  "env.mgr.row.protected": "保護あり",
  "env.mgr.row.unprotected": "保護なし",
  "env.mgr.row.used": "{count, plural, =0 {接続なし} other {# 件の接続}}",

  "env.mgr.name.label": "名前",
  "env.mgr.name.hint":
    "チームでの呼び方をそのまま — dev、QA、UAT、production など。入力したとおりに表示され、翻訳されることはありません。",
  "env.mgr.name.taken": "この名前の環境はすでにあります。",
  "env.mgr.name.required": "先に環境の名前を入力してください",

  "env.mgr.color.label": "色",
  "env.mgr.color.hint":
    "識別のためだけのものです。色は台帳の罫線とチップに反映されますが、Kavka で何ができるかを決めることはありません。",

  "env.mgr.protected.label": "この環境を保護対象として扱う",
  "env.mgr.protected.hint":
    "Kavka は警告用の下地に切り替え、破壊的な操作の前にトピック名やグループ名の入力を求め、ウィンドウに目印を付け、明示的に許可されていない限りコマンドラインや AI アシスタントからの書き込みを拒否します。",
  "env.mgr.unprotect.prompt": "保護を解除するには {name} と入力してください",
  "env.mgr.unprotect.hint":
    "{name} のすべての接続からガードレールがなくなります。入力による確認はなくなり、コマンドラインや AI アシスタントも書き込みを拒否しなくなります。",

  "env.mgr.delete.title": "{name} を削除しますか？",
  "env.mgr.delete.unused": "{name} を使っている接続はないため、他に変わるものはありません。",
  "env.mgr.delete.used":
    "{count, plural, other {# 件の接続}}が {name} を使っています。移動先を選んでください — Kavka は削除する前に移動します。",
  "env.mgr.delete.moveTo": "これらの接続の移動先",
  "env.mgr.delete.moveHint": "移動する接続: {names}。",
  "env.mgr.delete.confirm": "環境を削除",
  "env.mgr.delete.needTarget": "これらの接続の移動先となる環境を選んでください。",
  "env.mgr.delete.last": "残っている環境はこれだけです — 先に別の環境を追加してください",

  "unit.seconds": "{count, plural, other {#秒}}",
  "unit.minutes": "{count, plural, other {#分}}",
  "unit.hours": "{count, plural, other {#時間}}",
  "unit.days": "{count, plural, other {#日}}",
};

export default ja;
