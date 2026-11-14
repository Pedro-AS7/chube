//! Sessões do Claude Code rodando no modo interativo, cada uma no terminal do seu chat.
//!
//! O app só monta o comando; a conversa, as permissões e o resto da interface
//! são do próprio Claude Code, dentro do PTY.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    /// Sessão nova, com o id escolhido pelo app.
    New { session_id: String },
    /// Continua uma sessão salva (`--resume <id>`).
    Resume { session_id: String },
    /// Cópia de uma sessão existente (`--resume <id> --fork-session`), com um id novo.
    Fork { parent_session_id: String },
}

/// Programa e argumentos para rodar o `claude` no terminal do chat.
///
/// Passa pelo `env -u ANTHROPIC_API_KEY`: com a API key definida, o Claude Code cobraria
/// pela API em vez de usar o plano. O `env` faz `exec`, então o pid do PTY é o do `claude`.
pub fn command(start: &Start, model: Option<&str>, prompt: Option<&str>, auto: bool) -> (String, Vec<String>) {
    let mut command: Vec<String> = vec![
        "-u".into(),
        "ANTHROPIC_API_KEY".into(),
        claude_bin().display().to_string(),
    ];
    command.extend(args(start, model, prompt, auto));
    ("env".into(), command)
}

/// Argumentos do `claude`, sem o binário (os mesmos aqui e na VPS). `auto` é o modo do
/// `omarchy agent`: `--permission-mode auto`.
pub fn args(start: &Start, model: Option<&str>, prompt: Option<&str>, auto: bool) -> Vec<String> {
    let mut args = Vec::new();
    match start {
        Start::New { session_id } => args.extend(["--session-id".into(), session_id.clone()]),
        Start::Resume { session_id } => args.extend(["--resume".into(), session_id.clone()]),
        Start::Fork { parent_session_id } => args.extend([
            "--resume".into(),
            parent_session_id.clone(),
            "--fork-session".into(),
        ]),
    }
    if auto {
        args.extend(["--permission-mode".into(), "auto".into()]);
    }
    if let Some(model) = model {
        args.extend(["--model".into(), model.to_owned()]);
    }
    if let Some(prompt) = prompt {
        // `--` para um prompt que comece com `-` não virar opção.
        args.extend(["--".into(), prompt.to_owned()]);
    }
    args
}

/// Procura o `claude` no PATH e, se não achar (apps abertos pelo launcher podem não ter
/// `~/.local/bin` no PATH), usa `~/.local/bin/claude`.
pub fn claude_bin() -> PathBuf {
    let in_path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("claude"))
            .find(|candidate| candidate.is_file())
    });
    in_path.unwrap_or_else(|| home_dir().join(".local/bin/claude"))
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// Mostra o caminho com `~` no lugar da home.
pub fn display_path(path: &Path) -> String {
    match path.strip_prefix(home_dir()) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Título do terminal definido pelo Claude Code (`✳ Assunto`, com um spinner no lugar
/// do `✳` enquanto trabalha), sem o símbolo. `None` enquanto é só o nome do programa.
pub fn title_from_terminal(title: &str) -> Option<String> {
    let title = title
        .trim_start_matches(|c: char| !c.is_alphanumeric() && !c.is_ascii_punctuation())
        .trim();
    (!title.is_empty() && title != "Claude Code").then(|| title.to_owned())
}

/// Status pelo título do terminal, para as sessões da VPS (onde não há o
/// `~/.claude/sessions/<pid>.json` para ler): `✳` parado, outro símbolo (o spinner) trabalhando.
pub fn status_from_title(title: &str) -> Option<crate::chat::Status> {
    let first = title.trim_start().chars().next()?;
    if first == '✳' {
        Some(crate::chat::Status::Ready)
    } else if !first.is_alphanumeric() && !first.is_ascii() {
        Some(crate::chat::Status::Working)
    } else {
        None
    }
}

/// Modo de permissão do Claude Code, trocado com Shift+Tab na TUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Manual,
    AcceptEdits,
    Plan,
    Auto,
    Bypass,
}

/// Shift+Tab: passa para o próximo modo.
pub const NEXT_MODE_KEY: &[u8] = b"\x1b[Z";

impl Mode {
    pub const ALL: [Mode; 5] = [Mode::Manual, Mode::AcceptEdits, Mode::Plan, Mode::Auto, Mode::Bypass];
    /// Nomes no seletor, na ordem de `ALL`.
    pub const LABELS: [&'static str; 5] = [
        "Manual",
        "Aceitar edições",
        "Planejamento",
        "Automático",
        "Sem permissões",
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)
    }

    /// Lê o modo no rodapé da TUI (`⏵⏵ auto mode on (shift+tab to cycle)`, `⏸ manual mode on`…).
    /// `None` quando o rodapé não aparece, por exemplo com um pedido de permissão na tela.
    pub fn from_screen(lines: &[String]) -> Option<Self> {
        const MARKS: [(&str, Mode); 5] = [
            ("manual mode on", Mode::Manual),
            ("accept edits on", Mode::AcceptEdits),
            ("plan mode on", Mode::Plan),
            ("auto mode on", Mode::Auto),
            ("bypass permissions on", Mode::Bypass),
        ];
        lines.iter().rev().find_map(|line| {
            MARKS
                .iter()
                .find(|(mark, _)| line.contains(mark))
                .map(|(_, mode)| *mode)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_title() {
        assert_eq!(title_from_terminal("✳ Claude Code"), None);
        assert_eq!(title_from_terminal(""), None);
        assert_eq!(
            title_from_terminal("✳ Corrigir o parser"),
            Some("Corrigir o parser".into())
        );
        assert_eq!(title_from_terminal("⠂ Corrigir o parser"), Some("Corrigir o parser".into()));
        assert_eq!(title_from_terminal("\"cargo\" falhou"), Some("\"cargo\" falhou".into()));
    }

    #[test]
    fn mode_from_screen() {
        let screen = |last: &str| vec!["❯ oi".to_owned(), "────".to_owned(), last.to_owned()];
        assert_eq!(
            Mode::from_screen(&screen("  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents")),
            Some(Mode::Auto)
        );
        assert_eq!(
            Mode::from_screen(&screen("  ⏸ manual mode on · ? for shortcuts")),
            Some(Mode::Manual)
        );
        assert_eq!(Mode::from_screen(&screen("  ⏸ plan mode on (shift+tab to cycle)")), Some(Mode::Plan));
        assert_eq!(Mode::from_screen(&screen("  ⏵⏵ accept edits on")), Some(Mode::AcceptEdits));
        assert_eq!(Mode::from_screen(&screen("  Do you want to proceed?")), None);
    }

    #[test]
    fn command_args() {
        let start = Start::Fork {
            parent_session_id: "abc".into(),
        };
        let (program, args) = command(&start, Some("haiku"), Some("-oi"), false);
        assert_eq!(program, "env");
        assert_eq!(&args[..2], ["-u", "ANTHROPIC_API_KEY"]);
        assert_eq!(
            &args[3..],
            ["--resume", "abc", "--fork-session", "--model", "haiku", "--", "-oi"]
        );
        let auto = self::args(&Start::New { session_id: "x".into() }, None, None, true);
        assert_eq!(auto, ["--session-id", "x", "--permission-mode", "auto"]);
    }

    #[test]
    fn status_pelo_titulo() {
        use crate::chat::Status;
        assert_eq!(status_from_title("✳ Corrigir testes"), Some(Status::Ready));
        assert_eq!(status_from_title("⠂ Corrigir testes"), Some(Status::Working));
        assert_eq!(status_from_title("bash"), None);
    }
}
