//! Sessões salvas do Claude Code (`~/.claude/projects/<pasta>/<id>.jsonl`)
//! e sessões rodando agora (`~/.claude/sessions/<pid>.json`).

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::claude::home_dir;

#[derive(Clone, Debug)]
pub struct SavedSession {
    pub agent: crate::agent::Agent,
    pub id: String,
    pub cwd: PathBuf,
    pub title: String,
    pub modified: SystemTime,
    /// Status (`busy`, `idle`…) se a sessão está aberta em outro processo do Claude Code.
    pub running: Option<String>,
    /// Host do SSH, numa sessão da VPS; `None` nas desta máquina.
    pub host: Option<String>,
}

/// Lista as sessões salvas, da mais recente para a mais antiga.
pub fn list() -> Vec<SavedSession> {
    let running = running_sessions();
    let mut files: Vec<(PathBuf, SystemTime)> = Vec::new();
    let Ok(projects) = fs::read_dir(home_dir().join(".claude/projects")) else {
        return Vec::new();
    };
    for project in projects.flatten() {
        let Ok(entries) = fs::read_dir(project.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                let modified = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                files.push((path, modified));
            }
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));

    files
        .into_iter()
        .filter_map(|(path, modified)| summarize(&path, modified))
        .map(|mut session| {
            session.running = running.get(&session.id).cloned();
            session
        })
        .collect()
}

/// Lê só o necessário para a lista: pasta e título.
fn summarize(path: &Path, modified: SystemTime) -> Option<SavedSession> {
    let id = path.file_stem()?.to_str()?.to_owned();
    let file = fs::File::open(path).ok()?;

    let mut cwd = None;
    let mut custom_title = None;
    let mut ai_title = None;
    let mut last_prompt = None;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        // Filtra pelo texto antes de fazer o parse: os arquivos podem ser grandes.
        let wanted = (cwd.is_none() && line.contains("\"cwd\""))
            || line.contains("\"custom-title\"")
            || line.contains("\"ai-title\"")
            || line.contains("\"last-prompt\"");
        if !wanted {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if cwd.is_none() {
            cwd = value["cwd"].as_str().map(PathBuf::from);
        }
        match value["type"].as_str() {
            Some("custom-title") => custom_title = value["customTitle"].as_str().map(str::to_owned),
            Some("ai-title") => ai_title = value["aiTitle"].as_str().map(str::to_owned),
            Some("last-prompt") => last_prompt = value["lastPrompt"].as_str().map(str::to_owned),
            _ => {}
        }
    }

    // Sem nenhuma mensagem do usuário, não há o que retomar.
    let title = custom_title.or(ai_title).or(last_prompt)?;
    Some(SavedSession {
        agent: crate::agent::Agent::Claude,
        id,
        cwd: cwd?,
        title: title.lines().next().unwrap_or_default().to_owned(),
        modified,
        running: None,
        host: None,
    })
}

/// Sessões abertas agora em algum processo do Claude Code: id → status.
fn running_sessions() -> HashMap<String, String> {
    let mut running = HashMap::new();
    let Ok(entries) = fs::read_dir(home_dir().join(".claude/sessions")) else {
        return running;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Só os `.json`; os `.key` guardam tokens e não interessam aqui.
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let (Some(pid), Some(id)) = (value["pid"].as_u64(), value["sessionId"].as_str()) else {
            continue;
        };
        // O arquivo pode ter ficado para trás se o processo morreu sem limpar.
        if Path::new(&format!("/proc/{pid}")).exists() {
            let status = value["status"].as_str().unwrap_or("aberta").to_owned();
            running.insert(id.to_owned(), status);
        }
    }
    running
}

/// Processo do Claude Code aberto agora, pelo `~/.claude/sessions/<pid>.json`.
#[derive(Clone, Debug)]
pub struct Running {
    pub session_id: String,
    /// `busy`, `idle` ou `waiting`.
    pub status: String,
}

pub fn running(pid: u32) -> Option<Running> {
    let text = fs::read_to_string(home_dir().join(format!(".claude/sessions/{pid}.json"))).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    Some(Running {
        session_id: value["sessionId"].as_str()?.to_owned(),
        status: value["status"].as_str().unwrap_or_default().to_owned(),
    })
}

/// Tokens de contexto e modelo da última resposta de uma sessão.
#[derive(Clone, Debug)]
pub struct Stats {
    pub context_tokens: u32,
    pub model: Option<String>,
}

/// Quanto do fim do `.jsonl` é lido para achar a última resposta.
const TAIL_BYTES: u64 = 512 * 1024;

pub fn stats(session_id: &str) -> Option<Stats> {
    let path = session_file(session_id)?;
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    let tail = String::from_utf8_lossy(&tail);

    // A primeira linha pode estar cortada no meio; o parse dela falha e é ignorada.
    tail.lines().rev().find_map(|line| {
        if !line.contains("\"assistant\"") {
            return None;
        }
        let value = serde_json::from_str::<Value>(line).ok()?;
        if value["type"] != "assistant" || value["isSidechain"] == true {
            return None;
        }
        let message = &value["message"];
        let tokens: u64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
            .iter()
            .filter_map(|key| message["usage"][*key].as_u64())
            .sum();
        (tokens > 0).then(|| Stats {
            context_tokens: tokens as u32,
            model: message["model"].as_str().map(str::to_owned),
        })
    })
}

/// Acha o `.jsonl` da sessão em qualquer pasta de `~/.claude/projects`.
pub fn session_file(session_id: &str) -> Option<PathBuf> {
    let name = format!("{session_id}.jsonl");
    fs::read_dir(home_dir().join(".claude/projects"))
        .ok()?
        .flatten()
        .map(|project| project.path().join(&name))
        .find(|path| path.is_file())
}

/// "agora", "há 5 min", "há 3 h", "há 2 dias".
pub fn relative_time(time: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(time)
        .map_or(0, |elapsed| elapsed.as_secs());
    match seconds {
        0..60 => "agora".into(),
        60..3600 => format!("há {} min", seconds / 60),
        3600..86_400 => format!("há {} h", seconds / 3600),
        86_400..172_800 => "ontem".into(),
        _ => format!("há {} dias", seconds / 86_400),
    }
}
