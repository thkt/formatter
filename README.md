**English** | [日本語](README.ja.md)

# formatter

PostToolUse hook for Claude Code. Auto-formats files after Write/Edit using oxfmt or biome.

## Features

- **oxfmt integration** (priority): Rust-powered Prettier-compatible formatter from [oxc.rs](https://oxc.rs)
- **biome integration** (fallback): Code formatting + organizeImports from [biomejs.dev](https://biomejs.dev)
- **EOF newline**: Ensures files end with a newline (for files without a language formatter)
- **Project-local resolution**: Uses `node_modules/.bin/` when available

## Installation

### Claude Code Plugin (Recommended)

Installs the binary and registers the hook automatically:

```bash
claude plugins marketplace add thkt/sentinels
claude plugins install formatter
```

If the binary is not yet installed, run the bundled installer:

```bash
~/.claude/plugins/cache/formatter/formatter/*/hooks/install.sh
```

### Homebrew

```bash
brew install thkt/tap/formatter
```

### From Release

Download the latest binary from [Releases](https://github.com/thkt/formatter/releases):

```bash
# macOS (Apple Silicon)
curl -L https://github.com/thkt/formatter/releases/latest/download/formatter-aarch64-apple-darwin.tar.gz | tar xz
mv formatter ~/.local/bin/
```

### From Source

```bash
cd /tmp
git clone https://github.com/thkt/formatter.git
cd formatter
cargo build --release
cp target/release/formatter ~/.local/bin/
cd .. && rm -rf formatter
```

## Usage

### As Claude Code Hook

When installed as a plugin, hooks are registered automatically. For manual setup, add to `~/.claude/settings.json`:

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

Install at least one formatter (oxfmt is preferred):

- [oxfmt](https://oxc.rs/docs/guide/usage/formatter) (`npm i -g oxfmt`) — **recommended**
- [biome](https://biomejs.dev) (`brew install biome` or `npm i -g @biomejs/biome`) — fallback

### Formatter Priority

formatter uses **oxfmt first**. If oxfmt is not available, it falls back to biome. Only one runs per file.

| Condition                            | Formatter used   |
| ------------------------------------ | ---------------- |
| oxfmt installed                      | oxfmt            |
| oxfmt not installed, biome installed | biome            |
| Neither installed                    | EOF newline only |

## Supported File Types

| Formatter | Extensions                                                                                                                                                                  |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| oxfmt     | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.json5` `.css` `.scss` `.less` `.html` `.vue` `.yaml` `.yml` `.toml` `.md` `.mdx` `.graphql` `.gql` |
| biome     | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.css`                                                                                               |

## How It Works

1. Reads PostToolUse hook JSON from stdin
2. Ignores non-Write/Edit/MultiEdit tools
3. Canonicalizes the file path (rejects symlink tricks, null bytes, relative paths)
4. Verifies the file is within the current working directory
5. Loads config from `.claude/tools.json` or `.claude-formatter.json` (if present)
6. Selects formatter by priority: oxfmt > biome
7. Formats the file in-place

## Exit Codes

| Code | Meaning |
| ---- | ------- |
| 0    | Always  |

The formatter never blocks operations. It silently formats on success and logs errors to stderr.

## Configuration

Add a `formatter` key to `.claude/tools.json` at your project root. All fields are optional — only specify what you want to override.

> **Migration**: `.claude-formatter.json` at the project root is still supported as a legacy fallback. If both exist, `.claude/tools.json` takes priority.

**Defaults** (no config file needed):

- All formatters enabled

### Schema

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

### Examples

Disable biome (use oxfmt only):

```json
{
  "formatter": {
    "formatters": {
      "biome": false
    }
  }
}
```

Disable oxfmt (use biome):

```json
{
  "formatter": {
    "formatters": {
      "oxfmt": false
    }
  }
}
```

Disable formatter for a project:

```json
{
  "formatter": {
    "enabled": false
  }
}
```

### Config Resolution

The config file is found by walking up from the target file to the nearest `.git` directory. If `.claude/tools.json` exists there and contains a `formatter` key, it is loaded and merged with defaults.

```text
project-root/          ← .git/ + .claude/tools.json here
├── .claude/
│   └── tools.json     ← {"formatter": {"formatters": {"oxfmt": false}}}
├── src/
│   └── app.ts         ← file being formatted → walks up to find config
└── .git/
```

## Companion Tools

This tool is part of a 4-tool quality pipeline for Claude Code. Each covers a
different phase — install the full suite for comprehensive coverage:

```bash
brew install thkt/tap/guardrails thkt/tap/formatter thkt/tap/reviews thkt/tap/gates
```

| Tool                                             | Hook        | Timing            | Role                              |
| ------------------------------------------------ | ----------- | ----------------- | --------------------------------- |
| [guardrails](https://github.com/thkt/guardrails) | PreToolUse  | Before Write/Edit | Lint + security checks            |
| **formatter**                                    | PostToolUse | After Write/Edit  | Auto code formatting              |
| [reviews](https://github.com/thkt/reviews)       | PreToolUse  | Before Skill      | Static analysis context injection |
| [gates](https://github.com/thkt/gates)           | Stop        | Agent completion  | Quality gates (knip, tsgo, madge) |

See [thkt/tap](https://github.com/thkt/homebrew-tap) for setup details.

## License

MIT
