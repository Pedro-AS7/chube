use std::path::PathBuf;
use std::sync::Mutex;

use cosmic::widget::text_editor;

use crate::agent::Agent;
use crate::claude::{self, Mode, Start};
use crate::commands::Command;
use crate::restore::OpenChat;
use crate::sessions::SavedSession;
use crate::term::{self, Terminal};

/// `session_id` gravado de um chat que só entra num tmux aberto fora do Chube.
pub const TMUX_ID_PREFIX: &str = "tmux:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Restaurado da última execução: o `claude` só abre quando o chat for selecionado.
    Suspended,
    Starting,
    Working,
    /// O Claude Code espera uma resposta (permissão, pergunta).
    Waiting,
    Ready,
    Exited,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Suspended => "em espera",
            Status::Starting => "iniciando",
            Status::Working => "trabalhando",
            Status::Waiting => "precisa de você",
            Status::Ready => "pronto",
            Status::Exited => "encerrado",
        }
    }

    /// Status gravado pelo Claude Code em `~/.claude/sessions/<pid>.json`.
    pub fn from_claude(status: &str) -> Option<Self> {
        match status {
            "busy" => Some(Status::Working),
            "waiting" => Some(Status::Waiting),
            "idle" => Some(Status::Ready),
            _ => None,
        }
    }
}

/// Um chat é um `claude` interativo rodando no seu próprio terminal.
pub struct Chat {
    pub agent: Agent,
    /// Modo do `omarchy agent`: o agente aprova sozinho (`--permission-mode auto` no Claude,
    /// `--approve-for-me` no Codex).
    pub auto: bool,
    /// Host do SSH, num chat da VPS; `None` nesta máquina. Na VPS, `cwd` é a pasta de lá.
    pub host: Option<String>,
    /// Sessão do tmux da VPS onde o agente roda (`remote.rs`).
    pub tmux: Option<String>,
    /// Chat que só entra numa sessão do tmux aberta fora do Chube (pelo celular, por exemplo).
    pub attach_only: bool,
    pub codex_thread_hint: Option<String>,
    pub context_window: Option<u32>,
    pub codex_limits: Vec<crate::usage::Limit>,
    /// Identifica o chat e os eventos do terminal dele (mesma numeração das abas do terminal).
    pub id: term::TabId,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub start: Start,
    /// Mensagem passada ao `claude` ao abrir (`--prompt`).
    pub prompt: Option<String>,
    /// Id da sessão no Claude Code. Num sub-chat, só é conhecido depois que o processo sobe;
    /// muda também com `/clear`.
    pub session_id: Option<String>,
    pub title: String,
    pub parent: Option<String>,
    pub status: Status,
    /// Modelo usado de fato, lido do histórico da sessão.
    pub active_model: Option<String>,
    pub context_tokens: u32,
    /// `None` até o canal de eventos do terminal existir ou se o `claude` não abriu.
    pub terminal: Option<Mutex<Terminal>>,
    /// Texto do campo de mensagem abaixo do terminal.
    pub composer: text_editor::Content<cosmic::Renderer>,
    /// Modo de permissão lido na tela; `None` até aparecer no rodapé da TUI.
    pub mode: Option<Mode>,
    pub mode_change: Option<ModeChange>,
    /// Comandos `/…` sugeridos no campo; carregados na primeira `/`.
    pub commands: Option<Vec<Command>>,
}

/// Troca de modo em andamento: um Shift+Tab por vez até a tela mostrar o modo pedido.
#[derive(Clone, Copy, Debug)]
pub struct ModeChange {
    pub target: Mode,
    /// Modo antes da troca: voltar a ele quer dizer que o pedido não está no ciclo
    /// (ex.: "Sem permissões" sem `--dangerously-skip-permissions`).
    pub start: Option<Mode>,
    pub steps: u8,
}

impl Chat {
    pub fn new(id: term::TabId, agent: Agent, cwd: PathBuf, model: Option<String>, prompt: Option<String>) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        Self::with_start(
            id,
            agent,
            cwd,
            model,
            Start::New {
                session_id: session_id.clone(),
            },
            (agent == Agent::Claude).then_some(session_id),
            "Novo chat".into(),
        )
        .with_prompt(prompt)
    }

    /// Retoma uma sessão salva; o histórico aparece no próprio Claude Code.
    pub fn resume(id: term::TabId, saved: &SavedSession, model: Option<String>) -> Self {
        let mut chat = Self::with_start(
            id,
            saved.agent,
            saved.cwd.clone(),
            model,
            Start::Resume {
                session_id: saved.id.clone(),
            },
            Some(saved.id.clone()),
            saved.title.clone(),
        );
        chat.host = saved.host.clone();
        chat
    }

    /// Chat aberto na última execução, em espera até ser selecionado. Sem o `.jsonl` (sessão
    /// sem nenhuma mensagem, ou apagada pelo Claude Code), o `--resume` falharia: volta como
    /// chat novo na mesma pasta.
    pub fn restore(id: term::TabId, open: &OpenChat, saved: bool) -> Self {
        let attach_only = open.session_id.starts_with(TMUX_ID_PREFIX);
        let mut chat = if saved && !attach_only {
            let mut chat = Self::with_start(
                id,
                open.agent,
                open.cwd.clone(),
                open.model.clone(),
                Start::Resume {
                    session_id: open.session_id.clone(),
                },
                Some(open.session_id.clone()),
                open.title.clone(),
            );
            chat.parent = open.parent.clone();
            chat
        } else {
            Self::new(id, open.agent, open.cwd.clone(), open.model.clone(), None)
        };
        // O agente do Omarchy é deste notebook: um chat da VPS gravado com o modo dele volta sem.
        chat.auto = open.auto && open.host.is_none();
        chat.host = open.host.clone();
        chat.tmux = open.tmux.clone();
        chat.attach_only = attach_only;
        if attach_only {
            chat.session_id = None;
            chat.title = open.title.clone();
        }
        chat.status = Status::Suspended;
        chat
    }

    /// Entra numa sessão do tmux da VPS aberta fora do Chube.
    pub fn attach(id: term::TabId, host: String, tmux: String, cwd: PathBuf, title: String) -> Self {
        let mut chat = Self::new(id, Agent::Claude, cwd, None, None);
        chat.session_id = None;
        chat.title = title;
        chat.host = Some(host);
        chat.tmux = Some(tmux);
        chat.attach_only = true;
        chat
    }

    /// Com o modo do `omarchy agent` ou na VPS. Os dois juntos não existem: o agente do Omarchy
    /// é do sistema deste notebook, então na VPS o modo é ignorado.
    pub fn with_place(mut self, auto: bool, host: Option<String>) -> Self {
        self.auto = auto && host.is_none();
        self.host = host;
        self
    }

    /// Programa e argumentos do terminal do chat: o agente aqui ou, na VPS, o `ssh` que entra
    /// no tmux de lá. Na VPS, o prompt só vai na primeira vez (depois o tmux já existe).
    pub fn command(&mut self) -> (String, Vec<String>) {
        let prompt = self.prompt.take();
        let model = self.model.as_deref();
        let Some(host) = &self.host else {
            return self.agent.command(&self.start, model, prompt.as_deref(), self.auto);
        };
        let tmux = self.tmux.get_or_insert_with(crate::remote::new_tmux_name).clone();
        let agent = (!self.attach_only).then(|| {
            let args = self.agent.args(&self.start, model, prompt.as_deref(), self.auto);
            // Retomar uma sessão nova que nunca chegou a gravar o histórico falha: começa de novo
            // com o mesmo id.
            let fallback = match (&self.start, self.agent) {
                (Start::Resume { session_id }, Agent::Claude) => Some(claude::args(
                    &Start::New { session_id: session_id.clone() },
                    model,
                    None,
                    self.auto,
                )),
                _ => None,
            };
            crate::remote::agent_shell_command(self.agent, &args, fallback.as_deref())
        });
        crate::remote::command(host, &self.cwd.to_string_lossy(), &tmux, agent.as_deref())
    }

    /// Pasta do processo local do terminal: a do chat ou, na VPS, a home daqui.
    pub fn local_dir(&self) -> PathBuf {
        if self.host.is_some() { claude::home_dir() } else { self.cwd.clone() }
    }

    /// Pasta para mostrar: `vps-r1:~/projeto` na VPS.
    pub fn place_label(&self) -> String {
        match &self.host {
            Some(host) => format!("{host}:{}", self.cwd.display()),
            None => claude::display_path(&self.cwd),
        }
    }

    /// Nome do agente na lista: `omarchy · claude`, `claude`, `codex`.
    pub fn agent_label(&self) -> String {
        if self.attach_only {
            return "tmux".into();
        }
        let name = self.agent.key();
        if self.auto { format!("omarchy · {name}") } else { name.to_owned() }
    }

    /// Sub-chat: nova sessão que começa com todo o contexto desta.
    pub fn fork(&self, id: term::TabId) -> Option<Self> {
        let parent_session_id = self.session_id.clone()?;
        let mut chat = Self::with_start(
            id,
            self.agent,
            self.cwd.clone(),
            self.model.clone(),
            Start::Fork { parent_session_id },
            None,
            format!("{} (sub)", self.title),
        );
        chat.parent = Some(self.title.clone());
        chat.active_model = self.active_model.clone();
        chat.context_tokens = self.context_tokens;
        // Na VPS, o sub-chat ganha um tmux próprio.
        chat.auto = self.auto;
        chat.host = self.host.clone();
        Some(chat)
    }

    /// Abre de novo uma sessão encerrada, no mesmo lugar da lista.
    pub fn reopen(&self, id: term::TabId) -> Option<Self> {
        let Some(session_id) = self.session_id.clone() else {
            let mut chat = Self::new(id, self.agent, self.cwd.clone(), self.model.clone(), None);
            chat.auto = self.auto;
            chat.host = self.host.clone();
            // Na VPS, volta ao mesmo tmux (o `-A` entra nele se o agente ainda estiver lá).
            chat.tmux = self.tmux.clone();
            chat.attach_only = self.attach_only;
            if self.attach_only {
                chat.session_id = None;
                chat.title = self.title.clone();
            }
            return Some(chat);
        };
        let mut chat = Self::with_start(
            id,
            self.agent,
            self.cwd.clone(),
            self.model.clone(),
            Start::Resume {
                session_id: session_id.clone(),
            },
            Some(session_id),
            self.title.clone(),
        );
        chat.parent = self.parent.clone();
        chat.active_model = self.active_model.clone();
        chat.context_tokens = self.context_tokens;
        chat.auto = self.auto;
        chat.host = self.host.clone();
        chat.tmux = self.tmux.clone();
        Some(chat)
    }

    fn with_start(
        id: term::TabId,
        agent: Agent,
        cwd: PathBuf,
        model: Option<String>,
        start: Start,
        session_id: Option<String>,
        title: String,
    ) -> Self {
        Self {
            agent,
            auto: false,
            host: None,
            tmux: None,
            attach_only: false,
            codex_thread_hint: None,
            context_window: None,
            codex_limits: Vec::new(),
            id,
            cwd,
            model,
            start,
            prompt: None,
            session_id,
            title,
            parent: None,
            status: Status::Starting,
            active_model: None,
            context_tokens: 0,
            terminal: None,
            composer: text_editor::Content::new(),
            mode: None,
            mode_change: None,
            commands: None,
        }
    }

    fn with_prompt(mut self, prompt: Option<String>) -> Self {
        self.prompt = prompt;
        self
    }

    pub fn is_alive(&self) -> bool {
        self.status != Status::Exited
    }

    /// Manda bytes ao `claude`, como teclas digitadas no terminal.
    pub fn send(&self, bytes: impl Into<Vec<u8>>) {
        if let Some(Ok(terminal)) = self.terminal.as_ref().map(Mutex::lock) {
            terminal.input_scroll(bytes.into());
        }
    }

    /// Cola o texto no `claude` (bracketed paste: quebras de linha não enviam a mensagem).
    pub fn paste(&self, text: String) {
        if let Some(Ok(terminal)) = self.terminal.as_ref().map(Mutex::lock) {
            terminal.paste(text);
        }
    }

    /// Relê o modo no rodapé da TUI; mantém o anterior se o rodapé não está na tela.
    pub fn read_mode(&mut self) {
        if self.agent != Agent::Claude { return; }
        let Some(Ok(terminal)) = self.terminal.as_ref().map(Mutex::lock) else {
            return;
        };
        if let Some(mode) = Mode::from_screen(&term::screen_tail(&terminal, 4)) {
            self.mode = Some(mode);
        }
    }

    /// Pid do `claude` deste chat.
    pub fn pid(&self) -> Option<u32> {
        self.terminal.as_ref()?.lock().ok()?.pid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_session_keeps_its_agent_when_restored_forked_and_reopened() {
        let open = OpenChat {
            agent: Agent::Codex, session_id: "sessao-codex".into(), cwd: "/tmp".into(),
            title: "Teste".into(), model: Some("modelo-codex".into()), parent: None,
            auto: false, host: None, tmux: None,
        };
        let restored = Chat::restore(1, &open, true);
        assert_eq!(restored.agent, Agent::Codex);
        assert_eq!(restored.status, Status::Suspended);
        assert!(matches!(restored.start, Start::Resume { .. }));
        for chat in [restored.fork(2).unwrap(), restored.reopen(3).unwrap()] {
            assert_eq!(chat.agent, Agent::Codex);
            assert_eq!(chat.model.as_deref(), Some("modelo-codex"));
        }
        assert_eq!(restored.fork(2).unwrap().session_id, None);
    }

    #[test]
    fn omarchy_mode_never_runs_on_the_vps() {
        let new = Chat::new(1, Agent::Claude, "/tmp".into(), None, None).with_place(true, Some("vps-r1".into()));
        assert!(!new.auto);
        let local = Chat::new(2, Agent::Claude, "/tmp".into(), None, None).with_place(true, None);
        assert!(local.auto);
        let open = OpenChat {
            agent: Agent::Claude, session_id: "sessao".into(), cwd: "/tmp".into(),
            title: "Teste".into(), model: None, parent: None,
            auto: true, host: Some("vps-r1".into()), tmux: Some("chube-1".into()),
        };
        assert!(!Chat::restore(3, &open, true).auto);
    }

    #[test]
    fn missing_codex_history_starts_a_new_codex_chat() {
        let open = OpenChat {
            agent: Agent::Codex, session_id: "draft:1".into(), cwd: "/tmp".into(),
            title: "Novo chat".into(), model: None, parent: None,
            auto: false, host: None, tmux: None,
        };
        let chat = Chat::restore(1, &open, false);
        assert_eq!(chat.agent, Agent::Codex);
        assert_eq!(chat.session_id, None);
        assert!(matches!(chat.start, Start::New { .. }));
        assert!(chat.fork(2).is_none());
        assert_eq!(chat.reopen(3).unwrap().agent, Agent::Codex);
        let claude = Chat::new(2, Agent::Claude, "/tmp".into(), None, None);
        assert!(claude.session_id.is_some());
    }

    #[test]
    fn chat_da_vps_roda_no_tmux_e_volta_ao_mesmo() {
        let mut chat = Chat::new(1, Agent::Claude, "~/projeto".into(), None, Some("oi".into()))
            .with_place(false, Some("vps-r1".into()));
        let (program, args) = chat.command();
        assert_eq!(program, "ssh");
        let tmux = chat.tmux.clone().unwrap();
        let script = args.last().unwrap();
        assert!(script.contains(&format!("-s {tmux} ")));
        assert!(!script.contains("--permission-mode auto"));
        assert!(script.contains(" -- oi"));
        assert_eq!(chat.local_dir(), claude::home_dir());
        assert_eq!(chat.place_label(), "vps-r1:~/projeto");
        // Reabrir: mesmo tmux, retomando a sessão (ou começando de novo com o mesmo id).
        let mut reopened = chat.reopen(2).unwrap();
        assert_eq!(reopened.tmux.as_deref(), Some(tmux.as_str()));
        let script = reopened.command().1.pop().unwrap();
        assert!(script.contains("--resume") && script.contains("|| env"));
        assert!(!script.contains(" -- oi"));
        // Sub-chat: tmux próprio.
        let mut fork = chat.fork(3).unwrap();
        fork.command();
        assert_ne!(fork.tmux, chat.tmux);
        assert_eq!(fork.host.as_deref(), Some("vps-r1"));
    }

    #[test]
    fn tmux_de_fora_volta_so_entrando() {
        let open = OpenChat {
            agent: Agent::Claude, session_id: format!("{TMUX_ID_PREFIX}celular"), cwd: "~".into(),
            title: "celular".into(), model: None, parent: None,
            auto: false, host: Some("vps-r1".into()), tmux: Some("celular".into()),
        };
        let mut chat = Chat::restore(1, &open, true);
        assert!(chat.attach_only);
        assert_eq!(chat.session_id, None);
        let script = chat.command().1.pop().unwrap();
        assert!(script.starts_with("exec tmux attach-session -t celular"));
    }
}
