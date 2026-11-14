mod agent;
mod cli;
mod codex;
mod app;
mod chat;
mod claude;
mod dirs;
mod groups;
mod omarchy;
mod panels;
mod remote;
mod restore;
mod sessions;
mod shortcuts;
mod task;
mod terminal;
mod themes;
mod ui;
mod usage;

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

fn main() -> gtk4::glib::ExitCode {
    for var in CLAUDE_SESSION_VARS {
        // SAFETY: nenhuma outra thread existe ainda.
        unsafe { std::env::remove_var(var) };
    }
    if std::env::args_os().nth(1).is_some_and(|arg| arg == "--help" || arg == "-h") {
        println!("Uso: chube [--agent omarchy|claude|codex] [--model <modelo>] [--open-directory <pasta>] [-- <pastas>…]");
        return gtk4::glib::ExitCode::SUCCESS;
    }
    // A linha de comando é lida em `app::run` (no app já aberto, se houver um).
    app::run()
}
