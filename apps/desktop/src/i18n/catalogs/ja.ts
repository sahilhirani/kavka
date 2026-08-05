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

  "confirm.kicker.destructive": "破壊的な操作",
  "confirm.busy": "Kavka が処理しています",
  "confirm.type.label": "確認のため {name} と入力してください",
  "confirm.type.reason": "確認するには {name} を正確に入力してください",

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
  "sidebar.settings": "設定",

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
  "about.build": "ビルド {number}",
  "about.licence": "ライセンス",
  "about.licenceValue": "AGPL-3.0 のもとで自由に使えるオープンソース",
  "about.language": "言語",
  "about.language.hint":
    "Kavka の外枠と、各クラスター画面が最初に示す判断 — サイドバー、コマンドパレット、これらのダイアログ、接続フォーム、そして各画面の冒頭の一文です。その下にある表やフォームはまだ英語です。",
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

  "editor.perch.screen": "接続",
  "editor.perch.new":
    "まだ何も保存されていません。Kavka はブローカーに接続していないため、この画面の内容は一切確認されていません。",
  "editor.perch.saved":
    "保存済みですが未接続です。Kavka はまだ {name} と通信していないため、ここの設定はクラスターで検証されていません。",
  "editor.perch.connected":
    "{name} に接続しました。Kavka はクラスターの概要をまだ読み込んでいます。",
  "editor.perch.caveat.protected":
    "{name} は保護対象です。このクラスターでの破壊的な操作は、まず名前の入力を求めます。",
  "editor.perch.caveat.unknown":
    "このマシンには {name} の定義がないため、この接続にはガードレールが適用されません。",
  "editor.perch.caveat.readonly":
    "読み取り専用が有効です。Kavka はこのクラスターを閲覧しますが、書き込みは行いません。",
  "editor.cluster.legend": "クラスター",
  "editor.guardrails.legend": "ガードレール",
  "editor.fold.set": "設定済み",
  "editor.fold.notSet": "未設定",
  "editor.fold.connectCount": "{count, plural, other {#個のクラスター}}",


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

  // ── クラスターのナビゲーション (Jackdaw) ────────────────────────────────
  "rail.label": "クラスターの画面",
  "rail.group.cluster": "クラスター",
  "rail.group.observe": "観測",
  "rail.group.safety": "安全",
  "rail.group.integrations": "連携",
  "rail.item.overview": "ホーム",
  "rail.item.topics": "トピック",
  "rail.item.groups": "コンシューマーグループ",
  "rail.item.brokers": "ブローカー",
  "rail.item.monitoring": "モニタリング",
  "rail.item.alerts": "アラート",
  "rail.item.streams": "ストリーム",
  "rail.item.acls": "ACL",
  "rail.item.masking": "マスキング",
  "rail.item.connect": "Connect",
  "rail.firing": "発報中",
  "rail.firingTitle": "{count, plural, other {アラートルール#件が現在発報中です}}",
  "rail.disconnect": "切断",

  // ── パーチ (Jackdaw) ────────────────────────────────────────────────────
  "perch.label": "{screen} — Kavka が言えること",
  "perch.kicker": "{screen} · {state}",
  "perch.state.ok": "問題なさそうです",
  "perch.state.watch": "確認をおすすめします",
  "perch.state.problem": "異常があります",
  "perch.state.unknown": "まだ判断できません",
  "perch.state.checking": "確認中",
  "perch.checking":
    "まだ確認中です。クラスターから応答があり次第、分かったことをお伝えします。",
  "perch.overview.counts":
    "{brokers, plural, other {ブローカー#台}}に接続しています。{topics, plural, other {トピック#件}}、{partitions, plural, other {パーティション#個}}です。",
  "perch.overview.firing":
    "{count, plural, other {このクラスターでアラートルール#件が現在発報中です}}。{counts}",
  "perch.overview.snapshot":
    "これらの数値は接続した時点のもので、クラスターに追随しません。取り直すには接続し直してください。",
  "perch.overview.noBrokers":
    "クラスターは応答しましたが、ブローカーを一つも返しませんでした。",
  "perch.overview.noBrokers.next":
    "多くの場合、Kafka 本体ではなくロードバランサーに到達したか、メタデータが空で返っています。切断して接続し直し、ブートストラップアドレスを確認してください。",
  "perch.screen.messages": "メッセージ",
  "perch.screen.search": "検索",
  "perch.screen.sql": "SQL",
  "perch.screen.schemas": "スキーマ",
  "perch.topics.unreadable":
    "Kavka はこのクラスターの Topic 一覧を取得できていません。",
  "perch.topics.unreadable.next":
    "接続が成立していても、アカウントにクラスターの Describe 権限がないことがあります。更新すると再度問い合わせます。",
  "perch.topics.empty":
    "このクラスターには Topic が 1 つもありません。まだ作成されていません。",
  "perch.topics.internalOnly":
    "このクラスターにあるのは Kafka 自身の内部 Topic だけです。「内部を表示」を有効にすると表示されます。",
  "perch.topics.counts":
    "{count, plural, other {このクラスターに #個の Topic}}。",
  "perch.topics.countsHidden":
    "{count, plural, other {#個の Topic を表示中}}。",
  "perch.topics.hiddenNote":
    "{count, plural, other {ほかに #個は Kafka 自身の内部 Topic のため非表示です}}。",
  "perch.topics.snapshot":
    "この一覧は画面を開いた時点で読み取ったもので、クラスターの変化には追従しません。更新すると読み直します。",
  "perch.topics.readOnly":
    "この接続は読み取り専用のため、ここから Topic の作成・変更・削除はできません。",
  "perch.topic.unreadable":
    "Kavka は {topic} のパーティション一覧を取得できていないため、中身について何も言えません。",
  "perch.topic.unreadable.next":
    "Topic が削除されたか、アカウントにその Describe 権限がない可能性があります。",
  "perch.topic.underReplicated":
    "{count, plural, other {ここでは #個のパーティションでコピーが不足しています}} — Kafka はこの Topic が求める数より少ないレプリカしか保持していません。",
  "perch.topic.unpreferred":
    "{count, plural, other {#個のパーティション}}のリーダーが、レプリカ一覧の先頭以外のブローカーになっています。再起動後にはよくあることで、「優先リーダーを選出」で元に戻せます。",
  "perch.topic.healthy":
    "{count, plural, other {#個のパーティション}}、すべてのコピーが同期しています。",
  "perch.topic.records":
    "オフセットから見ておよそ {records} 件のメッセージです。",
  "perch.topic.approx":
    "このメッセージ数は各パーティションの最古と最新のオフセットの差なので、保持期間やコンパクションで既に削除されたレコードも含んでいます。",
  "perch.messages.waiting":
    "まだ何も読み取っていません。上でどこから読むかを選び、「取得」を押してください。",
  "perch.messages.range":
    "指定した範囲から{count, plural, other {#件のメッセージ}}です。",
  "perch.messages.none": "指定した範囲には何もありません。",
  "perch.messages.topicEmpty": "{topic} にはまだメッセージがありません。",
  "perch.messages.live":
    "{topic} をライブで監視中です。追従を開始してから{count, plural, other {#件のメッセージ}}が届きました。",
  "perch.messages.liveQuiet":
    "{topic} をライブで監視中です。少なくとも 30 秒間、何も書き込まれていません。",
  "perch.messages.notWhole":
    "これは指定した範囲であって Topic 全体ではありません。{topic} にはおよそ {total} 件のメッセージがあります。",
  "perch.messages.dropped":
    "{count, plural, other {#件のメッセージ}}がこのウィンドウの処理速度を超えて到着し、セッションは遅延するよりも破棄することを選びました。画面の行は追従が見たすべてではありません。",
  "perch.messages.trimmed":
    "Kavka はライブ行を直近 {cap} 件だけ保持します。それより古いものは既にバッファから出ています。",
  "perch.messages.masked":
    "マスキングルールが有効なため、画面上の一部の値は Topic 上の値ではありません。コピーやエクスポートには置換後の値が入ります。",
  "perch.search.waiting":
    "まだ何もスキャンしていません。範囲を決め、探すものを指定して「検索」を押してください。",
  "perch.search.running":
    "{topic} をスキャン中です。今のところ{count, plural, other {#件の一致}}です。",
  "perch.search.running.note":
    "途中経過です。スキャンが終わるまでこの数値は動き続けます。",
  "perch.search.matches":
    "このスキャンが読んだ {scanned} 件のレコードのうち{count, plural, other {#件が一致}}しました。",
  "perch.search.none":
    "このスキャンが読んだ {scanned} 件のレコードには一致がありませんでした。",
  "perch.search.stopped":
    "このスキャンは {scanned} 件で停止したため、範囲の一部についての答えであって全体についてではありません。",
  "perch.search.capped":
    "{matched} 件のレコードが一致しましたが、Kavka が保持したのは {kept} 件です。画面上のものを並べ替え・エクスポート・集計しても、答えはその範囲についてであり、すべての一致についてではありません。",
  "perch.search.unevaluated":
    "{count, plural, other {#件のレコード}}は式に対して読み取れませんでした。不一致と判定されたのではなく、スキップされています。",
  "perch.search.masked":
    "マスキングルールが有効なため、画面上の値やエクスポートした内容の一部は Topic 上の値ではありません。",
  "perch.sql.waiting":
    "まだクエリを実行していません。上の範囲設定が、クエリの見えるレコードを決めます。",
  "perch.sql.running":
    "実行中です。今のところ {scanned} 件のレコードを読みました。",
  "perch.sql.running.note":
    "途中経過です。スキャンが終わるまで、下にあるものは最終的な答えではありません。",
  "perch.sql.rows":
    "このスキャンが読んだ {scanned} 件のレコードから{count, plural, other {#行}}です。",
  "perch.sql.none":
    "このスキャンが読んだ {scanned} 件のレコードから、クエリは 1 行も返しませんでした。",
  "perch.sql.scope":
    "これはスキャンが読んだレコードについての答えであり、Topic 全体についてではありません。範囲が変われば答えも変わります。",
  "perch.sql.capped":
    "スキャンは上限の {cap} 件で停止したため、クエリが数えたり合計したりした値はその範囲についてのものです。",
  "perch.sql.stopped":
    "このスキャンは {scanned} 件で停止したため、答えは範囲の一部をカバーしています。",
  "perch.sql.masked":
    "このクエリの実行中はマスキングルールが有効だったため、ここにある一部の値は Topic 上の値ではありません。",
  "perch.schemas.noRegistry":
    "この接続には Schema Registry がないため、スキーマを読み取る先がありません。",
  "perch.schemas.noRegistry.next":
    "レジストリは独自のアドレスを持つ別のサービスです。この接続の設定の「Schema Registry」で追加してください。",
  "perch.schemas.missing":
    "レジストリに {subject} という Subject はありません。",
  "perch.schemas.missing.next":
    "Kavka は多くのプロデューサーが使う「Topic 名戦略」で探しました。別の戦略を使うプロデューサーは別の名前で登録します。",
  "perch.schemas.versions":
    "この Subject は{count, plural, other {#個のバージョン}}が登録されています。",
  "perch.schemas.level": "新しいバージョンは {level} として検査されます。",
  "perch.schemas.levelUnknown":
    "Kavka はこの Subject 自身の互換性設定を読み取れなかったため、レジストリが適用するレベルを確実には言えません。",
  "perch.groups.none":
    "このクラスターにはまだコンシューマーグループがありません。まだ誰も読み取っていません。",
  "perch.groups.counts":
    "{count, plural, other {#個のコンシューマーグループ}}がこのクラスターから読み取っています。",
  "perch.groups.rebalancing":
    "現在{unstable, plural, other {#個のグループ}}がリバランス中です。パーティションが割り当て直され、その間は消費が止まります。{counts}",
  "perch.groups.unread":
    "Kavka はこのクラスターのコンシューマーグループを読み取れなかったため、何も言えません。読み取れるまで、この画面の内容はクラスターについての主張ではありません。",
  "perch.groups.caveat":
    "これは Kavka が最後に読み取った時点の一覧です。グループの状態はリバランスのたびに変わります。「更新」で読み直してください。",
  "perch.group.caughtUp":
    "{group} は Kavka が見えるすべてのパーティションで追いついています。",
  "perch.group.behind":
    "{group} は{partitions, plural, other {#個のパーティション}}で合計およそ {lag} 件遅れています。最も遅れているのは {topic} のパーティション {partition} で、{worst} 件です。",
  "perch.group.noOffsets":
    "{group} は一度もオフセットをコミットしていないため、報告できる位置がありません。書き込みしかしていないか、作成されたまま何も読んでいない可能性があります。",
  "perch.group.noMembers":
    "現在 {group} には何も接続していないため、何も読み取っていません。コミット済みオフセットは残っており、起動したアプリケーションはそこから続きます。",
  "perch.group.caveat":
    "Kavka はこの画面を開いた時に一度だけこれらのオフセットを読みました。グループの変化には追従しません。開き直すと新しく読み取ります。",
  "perch.brokers.counts":
    "このクラスターには{count, plural, other {#台のブローカー}}があります。1 台開くと、実行中のすべての設定を確認できます。",
  "perch.brokers.none":
    "クラスターは応答しましたが、ブローカーを 1 台も返しませんでした。",
  "perch.brokers.noneNext":
    "たいていはメタデータが空で返ったか、Kafka 本体ではなくロードバランサーに接続しています。切断して接続し直し、ブートストラップアドレスを確認してください。",
  "perch.brokers.caveat":
    "ブローカー一覧は接続時に返されたもので、クラスターの変化には追従しません。接続し直すと再取得します。",
  "perch.broker.noOverrides":
    "ブローカー {broker} は Kafka の既定値を何も変更していません。設定はすべて Kafka が算出したものです。",
  "perch.broker.overrides":
    "ブローカー {broker} は{count, plural, other {#件の設定}}を上書きしています。残りの {rest} 件は現時点で算出された値です。",
  "perch.broker.unread":
    "Kavka はこのブローカーの設定を読み取れなかったため、何を使って動いているか言えません。通常、アカウントにクラスターの DescribeConfigs 権限が必要です。",
  "perch.broker.caveat":
    "+ が付いた行だけがこのブローカーで設定されています。算出された既定値はクラスターの変化に伴って変わることがあり、Kafka はクライアントに対して一部の設定を読み取り専用として報告します。それらは無効化された「編集」ボタンを残し、理由をホバーで表示します。",
  "perch.connect.noClusters":
    "この接続には Kafka Connect のワーカーがないため、ここから操作できるものはありません。",
  "perch.connect.noClustersNext":
    "Connect は独自の REST アドレス（通常はポート 8083）を持つ、独立したワーカー群として動きます。この接続の設定の「Kafka Connect クラスター」で追加してください。",
  "perch.connect.empty":
    "{cluster} にはまだコネクターがないため、ここから Kafka への出入りは発生していません。",
  "perch.connect.allRunning":
    "{count, plural, other {{cluster} に #個のコネクター}}があり、すべてのタスクが動いています。",
  "perch.connect.failed":
    "{cluster} で{failed, plural, other {#個のタスク}}が失敗しました。失敗したタスクは再起動されるまでレコードを 1 件も動かしません。まずコネクターを開いてワーカー自身のトレースを読んでください。",
  "perch.connect.paused":
    "{cluster} で{paused, plural, other {#個のコネクター}}が一時停止しており、そこを通るものはありません。設定とコミット済みオフセットは保持されます。",
  "perch.connect.unread":
    "Kavka は Connect ワーカーに到達できなかったため、何が動いているか言えません。これはブローカーとは別のアドレスで、そこだけが停止している可能性があります。",
  "perch.connect.caveat":
    "これらの状態は Kavka が最後に問い合わせた時点でワーカーから返ったものです。Connect は独自に状態を変えます。「更新」で読み直してください。",
  "perch.connector.running":
    "{name} は稼働中です。{total} 個中 {running} 個のタスクがレコードを動かしています。",
  "perch.connector.failed":
    "{name} には{failed, plural, other {#個の失敗したタスク}}があり、何も動かしていません。再起動の前に停止した理由を読んでください。原因が残ったままの再起動は再び失敗するだけです。",
  "perch.connector.paused":
    "{name} は一時停止中のため、レコードを動かしていません。設定とコミット済みオフセットは保持され、再開するとそこから続きます。",
  "perch.connector.noTasks":
    "{name} にはタスクが 1 つもないため、何も動いていません。ワーカーはコネクターの設定からタスクを作るので、使えない設定だとタスクが 1 つも作られません。",
  "perch.connector.caveat":
    "これは Kavka が最後にワーカーへ問い合わせた時点の 1 回の読み取りです。タスクの状態はひとりでに変わります。",
  "perch.monitoring.origin":
    "Kafka はラグを記憶しません。ブローカーが言えるのは、グループの現在位置だけです。この画面のすべては、この接続が生きている間に Kavka 自身が記録したものです。",
  "perch.monitoring.unread":
    "Kavka はこの接続の自身のラグ記録を読み取れなかったため、どれだけ遅れているのか、そもそも計測があるのかを言えません。",
  "perch.monitoring.noHistory":
    "Kavka にはこの接続のラグ計測がまだありません。最初の計測は接続から {interval} 以内に現れ、グループは少なくとも一度オフセットをコミットして初めてここに現れます。",
  "perch.monitoring.noWindow":
    "この期間には {group} の計測がありません。もっと長い期間を選ぶか、下のサンプラーを確認してください。",
  "perch.monitoring.caughtUp":
    "最後の計測時点で {group} は追いついていました。読み取り待ちのものはありませんでした。",
  "perch.monitoring.rising":
    "{group} は{partitions, plural, other {#個のパーティション}}で合計およそ {lag} 件遅れており、増加傾向です。最も悪いのは {topic} のパーティション {partition} で、{peak} 件に達しました。",
  "perch.monitoring.steady":
    "{group} は{partitions, plural, other {#個のパーティション}}で合計およそ {lag} 件遅れており、この期間の初めから横ばいです。",
  "perch.monitoring.falling":
    "{group} は{partitions, plural, other {#個のパーティション}}で合計およそ {lag} 件遅れており、減少傾向です。",
  "perch.monitoring.caveat.sampled":
    "これらのグラフの点は、その区間で最も悪い計測値であって平均ではありません。線の途切れは Kavka が動いていなかった時間であり、障害ではありません。",
  "perch.monitoring.caveat.stale":
    "サンプラーが遅れています。最後の計測は {ago} 前で、3 間隔以上前です。以下の内容は見た目より古いものです。",
  "perch.monitoring.caveat.stopped":
    "現在この接続については何も記録されていないため、この判断は Kavka が最後に取得できた計測と同じ古さです。",
  "perch.monitoring.caveat.unknownSampler":
    "Kavka は現在サンプラーが何をしているか把握できないため、これらの計測が最新であるとは保証できません。",
  "perch.alerts.none":
    "このクラスターにはルールがないため、Kavka はここで何も監視していません。",
  "perch.alerts.quiet":
    "{count, plural, other {#件のルール}}がこのクラスターを監視しており、発報しているものはありません。",
  "perch.alerts.firingOne": "{rule} は {time} から発報しています。{detail}",
  "perch.alerts.firingMany":
    "現在このクラスターで{count, plural, other {#件のルール}}が発報しています。最も古いのは {rule} で、{time} からです。",
  "perch.alerts.unread":
    "Kavka はこの接続のアラートルールを読み取れなかったため、何が監視されているか、そもそも監視されているかを言えません。",
  "perch.alerts.unreadHistory":
    "Kavka はこの接続のアラートログを読み取れなかったため、いま何かが発報しているのか、これまでに発報したことがあるのかを言えません。",
  "perch.alerts.caveat.desktop":
    "気づくためには Kavka が動いている必要があります。ウィンドウを閉じれば何も監視されません。これはデスクトップアプリであり、サービスではありません。",
  "perch.alerts.caveat.silent":
    "通知チャネルが 1 つも有効でないため、発報はこのウィンドウと下のログにしか届きません。Kavka が目の前にないときは、何も届きません。",
  "perch.masking.none":
    "この接続にはマスキングルールがないため、Kavka が表示するものはすべてプロデューサーが送ったそのものです。",
  "perch.masking.inForce":
    "{count, plural, other {#件のマスキングルール}}が有効なため、該当するテキストはこのウィンドウに届く前に置き換えられます。",
  "perch.masking.off":
    "{count, plural, other {#件のマスキングルール}}がありますが、どれも有効になっていないため、画面上で隠されているものはありません。",
  "perch.masking.unread":
    "Kavka はこの接続のマスキングルールを読み取れなかったため、表示内容がそのままの値だとは保証できません。",
  "perch.masking.caveat":
    "いま有効にしたルールは、次の取得・追従バッチ・検索・クエリに適用されます。すでに画面にある行には適用されません。",
  "perch.masking.caveat.sawMasked":
    "このセッションでは既に画面上の何かがマスクされています。ここにある少なくとも 1 件のペイロードは、プロデューサーが送ったものではありません。",
  "perch.streams.noGroups":
    "このクラスターにはまだコンシューマーグループがないため、トポロジーを推定する材料がありません。",
  "perch.streams.pick":
    "上でアプリケーションを選ぶと、Kavka がそれが読むもの・書くもの・その間に保持するものを推定します。",
  "perch.streams.notStreams":
    "{group} は Kafka Streams アプリケーションには見えないため、描けるトポロジーがありません。通常のコンシューマーグループにトポロジーがないのは異常ではありません。",
  "perch.streams.inferred":
    "この {app} の図は推測です。Topic 名から導き出した{nodes, plural, other {#個のノード}}と{edges, plural, other {#本のリンク}}です。",
  "perch.streams.unread":
    "Kavka は {group} のトポロジーを導き出せなかったため、表示できるものがありません。下のメッセージはクラスターが返した内容です。",
  "perch.streams.caveat":
    "Kafka は Streams のトポロジーをクライアントが読める場所に公開しません。ここにあるものはアプリケーション自身から読んだものではないため、Topic を残さないプロセッサーはまったく現れません。",
  "perch.acls.noAuthorizer":
    "このクラスターにはオーソライザーがないため、一覧にできるアクセスルールがなく、すべてのリクエストはブローカー自身の既定値で決まります。",
  "perch.acls.noAuthorizerNext":
    "これはブローカーの設定（authorizer.class.name）であって、権限不足ではありません。Kafka は空の一覧を返す代わりに、リクエスト自体を拒否します。",
  "perch.acls.none":
    "このクラスターにはオーソライザーがありますが、アクセスルールはまだありません。リクエストの扱いはすべてブローカーの既定値次第です。",
  "perch.acls.allAllow":
    "このクラスターには{count, plural, other {#件のアクセスルール}}があり、そのすべてが許可です。",
  "perch.acls.someDeny":
    "このクラスターには{count, plural, other {#件のアクセスルール}}があります。{denies, plural, other {うち #件が拒否}}で、拒否は同じリクエストに一致するすべての許可に優先します。",
  "perch.acls.filtered":
    "このフィルターに一致する{count, plural, other {#件のルール}}を表示しています。",
  "perch.acls.unread":
    "Kavka はこのクラスターのアクセスルールを読み取れなかったため、誰が何を許可されているか言えません。一覧するには通常、アカウントにクラスターの Describe 権限が必要です。",
  "perch.acls.caveat.filtered":
    "フィルターが有効なため、これはフィルターに一致するルールの数であり、クラスター上のルールの数ではありません。",
  "perch.acls.caveat.removing":
    "拒否を削除すると、アクセスは狭まるのではなく広がります。Kavka は削除の前にもう一度そう伝えます。",

  // ── クラスター画面の周辺文言 (Jackdaw) ──────────────────────────────────
  "topics.partitions.detail": "レプリカの詳細を表示",
  "topics.partitions.detailTitle":
    "レプリカ一覧、同期済みレプリカ一覧、各パーティションの最古と最新のオフセットを追加します。どちらの場合も健全性は画面に残ります。",
  "acls.filter.summary": "これらのルールを絞り込む",
  "acls.filter.note": "リソース種別・リソース名・プリンシパルで",
  "acls.filter.active": "フィルターが有効です",
  "alerts.state.firing": "発報中",
  "alerts.since": "{time} から",
  "alerts.details.summary": "詳細",
  "alerts.details.note": "Kavka が何をどれくらいの頻度で比較しているか",
  "alerts.facts.kind": "種類",
  "alerts.facts.waitsFor": "待機時間",
  "alerts.facts.noWait": "なし — 条件が成立した瞬間に発報します",
  "alerts.facts.checked": "確認",
  "alerts.facts.checkedValue":
    "Kavka が計測を取るたび、そして Kavka が開いている間だけ",
  "alerts.facts.since": "発報開始",
  "alerts.history.started": "{rule} — 開始",
  "alerts.history.cleared": "{rule} — 解消",
  "alerts.history.lasted": "{time} に解消しました（{duration} 後）。",
  "alerts.history.stillFiring": "まだ発報中です。これまで {duration}。",
  "alerts.history.gap":
    "この記録は Kavka が開いていた時間だけを扱います。記録の空白は誰も見ていなかった時間であり、Kavka はその間に何が起きたかを推測しません。",
  "monitoring.tile.lagNow": "最後の計測時点のラグ",
  "monitoring.tile.lagNowSub":
    "Kavka が最後にサンプリングした時点で読み取り待ちだったメッセージ数",
  "monitoring.tile.peak": "この期間のピーク",
  "monitoring.tile.peakSub":
    "Kavka が取った単一計測のうち最も悪い値であり、平均ではありません",
  "monitoring.tile.trend": "傾向",
  "monitoring.tile.trendSub": "この期間の開始時点との比較",
  "monitoring.tile.partitionsSub": "この期間に少なくとも 1 回の計測があるもの",

  // ── 設定 (Jackdaw) ──────────────────────────────────────────────────────
  "settings.title": "設定",
  "settings.navLabel": "設定のセクション",
  "settings.perch":
    "ここでの変更はすぐに反映され、このマシンに保存されます。現在は{theme}テーマを表示しています。",
  "settings.section.appearance": "外観",
  "settings.section.language": "言語",
  "settings.section.about": "情報",

  "settings.theme.title": "テーマ",
  "settings.theme.help":
    "「システム」は OS の設定に従い、Kavka を開いたままでも切り替わります。",
  "settings.theme.system": "システム",
  "settings.theme.light": "ライト",
  "settings.theme.dark": "ダーク",

  "settings.accent.title": "アクセント",
  "settings.accent.help":
    "ボタン・リンク・現在の画面に使う色です。それ自体は意味を持たないので、変更しても警告が隠れることはありません。",
  "settings.accent.brass": "真鍮",
  "settings.accent.moss": "苔",
  "settings.accent.sky": "空",
  "settings.accent.plum": "梅",

  "settings.density.title": "密度",
  "settings.density.help":
    "「ゆったり」は各行に余白を取ります。「コンパクト」は約 3 分の 1 多く行を表示します（出荷時の行の高さです）。",
  "settings.density.comfortable": "ゆったり",
  "settings.density.compact": "コンパクト",

  "settings.font.title": "文字サイズ",
  "settings.font.help":
    "アプリ内のすべてのサイズをまとめて拡大縮小するので、最大でも重なりません。",
  "settings.font.s": "小",
  "settings.font.m": "中",
  "settings.font.l": "大",

  "settings.motion.title": "動き",
  "settings.motion.help":
    "「システム」は OS の「視差効果を減らす」設定に従います。「減らす」は Kavka 側のアニメーションもすべて止めます。",
  "settings.motion.system": "システム",
  "settings.motion.reduce": "減らす",

  "settings.language.title": "言語",
  "settings.language.help":
    "Kavka の外枠と、各クラスター画面が最初に示す判断を対象とします。サイドバー、コマンドパレット、このパネル、接続フォーム、そして各画面の冒頭の一文です。その下にある表やフォームはまだ英語です。",
  "settings.language.machine":
    "このカタログは機械翻訳で、ネイティブによる確認は済んでいません。修正を歓迎します。",

  "settings.about.title": "バージョン・ライセンス・診断",
  "settings.about.help":
    "「情報」パネルに Kavka のバージョンとライセンス、MCP サーバー設定、クラッシュ診断のスイッチがあります。",
  "settings.about.open": "情報を開く",

  // ── Updates ─────────────────────────────────────────────────────────────
  "settings.section.updates": "アップデート",

  "settings.updates.auto.title": "アップデートを確認する",
  "settings.updates.auto.label": "Kavka に新しいリリースを探させる",
  "settings.updates.auto.hint":
    "既定でオンです。Kavka は最新のリリースが何かを github.com に尋ねます。多くても 1 日に 1 回で、公開されている Releases ページが誰にでも答えているのと同じ質問です。あなたを特定するものも、クラスターに関する情報も一切送りません。また、Kavka が自分の判断でダウンロードやインストールを行うことはありません。これは Kavka が頼まれずに行う唯一の通信です。ここをオフにすれば、その通信もなくなります。",

  "settings.updates.channel.title": "対象のリリース",
  "settings.updates.channel.help":
    "「安定版」は人が意図してタグを付けたリリースを追います。「すべてのビルド」は main へのマージごとに公開されるプレリリースを追います。より新しい代わりに、同じ基準で判断されてはいません。",
  "settings.updates.channel.stable": "安定版",
  "settings.updates.channel.builds": "すべてのビルド",
  "settings.updates.channel.warning":
    "ビルドは main から自動で公開されます。ビルドは通り、チェックも通っていますが、それが良いものだと誰かが判断したわけではありません。最新の成果がほしく、問題があれば安定版を入れ直せる場合にだけ選んでください。",

  "settings.updates.check.title": "今すぐ確認",
  "settings.updates.check.help":
    "上のスイッチの状態にかかわらず、すぐに github.com に尋ねます。何もダウンロードしません。",
  "settings.updates.check.button": "今すぐ確認",
  "settings.updates.check.checking": "github.com に問い合わせています…",

  "settings.updates.result.update":
    "Kavka {version} が利用できます。ウィンドウ上部の通知にインストールのボタンがあります。",
  "settings.updates.result.currentStable": "最新の安定版です。",
  "settings.updates.result.currentBuild": "最新のビルドです。",
  "settings.updates.result.noStable":
    "安定版はまだ一度も公開されていません。今のところ main からの自動ビルドしかありません。それを追うには「すべてのビルド」に切り替えてください。",

  "settings.updates.lastChecked": "Kavka が最後に確認したのは {when} です。",
  "settings.updates.lastCheckedFailed":
    "Kavka が最後に試したのは {when} で、github.com に接続できませんでした。",
  "settings.updates.never": "Kavka はまだ確認していません。",

  "updates.banner.label": "アップデートの通知 — Kavka {version}",
  "updates.banner.title": "Kavka {version} が利用できます",
  "updates.banner.body":
    "まだ何もダウンロードしていません。Kavka はインストールを押したときにだけインストーラーを取得し、実行する前に Kavka 自身の署名鍵で照合します。",
  "updates.banner.bodyBuild":
    "これは main への最新のマージから作られた自動ビルドで、安定版ではありません。良いものだと誰かが判断したわけではありません。まだ何もダウンロードしていません。Kavka はインストールを押したときにだけインストーラーを取得し、実行する前に Kavka 自身の署名鍵で照合します。",
  "updates.banner.willClose":
    "インストールすると、インストーラーが置き換えられるように Kavka は終了します。作業を終えてから実行し、インストーラーが終わったら Kavka を開き直してください。",
  "updates.banner.willRestart":
    "インストールすると Kavka はいったん終了し、更新が済んだら開き直します。先に作業を終えてください。",
  "updates.banner.notes": "変更点",
  "updates.banner.releasePage": "リリースページ",
  "updates.banner.install": "インストール…",
  "updates.banner.installing": "ダウンロード中…",
  "updates.banner.notNow": "今はしない",

  "updates.error.unreachable.title": "Kavka は github.com に接続できませんでした",
  "updates.error.unreachable.detail":
    "何もダウンロードしておらず、このマシンには何の変更もありません。接続、または github.com との間にプロキシやファイアウォールがないかを確認してから、もう一度お試しください。",
  "updates.error.title": "アップデートは完了しませんでした",
  "updates.error.detail":
    "何もインストールしておらず、このマシンには何の変更もありません。全文は「詳細を表示」にあります。リリースページには自分でダウンロードできるインストーラーもあります。",

  "unit.seconds": "{count, plural, other {#秒}}",
  "unit.minutes": "{count, plural, other {#分}}",
  "unit.hours": "{count, plural, other {#時間}}",
  "unit.days": "{count, plural, other {#日}}",
};

export default ja;
