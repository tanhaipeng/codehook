# codehook

`codehook` is a command hook binary that can be used by Codex, Claude Code, and
Hermes. It can also be used from OpenClaw through the typed plugin wrapper in
`examples/openclaw-plugin/`.

It reads hook event JSON from stdin, evaluates local policy, and emits a JSON
decision only when the event should be blocked.

Set `CODEHOOK_AGENT=codex`, `claude`, `hermes`, or `openclaw` to make the
request source explicit. If it is not set, `codehook` falls back to environment,
event-name, and tool-name detection.

The examples below use `/path/to/codehook/target/release/codehook` as a demo
absolute path. Replace it with the actual binary path after building.

Default behavior:

- Blocks high-risk shell commands such as `git reset --hard`, force pushes,
  direct disk writes, risky recursive removals, and `curl | sh` style commands.
- Blocks access to protected paths such as `.env*`, `.git/`, `.ssh/`, private
  key files, and common credential files.
- Blocks submitted prompts that appear to contain a private key.
- Does nothing for unsupported events, so it can be attached broadly.

## Build

```sh
cargo build --release
```

## Architecture

```text
codehook/
├── Cargo.toml
├── src/
│   ├── main.rs
│   └── lib.rs
├── examples/
│   ├── codex-hooks.json
│   ├── claude-settings.json
│   ├── hermes-config.yaml
│   └── openclaw-plugin/
│       ├── README.md
│       ├── index.ts
│       ├── openclaw.plugin.json
│       └── package.json
└── README.md
```

`src/main.rs` is the binary entry point. It handles `--help`, reads hook JSON
from stdin, loads runtime configuration from environment variables, optionally
writes metadata-only audit entries, and prints a JSON decision only when the
hook needs to block something.

`src/lib.rs` contains the reusable hook logic. It defines the shared input
shape, detects the caller agent, evaluates policy for tool and prompt events,
builds compatible denial JSON, and contains the unit tests for the guard rules.

The runtime flow is:

```text
stdin JSON -> HookInput -> Policy::from_env -> evaluate -> optional stdout JSON
```

Usage tracking uses a side-channel flow:

```text
stdin JSON or transcript JSONL -> usage extractor -> CODEHOOK_USAGE_LOG -> --usage-summary
```

`examples/` contains ready-to-adapt configuration snippets for Codex, Claude
Code, Hermes, and OpenClaw. All examples pass `CODEHOOK_AGENT` explicitly so
source detection does not depend on tool-specific environment variables.

## Codex

Add this to `<repo>/.codex/hooks.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|apply_patch|Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=codex /path/to/codehook/target/release/codehook",
            "timeout": 10,
            "statusMessage": "Checking repository hook policy"
          }
        ]
      }
    ],
    "PermissionRequest": [
      {
        "matcher": "Bash|apply_patch|Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=codex /path/to/codehook/target/release/codehook",
            "timeout": 10,
            "statusMessage": "Checking approval request"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=codex /path/to/codehook/target/release/codehook",
            "timeout": 10
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=codex CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl /path/to/codehook/target/release/codehook",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

Codex requires new or changed non-managed hooks to be reviewed. Open `/hooks`
inside Codex after adding this file and trust the hook definition.

## Claude Code

Add this to `<repo>/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|Read|Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=claude /path/to/codehook/target/release/codehook",
            "timeout": 10
          }
        ]
      }
    ],
    "PermissionRequest": [
      {
        "matcher": "Bash|Read|Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=claude /path/to/codehook/target/release/codehook",
            "timeout": 10
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=claude /path/to/codehook/target/release/codehook",
            "timeout": 10
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "CODEHOOK_AGENT=claude CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl /path/to/codehook/target/release/codehook",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

Use `/hooks` in Claude Code to verify that the hook is registered.

## Token Usage Tracking

Set `CODEHOOK_USAGE_LOG` to append token usage records as JSON Lines:

```sh
export CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl
```

Each record contains metadata plus normalized token buckets:

- `input_tokens`
- `output_tokens`
- `cache_creation_input_tokens`
- `cache_read_input_tokens`
- `cached_input_tokens`
- `reasoning_output_tokens`
- `total_tokens`

The extractor accepts common OpenAI, Anthropic, and agent-specific aliases such
as `prompt_tokens`, `completion_tokens`, `inputTokens`, `outputTokens`,
`cacheReadInputTokens`, and `reasoningOutputTokens`.

For Claude Code, hook payloads may not include usage directly. When
`transcript_path` is present, `codehook` tries to read the transcript JSONL and
uses the last entry with usage data. Disable this fallback with:

```sh
export CODEHOOK_USAGE_FROM_TRANSCRIPT=0
```

Summarize a usage log:

```sh
/path/to/codehook/target/release/codehook --usage-summary /path/to/codehook-usage.jsonl
```

The summary groups totals by agent, model, and session.

## Hermes

Add this to `~/.hermes/config.yaml` or merge it into your existing Hermes
config:

```yaml
hooks:
  pre_tool_call:
    - matcher: "terminal|write_file|patch"
      command: "/usr/bin/env CODEHOOK_AGENT=hermes CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl /path/to/codehook/target/release/codehook"
      timeout: 10
  pre_llm_call:
    - command: "/usr/bin/env CODEHOOK_AGENT=hermes CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl /path/to/codehook/target/release/codehook"
      timeout: 10
```

Hermes runs configured commands with `shell=false`, so the example uses
`/usr/bin/env CODEHOOK_AGENT=hermes ...` instead of shell-style inline
assignment.

## OpenClaw

OpenClaw tool and prompt blocking should use typed plugin hooks. The adapter in
`examples/openclaw-plugin/` registers:

- `before_tool_call` for tool policy.
- `before_agent_run` for prompt policy.
- `llm_output` and `model_call_ended` for token usage metadata.

Set the binary path before starting the OpenClaw Gateway:

```sh
export CODEHOOK_BIN=/path/to/codehook/target/release/codehook
export CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl
```

Then install or copy `examples/openclaw-plugin/` as a local OpenClaw plugin,
enable it, and restart the Gateway.

## Configuration

Runtime configuration is via environment variables:

- `CODEHOOK_AGENT=codex|claude|hermes|openclaw` explicitly sets the request
  source. Accepted Claude aliases are `cc`, `claudecode`, `claude-code`, and
  `claude_code`. Accepted OpenClaw aliases are `open-claw`, `open_claw`, and
  `oc`.
- `CODEHOOK_ENFORCE=0` disables blocking decisions.
- `CODEHOOK_BLOCK_SECRET_PROMPTS=0` disables prompt private-key blocking.
- `CODEHOOK_PROTECTED_PATTERNS=secret/,prod.env` adds comma-separated protected
  path fragments.
- `CODEHOOK_AUDIT_LOG=/path/to/codehook.jsonl` appends metadata-only audit
  entries. The log does not include full prompts, commands, or tool outputs.
- `CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl` appends normalized token
  usage records when usage data is present in the payload or transcript.
- `CODEHOOK_USAGE_FROM_TRANSCRIPT=0` disables transcript fallback extraction.
