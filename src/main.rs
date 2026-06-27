use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    process::ExitCode,
};

use codehook::{
    HookInput, Policy, audit_line, evaluate, summarize_usage_jsonl, usage_record_line,
    usage_record_line_from_value,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    if args.first().is_some_and(|arg| arg == "--usage-summary") {
        let path = args
            .get(1)
            .ok_or_else(|| "missing path for --usage-summary".to_string())?;
        let content = fs::read_to_string(path)
            .map_err(|error| format!("failed to read usage log {path}: {error}"))?;
        println!("{}", summarize_usage_jsonl(&content));
        return Ok(());
    }

    let mut stdin = String::new();
    io::stdin()
        .read_to_string(&mut stdin)
        .map_err(|error| format!("failed to read hook input: {error}"))?;

    if stdin.trim().is_empty() {
        return Ok(());
    }

    let input: HookInput = serde_json::from_str(&stdin)
        .map_err(|error| format!("failed to parse hook input JSON: {error}"))?;
    let policy = Policy::from_env();
    let outcome = evaluate(&input, &policy);

    if let Ok(path) = env::var("CODEHOOK_AUDIT_LOG") {
        append_audit_log(&path, &audit_line(&input, &outcome))?;
    }

    if let Ok(path) = env::var("CODEHOOK_USAGE_LOG") {
        append_usage_log(&path, &input);
    }

    if let Some(output) = outcome.output {
        let stdout = serde_json::to_string(&output)
            .map_err(|error| format!("failed to encode hook output JSON: {error}"))?;
        println!("{stdout}");
    }

    Ok(())
}

fn append_audit_log(path: &str, line: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open CODEHOOK_AUDIT_LOG {path}: {error}"))?;

    writeln!(file, "{line}").map_err(|error| format!("failed to write audit log {path}: {error}"))
}

fn append_usage_log(path: &str, input: &HookInput) {
    let Some(line) = usage_record_line(input).or_else(|| transcript_usage_record_line(input))
    else {
        return;
    };

    if let Err(error) = append_line(path, &line) {
        eprintln!("{error}");
    }
}

fn append_line(path: &str, line: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open log {path}: {error}"))?;

    writeln!(file, "{line}").map_err(|error| format!("failed to write log {path}: {error}"))
}

fn transcript_usage_record_line(input: &HookInput) -> Option<String> {
    if env::var("CODEHOOK_USAGE_FROM_TRANSCRIPT").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    }) {
        return None;
    }

    let path = input.transcript_path.as_deref()?;
    let path = expand_home(path);
    let file = File::open(&path).ok()?;
    let reader = BufReader::new(file);
    let mut last_usage = None;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if let Some(record) = usage_record_line_from_value(input, &value, "transcript") {
            last_usage = Some(record);
        }
    }

    last_usage
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return env::var("HOME").unwrap_or_else(|_| path.to_string());
    }

    if let Some(stripped) = path.strip_prefix("~/")
        && let Ok(home) = env::var("HOME")
    {
        return format!("{home}/{stripped}");
    }

    path.to_string()
}

fn print_help() {
    println!(
        "codehook\n\n\
         Reads a Codex, Claude Code, Hermes, or OpenClaw-adapted hook event JSON from stdin and emits a compatible JSON decision.\n\n\
         Commands:\n\
           --usage-summary <usage.jsonl>       Print aggregate token usage as JSON.\n\n\
         Environment:\n\
           CODEHOOK_AGENT=codex|claude|hermes|openclaw\n\
                                             Explicitly set the caller agent.\n\
           CODEHOOK_ENFORCE=0                 Disable blocking decisions.\n\
           CODEHOOK_BLOCK_SECRET_PROMPTS=0    Disable private-key prompt blocking.\n\
           CODEHOOK_PROTECTED_PATTERNS=a,b    Add comma-separated protected path fragments.\n\
           CODEHOOK_AUDIT_LOG=/path/log.jsonl Append metadata-only audit entries.\n\
           CODEHOOK_USAGE_LOG=/path/usage.jsonl\n\
                                             Append token usage records when usage data is present.\n\
           CODEHOOK_USAGE_FROM_TRANSCRIPT=0    Disable transcript fallback usage extraction.\n"
    );
}
