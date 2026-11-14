mod agent;
mod codex;
mod app;
mod chat;
mod claude;
mod commands;
mod dirs;
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

    cosmic::app::run::<app::App>(settings, parse_args())
}

/// `chube [--model <modelo>]`
fn parse_args() -> app::Flags {
    let mut flags = app::Flags::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => flags.model = args.next(),
            "--agent" => {
                flags.agent = args.next().as_deref().and_then(agent::Agent::parse).unwrap_or_default();
            }
            _ => eprintln!("argumento ignorado: {arg}"),
        }
    }
    flags
}
