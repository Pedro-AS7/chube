//! Estado e lógica do app. A interface (`ui.rs`) só lê este estado e manda `Message`s; cada
//! mensagem passa por `App::update`, que devolve uma `Task` (o que fazer fora da thread do GTK).
//!
//! Migração do libcosmic para o GTK4 em andamento (branch `gtk`): o `app.rs` antigo, com o painel
//! de nova sessão, as sessões salvas, a VPS e o uso, está na branch `omarchy`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use async_channel::Sender;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use crate::agent::Agent;
use crate::chat::{Chat, Status, TMUX_ID_PREFIX, TermId};
use crate::claude::{self, Start};
use crate::cli::{self, Flags};
use crate::codex;
use crate::dirs;
use crate::groups::{self, Category, CategoryGroup};
use crate::omarchy;
use crate::remote::{self, Listing};
use crate::restore::{self, OpenChat, OpenChats};
use crate::sessions::{self, Running, SavedSession, Stats};
use crate::shortcuts::Shortcut;
use crate::task::{self, Task};
use crate::terminal;
use crate::themes;
use crate::ui::Ui;

pub const APP_ID: &str = "com.pedro.Chube";
/// Intervalo da conferência do tema do Omarchy (o Chube muda junto com `omarchy theme set`).
const THEME_CHECK_EVERY: Duration = Duration::from_secs(2);
/// Intervalo da leitura do status das sessões (`~/.claude/sessions`).
const POLL_EVERY: Duration = Duration::from_secs(1);
const TITLE_MAX_CHARS: usize = 40;

/// O que abrir num chat novo: o agente padrão do Omarchy no modo do `omarchy agent`, ou um
/// agente escolhido, no modo dele.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    Omarchy,
    Claude,
    Codex,
}

/// Painel mostrado no lugar do terminal do chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    None,
    NewChat,
    Sessions,
}

/// Sessões de uma VPS, buscadas por SSH.
#[derive(Clone, Debug)]
pub enum RemoteState {
    Loading,
    Loaded(Listing),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Sidebar,
    Chat,
    Terminal,
}

#[derive(Debug)]
pub enum Message {
    SelectChat(usize),
    CloseChat(usize),
    ReopenChat,
    /// Abre o painel de nova sessão.
    NewChat,
    NewChatInput(String),
    NewChatLaunch(Launch),
    /// Onde rodar: esta máquina (`None`) ou um host do SSH.
    NewChatHost(Option<String>),
    NewChatConfirm,
    /// Pasta escolhida no seletor de pastas (o diálogo é da interface).
    NewChatPicked(Option<PathBuf>),
    ClosePanel,
    ShowSessions,
    /// Sessões de qual lugar mostrar: esta máquina (`None`) ou um host do SSH.
    SessionsHost(Option<String>),
    SessionsLoaded(Vec<SavedSession>),
    /// Recarrega as sessões do lugar mostrado.
    ReloadSessions,
    RemoteListed(String, Result<Listing, String>),
    ToggleDirGroup(PathBuf),
    ResumeSession(usize),
    /// Retoma uma sessão salva da VPS (host, índice na lista dela).
    ResumeRemote(String, usize),
    /// Entra numa sessão do tmux aberta na VPS (host, índice na lista dela).
    AttachTmux(String, usize),
    /// Sub-chat: nova sessão com o contexto do chat em foco.
    SubChat,
    OpenDirectories(Result<Flags, String>),
    Poll,
    Polled(Vec<Polled>),
    /// Título que o programa do terminal pôs (OSC 0/2).
    TermTitle(TermId, String),
    /// O processo do terminal saiu (ou não abriu).
    TermExit(TermId),
    NewTerminalTab,
    TerminalTabActivated(TermId),
    TerminalTabClosed(TermId),
    Shortcut(Shortcut),
    HideSection(Section),
    ShowSection(Section),
    /// Confere se o tema do Omarchy mudou.
    CheckTheme,
    ThemeChecked(String),
    /// Uma tarefa em segundo plano entrou em pânico.
    TaskFailed(String),
}

/// Estado de um chat lido do disco pelo `Message::Poll`.
#[derive(Debug)]
pub struct Polled {
    chat: TermId,
    running: Option<Running>,
    /// Só do chat em foco (é o único mostrado no cabeçalho).
    stats: Option<Stats>,
    codex: Option<codex::Snapshot>,
    codex_hint: Option<String>,
}

/// Aba do terminal integrado: um shell.
pub struct Tab {
    pub id: TermId,
    pub terminal: vte4::Terminal,
    pub pid: Rc<Cell<Option<u32>>>,
    /// Pasta atual do shell (`/proc/<pid>/cwd`), que dá o nome da aba.
    pub dir: PathBuf,
}

/// O que a interface deve focar depois desta mensagem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Chat,
    Terminal,
    /// O campo de pasta do painel de nova sessão.
    NewChatInput,
}

pub struct App {
    pub chats: Vec<Chat>,
    pub focused: usize,
    default_dir: PathBuf,
    model: Option<String>,
    default_agent: Agent,
    /// `--agent omarchy` na linha de comando.
    default_auto: bool,
    /// Agente padrão do Omarchy (`omarchy default agent`), se for o Claude Code ou o Codex.
    pub omarchy_agent: Option<Agent>,
    /// Hosts do `~/.ssh/config`, oferecidos como lugar dos chats.
    pub hosts: Vec<String>,
    pub panel: Panel,
    pub new_chat_launch: Launch,
    pub new_chat_host: Option<String>,
    /// Pasta digitada no painel de nova sessão.
    pub new_chat_input: String,
    /// O texto do campo mudou por fora (painel aberto, pasta escolhida): a interface o copia.
    pub new_chat_input_reset: bool,
    /// Sessões salvas desta máquina; `None` enquanto carrega.
    pub saved_sessions: Option<Vec<SavedSession>>,
    /// Lugar mostrado no painel de sessões: esta máquina (`None`) ou um host do SSH.
    pub sessions_host: Option<String>,
    pub remote_sessions: HashMap<String, RemoteState>,
    /// Pastas abertas na lista de sessões salvas.
    pub expanded_dirs: HashSet<PathBuf>,
    /// Muda a cada mudança no painel de sessões: a interface só o refaz quando ela muda.
    pub sessions_version: u64,
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TermId>,
    /// Numeração única dos terminais: abas e chats.
    next_terminal_id: TermId,
    polling: bool,
    pub sidebar_visible: bool,
    pub chat_visible: bool,
    pub terminal_visible: bool,
    /// Marca do tema do Omarchy aplicado, para saber quando ele muda.
    theme_fingerprint: String,
    checking_theme: bool,
    /// Tema relido: a interface refaz o CSS.
    pub theme_changed: bool,
    /// Aviso mostrado abaixo do cabeçalho do chat.
    pub notice: Option<String>,
    /// Última lista de chats gravada (`restore`), para só gravar quando mudar.
    open_chats: OpenChats,
    pub focus: Option<Focus>,
    tx: Sender<Message>,
}

impl App {
    fn new(flags: &Flags, tx: Sender<Message>) -> Self {
        // Aberto pelo launcher, o diretório atual é `/`: nesse caso, usa a home.
        let default_dir = std::env::current_dir()
            .ok()
            .filter(|dir| dir.parent().is_some())
            .unwrap_or_else(claude::home_dir);
        themes::reload();
        let mut app = App {
            chats: Vec::new(),
            focused: 0,
            default_dir,
            model: flags.model.clone(),
            default_agent: flags.agent,
            default_auto: flags.auto,
            omarchy_agent: omarchy::default_agent().and_then(|name| Agent::parse(&name)),
            hosts: remote::hosts(),
            panel: Panel::None,
            new_chat_launch: Launch::Claude,
            new_chat_host: None,
            new_chat_input: String::new(),
            new_chat_input_reset: false,
            saved_sessions: None,
            sessions_host: None,
            remote_sessions: HashMap::new(),
            expanded_dirs: HashSet::new(),
            sessions_version: 0,
            tabs: Vec::new(),
            active_tab: None,
            next_terminal_id: 0,
            polling: false,
            sidebar_visible: true,
            chat_visible: true,
            terminal_visible: true,
            theme_fingerprint: omarchy::theme_fingerprint(),
            checking_theme: false,
            theme_changed: false,
            notice: None,
            open_chats: OpenChats::default(),
            focus: Some(Focus::Chat),
            tx,
        };
        // Volta a lista de chats da última execução, em espera; só o do foco abre agora.
        let open = restore::load();
        for saved in &open.chats {
            let id = app.take_terminal_id();
            // Na VPS não dá para conferir o histórico daqui: o comando de lá começa de novo se
            // não houver o que retomar.
            let has_file = if saved.host.is_some() {
                !saved.session_id.starts_with("draft:")
            } else {
                saved.agent.session_file(&saved.session_id).is_some()
            };
            app.chats.push(Chat::restore(id, saved, has_file));
        }
        app.focused = open.focused.min(app.chats.len().saturating_sub(1));
        app.open_chats = open;
        app.add_terminal_tab();
        app
    }

    /// Trata a mensagem; depois, abre o chat em foco se ele estava em espera e grava a lista.
    pub fn update(&mut self, message: Message) -> Task {
        let task = self.handle(message);
        self.wake_focused();
        self.save_open_chats();
        task
    }

    fn handle(&mut self, message: Message) -> Task {
        match message {
            Message::SelectChat(index) => {
                self.notice = None;
                self.panel = Panel::None;
                if index < self.chats.len() {
                    self.focused = index;
                    self.focus = Some(Focus::Chat);
                }
            }
            Message::CloseChat(index) => self.close_chat(index),
            Message::ReopenChat => {
                let id = self.take_terminal_id();
                if let Some(chat) = self.chats.get(self.focused).and_then(|chat| chat.reopen(id)) {
                    let old = self.chats.remove(self.focused);
                    hang_up(&old);
                    self.open_chat(self.focused, chat);
                }
            }
            Message::NewChat => {
                // Herda o agente e o lugar do chat em foco; sem chat, o padrão da linha de comando.
                let (launch, host) = match self.chats.get(self.focused) {
                    Some(chat) if chat.auto => (Launch::Omarchy, None),
                    Some(chat) => (launch_of(chat.agent), chat.host.clone()),
                    None if self.default_auto => (Launch::Omarchy, None),
                    // Sem chat, o agente do Omarchy, se ele for um dos que o Chube abre.
                    None if self.omarchy_agent.is_some() => (Launch::Omarchy, None),
                    None => (launch_of(self.default_agent), None),
                };
                self.new_chat_launch = launch;
                self.new_chat_host = host;
                self.new_chat_input.clear();
                self.new_chat_input_reset = true;
                self.panel = Panel::NewChat;
                self.chat_visible = true;
                self.focus = Some(Focus::NewChatInput);
            }
            Message::NewChatInput(text) => self.new_chat_input = text,
            Message::NewChatLaunch(launch) => {
                // O agente do Omarchy só roda neste notebook.
                if launch == Launch::Omarchy {
                    self.new_chat_host = None;
                }
                self.new_chat_launch = launch;
                self.focus = Some(Focus::NewChatInput);
            }
            Message::NewChatHost(host) => {
                // Na VPS não há o agente do Omarchy: troca pelo agente que ele usaria.
                if host.is_some() && self.new_chat_launch == Launch::Omarchy {
                    self.new_chat_launch = launch_of(self.omarchy_agent.unwrap_or(self.default_agent));
                }
                self.new_chat_host = host;
                self.focus = Some(Focus::NewChatInput);
            }
            Message::NewChatConfirm => {
                let Some((agent, auto)) = self.launch_agent(self.new_chat_launch) else {
                    return Task::none();
                };
                let folder = match &self.new_chat_host {
                    // Na VPS, a pasta vai como foi digitada (o `~` é a home de lá).
                    Some(_) => PathBuf::from(remote_dir(&self.new_chat_input)),
                    None => match self.new_chat_dir() {
                        Ok(folder) => folder,
                        Err(_) => return Task::none(),
                    },
                };
                self.panel = Panel::None;
                let id = self.take_terminal_id();
                let chat = Chat::new(id, agent, folder, self.new_model(agent), None)
                    .with_place(auto, self.new_chat_host.clone());
                self.open_chat(self.chats.len(), chat);
            }
            // A pasta escolhida vai para o campo, para ser conferida antes de criar.
            Message::NewChatPicked(Some(folder)) => {
                self.new_chat_input = folder.display().to_string();
                self.new_chat_input_reset = true;
                self.focus = Some(Focus::NewChatInput);
            }
            Message::NewChatPicked(None) => {}
            Message::ClosePanel => {
                self.panel = Panel::None;
                self.focus = Some(Focus::Chat);
            }
            Message::ShowSessions => {
                self.panel = Panel::Sessions;
                self.chat_visible = true;
                self.sessions_host = self.chats.get(self.focused).and_then(|chat| chat.host.clone());
                self.expanded_dirs = HashSet::from([self.focused_local_dir()]);
                return self.load_sessions();
            }
            Message::SessionsHost(host) => {
                self.sessions_host = host;
                return self.load_sessions();
            }
            Message::ReloadSessions => return self.load_sessions(),
            Message::SessionsLoaded(list) => {
                self.saved_sessions = Some(list);
                self.sessions_version += 1;
            }
            Message::RemoteListed(host, result) => {
                let state = match result {
                    Ok(listing) => RemoteState::Loaded(listing),
                    Err(error) => RemoteState::Failed(error),
                };
                self.remote_sessions.insert(host, state);
                self.sessions_version += 1;
            }
            Message::ToggleDirGroup(dir) => {
                if !self.expanded_dirs.remove(&dir) {
                    self.expanded_dirs.insert(dir);
                }
                self.sessions_version += 1;
            }
            Message::ResumeSession(index) => {
                let saved = self.saved_sessions.as_ref().and_then(|list| list.get(index)).cloned();
                if let Some(saved) = saved {
                    self.resume_saved(saved);
                }
            }
            Message::ResumeRemote(host, index) => {
                let saved = match self.remote_sessions.get(&host) {
                    Some(RemoteState::Loaded(listing)) => listing.sessions.get(index).cloned(),
                    _ => None,
                };
                if let Some(saved) = saved {
                    self.resume_saved(saved);
                }
            }
            Message::AttachTmux(host, index) => {
                let tmux = match self.remote_sessions.get(&host) {
                    Some(RemoteState::Loaded(listing)) => listing.tmux.get(index).cloned(),
                    _ => None,
                };
                let Some(tmux) = tmux else {
                    return Task::none();
                };
                self.panel = Panel::None;
                if let Some(open) = self.chats.iter().position(|chat| {
                    chat.host.as_deref() == Some(host.as_str()) && chat.tmux.as_deref() == Some(tmux.name.as_str())
                }) {
                    self.focused = open;
                    self.focus = Some(Focus::Chat);
                    // Encerrado (rede caída, por exemplo), volta a entrar no mesmo tmux.
                    if !self.chats[open].is_alive() {
                        return self.handle(Message::ReopenChat);
                    }
                    return Task::none();
                }
                let title = claude::title_from_terminal(&tmux.title).unwrap_or_else(|| tmux.name.clone());
                let id = self.take_terminal_id();
                let chat = Chat::attach(id, host, tmux.name, PathBuf::from(tmux.path), short_title(&title));
                self.open_chat(self.chats.len(), chat);
            }
            Message::SubChat => {
                let id = self.take_terminal_id();
                if let Some(chat) = self.chats.get(self.focused).and_then(|chat| chat.fork(id)) {
                    self.open_chat(self.focused + 1, chat);
                }
            }
            Message::OpenDirectories(Ok(flags)) => self.open_directories(flags),
            Message::OpenDirectories(Err(error)) => self.notice = Some(error),
            Message::Poll => {
                self.refresh_tab_dirs();
                return self.poll();
            }
            Message::Polled(list) => {
                self.polling = false;
                self.apply_poll(list);
            }
            Message::TermTitle(id, title) => self.term_title(id, &title),
            Message::TermExit(id) => {
                if let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == id) {
                    // O agente saiu: o terminal fica na tela com a última saída.
                    chat.status = Status::Exited;
                } else {
                    // O shell de uma aba saiu (`exit`): fecha a aba.
                    self.close_tab(id);
                }
            }
            Message::NewTerminalTab => {
                self.add_terminal_tab();
                self.terminal_visible = true;
                self.focus = Some(Focus::Terminal);
            }
            Message::TerminalTabActivated(id) => {
                self.active_tab = Some(id);
                self.focus = Some(Focus::Terminal);
            }
            Message::TerminalTabClosed(id) => self.close_tab(id),
            Message::Shortcut(shortcut) => self.handle_shortcut(shortcut),
            Message::HideSection(section) => {
                // Chat e terminal não somem os dois ao mesmo tempo.
                match section {
                    Section::Sidebar => self.sidebar_visible = false,
                    Section::Chat if self.terminal_visible => self.chat_visible = false,
                    Section::Terminal if self.chat_visible => self.terminal_visible = false,
                    _ => {}
                }
            }
            Message::ShowSection(section) => match section {
                Section::Sidebar => self.sidebar_visible = true,
                Section::Chat => self.chat_visible = true,
                Section::Terminal => self.terminal_visible = true,
            },
            Message::CheckTheme => {
                if !self.checking_theme {
                    self.checking_theme = true;
                    return Task::blocking(|| Message::ThemeChecked(omarchy::theme_fingerprint()));
                }
            }
            Message::ThemeChecked(fingerprint) => {
                self.checking_theme = false;
                if fingerprint != self.theme_fingerprint {
                    self.theme_fingerprint = fingerprint;
                    self.apply_omarchy_theme();
                }
            }
            Message::TaskFailed(error) => log::error!("tarefa falhou: {error}"),
        }
        Task::none()
    }

    fn handle_shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::NewChat => {
                let _ = self.handle(Message::NewChat);
            }
            Shortcut::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            Shortcut::ToggleChat => {
                if self.chat_visible && self.terminal_visible {
                    self.chat_visible = false;
                    self.focus = Some(Focus::Terminal);
                } else {
                    self.chat_visible = true;
                    self.focus = Some(Focus::Chat);
                }
            }
            Shortcut::ToggleTerminal => {
                if self.terminal_visible && self.chat_visible {
                    self.terminal_visible = false;
                    self.focus = Some(Focus::Chat);
                } else {
                    self.terminal_visible = true;
                    self.focus = Some(Focus::Terminal);
                }
            }
            Shortcut::PreviousChat | Shortcut::NextChat if !self.chats.is_empty() => {
                // Dá a volta nas pontas e segue a ordem da lista agrupada, não a de abertura.
                let order = groups::display_order(&self.chat_groups());
                let len = order.len();
                let position = order.iter().position(|&index| index == self.focused).unwrap_or(0);
                let index = match shortcut {
                    Shortcut::PreviousChat => order[(position + len - 1) % len],
                    _ => order[(position + 1) % len],
                };
                let _ = self.handle(Message::SelectChat(index));
            }
            Shortcut::PreviousChat | Shortcut::NextChat => {}
            // A interface sabe qual terminal tem o foco; ela troca sozinha (`Ui::switch_focus`).
            Shortcut::SwitchFocus => {}
        }
    }

    pub fn chat_groups(&self) -> Vec<CategoryGroup> {
        let keys: Vec<_> = self
            .chats
            .iter()
            .map(|chat| (Category::of(chat.auto, chat.host.is_some()), chat.place_label()))
            .collect();
        groups::group(&keys)
    }

    fn launch_agent(&self, launch: Launch) -> Option<(Agent, bool)> {
        match launch {
            Launch::Omarchy => self.omarchy_agent.map(|agent| (agent, true)),
            Launch::Claude => Some((Agent::Claude, false)),
            Launch::Codex => Some((Agent::Codex, false)),
        }
    }

    /// Pasta do chat novo nesta máquina. Vazio é a pasta do chat em foco ou, no modo do Omarchy,
    /// a mesma do `omarchy agent` (`~/Work`).
    pub fn new_chat_dir(&self) -> Result<PathBuf, String> {
        if self.new_chat_input.trim().is_empty() {
            return Ok(if self.new_chat_launch == Launch::Omarchy {
                omarchy::work_dir()
            } else {
                self.focused_local_dir()
            });
        }
        dirs::resolve(&self.new_chat_input, &self.focused_local_dir())
    }

    /// O que o painel de nova sessão vai abrir: o caminho (ou o erro) e se o agente pode abrir.
    pub fn new_chat_preview(&self) -> Result<String, String> {
        if self.launch_agent(self.new_chat_launch).is_none() {
            return Err("o agente padrão do Omarchy não é o Claude Code nem o Codex".into());
        }
        match &self.new_chat_host {
            Some(host) => Ok(format!("{host}:{}", remote_dir(&self.new_chat_input))),
            None => self.new_chat_dir().map(|path| claude::display_path(&path)),
        }
    }

    /// Pasta do chat em foco, se ele é desta máquina; senão a padrão.
    fn focused_local_dir(&self) -> PathBuf {
        match self.chats.get(self.focused) {
            Some(chat) if chat.host.is_none() => chat.cwd.clone(),
            _ => self.default_dir.clone(),
        }
    }

    /// Carrega as sessões do lugar escolhido no painel (desta máquina ou da VPS).
    fn load_sessions(&mut self) -> Task {
        self.sessions_version += 1;
        match self.sessions_host.clone() {
            None => {
                self.saved_sessions = None;
                Task::blocking(|| {
                    let mut list = sessions::list();
                    list.extend(codex::list());
                    list.sort_by(|a, b| b.modified.cmp(&a.modified));
                    Message::SessionsLoaded(list)
                })
            }
            Some(host) => {
                self.remote_sessions.insert(host.clone(), RemoteState::Loading);
                Task::future(async move {
                    let result = remote::list(host.clone()).await;
                    Message::RemoteListed(host, result)
                })
            }
        }
    }

    /// Abre uma sessão salva (daqui ou da VPS), ou foca o chat que já a tem aberta.
    fn resume_saved(&mut self, saved: SavedSession) {
        self.panel = Panel::None;
        self.focus = Some(Focus::Chat);
        if let Some(open) = self.chat_index_for_session(saved.agent, &saved.id) {
            self.focused = open;
            return;
        }
        let id = self.take_terminal_id();
        let chat = Chat::resume(id, &saved, self.new_model(saved.agent));
        self.open_chat(self.chats.len(), chat);
    }

    pub fn chat_index_for_session(&self, agent: Agent, session_id: &str) -> Option<usize> {
        self.chats
            .iter()
            .position(|chat| chat.agent == agent && chat.is_alive() && chat.session_id.as_deref() == Some(session_id))
    }

    // --model se aplica apenas ao agente escolhido na linha de comando.
    fn new_model(&self, agent: Agent) -> Option<String> {
        (agent == self.default_agent).then(|| self.model.clone()).flatten()
    }

    /// Uma chamada externa cria chats novos; não retoma outra sessão da mesma pasta.
    fn open_directories(&mut self, flags: Flags) {
        if flags.directories.is_empty() {
            return;
        }
        self.notice = None;
        self.chat_visible = true;
        for directory in flags.directories {
            let id = self.take_terminal_id();
            let chat = Chat::new(id, flags.agent, PathBuf::from(directory), flags.model.clone(), None)
                .with_place(flags.auto, None);
            self.open_chat(self.chats.len(), chat);
        }
    }

    /// Põe o chat na lista, em `index`, abre o agente dele e o deixa em foco.
    fn open_chat(&mut self, index: usize, chat: Chat) {
        self.panel = Panel::None;
        self.chats.insert(index, chat);
        self.focused = index;
        self.focus = Some(Focus::Chat);
        self.spawn_chat_terminal(index);
    }

    /// Abre o terminal do chat rodando o agente.
    fn spawn_chat_terminal(&mut self, index: usize) {
        let tx = self.tx.clone();
        let chat = &mut self.chats[index];
        if chat.terminal.is_some() || !chat.is_alive() || chat.status == Status::Suspended {
            return;
        }
        let (program, args) = chat.command();
        let argv: Vec<String> = std::iter::once(program).chain(args).collect();
        let terminal = terminal::new(chat.id, &tx);
        terminal::spawn(&terminal, chat.id, &chat.local_dir(), &argv, chat.pid.clone(), &tx);
        chat.terminal = Some(terminal);
    }

    /// Abre o agente do chat em foco se ele estava em espera (restaurado ao abrir o app).
    fn wake_focused(&mut self) {
        let Some(chat) = self.chats.get_mut(self.focused) else {
            return;
        };
        if chat.status == Status::Suspended {
            chat.status = Status::Starting;
            self.spawn_chat_terminal(self.focused);
        }
    }

    /// Tira o chat da lista. O agente recebe SIGHUP: sai como no `/exit`, com a sessão salva.
    fn close_chat(&mut self, index: usize) {
        if index >= self.chats.len() {
            return;
        }
        let chat = self.chats.remove(index);
        hang_up(&chat);
        if index < self.focused {
            self.focused -= 1;
        }
        self.focused = self.focused.min(self.chats.len().saturating_sub(1));
        self.focus = Some(Focus::Chat);
    }

    fn take_terminal_id(&mut self) -> TermId {
        self.next_terminal_id += 1;
        self.next_terminal_id
    }

    /// Abre um shell numa aba nova, na pasta do chat em foco.
    fn add_terminal_tab(&mut self) {
        let id = self.take_terminal_id();
        let dir = self.chats.get(self.focused).map_or_else(|| self.default_dir.clone(), Chat::local_dir);
        let terminal = terminal::new(id, &self.tx);
        let pid = Rc::new(Cell::new(None));
        terminal::spawn(&terminal, id, &dir, &[], pid.clone(), &self.tx);
        self.tabs.push(Tab { id, terminal, pid, dir });
        self.active_tab = Some(id);
    }

    fn close_tab(&mut self, id: TermId) {
        let Some(position) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.tabs.remove(position);
        if let Some(pid) = tab.pid.get() {
            // SAFETY: só manda um sinal ao shell que este app abriu.
            unsafe { libc::kill(pid as i32, libc::SIGHUP) };
        }
        if self.active_tab == Some(id) {
            self.active_tab = self.tabs.get(position.min(self.tabs.len().saturating_sub(1))).map(|tab| tab.id);
        }
    }

    /// Nome de cada aba, na ordem: a última pasta do shell; a segunda aba na mesma pasta vira
    /// `chube 2`, a terceira `chube 3`…
    pub fn tab_names(&self) -> Vec<String> {
        let mut seen: HashMap<String, usize> = HashMap::new();
        self.tabs
            .iter()
            .map(|tab| {
                let name = dir_name(&tab.dir);
                let count = seen.entry(name.clone()).or_default();
                *count += 1;
                if *count == 1 { name } else { format!("{name} {count}") }
            })
            .collect()
    }

    /// Relê a pasta do shell de cada aba (barato: um `readlink` por aba).
    fn refresh_tab_dirs(&mut self) {
        for tab in &mut self.tabs {
            if let Some(dir) = tab.pid.get().and_then(|pid| std::fs::read_link(format!("/proc/{pid}/cwd")).ok()) {
                tab.dir = dir;
            }
        }
    }

    fn term_title(&mut self, id: TermId, title: &str) {
        let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == id) else {
            return;
        };
        if chat.host.is_some() {
            // Na VPS, o título do agente (repassado pelo tmux) dá o assunto e uma resposta
            // rápida do status.
            match claude::status_from_title(title) {
                Some(Status::Working) => chat.status = Status::Working,
                Some(status) if chat.status != Status::Waiting => chat.status = status,
                Some(_) => {}
                None if chat.status == Status::Starting => chat.status = Status::Ready,
                None => {}
            }
            // Antes do agente pôr o título dele, o tmux manda o do painel (o hostname).
            if claude::status_from_title(title).is_some() {
                if let Some(title) = claude::title_from_terminal(title) {
                    chat.title = short_title(&title);
                }
            }
            return;
        }
        if chat.agent == Agent::Codex {
            if let Some(hint) = codex::title_hint(title) {
                if chat.codex_thread_hint.as_ref() != Some(&hint) {
                    if chat.session_id.as_ref().is_some_and(|id| !id.starts_with(&hint)) {
                        chat.session_id = None;
                        chat.context_tokens = 0;
                        chat.context_window = None;
                        chat.active_model = None;
                        chat.title = "Novo chat".into();
                    }
                    chat.codex_thread_hint = Some(hint);
                }
                if chat.status == Status::Starting {
                    chat.status = Status::Ready;
                }
            }
        }
        if let Some(title) = (chat.agent == Agent::Claude).then(|| claude::title_from_terminal(title)).flatten() {
            chat.title = short_title(&title);
        }
    }

    /// Lê o status de cada agente (e o contexto do chat em foco) fora da thread do GTK.
    fn poll(&mut self) -> Task {
        if self.polling {
            return Task::none();
        }
        let focused = self.chats.get(self.focused).map(|chat| chat.id);
        let targets: Vec<(TermId, Agent, Option<u32>, Option<String>, Option<String>)> = self
            .chats
            .iter()
            // Os da VPS não têm arquivos aqui: o status vem do título do terminal.
            .filter(|chat| chat.is_alive() && chat.status != Status::Suspended && chat.host.is_none())
            .map(|chat| (chat.id, chat.agent, chat.pid(), chat.session_id.clone(), chat.codex_thread_hint.clone()))
            .collect();
        if targets.is_empty() {
            return Task::none();
        }
        self.polling = true;
        Task::blocking(move || {
            let list = targets
                .into_iter()
                .map(|(chat, agent, pid, session_id, hint)| {
                    if agent == Agent::Codex {
                        let session_id = hint.as_deref().and_then(codex::resolve_hint).or(session_id);
                        let snapshot = session_id.as_deref().and_then(codex::snapshot);
                        let running = snapshot.as_ref().and_then(|_| session_id.clone()).map(|session_id| Running {
                            session_id,
                            status: String::new(),
                        });
                        return Polled { chat, running, stats: None, codex: snapshot, codex_hint: hint };
                    }
                    let running = pid.and_then(sessions::running);
                    let session_id = running.as_ref().map(|running| running.session_id.clone()).or(session_id);
                    let stats = (Some(chat) == focused)
                        .then(|| session_id.as_deref().and_then(sessions::stats))
                        .flatten();
                    Polled { chat, running, stats, codex: None, codex_hint: None }
                })
                .collect();
            Message::Polled(list)
        })
    }

    fn apply_poll(&mut self, list: Vec<Polled>) {
        for chat in self.chats.iter_mut().filter(|chat| chat.is_alive()) {
            chat.read_mode();
        }
        for polled in list {
            let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == polled.chat) else {
                continue;
            };
            // /new ou /resume pode ter trocado a conversa enquanto o disco era lido.
            if chat.agent == Agent::Codex && chat.codex_thread_hint != polled.codex_hint {
                continue;
            }
            // O processo pode ter saído depois da leitura.
            if !chat.is_alive() {
                continue;
            }
            if let Some(running) = polled.running {
                // `/clear` troca a sessão: o contexto volta a zero. Num sub-chat, o id
                // aparece aqui pela primeira vez e o contexto herdado continua valendo.
                if chat.session_id.as_ref().is_some_and(|id| *id != running.session_id) {
                    chat.context_tokens = 0;
                }
                chat.session_id = Some(running.session_id);
                if let Some(status) = Status::from_claude(&running.status) {
                    chat.status = status;
                }
            }
            if let Some(snapshot) = polled.codex {
                if let Some(status) = snapshot.status {
                    chat.status = status;
                } else if chat.status == Status::Starting {
                    chat.status = Status::Ready;
                }
                chat.context_tokens = snapshot.context_tokens;
                chat.context_window = snapshot.context_window;
                if snapshot.model.is_some() {
                    chat.active_model = snapshot.model;
                }
                if let Some(title) = snapshot.title {
                    chat.title = title;
                }
                if !snapshot.limits.is_empty() {
                    chat.codex_limits = snapshot.limits;
                }
            }
            if let Some(stats) = polled.stats {
                chat.context_tokens = stats.context_tokens;
                if stats.model.is_some() {
                    chat.active_model = stats.model;
                }
            }
        }
    }

    /// Relê o tema do Omarchy e o aplica nos terminais abertos; o CSS fica com a interface.
    fn apply_omarchy_theme(&mut self) {
        themes::reload();
        let chats = self.chats.iter().filter_map(|chat| chat.terminal.as_ref());
        for terminal in chats.chain(self.tabs.iter().map(|tab| &tab.terminal)) {
            terminal::apply_theme(terminal);
        }
        self.theme_changed = true;
    }

    /// Grava a lista de chats se ela mudou. Arquivo pequeno e só quando muda: direto, sem
    /// thread, para as gravações não trocarem de ordem.
    fn save_open_chats(&mut self) {
        // Sub-chat cujo id ainda não apareceu fica de fora até aparecer.
        let with_id: Vec<(usize, &Chat)> = self
            .chats
            .iter()
            .enumerate()
            .filter(|(_, chat)| chat.session_id.is_some() || matches!(chat.start, Start::New { .. }) || chat.attach_only)
            .collect();
        let open = OpenChats {
            focused: with_id.iter().position(|(index, _)| *index == self.focused).unwrap_or(0),
            chats: with_id
                .iter()
                .map(|(_, chat)| OpenChat {
                    agent: chat.agent,
                    session_id: match (&chat.session_id, &chat.tmux) {
                        (_, Some(tmux)) if chat.attach_only => format!("{TMUX_ID_PREFIX}{tmux}"),
                        (Some(id), _) => id.clone(),
                        (None, _) => format!("draft:{}", chat.id),
                    },
                    cwd: chat.cwd.clone(),
                    title: chat.title.clone(),
                    model: chat.model.clone(),
                    parent: chat.parent.clone(),
                    auto: chat.auto,
                    host: chat.host.clone(),
                    tmux: chat.tmux.clone(),
                })
                .collect(),
        };
        if open == self.open_chats {
            return;
        }
        if let Err(error) = restore::save(&open) {
            log::error!("não consegui gravar a lista de chats: {error}");
        }
        self.open_chats = open;
    }
}

/// Manda SIGHUP ao processo do chat (o agente sai como no `/exit`, com a sessão salva). Sem isso,
/// o processo só morreria quando o GTK liberasse o widget do terminal.
fn hang_up(chat: &Chat) {
    if let Some(pid) = chat.pid() {
        // SAFETY: só manda um sinal ao processo que este app abriu.
        unsafe { libc::kill(pid as i32, libc::SIGHUP) };
    }
}

/// Liga o app ao GApplication: a primeira linha de comando abre a janela; as seguintes (o Files,
/// o launcher de novo) chegam pelo D-Bus à instância aberta e só abrem pastas.
pub fn run() -> glib::ExitCode {
    let application = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let state: Rc<RefCell<Option<Sender<Message>>>> = Rc::new(RefCell::new(None));
    application.connect_command_line(move |application, command_line| {
        let args: Vec<_> = command_line.arguments().into_iter().skip(1).collect();
        let cwd = command_line.cwd();
        let flags = cli::parse_in(args, cwd.as_deref());
        if let Some(tx) = state.borrow().as_ref() {
            let _ = tx.try_send(Message::OpenDirectories(flags));
            if let Some(window) = application.active_window() {
                window.present();
            }
            return glib::ExitCode::SUCCESS;
        }
        let flags = match flags {
            Ok(flags) => flags,
            Err(error) => {
                eprintln!("C-hub: {error}");
                return glib::ExitCode::from(2);
            }
        };
        let tx = start(application, flags);
        *state.borrow_mut() = Some(tx);
        glib::ExitCode::SUCCESS
    });
    application.run()
}

/// Abre a janela e o laço das mensagens; devolve o canal delas.
fn start(application: &gtk4::Application, flags: Flags) -> Sender<Message> {
    let (tx, rx) = async_channel::unbounded::<Message>();
    let app = Rc::new(RefCell::new(App::new(&flags, tx.clone())));
    let ui = Ui::new(application, &app.borrow(), tx.clone());
    {
        let mut app = app.borrow_mut();
        let _ = app.update(Message::OpenDirectories(Ok(flags)));
        ui.sync(&mut app);
    }
    ui.present();

    let timers: [(Duration, fn() -> Message); 2] =
        [(POLL_EVERY, || Message::Poll), (THEME_CHECK_EVERY, || Message::CheckTheme)];
    for (every, message) in timers {
        let tx = tx.clone();
        glib::timeout_add_local(every, move || {
            let _ = tx.try_send(message());
            glib::ControlFlow::Continue
        });
    }

    let sender = tx.clone();
    glib::spawn_future_local(async move {
        while let Ok(message) = rx.recv().await {
            if matches!(message, Message::Shortcut(Shortcut::SwitchFocus)) {
                ui.switch_focus(&app.borrow());
                continue;
            }
            let task = {
                let mut app = app.borrow_mut();
                let task = app.update(message);
                ui.sync(&mut app);
                task
            };
            task::run(task, &sender);
        }
    });
    tx
}

/// O que o painel de nova sessão mostra para um agente escolhido sem o modo do Omarchy.
fn launch_of(agent: Agent) -> Launch {
    match agent {
        Agent::Claude => Launch::Claude,
        Agent::Codex => Launch::Codex,
    }
}

/// Pasta da VPS como digitada; vazio é a home de lá.
pub fn remote_dir(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() { "~".into() } else { input.to_owned() }
}

/// Última pasta do caminho (`~/projects/chube` → `chube`); a home vira `~`.
pub fn dir_name(path: &std::path::Path) -> String {
    if path == claude::home_dir() {
        return "~".into();
    }
    path.file_name()
        .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn short_title(text: &str) -> String {
    short_text(text.lines().next().unwrap_or_default(), TITLE_MAX_CHARS)
}

pub fn short_text(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max).collect::<String>())
    } else {
        text.to_owned()
    }
}

/// `84,2k`, `200k`, `1m`.
pub fn short_tokens(tokens: u32) -> String {
    let (value, unit) = match tokens {
        1_000_000.. => (tokens as f32 / 1_000_000.0, "m"),
        1_000.. => (tokens as f32 / 1_000.0, "k"),
        _ => return tokens.to_string(),
    };
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{}{unit}", text.replace('.', ","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_tokens_abrevia() {
        assert_eq!(short_tokens(950), "950");
        assert_eq!(short_tokens(84_230), "84,2k");
        assert_eq!(short_tokens(200_000), "200k");
        assert_eq!(short_tokens(1_000_000), "1m");
    }
}
