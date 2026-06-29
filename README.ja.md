[English](README.md) | **日本語**

# formatter

Claude CodeのPostToolUse hook。Write/Edit後にファイルを自動整形します（oxfmt対応）。

## 特徴

| 機能                     | 説明                                                       |
| ------------------------ | ---------------------------------------------------------- |
| oxfmt統合                | [oxc.rs](https://oxc.rs)のRust製Prettier互換フォーマッター |
| EOF改行                  | 言語フォーマッターの対象外ファイルに末尾改行を付与         |
| プロジェクトローカル解決 | `node_modules/.bin/`のバイナリを優先使用                   |

## インストール

### Claude Code Plugin（推奨）

バイナリのインストールとhookの登録が自動で行われます。

```bash
claude plugins marketplace add thkt/sentinels
claude plugins install formatter
```

バイナリが未インストールの場合、同梱のインストーラを実行してください。

```bash
~/.claude/plugins/cache/formatter/formatter/*/hooks/install.sh
```

### Homebrew

```bash
brew install thkt/tap/formatter
```

### リリースバイナリから

[Releases](https://github.com/thkt/formatter/releases)から最新バイナリをダウンロードしてください。

```bash
# macOS (Apple Silicon)
curl -L https://github.com/thkt/formatter/releases/latest/download/formatter-aarch64-apple-darwin.tar.gz | tar xz
mv formatter ~/.local/bin/
```

### ソースから

```bash
cd /tmp
git clone https://github.com/thkt/formatter.git
cd formatter
cargo build --release
cp target/release/formatter ~/.local/bin/
cd .. && rm -rf formatter
```

## 使い方

### Claude Code Hookとして

プラグインとしてインストールした場合、hookは自動で登録されます。手動で設定する場合は `~/.claude/settings.json` に追加してください。

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "formatter",
            "timeout": 2000
          }
        ],
        "matcher": "Write|Edit|MultiEdit"
      }
    ]
  }
}
```

### guardrails併用（推奨）

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "guardrails",
            "timeout": 1000
          }
        ],
        "matcher": "Write|Edit|MultiEdit"
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "formatter",
            "timeout": 2000
          }
        ],
        "matcher": "Write|Edit|MultiEdit"
      }
    ]
  }
}
```

## 要件

oxfmtをインストールしてください。

- [oxfmt](https://oxc.rs/docs/guide/usage/formatter)（`npm i -g oxfmt`）

### 動作

formatterは対応ファイルにoxfmtを実行します。oxfmtが利用できない場合、対応ファイルは整形されないまま degraded outcome として報告され（終了コード0）、EOF改行は付与されません。EOF改行の付与はoxfmtの対象拡張子外のファイルにのみ適用されます。

| 条件                            | 使用する処理                 |
| ------------------------------- | ---------------------------- |
| 対応拡張子・oxfmtインストール済 | oxfmt                        |
| 対応拡張子・oxfmt利用不可       | 整形せず degraded として報告 |
| 対象外拡張子                    | EOF 改行のみ                 |

## 対応ファイル

| フォーマッター | 拡張子                                                                                                                                                                      |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| oxfmt          | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.json5` `.css` `.scss` `.less` `.html` `.vue` `.yaml` `.yml` `.toml` `.md` `.mdx` `.graphql` `.gql` |

## 動作フロー

1. stdinからPostToolUse hookのJSONを読み取り
2. Write/Edit/MultiEdit以外のツールは無視
3. ファイルパスを正規化（シンボリックリンク、nullバイト、相対パスを拒否）
4. ファイルがカレントディレクトリ配下にあることを検証
5. `.claude/tools.json` から設定を読み込み（存在する場合）
6. ファイルをインプレースで整形。対応拡張子はoxfmt、それ以外はEOF改行の付与。対応ファイルはoxfmtが終端まで所有し、バイナリが利用不可なら整形せず degraded として報告（終了コード0）、EOF改行へは降格しない

## 終了コード

ADR-0066 のGroup 3 (Hook tool) ベースラインに沿い、sysexits.h に準拠した終了コードを返します。

| コード | 由来          | 意味                                                      |
| ------ | ------------- | --------------------------------------------------------- |
| 0      | `EX_OK`       | 整形の成否を問わずすべての整形結果（サイレント fix 方針） |
| 64     | `EX_USAGE`    | stdin のhook入力JSONが期待する形ではない                  |
| 70     | `EX_SOFTWARE` | フォーマッター自身の内部不具合（panic を捕捉）            |

サイレント fix 方針: 整形そのものの失敗（oxfmt が直せない構文エラー、バイナリ不在など）は終了コード0のままにします。スタイルの問題で開発者をブロックしないためで、失敗は後述の構造化出力で advisory として伝えます。非ゼロコードはhook契約違反（不正な入力）と内部バグの2つのインフラ障害だけに限定し、メトリクスダッシュボードがこの2つで分岐できるようにします。

PostToolUse hookとして、終了コードはClaude Codeが解釈します（コード2はstderrをClaudeに返し、その他の非ゼロコードはトランスクリプトに `hook error` を表示します）。そのため終了コードに「整形/エラー」の区別を載せることはできず、その区別は後述の構造化出力が担います。

## 構造化出力

`FORMATTER_VERBOSE=1` を設定すると、各整形アクションについて何が起きたかを記述するJSON行をstderrに出力します。1ファイルにつき出力は1行です。`oxfmt` と `eof-newline` はファイル単位で排他（対応拡張子はoxfmt、それ以外はeof-newline）のため、同じファイルが両方を報告することはありません。デフォルト動作はサイレントのままです。これにより、エージェントは終了コードを解析せずにどのフォーマッターがどのファイルを処理したかを把握できます。

```json
{ "file": "/path/to/app.ts", "formatter": "oxfmt", "action": "formatted" }
```

`formatter` は `oxfmt` または `eof-newline` です。`action` は次のいずれかです。

| `action`       | 出力される条件                                                                         |
| -------------- | -------------------------------------------------------------------------------------- |
| `formatted`    | フォーマッターが書き込みモードで実行された。oxfmtは差分を取らないため、no-op実行も含む |
| `would-format` | ドライラン専用。ファイルは未整形で変更される見込み                                     |
| `unchanged`    | ドライラン専用。ファイルは既に整形済み                                                 |
| `error`        | フォーマッターが失敗した。oxfmtバイナリ不在も含む                                      |

### 劣化時の出力

対応ファイルが未整形のまま残った場合（`error`）、エージェントが自由形式のstderrを解析せずに反応できるよう、レコードは3つの追加フィールドを持ちます。`degraded` は `true`、`next_step` は対処法、`notes` は根本原因の診断（無い場合は省略）を示します。

```json
{
  "file": "/p/app.ts",
  "formatter": "oxfmt",
  "action": "error",
  "degraded": true,
  "next_step": "fix the reported error before saving; the file was left unformatted",
  "notes": ["x Unexpected token"]
}
```

中立なレコード（`formatted`、`unchanged`、`would-format`）は元の3キー形状を保ち、これらのフィールドを含めません。劣化時の出力は `FORMATTER_VERBOSE` が無くても常に表面化し、その場合はJSONではなく人間可読な行として出力されます。

## ドライラン

`FORMATTER_DRY_RUN=1` を設定すると、ファイルを書き込まずに何が変更されるかを報告します。このモードでは構造化出力が常に出力されます。oxfmtファイルは `oxfmt --check` を使うため、`action` の `would-format` はファイルが未整形であることを、`unchanged` は既に整形済みであることを意味します。

`eof-newline` フォーマッターは、末尾改行が必要なファイルについてのみ `would-format` を報告します。既に正しく終端しているファイルは行を出力しません。一方 `oxfmt --check` は `unchanged` も報告します。したがってドライラン出力は、検査した全ファイルではなく、変更されるファイルを列挙します。

## 設定

プロジェクトルートの `.claude/tools.json` に `formatter` キーを追加します。すべてのフィールドはオプションで、オーバーライドしたいもののみ指定してください。

設定ファイルがない場合のデフォルト構成です。

- すべてのフォーマッターが有効

### スキーマ

```json
{
  "formatter": {
    "enabled": true,
    "oxfmt": true,
    "eofNewline": true
  }
}
```

### 設定例

oxfmtを無効化する設定です（EOF改行のみ）。

```json
{
  "formatter": {
    "oxfmt": false
  }
}
```

プロジェクト単位でformatterを無効化できます。

```json
{
  "formatter": {
    "enabled": false
  }
}
```

### 設定の解決

設定ファイルは、対象ファイルからもっとも近い `.git` ディレクトリまで上方向に探索されます。`.claude/tools.json` に `formatter` キーがあればデフォルトとマージされます。

```text
project-root/          ← .git/ + .claude/tools.json はここ
├── .claude/
│   └── tools.json     ← {"formatter": {"oxfmt": false}}
├── src/
│   └── app.ts         ← 整形対象ファイル → 上方向に設定を探索
└── .git/
```

## 関連ツール

| ツール                                           | Hook        | タイミング              | 役割                          |
| ------------------------------------------------ | ----------- | ----------------------- | ----------------------------- |
| [guardrails](https://github.com/thkt/guardrails) | PreToolUse  | Write/Edit 前           | リント + セキュリティチェック |
| **formatter**                                    | PostToolUse | Write/Edit 後           | 自動コード整形                |
| [reviews](https://github.com/thkt/reviews)       | PreToolUse  | レビュー系 Skill 実行時 | 静的解析コンテキスト提供      |
| [gates](https://github.com/thkt/gates)           | Stop        | エージェント完了時      | 品質ゲート (knip/tsgo/madge)  |

## ライセンス

MIT
