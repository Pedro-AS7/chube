mod agent;
mod cli;
mod codex;
mod app;
mod chat;
mod claude;
mod commands;
mod dirs;
mod omarchy;
mod remote;
mod restore;
mod sessions;
mod shortcuts;
mod term;
mod themes;
mod usage;

use cosmic::iced::{Limits, Size};

/// Variáveis que um Claude Code define para os processos filhos. Herdadas (Chube aberto de
/// dentro de uma sessão), fazem o `claude` dos chats se achar aninhado e não gravar a sessão.
const CLAUDE_SESSION_VARS: [&str; 10] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];

fn main() -> cosmic::iced::Result {
    for var in CLAUDE_SESSION_VARS {
        // SAFETY: nenhuma outra thread existe ainda.
        unsafe { std::env::remove_var(var) };
    }
    // TERM e COLORTERM=truecolor para os terminais, como no cosmic-term. Aberto pelo launcher,
    // o Chube não herda TERM, e sem ele o `claude` não manda cor nenhuma.
    alacritty_terminal::tty::setup_env();

    let settings = cosmic::app::Settings::default()
        .size(Size::new(1400.0, 860.0))
        .size_limits(Limits::NONE.min_width(900.0).min_height(560.0));

    if std::env::args_os().nth(1).is_some_and(|arg| arg == "--help" || arg == "-h") {
        println!("Uso: chube [--agent omarchy|claude|codex] [--model <modelo>] [--open-directory <pasta>] [-- <pastas>…]");
        return Ok(());
    }
    let flags = cli::parse(std::env::args_os().skip(1)).unwrap_or_else(|error| {
        eprintln!("C-hub: {error}");
        std::process::exit(2);
    });
    cosmic::app::run_single_instance::<app::App>(settings, flags)
}

