[English](README.md) | **日本語**

# formatter

Claude CodeのPostToolUse hook。Write/Edit後にファイルを自動整形します（oxfmt, biome対応）。

## 特徴

| 機能                     | 説明                                                                |
| ------------------------ | ------------------------------------------------------------------- |
| oxfmt統合（優先）        | [oxc.rs](https://oxc.rs)のRust製Prettier互換フォーマッター          |
| biome統合（フォールバック） | [biomejs.dev](https://biomejs.dev)のコード整形 + import整理      |
| EOF改行                  | 言語フォーマッターの対象外ファイルに末尾改行を付与                  |
| プロジェクトローカル解決 | `node_modules/.bin/`のバイナリを優先使用                            |

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

フォーマッターを少なくとも1つインストールしてください（oxfmt推奨）。

- [oxfmt](https://oxc.rs/docs/guide/usage/formatter)（`npm i -g oxfmt`）— 推奨
- [biome](https://biomejs.dev)（`brew install biome` または `npm i -g @biomejs/biome`）— フォールバック

### フォーマッター優先順位

formatterは **oxfmtを優先**します。oxfmtが利用できない場合はbiomeにフォールバックし、ファイルごとに1つだけ実行されます。

| 条件                             | 使用フォーマッター |
| -------------------------------- | ------------------ |
| oxfmt がインストール済み         | oxfmt              |
| oxfmt 未インストール、biome あり | biome              |
| どちらも未インストール           | EOF 改行のみ       |

## 対応ファイル

| フォーマッター | 拡張子                                                                                                                                                                      |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| oxfmt          | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.json5` `.css` `.scss` `.less` `.html` `.vue` `.yaml` `.yml` `.toml` `.md` `.mdx` `.graphql` `.gql` |
| biome          | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.css`                                                                                               |

## 動作フロー

1. stdinからPostToolUse hookのJSONを読み取り
2. Write/Edit/MultiEdit以外のツールは無視
3. ファイルパスを正規化（シンボリックリンク、nullバイト、相対パスを拒否）
4. ファイルがカレントディレクトリ配下にあることを検証
5. `.claude/tools.json` または `.claude-formatter.json` から設定を読み込み（存在する場合）
6. 優先順位に従ってフォーマッターを選択: oxfmt > biome
7. ファイルをインプレースで整形

## 終了コード

| コード | 意味 |
| ------ | ---- |
| 0      | 常に |

フォーマッターは操作をブロックしません。成功時はサイレントに整形し、エラーはstderrに出力します。

## 設定

プロジェクトルートの `.claude/tools.json` に `formatter` キーを追加します。すべてのフィールドはオプションで、オーバーライドしたいもののみ指定してください。

> **移行**: プロジェクトルートの `.claude-formatter.json` もレガシーフォールバックとしてサポートされています。両方存在する場合、`.claude/tools.json` が優先されます。

設定ファイルがない場合のデフォルト構成です。

- すべてのフォーマッターが有効

### スキーマ

```json
{
  "formatter": {
    "enabled": true,
    "formatters": {
      "oxfmt": true,
      "biome": true,
      "eofNewline": true
    }
  }
}
```

### 設定例

biomeを無効化する設定です（oxfmtのみ使用）。

```json
{
  "formatter": {
    "formatters": {
      "biome": false
    }
  }
}
```

oxfmtを無効化する設定です（biomeを使用）。

```json
{
  "formatter": {
    "formatters": {
      "oxfmt": false
    }
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
│   └── tools.json     ← {"formatter": {"formatters": {"oxfmt": false}}}
├── src/
│   └── app.ts         ← 整形対象ファイル → 上方向に設定を探索
└── .git/
```

## 関連ツール

| ツール | Hook | タイミング | 役割 |
| --- | --- | --- | --- |
| [guardrails](https://github.com/thkt/guardrails) | PreToolUse | Write/Edit 前 | リント + セキュリティチェック |
| **formatter** | PostToolUse | Write/Edit 後 | 自動コード整形 |
| [reviews](https://github.com/thkt/reviews) | PreToolUse | レビュー系 Skill 実行時 | 静的解析コンテキスト提供 |
| [gates](https://github.com/thkt/gates) | Stop | エージェント完了時 | 品質ゲート (knip/tsgo/madge) |

## ライセンス

MIT
