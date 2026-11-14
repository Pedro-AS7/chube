//! Agente de cada chat; arquivos antigos continuam sendo do Claude Code.
use crate::claude::{self, Start};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Agent {
    #[default]
    Claude,
    Codex,
}

impl Agent {
    pub fn key(self) -> &'static str {
        match self { Self::Claude => "claude", Self::Codex => "codex" }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value { "claude" => Some(Self::Claude), "codex" => Some(Self::Codex), _ => None }
    }

    /// Comando para rodar o agente nesta máquina.
    pub fn command(self, start: &Start, model: Option<&str>, prompt: Option<&str>, auto: bool) -> (String, Vec<String>) {
        match self {
            Self::Claude => claude::command(start, model, prompt, auto),
            Self::Codex => crate::codex::command(start, model, prompt, auto),
        }
    }

    /// Só os argumentos, para rodar o agente na VPS.
    pub fn args(self, start: &Start, model: Option<&str>, prompt: Option<&str>, auto: bool) -> Vec<String> {
        match self {
            Self::Claude => claude::args(start, model, prompt, auto),
            Self::Codex => crate::codex::args(start, model, prompt, auto),
        }
    }

    pub fn session_file(self, id: &str) -> Option<std::path::PathBuf> {
        match self {
            Self::Claude => crate::sessions::session_file(id),
            Self::Codex => crate::codex::session_file(id),
        }
    }
}
