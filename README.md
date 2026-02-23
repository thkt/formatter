# claude-formatter

PostToolUse hook for Claude Code. Auto-formats files after Write/Edit using rustfmt, oxfmt, or biome.

## Features

- **rustfmt**: Rust source formatting (highest priority for `.rs` files)
- **oxfmt**: Rust-powered Prettier-compatible formatter (preferred for web)
- **biome**: Code formatting + organizeImports (fallback)
- **Auto-detection**: Priority: rustfmt (.rs) > oxfmt > biome
- **Project-local resolution**: Uses `node_modules/.bin/` when available

## Installation

### Homebrew

```bash
brew install thkt/tap/formatter
```

### From Source

```bash
cd /tmp
git clone https://github.com/thkt/claude-formatter.git
cd claude-formatter
cargo build --release
cp target/release/formatter ~/.local/bin/
```

## Usage

### As Claude Code Hook

Add to `~/.claude/settings.json`:

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

### With guardrails (recommended)

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

## Requirements

At least one of:

- [rustfmt](https://github.com/rust-lang/rustfmt) (included with `rustup`)
- [oxfmt](https://oxc.rs/docs/guide/usage/formatter) (`npm i -g oxfmt`)
- [biome](https://biomejs.dev) (`brew install biome` or `npm i -g @biomejs/biome`)

## Supported File Types

| Formatter | Extensions                                                                                                                                                                  |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| rustfmt   | `.rs`                                                                                                                                                                       |
| oxfmt     | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.json5` `.css` `.scss` `.less` `.html` `.vue` `.yaml` `.yml` `.toml` `.md` `.mdx` `.graphql` `.gql` |
| biome     | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.css`                                                                                               |

## How It Works

1. Reads PostToolUse hook input from Claude Code (stdin JSON)
2. Selects formatter by priority: rustfmt (.rs only) > oxfmt > biome

## Configuration

Place `.claude-formatter.json` at your git root. The formatter walks up from the target file to the nearest `.git` directory and looks for the config there. Only specified fields override defaults.

No config file = all formatters enabled (zero-config by default).

| Field                | Default | Description                       |
| -------------------- | ------- | --------------------------------- |
| `enabled`            | `true`  | Enable/disable formatter entirely |
| `formatters.rustfmt` | `true`  | Enable rustfmt                    |
| `formatters.oxfmt`   | `true`  | Enable oxfmt                      |
| `formatters.biome`   | `true`  | Enable biome                      |

### Examples

Disable biome (use oxfmt):

```json
{
  "formatters": {
    "biome": false
  }
}
```

Disable oxfmt (use biome):

```json
{
  "formatters": {
    "oxfmt": false
  }
}
```

### Global config via `~/.claude`

If `~/.claude` is a git repository, placing `.claude-formatter.json` there acts as a global default for all files under `~/.claude/`. Each project's own `.claude-formatter.json` takes precedence for files within that project.

## Exit Codes

| Code | Meaning |
| ---- | ------- |
| 0    | Always  |

The formatter never blocks operations. It silently formats on success and logs errors to stderr.

## License

MIT
