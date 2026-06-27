# OpenClaw codehook policy plugin

This plugin adapts OpenClaw typed plugin hooks to the `codehook` binary.

It registers:

- `before_tool_call` to block risky tool calls.
- `before_agent_run` to block unsafe user input before the model sees it.
- `llm_output` and `model_call_ended` to forward token usage metadata to
  `codehook`.

Set `CODEHOOK_BIN` to the absolute path of the compiled binary:

```sh
export CODEHOOK_BIN=/path/to/codehook/target/release/codehook
export CODEHOOK_USAGE_LOG=/path/to/codehook-usage.jsonl
```

Install this folder as a local OpenClaw plugin or copy it into your OpenClaw
plugin directory, then enable it and restart the Gateway.
