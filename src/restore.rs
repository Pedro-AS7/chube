//! Lista de chats abertos, gravada em `~/.local/state/chube/chats.json` a cada mudança
//! (não só ao fechar: um logout ou um `kill` não passam pelo fechamento) e reaberta quando o
//! app inicia, cada chat com `--resume`.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::claude::home_dir;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenChat {
    pub agent: crate::agent::Agent,
    pub session_id: String,
    pub cwd: PathBuf,
    pub title: String,
    /// `--model` com que o chat foi aberto.
    pub model: Option<String>,
    /// Título do chat de origem, num sub-chat.
    pub parent: Option<String>,
    /// Modo do `omarchy agent`.
    pub auto: bool,
    /// Host do SSH e sessão do tmux, num chat da VPS.
    pub host: Option<String>,
    pub tmux: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenChats {
    pub chats: Vec<OpenChat>,
    pub focused: usize,
}

fn path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home_dir().join(".local/state"))
        .join("chube/chats.json")
}

/// Lista gravada na última execução; vazia se não houver (ou se o arquivo estiver estragado).
pub fn load() -> OpenChats {
    fs::read_to_string(path())
        .ok()
        .and_then(|text| parse(&text))
        .unwrap_or_default()
}

pub fn save(open: &OpenChats) -> std::io::Result<()> {
    let path = path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Grava ao lado e renomeia: o app fechar no meio não deixa um JSON pela metade.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, to_json(open))?;
    fs::rename(tmp, path)
}

fn to_json(open: &OpenChats) -> String {
    let chats: Vec<Value> = open
        .chats
        .iter()
        .map(|chat| {
            json!({
                "agent": chat.agent.key(),
                "session_id": chat.session_id,
                "cwd": chat.cwd.display().to_string(),
                "title": chat.title,
                "model": chat.model,
                "parent": chat.parent,
                "auto": chat.auto,
                "host": chat.host,
                "tmux": chat.tmux,
            })
        })
        .collect();
    let value = json!({ "focused": open.focused, "chats": chats });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn parse(text: &str) -> Option<OpenChats> {
    let value: Value = serde_json::from_str(text).ok()?;
    let chats = value
        .get("chats")?
        .as_array()?
        .iter()
        .filter_map(|chat| {
            let field = |key: &str| chat.get(key).and_then(Value::as_str).map(str::to_owned);
            Some(OpenChat {
                // Ausente nos arquivos anteriores ao suporte ao Codex; desconhecido é ignorado.
                agent: match field("agent") {
                    Some(value) => crate::agent::Agent::parse(&value)?,
                    None => crate::agent::Agent::Claude,
                },
                session_id: field("session_id")?,
                cwd: PathBuf::from(field("cwd")?),
                title: field("title").unwrap_or_else(|| "Chat".into()),
                model: field("model"),
                parent: field("parent"),
                auto: chat.get("auto").and_then(Value::as_bool).unwrap_or(false),
                host: field("host"),
                tmux: field("tmux"),
            })
        })
        .collect();
    let focused = value.get("focused").and_then(Value::as_u64).unwrap_or(0) as usize;
    Some(OpenChats { chats, focused })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: &str, parent: Option<&str>) -> OpenChat {
        OpenChat {
            agent: crate::agent::Agent::Claude,
            session_id: id.into(),
            cwd: PathBuf::from("/tmp/projeto"),
            title: format!("Chat {id}"),
            model: Some("haiku".into()),
            parent: parent.map(str::to_owned),
            auto: false,
            host: None,
            tmux: None,
        }
    }

    #[test]
    fn round_trip() {
        let open = OpenChats {
            chats: vec![chat("a", None), chat("b", Some("Chat a"))],
            focused: 1,
        };
        assert_eq!(parse(&to_json(&open)), Some(open));
    }

    #[test]
    fn skips_chats_without_id_or_folder() {
        let text = r#"{"focused": 0, "chats": [
            {"cwd": "/tmp", "title": "sem id"},
            {"session_id": "x", "title": "sem pasta"},
            {"session_id": "y", "cwd": "/tmp"}
        ]}"#;
        let open = parse(text).unwrap();
        assert_eq!(open.chats.len(), 1);
        assert_eq!(open.chats[0].session_id, "y");
        assert_eq!(open.chats[0].title, "Chat");
        assert_eq!(open.chats[0].model, None);
    }

    #[test]
    fn mixed_agents_and_legacy_state() {
        let old = parse(r#"{"chats":[{"session_id":"a","cwd":"/tmp"}]}"#).unwrap();
        assert_eq!(old.chats[0].agent, crate::agent::Agent::Claude);
        let mut codex = chat("b", None);
        codex.agent = crate::agent::Agent::Codex;
        codex.auto = true;
        codex.host = Some("vps-r1".into());
        codex.tmux = Some("chube-1".into());
        let mixed = OpenChats { chats: vec![old.chats[0].clone(), codex], focused: 1 };
        assert_eq!(parse(&to_json(&mixed)), Some(mixed));
        let unknown = parse(r#"{"chats":[{"agent":"outro","session_id":"a","cwd":"/tmp"}]}"#).unwrap();
        assert!(unknown.chats.is_empty());
    }

    #[test]
    fn invalid_json() {
        assert_eq!(parse("{"), None);
        assert_eq!(parse("[]"), None);
    }
}
