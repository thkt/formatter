# claude-formatter

[English](README.md)

Claude Code の PostToolUse hook。Write/Edit 後にファイルを自動整形します（rustfmt, oxfmt, biome 対応）。

## 特徴

- **rustfmt**: Rust ソースの整形（`.rs` ファイルで最優先）
- **oxfmt**: Rust 製の Prettier 互換フォーマッター（Web 系で優先）
- **biome**: コード整形 + import 整理（フォールバック）
- **自動検出**: 優先順位 rustfmt (.rs) > oxfmt > biome
- **プロジェクトローカル解決**: `node_modules/.bin/` のバイナリを優先使用

## インストール

### Homebrew

```bash
brew install thkt/tap/formatter
```

### ソースから

```bash
cd /tmp
git clone https://github.com/thkt/claude-formatter.git
cd claude-formatter
cargo build --release
cp target/release/formatter ~/.local/bin/
```

## 使い方

### Claude Code Hook として

`~/.claude/settings.json` に追加:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "formatter",
            "timeout": 2000
          }
        ]
      }
    ]
  }
}
```

### guardrails 併用（推奨）

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "guardrails",
            "timeout": 1000
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "formatter",
            "timeout": 2000
          }
        ]
      }
    ]
  }
}
```

## 必要なツール

以下のいずれか:

- [rustfmt](https://github.com/rust-lang/rustfmt)（`rustup` に同梱）
- [oxfmt](https://oxc.rs/docs/guide/usage/formatter)（`npm i -g oxfmt`）
- [biome](https://biomejs.dev)（`brew install biome` または `npm i -g @biomejs/biome`）

## 対応ファイル

| フォーマッター | 拡張子                                                                                                                                                                      |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| rustfmt        | `.rs`                                                                                                                                                                       |
| oxfmt          | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.json5` `.css` `.scss` `.less` `.html` `.vue` `.yaml` `.yml` `.toml` `.md` `.mdx` `.graphql` `.gql` |
| biome          | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.css`                                                                                               |

## 動作フロー

1. stdin から PostToolUse hook の JSON を読み取る
2. Write/Edit/MultiEdit 以外のツールは無視
3. ファイルパスを正規化（シンボリックリンク、null バイト、相対パスを拒否）
4. ファイルがカレントディレクトリ配下にあることを検証
5. git ルートの `.claude-formatter.json` を読み込む（存在する場合）
6. 優先順位に従ってフォーマッターを選択: rustfmt (.rs) > oxfmt > biome
7. ファイルをインプレースで整形

## 設定

git ルートに `.claude-formatter.json` を配置します。対象ファイルから最寄りの `.git` ディレクトリまで遡り、そこで設定ファイルを探します。指定されたフィールドのみデフォルトを上書きします。

設定ファイルなし = 全フォーマッター有効（ゼロコンフィグ）。

| フィールド           | デフォルト | 説明                          |
| -------------------- | ---------- | ----------------------------- |
| `enabled`            | `true`     | フォーマッター全体の有効/無効 |
| `formatters.rustfmt` | `true`     | rustfmt の有効/無効           |
| `formatters.oxfmt`   | `true`     | oxfmt の有効/無効             |
| `formatters.biome`   | `true`     | biome の有効/無効             |

### 設定例

biome を無効化（oxfmt を使用）:

```json
{
  "formatters": {
    "biome": false
  }
}
```

oxfmt を無効化（biome を使用）:

```json
{
  "formatters": {
    "oxfmt": false
  }
}
```

### `~/.claude` によるグローバル設定

`~/.claude` が git リポジトリの場合、そこに `.claude-formatter.json` を置くと `~/.claude/` 配下の全ファイルに対するグローバルデフォルトとして機能します。各プロジェクトの `.claude-formatter.json` が優先されます。

## 終了コード

| コード | 意味 |
| ------ | ---- |
| 0      | 常に |

フォーマッターは操作をブロックしません。成功時はサイレントに整形し、エラーは stderr に出力します。

## ライセンス

MIT
