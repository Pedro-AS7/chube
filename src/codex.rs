//! CLI e histórico local do Codex. Não lê credenciais nem altera sua configuração.
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::agent::Agent;
use crate::claude::{Start, home_dir};
use crate::sessions::SavedSession;
use crate::usage::{Limit, Window};

pub fn home() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex"))
}

/// O launcher não herda o nvm. Prefere o PATH, depois ~/.local/bin e o Node mais recente.
pub fn binary() -> PathBuf {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).map(|p| p.join("codex")).collect())
        .unwrap_or_default();
    candidates.push(home_dir().join(".local/bin/codex"));
    let nvm = std::env::var_os("NVM_DIR").map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".nvm"));
    let mut versions: Vec<_> = fs::read_dir(nvm.join("versions/node")).into_iter().flatten().flatten()
        .map(|entry| entry.path()).collect();
    versions.sort_by_key(|path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let parts: Vec<u32> = name.trim_start_matches('v').split('.').map(|v| v.parse().unwrap_or(0)).collect();
        (parts.first().copied().unwrap_or(0), parts.get(1).copied().unwrap_or(0), parts.get(2).copied().unwrap_or(0))
    });
    candidates.extend(versions.into_iter().rev().map(|p| p.join("bin/codex")));
    candidates.into_iter().find(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("codex"))
}

pub fn command(start: &Start, model: Option<&str>, prompt: Option<&str>, auto: bool) -> (String, Vec<String>) {
    let bin = binary();
    let mut args: Vec<String> = ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_THREAD_ID", "CODEX_SESSION_ID"]
        .into_iter().flat_map(|key| ["-u".into(), key.into()]).collect();
    // O wrapper npm usa /usr/bin/env node, então o diretório do binário precisa estar no PATH.
    if let Some(parent) = bin.parent().filter(|p| !p.as_os_str().is_empty()) {
        let mut paths = vec![parent.to_path_buf()];
        paths.extend(std::env::var_os("PATH").iter().flat_map(|p| std::env::split_paths(p)));
        if let Ok(path) = std::env::join_paths(paths) {
            args.push(format!("PATH={}", path.to_string_lossy()));
        }
    }
    args.push(bin.display().to_string());
    args.extend(self::args(start, model, prompt, auto));
    ("env".into(), args)
}

/// Argumentos do `codex`, sem o binário (os mesmos aqui e na VPS). `auto` é o modo do
/// `omarchy agent`: aprova sozinho (`--approve-for-me`).
pub fn args(start: &Start, model: Option<&str>, prompt: Option<&str>, auto: bool) -> Vec<String> {
    let mut args = Vec::new();
    match start {
        Start::New { .. } => {},
        Start::Resume { session_id } => args.extend(["resume".into(), session_id.clone()]),
        Start::Fork { parent_session_id } => args.extend(["fork".into(), parent_session_id.clone()]),
    }
    if auto { args.push("--approve-for-me".into()); }
    // O título OSC identifica /new e /resume sem depender da largura ou do texto da conversa.
    args.extend(["-c".into(), "tui.terminal_title=[\"thread-id\"]".into()]);
    if let Some(model) = model { args.extend(["--model".into(), model.into()]); }
    if let Some(prompt) = prompt { args.extend(["--".into(), prompt.into()]); }
    args
}


/// O Codex abrevia o título OSC com `...`; exige um prefixo longo e uma única sessão.
pub fn title_hint(title: &str) -> Option<String> {
    let hint = title.trim().strip_suffix("...").unwrap_or(title.trim());
    (hint.len() >= 16 && hint.len() <= 36
        && hint.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-'))
        .then(|| hint.to_ascii_lowercase())
}

pub fn resolve_hint(hint: &str) -> Option<String> {
    let mut files = Vec::new();
    rollouts(&home().join("sessions"), &mut files);
    resolve_in(hint, &files)
}

fn resolve_in(hint: &str, files: &[PathBuf]) -> Option<String> {
    title_hint(hint)?;
    let mut ids = files.iter().filter_map(|path| {
        let name = path.file_stem()?.to_str()?;
        let id = name.get(name.len().checked_sub(36)?..)?;
        (id.starts_with(hint) && uuid::Uuid::parse_str(id).is_ok()).then(|| id.to_owned())
    });
    let id = ids.next()?;
    ids.all(|other| other == id).then_some(id)
}

fn rollouts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) { rollouts(&path, out); }
        else if path.extension().is_some_and(|ext| ext == "jsonl") { out.push(path); }
    }
}

pub fn session_file(id: &str) -> Option<PathBuf> {
    // Não aceita fragmentos de caminho nem um id parcial como correspondência.
    uuid::Uuid::parse_str(id).ok()?;
    let mut files = Vec::new();
    rollouts(&home().join("sessions"), &mut files);
    let suffix = format!("-{id}.jsonl");
    files.into_iter().find(|p| p.file_name().is_some_and(|name| name.to_string_lossy().ends_with(&suffix)))
}

pub fn list() -> Vec<SavedSession> {
    let mut files = Vec::new();
    rollouts(&home().join("sessions"), &mut files);
    let mut names = std::collections::HashMap::new();
    if let Ok(file) = fs::File::open(home().join("session_index.jsonl")) {
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if let (Some(id), Some(name)) = (v["id"].as_str(), v["thread_name"].as_str()) {
                    names.insert(id.to_owned(), name.to_owned());
                }
            }
        }
    }
    let mut sessions: Vec<_> = files.into_iter().filter_map(|path| {
        let file = fs::File::open(&path).ok()?;
        let modified = file.metadata().ok()?.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        summarize(BufReader::new(file), modified).map(|mut saved| {
            if let Some(name) = names.get(&saved.id) { saved.title = name.clone(); }
            saved
        })
    }).collect();
    sessions.sort_by(|a,b| b.modified.cmp(&a.modified));
    sessions
}

fn summarize(reader: impl BufRead, modified: SystemTime) -> Option<SavedSession> {
    let mut id = None;
    let mut cwd = None;
    let mut title = None;
    // Só o começo: não é preciso percorrer todo o histórico para encontrar a primeira mensagem.
    for line in reader.take(2 * 1024 * 1024).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue; };
        let p = &v["payload"];
        if v["type"] == "session_meta" {
            // Subagentes e execuções automáticas não são chats interativos.
            if p["source"].is_object() || p["source"] == "exec" { return None; }
            id = p["id"].as_str().or(p["session_id"].as_str()).map(str::to_owned);
            cwd = p["cwd"].as_str().map(PathBuf::from);
        }
        if v["type"] == "event_msg" && p["type"] == "user_message" {
            title = p["message"].as_str().map(|s| s.lines().next().unwrap_or_default().chars().take(120).collect());
            break;
        }
    }
    Some(SavedSession { agent: Agent::Codex, id: id?, cwd: cwd?, title: title.unwrap_or_else(|| "Codex".into()), modified, running: None, host: None })
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub status: Option<crate::chat::Status>,
    pub context_tokens: u32,
    pub context_window: Option<u32>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub limits: Vec<Limit>,
}

pub fn snapshot(id: &str) -> Option<Snapshot> {
    let mut file = fs::File::open(session_file(id)?).ok()?;
    let start = file.metadata().ok()?.len().saturating_sub(512 * 1024);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut text = String::new();
    // O corte pode cair num caractere UTF-8.
    let mut bytes = Vec::new(); file.read_to_end(&mut bytes).ok()?;
    text.push_str(&String::from_utf8_lossy(&bytes));
    Some(parse_snapshot(&text))
}

fn parse_snapshot(text: &str) -> Snapshot {
    use crate::chat::Status;
    let mut result = Snapshot::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue; };
        let p = &v["payload"];
        if v["type"] == "turn_context" {
            result.model = p["model"].as_str().map(str::to_owned).or(result.model);
        }
        if v["type"] != "event_msg" { continue; }
        match p["type"].as_str() {
            Some("task_started" | "turn_started") => {
                result.status = Some(Status::Working);
                result.context_window = number(&p["model_context_window"]).or(result.context_window);
            },
            Some("task_complete" | "turn_complete" | "turn_aborted") => result.status = Some(Status::Ready),
            Some("user_message") => result.title = p["message"].as_str()
                .map(|s| s.lines().next().unwrap_or_default().chars().take(40).collect()),
            Some("token_count") => {
                if let Some(tokens) = number(&p["info"]["last_token_usage"]["input_tokens"]) { result.context_tokens = tokens; }
                result.context_window = number(&p["info"]["model_context_window"]).or(result.context_window);
                if p["rate_limits"].is_object() { result.limits = parse_limits(&p["rate_limits"]); }
            },
            _ => {},
        }
    }
    result
}

fn number(v: &Value) -> Option<u32> { v.as_u64().and_then(|n| n.try_into().ok()) }

fn parse_limits(v: &Value) -> Vec<Limit> {
    ["primary", "secondary"].into_iter().filter_map(|key| {
        let p = &v[key];
        let window = match p["window_minutes"].as_u64()? {
            300 => Window::Session,
            10080 => Window::Week("all models".into()),
            _ => return None,
        };
        Some(Limit { window, used: (p["used_percent"].as_f64()? / 100.0).clamp(0.0,1.0) as f32,
            resets: p["resets_at"].as_i64().and_then(|n| chrono::DateTime::from_timestamp(n,0)).map(|t| t.with_timezone(&chrono::Local)) })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starts_resume_fork_and_literal_prompt() {
        for (start, expected) in [
            (Start::New { session_id: "unused".into() }, vec![]),
            (Start::Resume { session_id: "abc".into() }, vec!["resume", "abc"]),
            (Start::Fork { parent_session_id: "abc".into() }, vec!["fork", "abc"]),
        ] {
            let (program,args) = command(&start, Some("modelo"), Some("-oi"), false);
            assert_eq!(program,"env");
            assert_eq!(&args[..4], ["-u","OPENAI_API_KEY","-u","CODEX_API_KEY"]);
            assert!(!args.iter().any(|arg| arg == "--session-id" || arg == "--resume" || arg.contains("bypass")));
            if !expected.is_empty() { assert!(args.windows(2).any(|w| w == expected.as_slice())); }
            assert_eq!(&args[args.len()-4..],["--model","modelo","--","-oi"]);
        }
    }
    #[test]
    fn modo_omarchy_aprova_sozinho() {
        let args = args(&Start::New { session_id: "x".into() }, None, None, true);
        assert!(args.contains(&"--approve-for-me".to_owned()));
    }
    #[test]
    fn abbreviated_titles_require_unambiguous_session_identity() {
        let a = "01a15f21-3a2d-7c13-8d59-c55570000000";
        let b = "01a15f21-3a2d-7c13-8d59-c55571111111";
        let hint = title_hint("01a15f21-3a2d-7c13-8d59-c5557...").unwrap();
        let file = |id| PathBuf::from(format!("rollout-2026-10-21T12-00-00-{id}.jsonl"));
        assert_eq!(resolve_in(&hint, &[file(a)]), Some(a.into()));
        assert_eq!(resolve_in(&hint, &[file(a), file(b)]), None);
        assert_eq!(resolve_in(&hint, &[]), None);
        assert_eq!(title_hint("projeto"), None);
        assert_eq!(title_hint("../../etc/passwd"), None);
    }
    #[test]
    fn reads_metadata_and_user_title() {
        let data = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"cwd\":\"/tmp\",\"source\":\"cli\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"Corrigir teste\\nDetalhes\"}}\n";
        let saved = summarize(&data[..],SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(saved.agent,Agent::Codex); assert_eq!(saved.title,"Corrigir teste"); assert_eq!(saved.cwd,PathBuf::from("/tmp"));
    }
    #[test]
    fn latest_usage_is_context_not_accumulated_billing() {
        let s = parse_snapshot(r#"linha cortada
{"type":"turn_context","payload":{"model":"modelo"}}
{"type":"event_msg","payload":{"type":"task_started","model_context_window":475000}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":900000},"last_token_usage":{"input_tokens":12000},"model_context_window":475000},"rate_limits":{"primary":{"used_percent":25,"window_minutes":300},"secondary":{"used_percent":10,"window_minutes":10080}}}}
{"type":"event_msg","payload":{"type":"task_complete"}}
"#);
        assert_eq!(s.context_tokens,12000); assert_eq!(s.context_window,Some(475000)); assert_eq!(s.model.as_deref(),Some("modelo"));
        assert_eq!(s.status,Some(crate::chat::Status::Ready)); assert_eq!(s.limits.len(),2); assert_eq!(s.limits[0].used,0.25);
    }
}
