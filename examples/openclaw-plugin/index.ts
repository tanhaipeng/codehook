import { spawn } from "node:child_process";
import { definePluginEntry } from "openclaw/plugin-sdk/plugin-entry";

const DEFAULT_CODEHOOK_BIN = "/path/to/codehook/target/release/codehook";

type CodehookResult = {
  blocked: boolean;
  reason?: string;
};

export default definePluginEntry({
  id: "codehook_policy",
  name: "Codehook Policy",
  description: "Runs the codehook policy binary before tool calls and agent runs.",
  register(api) {
    api.on(
      "before_tool_call",
      async (event: any) => {
        const result = await runCodehook({
          hook_event_name: "before_tool_call",
          tool_name: event.toolName,
          tool_input: event.params,
          cwd: event.cwd,
          session_id: event.sessionId,
          extra: {
            derivedPaths: event.derivedPaths,
            runId: event.runId,
            toolCallId: event.toolCallId,
          },
        });

        if (!result.blocked) {
          return;
        }

        return {
          block: true,
          blockReason: result.reason ?? "Blocked by codehook policy.",
        };
      },
      { priority: 100, timeoutMs: 5000 },
    );

    api.on(
      "before_agent_run",
      async (event: any) => {
        const result = await runCodehook({
          hook_event_name: "before_agent_run",
          prompt: event.prompt,
          cwd: event.cwd,
          session_id: event.sessionId,
          extra: {
            runId: event.runId,
          },
        });

        if (!result.blocked) {
          return;
        }

        const reason = result.reason ?? "Blocked by codehook policy.";
        return {
          outcome: "block",
          reason,
          message: reason,
        };
      },
      { priority: 100, timeoutMs: 5000 },
    );

    api.on(
      "llm_output",
      async (event: any) => {
        await recordUsage({
          hook_event_name: "llm_output",
          model: event.model ?? event.resolvedModel,
          session_id: event.sessionId,
          extra: {
            provider: event.provider ?? event.resolvedProvider,
            runId: event.runId,
            callId: event.callId,
            usage: event.usage,
            usageState: event.usageState,
            contextTokenBudget: event.contextTokenBudget,
          },
        });
      },
      { priority: 10, timeoutMs: 5000 },
    );

    api.on(
      "model_call_ended",
      async (event: any) => {
        await recordUsage({
          hook_event_name: "model_call_ended",
          model: event.model ?? event.resolvedModel,
          session_id: event.sessionId,
          extra: {
            provider: event.provider ?? event.resolvedProvider,
            runId: event.runId,
            callId: event.callId,
            outcome: event.outcome,
            durationMs: event.durationMs,
            usage: event.usage,
            usageState: event.usageState,
          },
        });
      },
      { priority: 10, timeoutMs: 5000 },
    );
  },
});

async function runCodehook(payload: Record<string, unknown>): Promise<CodehookResult> {
  const codehookBin = process.env.CODEHOOK_BIN ?? DEFAULT_CODEHOOK_BIN;
  const child = spawn(codehookBin, {
    env: {
      ...process.env,
      CODEHOOK_AGENT: "openclaw",
    },
    stdio: ["pipe", "pipe", "pipe"],
  });

  const stdoutChunks: Buffer[] = [];
  const stderrChunks: Buffer[] = [];

  child.stdout.on("data", (chunk) => stdoutChunks.push(Buffer.from(chunk)));
  child.stderr.on("data", (chunk) => stderrChunks.push(Buffer.from(chunk)));

  child.stdin.end(JSON.stringify(payload));

  const exitCode = await new Promise<number | null>((resolve, reject) => {
    child.on("error", reject);
    child.on("close", resolve);
  });

  if (exitCode !== 0) {
    const stderr = Buffer.concat(stderrChunks).toString("utf8").trim();
    throw new Error(stderr || `codehook exited with status ${exitCode}`);
  }

  const stdout = Buffer.concat(stdoutChunks).toString("utf8").trim();
  if (!stdout) {
    return { blocked: false };
  }

  const parsed = JSON.parse(stdout);
  return parseCodehookDecision(parsed);
}

function parseCodehookDecision(value: any): CodehookResult {
  if (value?.decision === "block") {
    return { blocked: true, reason: value.reason };
  }

  if (value?.action === "block") {
    return { blocked: true, reason: value.message };
  }

  const hookOutput = value?.hookSpecificOutput;
  if (hookOutput?.permissionDecision === "deny") {
    return {
      blocked: true,
      reason: hookOutput.permissionDecisionReason,
    };
  }

  return { blocked: false };
}

async function recordUsage(payload: Record<string, unknown>): Promise<void> {
  if (!process.env.CODEHOOK_USAGE_LOG) {
    return;
  }

  try {
    await runCodehook(payload);
  } catch (error) {
    console.warn("[codehook_policy] usage recording failed", error);
  }
}
