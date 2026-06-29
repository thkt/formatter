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

formatter runs oxfmt on supported files. When oxfmt is not available, a supported file is left unchanged and reported as a degraded outcome (exit 0); it is not given an EOF newline. EOF-newline enforcement applies only to files outside oxfmt's extensions.

| Condition                              | Action used                          |
| -------------------------------------- | ------------------------------------ |
| Supported extension, oxfmt installed   | oxfmt                                |
| Supported extension, oxfmt unavailable | left unchanged, reported as degraded |
| Other extension                        | EOF newline only                     |

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
6. Formats the file in-place: oxfmt for supported extensions, EOF-newline enforcement for the rest. A supported file is owned by oxfmt end to end — if the binary is unavailable the file is left unchanged and reported as degraded (exit 0), not downgraded to EOF newline

## Exit Codes

Exit codes follow sysexits.h, matching the Group 3 (Hook tool) baseline of ADR-0066.

| Code | Source        | Meaning                                                        |
| ---- | ------------- | -------------------------------------------------------------- |
| 0    | `EX_OK`       | Any formatting outcome, success or failure (silent-fix policy) |
| 64   | `EX_USAGE`    | The hook input JSON on stdin was not the expected shape        |
| 70   | `EX_SOFTWARE` | An internal defect in the formatter itself (a caught panic)    |

Silent-fix policy: a failure of the formatting itself (a syntax error oxfmt cannot fix, a missing binary, etc.) stays exit 0. This keeps the hook from blocking the developer over a style concern; such failures are reported as advisory through the structured output below. Non-zero codes are reserved for the two infrastructure faults — a broken hook contract (invalid input) and an internal bug — so a metrics dashboard can branch on them.

As a PostToolUse hook, the exit code is interpreted by Claude Code (code 2 feeds stderr back to Claude, other non-zero codes surface a `hook error` in the transcript), so it cannot also encode formatted-vs-error. That distinction is carried by the structured output below instead.

## Structured Output

Set `FORMATTER_VERBOSE=1` to emit a JSON line to stderr for each formatting action describing what happened. Each file produces exactly one line: `oxfmt` and `eof-newline` are mutually exclusive per file (a supported extension takes oxfmt, the rest take eof-newline), so a single file never reports both. Default operation stays silent. This lets an agent see which formatter touched which file without parsing exit codes.

```json
{ "file": "/path/to/app.ts", "formatter": "oxfmt", "action": "formatted" }
```

`formatter` is `oxfmt` or `eof-newline`. `action` is one of:

| `action`       | When it appears                                                                 |
| -------------- | ------------------------------------------------------------------------------- |
| `formatted`    | The formatter ran in write mode. oxfmt does not diff, so this covers no-op runs |
| `would-format` | Dry-run only. The file is not yet formatted and would change                    |
| `unchanged`    | Dry-run only. The file is already formatted                                     |
| `error`        | The formatter failed, including a missing oxfmt binary                          |

### Degraded outcomes

When a supported file is left unformatted (`error`), the record carries three extra fields so an agent can react without parsing free-form stderr. `degraded` is `true`, `next_step` names the remediation, and `notes` lists the underlying diagnostics (omitted when there are none).

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
