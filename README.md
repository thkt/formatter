**English** | [日本語](README.ja.md)

# formatter

PostToolUse hook for Claude Code. Auto-formats files after Write/Edit using oxfmt.

## Features

- **oxfmt integration**: Rust-powered Prettier-compatible formatter from [oxc.rs](https://oxc.rs)
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

Install oxfmt:

- [oxfmt](https://oxc.rs/docs/guide/usage/formatter) (`npm i -g oxfmt`)

### Behavior

formatter runs oxfmt on supported files. When oxfmt is not available, supported files are left untouched and only the EOF newline is ensured.

| Condition           | Action used      |
| ------------------- | ---------------- |
| oxfmt installed     | oxfmt            |
| oxfmt not installed | EOF newline only |

## Supported File Types

| Formatter | Extensions                                                                                                                                                                  |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| oxfmt     | `.ts` `.tsx` `.js` `.jsx` `.mts` `.cts` `.mjs` `.cjs` `.json` `.jsonc` `.json5` `.css` `.scss` `.less` `.html` `.vue` `.yaml` `.yml` `.toml` `.md` `.mdx` `.graphql` `.gql` |

## How It Works

1. Reads PostToolUse hook JSON from stdin
2. Ignores non-Write/Edit/MultiEdit tools
3. Canonicalizes the file path (rejects symlink tricks, null bytes, relative paths)
4. Verifies the file is within the current working directory
5. Loads config from `.claude/tools.json` (if present)
6. Formats the file in-place with oxfmt (falls back to EOF newline if oxfmt is unavailable)

## Exit Codes

| Code | Meaning |
| ---- | ------- |
| 0    | Always  |

The formatter never blocks operations. It silently formats on success and logs errors to stderr.

As a PostToolUse hook, the exit code is interpreted by Claude Code (code 2 feeds stderr back to Claude, other non-zero codes surface a `hook error` in the transcript), so it cannot also encode formatted-vs-skipped-vs-error. That distinction is carried by the structured output below instead.

## Structured Output

Set `FORMATTER_VERBOSE=1` to emit a JSON line to stderr for each formatting action describing what happened. A single file can produce more than one line when it falls back (for example `oxfmt` skipped, then `eof-newline` applied). Default operation stays silent. This lets an agent see which formatter touched which file without parsing exit codes.

```json
{ "file": "/path/to/app.ts", "formatter": "oxfmt", "action": "formatted" }
```

`formatter` is `oxfmt` or `eof-newline`. `action` is one of:

| `action`       | When it appears                                                                 |
| -------------- | ------------------------------------------------------------------------------- |
| `formatted`    | The formatter ran in write mode. oxfmt does not diff, so this covers no-op runs |
| `would-format` | Dry-run only. The file is not yet formatted and would change                    |
| `unchanged`    | Dry-run only. The file is already formatted                                     |
| `skipped`      | A supported file had no available formatter                                     |
| `error`        | The formatter failed                                                            |

### Degraded outcomes

When a supported file is left unformatted (`skipped` or `error`), the record carries three extra fields so an agent can react without parsing free-form stderr. `degraded` is `true`, `next_step` names the remediation, and `notes` lists the underlying diagnostics (omitted when there are none).

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

Neutral records (`formatted`, `unchanged`, `would-format`) keep the original three-key shape and never include these fields. Degraded outcomes always surface even without `FORMATTER_VERBOSE`, where they print as a human-readable line instead of JSON.

## Dry Run

Set `FORMATTER_DRY_RUN=1` to report what would change without writing any file. The structured output is always emitted in this mode. oxfmt files use `oxfmt --check`, so an `action` of `would-format` means the file is not yet formatted and `unchanged` means it already is.

The `eof-newline` formatter reports `would-format` only for files that need a trailing newline. A file that already ends correctly produces no line, whereas `oxfmt --check` also reports `unchanged`. Dry-run output therefore lists the files that would change, not every file inspected.

## Configuration

Add a `formatter` key to `.claude/tools.json` at your project root. All fields are optional — only specify what you want to override.

**Defaults** (no config file needed):

- All formatters enabled

### Schema

```json
{
  "formatter": {
    "enabled": true,
    "oxfmt": true,
    "eofNewline": true
  }
}
```

### Examples

Disable oxfmt (EOF newline only):

```json
{
  "formatter": {
    "oxfmt": false
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
│   └── tools.json     ← {"formatter": {"oxfmt": false}}
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
