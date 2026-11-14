use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{Key, key::Named};
use cosmic::iced::font::Weight;
use cosmic::iced::widget::scrollable;
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Alignment, Border, Color, Font, Length, Subscription};
use cosmic::widget::{self, pane_grid, segmented_button, text_editor, tooltip};
use cosmic::{Application, ApplicationExt, Element, theme};

use crate::agent::Agent;
use crate::codex;
use crate::cli::{self, Flags};
use crate::chat::{Chat, ModeChange, Status, TMUX_ID_PREFIX};
use crate::claude::{self, Mode, Start};
use crate::commands::{self, Command};
use crate::dirs;
use crate::omarchy;
use crate::remote::{self, Listing, RemoteStatus};
use crate::restore::{self, OpenChat, OpenChats};
use crate::sessions::{self, Running, SavedSession, Stats};
use crate::shortcuts::{self, Shortcut};
use crate::term::{self, Terminal};
use crate::themes;
use crate::usage::{self, Limit, Window};

/// Intervalo de atualização automática do painel de uso. Os limites vêm do arquivo que o
/// Omarchy atualiza; só sem ele o Chube roda o `/usage`.
const REFRESH_EVERY: Duration = Duration::from_secs(120);
/// Intervalo da conferência do tema do Omarchy (o Chube muda junto com `omarchy theme set`).
const THEME_CHECK_EVERY: Duration = Duration::from_secs(2);
/// Intervalo da leitura do status das sessões (`~/.claude/sessions`).
const POLL_EVERY: Duration = Duration::from_secs(1);
const TITLE_MAX_CHARS: usize = 40;
/// Espera entre colar a mensagem e mandar o Enter: junto da colagem, a TUI trataria o
/// Enter como parte do texto.
const SUBMIT_DELAY: Duration = Duration::from_millis(80);
/// Espera entre um Shift+Tab e a leitura do modo na tela.
const MODE_STEP_DELAY: Duration = Duration::from_millis(250);
/// Tempo máximo esperando o `claude` de um chat sair antes de abrir a sessão no terminal do Omarchy.
const EXIT_WAIT: Duration = Duration::from_secs(5);
/// Mais Shift+Tab do que isso e o modo pedido não está no ciclo.
const MODE_MAX_STEPS: u8 = 6;
/// Altura máxima do campo de mensagem; acima disso, ele rola.
const COMPOSER_MAX_HEIGHT: f32 = 360.0;
/// Largura de um caractere da fonte mono, em relação ao tamanho (0,6 na JetBrains Mono e na
/// maioria das mono); usada para saber quantos caracteres cabem numa barra.
const MONO_ADVANCE: f32 = 0.6;
const TAB_TITLE_MAX_CHARS: usize = 24;

/// Tamanho do texto da interface: o mesmo da fonte do terminal do Omarchy.
fn text_size() -> f32 {
    themes::current().font_px
}

/// Altura do cabeçalho das seções: duas linhas, como o do chat (título e pasta/modo).
fn header_height() -> f32 {
    2.0 * text_size() * 1.4
}

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
enum Panel {
    None,
    NewChat,
    Sessions,
}

/// Sessões de uma VPS, buscadas por SSH.
#[derive(Clone, Debug)]
enum RemoteState {
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

#[derive(Clone, Debug)]
pub enum Message {
    SelectChat(usize),
    OpenDirectories(Result<Flags, String>),
    NewChat,
    NewChatInput(String),
    NewChatLaunch(Launch),
    /// Onde rodar: esta máquina (`None`) ou um host do SSH.
    NewChatHost(Option<String>),
    NewChatConfirm,
    NewChatBrowse,
    NewChatPicked(Option<PathBuf>),
    ToggleDirGroup(PathBuf),
    ShowSessions,
    ClosePanel,
    /// Sessões de qual lugar mostrar: esta máquina (`None`) ou um host do SSH.
    SessionsHost(Option<String>),
    SessionsLoaded(Vec<SavedSession>),
    RemoteListed(String, Result<Listing, String>),
    ResumeSession(usize),
    /// Retoma uma sessão salva da VPS (host, índice na lista dela).
    ResumeRemote(String, usize),
    /// Entra numa sessão do tmux aberta na VPS (host, índice na lista dela).
    AttachTmux(String, usize),
    /// Encerra o agente do chat em foco na VPS (mata o tmux dele).
    KillRemote,
    RemoteKilled(Result<(), String>),
    SubChat,
    ReopenChat,
    CloseChat(usize),
    /// Leva a sessão do chat em foco para o terminal do Omarchy.
    OpenInTerminal,
    /// Se a sessão do chat já tem `.jsonl` (sem ele, o `--resume` falha).
    OpenInTerminalChecked(term::TabId, bool),
    OpenedInTerminal(Result<(), String>),
    SetMode(usize),
    ModeMenu(bool),
    /// Confere se o tema do Omarchy mudou.
    CheckTheme,
    ThemeChecked(String),
    ModeStep(term::TabId),
    ComposerAction(text_editor::Action),
    ComposerSend,
    /// Teclas repassadas do campo ao `claude` do chat em foco (Esc, Shift+Tab, setas…).
    ComposerKey(&'static [u8]),
    ComposerComplete(String),
    SubmitEnter(term::TabId),
    CommandsLoaded(term::TabId, Vec<Command>),
    Poll,
    Polled(Vec<Polled>),
    /// Estado dos chats de um host da VPS.
    RemotePolled(String, Result<Vec<RemoteStatus>, String>),
    OpenLink(String),
    RefreshInfo,
    /// Limites do Claude (do Omarchy ou do `/usage`), os do Codex (do Omarchy) e a hora da
    /// leitura feita pelo Omarchy.
    UsageFetched(Result<Vec<Limit>, String>, Option<Vec<Limit>>, Option<String>),
    ContextWindowFetched(String, Option<u32>),
    PaneResized(pane_grid::ResizeEvent),
    HideSection(Section),
    ShowSection(Section),
    NewTerminalTab,
    /// Roda do mouse sobre as abas do terminal (a rolagem delas é só horizontal).
    TerminalTabsWheel(f32),
    TerminalTabActivated(segmented_button::Entity),
    TerminalTabClosed(segmented_button::Entity),
    TermEventTx(term::EventSender),
    TermEvent(term::TabId, term::Event),
    Shortcut(Shortcut),
    /// Dá o foco do teclado ao terminal da seção (mostrando-a, se escondida).
    FocusSection(Section),
}

/// Pasta atual do shell de uma aba do terminal (dado da aba no `segmented_button::Model`).
struct TabDir(PathBuf);

/// Estado de um chat lido do disco pelo `Message::Poll`.
#[derive(Clone, Debug)]
pub struct Polled {
    chat: term::TabId,
    running: Option<Running>,
    /// Só do chat em foco (é o único mostrado no painel "Uso").
    stats: Option<Stats>,
    codex: Option<codex::Snapshot>,
    codex_hint: Option<String>,
}

pub struct App {
    core: Core,
    chats: Vec<Chat>,
    focused: usize,
    default_dir: PathBuf,
    model: Option<String>,
    default_agent: Agent,
    /// Agente padrão do Omarchy (`omarchy default agent`), se for o Claude Code ou o Codex.
    omarchy_agent: Option<Agent>,
    /// `--agent omarchy` na linha de comando.
    default_auto: bool,
    new_chat_launch: Launch,
    new_chat_host: Option<String>,
    /// Hosts do `~/.ssh/config`, oferecidos como lugar dos chats.
    hosts: Vec<String>,
    panel: Panel,
    /// Id do campo de mensagem do chat (hoje fora da tela).
    composer_id: widget::Id,
    /// Ids dos terminais na tela (o do chat em foco e o da aba ativa), para dar o foco a eles.
    chat_terminal_id: widget::Id,
    tab_terminal_id: widget::Id,
    /// Id da rolagem horizontal das abas do terminal.
    terminal_tabs_id: widget::Id,
    polling: bool,
    /// Hosts cuja conexão de estado caiu ou foi negada: não reabre sozinha (cada conexão pede
    /// permissão), só quando um chat de lá (re)conecta ou a lista de lá carrega.
    remote_poll_stopped: HashSet<String>,
    panes: pane_grid::State<Section>,
    sidebar_visible: bool,
    chat_visible: bool,
    terminal_visible: bool,
    /// Proporções das divisões, guardadas para refazer o layout ao esconder/mostrar seções.
    sidebar_ratio: f32,
    chat_ratio: f32,
    /// Pasta digitada no painel de nova sessão.
    new_chat_input: String,
    new_chat_input_id: widget::Id,
    /// Abas do terminal; cada uma guarda seu `TabId` e o `Mutex<Terminal>`.
    terminal_tabs: segmented_button::SingleSelectModel,
    /// Numeração única dos terminais: abas e chats.
    next_terminal_id: term::TabId,
    /// Canal dos eventos do PTY; chega pela subscription logo depois de abrir o app.
    term_events: Option<term::EventSender>,
    /// Limites do plano, do `/usage` e atualizados pelo `rate_limit_event` das respostas.
    limits: Vec<Limit>,
    usage_updated: Option<String>,
    usage_error: Option<String>,
    fetching_usage: bool,
    /// Janela de contexto de cada modelo, pelo `/context`; `None` enquanto busca ou se falhou
    /// (não tenta de novo a cada `Poll`).
    context_windows: HashMap<String, Option<u32>>,
    /// Sessões salvas desta máquina; `None` enquanto carrega.
    saved_sessions: Option<Vec<SavedSession>>,
    /// Lugar mostrado no painel de sessões: esta máquina (`None`) ou um host do SSH.
    sessions_host: Option<String>,
    remote_sessions: HashMap<String, RemoteState>,
    /// Limites do Codex lidos do Omarchy (os do histórico da sessão, sem ele).
    codex_usage: Option<Vec<Limit>>,
    /// Conta ativa no `omarchy agent account`.
    account: Option<String>,
    /// Marca do tema do Omarchy aplicado, para saber quando ele muda.
    theme_fingerprint: String,
    checking_theme: bool,
    /// Diretórios abertos na lista de sessões salvas.
    expanded_dirs: HashSet<PathBuf>,
    /// Menu do seletor de modo aberto.
    mode_menu: bool,
    /// Aviso mostrado abaixo do cabeçalho do chat (ex.: falha ao abrir no terminal do Omarchy).
    notice: Option<String>,
    /// Última lista de chats gravada (`restore`), para só gravar quando mudar.
    open_chats: OpenChats,
}

impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = "com.pedro.Chube";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Flags) -> (Self, Task<Message>) {
        // Aberto pelo launcher, o diretório atual é `/`: nesse caso, usa a home.
        let default_dir = std::env::current_dir()
            .ok()
            .filter(|dir| dir.parent().is_some())
            .unwrap_or_else(claude::home_dir);

        // Cores e fonte do tema do Omarchy antes de abrir qualquer terminal.
        let palette = themes::reload();
        let omarchy_agent = omarchy::default_agent().and_then(|name| Agent::parse(&name));
        let mut app = App {
            core,
            chats: Vec::new(),
            focused: 0,
            default_dir,
            model: flags.model.clone(),
            default_agent: flags.agent,
            omarchy_agent,
            default_auto: flags.auto,
            new_chat_launch: Launch::Claude,
            new_chat_host: None,
            hosts: remote::hosts(),
            panel: Panel::None,
            composer_id: widget::Id::unique(),
            chat_terminal_id: widget::Id::unique(),
            tab_terminal_id: widget::Id::unique(),
            terminal_tabs_id: widget::Id::unique(),
            polling: false,
            remote_poll_stopped: HashSet::new(),
            panes: pane_grid::State::new(Section::Chat).0,
            sidebar_visible: true,
            chat_visible: true,
            terminal_visible: true,
            sidebar_ratio: 0.22,
            chat_ratio: 0.64,
            new_chat_input: String::new(),
            new_chat_input_id: widget::Id::unique(),
            terminal_tabs: segmented_button::SingleSelectModel::default(),
            next_terminal_id: 0,
            term_events: None,
            limits: Vec::new(),
            usage_updated: None,
            usage_error: None,
            fetching_usage: false,
            context_windows: HashMap::new(),
            saved_sessions: None,
            sessions_host: None,
            remote_sessions: HashMap::new(),
            codex_usage: None,
            account: omarchy::active_account("claude"),
            theme_fingerprint: omarchy::theme_fingerprint(),
            checking_theme: false,
            expanded_dirs: HashSet::new(),
            mode_menu: false,
            notice: None,
            open_chats: OpenChats::default(),
        };
        // Sem a barra de título, como as janelas do Omarchy: o Hyprland move, fecha e
        // redimensiona (Super+W fecha).
        app.core.window.show_headerbar = false;
        app.core.window.content_container = false;
        app.rebuild_panes();
        // Volta a lista de chats da última execução, em espera; o do foco abre quando o canal de
        // eventos dos terminais chegar (`Message::TermEventTx`).
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

        let title = match app.core.main_window_id() {
            Some(id) => app.set_window_title("C-hub".into(), id),
            None => Task::none(),
        };
        let usage = app.refresh_usage();
        let theme = cosmic::command::set_theme(palette.cosmic());
        let open = app.open_directories(flags);
        let debug = app.debug_panel();
        (app, Task::batch([title, usage, theme, open, debug]))
    }

    fn dbus_activation(&mut self, message: cosmic::dbus_activation::Message) -> Task<Message> {
        if let cosmic::dbus_activation::Details::ActivateAction { action, args } = message.msg {
            if action == cli::OPEN_ACTION {
                return cosmic::task::future(async move {
                    // Valida as pastas fora da thread da interface, inclusive se foram removidas
                    // entre a chamada do Files e a chegada à janela aberta.
                    let result = tokio::task::spawn_blocking(move || cli::parse(args)).await
                        .unwrap_or_else(|error| Err(format!("Não consegui abrir as pastas: {error}")));
                    Message::OpenDirectories(result)
                });
            }
        }
        self.focus_chat()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        // Qualquer mensagem pode trocar o chat em foco (selecionar, fechar, retomar…).
        self.wake_focused();
        self.save_open_chats();
        task
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            cosmic::iced::time::every(REFRESH_EVERY).map(|_| Message::RefreshInfo),
            Subscription::run(term_events),
            // Também os eventos já capturados: os terminais capturam as teclas dos atalhos.
            cosmic::iced::event::listen_with(|event, _status, _window| match event {
                cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    physical_key,
                    repeat: false,
                    ..
                }) => shortcuts::find(modifiers, &key, &physical_key).map(Message::Shortcut),
                _ => None,
            }),
        ];
        subscriptions.push(cosmic::iced::time::every(THEME_CHECK_EVERY).map(|_| Message::CheckTheme));
        // Uma conexão de estado por host da VPS (uma confirmação da chave), aberta enquanto houver
        // chat lá; sair da lista fecha a conexão.
        for host in self.remote_poll_hosts() {
            subscriptions.push(
                Subscription::run_with(host, remote::poll_stream).map(|(host, result)| Message::RemotePolled(host, result)),
            );
        }
        if self.chats.iter().any(Chat::is_alive) {
            subscriptions.push(cosmic::iced::time::every(POLL_EVERY).map(|_| Message::Poll));
        }
        Subscription::batch(subscriptions)
    }

    /// A cor padrão do texto é a do tema do Omarchy. O fundo fica transparente (é pintado no
    /// `view`, com a opacidade do modo vidro).
    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        let palette = themes::current();
        Some(cosmic::iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: palette.text,
            icon_color: palette.text,
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let space = theme::spacing();

        widget::container(
            pane_grid::PaneGrid::new(&self.panes, |_pane, section, _maximized| {
                pane_grid::Content::new(self.section_view(*section))
            })
            .spacing(space.space_xxs)
            .min_size(180)
            .on_resize(10, Message::PaneResized),
        )
        .padding(space.space_xxs)
        // Sem o `content_container` do libcosmic, o fundo da janela é daqui: o do tema, com a
        // opacidade do modo vidro (o Hyprland desfoca o que fica atrás), cantos retos.
        .class(theme::Container::custom(|_| widget::container::Style {
            background: Some(themes::current().window().into()),
            ..Default::default()
        }))
        .into()
    }
}

impl App {
    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenDirectories(Ok(flags)) => return self.open_directories(flags),
            Message::OpenDirectories(Err(error)) => self.notice = Some(error),
            Message::SelectChat(index) => {
                self.notice = None;
                if index < self.chats.len() {
                    self.focused = index;
                    return self.focus_chat();
                }
            }
            Message::NewChat => {
                // Herda o agente e o lugar do chat em foco; sem chat, o padrão da linha de comando.
                let (launch, host) = match self.chats.get(self.focused) {
                    Some(chat) if chat.auto => (Launch::Omarchy, chat.host.clone()),
                    Some(chat) => (launch_of(chat.agent), chat.host.clone()),
                    None if self.default_auto => (Launch::Omarchy, None),
                    // Sem chat, o agente do Omarchy, se ele for um dos que o Chube abre.
                    None if self.omarchy_agent.is_some() => (Launch::Omarchy, None),
                    None => (launch_of(self.default_agent), None),
                };
                self.new_chat_launch = launch;
                self.new_chat_host = host;
                self.new_chat_input = String::new();
                self.panel = Panel::NewChat;
                return widget::text_input::focus(self.new_chat_input_id.clone());
            }
            Message::NewChatInput(text) => self.new_chat_input = text,
            Message::NewChatLaunch(launch) => self.new_chat_launch = launch,
            Message::NewChatHost(host) => {
                self.new_chat_host = host;
                return widget::text_input::focus(self.new_chat_input_id.clone());
            }
            Message::ClosePanel => {
                self.panel = Panel::None;
                return self.focus_chat();
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
                return self.open_chat(self.chats.len(), chat);
            }
            Message::NewChatBrowse => {
                return cosmic::task::future(async {
                    let folder = cosmic::dialog::file_chooser::open::Dialog::new()
                        .title("Pasta do novo chat")
                        .open_folder()
                        .await
                        .ok()
                        .and_then(|response| response.url().to_file_path().ok());
                    Message::NewChatPicked(folder)
                });
            }
            // A pasta escolhida vai para o campo, para ser conferida antes de criar.
            Message::NewChatPicked(Some(folder)) => {
                self.new_chat_input = folder.display().to_string();
            }
            Message::NewChatPicked(None) => {}
            Message::ToggleDirGroup(dir) => {
                if !self.expanded_dirs.remove(&dir) {
                    self.expanded_dirs.insert(dir);
                }
            }
            Message::ShowSessions => {
                self.panel = Panel::Sessions;
                self.sessions_host = self.chats.get(self.focused).and_then(|chat| chat.host.clone());
                self.expanded_dirs = HashSet::from([self.focused_dir()]);
                return self.load_sessions();
            }
            Message::SessionsHost(host) => {
                self.sessions_host = host;
                return self.load_sessions();
            }
            Message::SessionsLoaded(list) => self.saved_sessions = Some(list),
            Message::RemoteListed(host, result) => {
                let state = match result {
                    Ok(listing) => {
                        self.remote_poll_stopped.remove(&host);
                        RemoteState::Loaded(listing)
                    }
                    Err(error) => RemoteState::Failed(error),
                };
                self.remote_sessions.insert(host, state);
            }
            Message::ResumeSession(index) => {
                let Some(saved) = self
                    .saved_sessions
                    .as_ref()
                    .and_then(|list| list.get(index))
                    .cloned()
                else {
                    return Task::none();
                };
                return self.resume_saved(saved);
            }
            Message::ResumeRemote(host, index) => {
                let saved = match self.remote_sessions.get(&host) {
                    Some(RemoteState::Loaded(listing)) => listing.sessions.get(index).cloned(),
                    _ => None,
                };
                if let Some(saved) = saved {
                    return self.resume_saved(saved);
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
                    // Encerrado (rede caída, por exemplo), volta a entrar no mesmo tmux.
                    if !self.chats[open].is_alive() {
                        return self.update(Message::ReopenChat);
                    }
                    return self.focus_chat();
                }
                let title = claude::title_from_terminal(&tmux.title).unwrap_or_else(|| tmux.name.clone());
                let id = self.take_terminal_id();
                let chat = Chat::attach(id, host, tmux.name, PathBuf::from(tmux.path), short_title(&title));
                return self.open_chat(self.chats.len(), chat);
            }
            Message::KillRemote => {
                let Some(chat) = self.chats.get(self.focused) else {
                    return Task::none();
                };
                let (Some(host), Some(tmux)) = (chat.host.clone(), chat.tmux.clone()) else {
                    return Task::none();
                };
                self.remote_sessions.remove(&host);
                let close = self.close_chat(self.focused);
                let kill = cosmic::task::future(async move { Message::RemoteKilled(remote::kill(host, tmux).await) });
                return Task::batch([close, kill]);
            }
            Message::RemoteKilled(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("não consegui encerrar na VPS: {error}"));
                }
            }
            Message::SubChat => {
                let id = self.take_terminal_id();
                if let Some(chat) = self.chats.get(self.focused).and_then(|chat| chat.fork(id)) {
                    return self.open_chat(self.focused + 1, chat);
                }
            }
            Message::ReopenChat => {
                let id = self.take_terminal_id();
                if let Some(chat) = self.chats.get(self.focused).and_then(|chat| chat.reopen(id)) {
                    // Descartar o chat antigo encerra o terminal dele.
                    self.chats.remove(self.focused);
                    return self.open_chat(self.focused, chat);
                }
            }
            Message::CloseChat(index) => return self.close_chat(index),
            Message::OpenInTerminal => {
                self.notice = None;
                let Some(chat) = self.chats.get(self.focused) else {
                    return Task::none();
                };
                let id = chat.id;
                // Na VPS, o terminal do Omarchy entra no mesmo tmux: não depende do histórico.
                if chat.host.is_some() {
                    return self.update(Message::OpenInTerminalChecked(id, true));
                }
                let Some(session_id) = chat.session_id.clone() else {
                    return Task::none();
                };
                let agent = chat.agent;
                return cosmic::task::future(async move {
                    let saved = tokio::task::spawn_blocking(move || agent.session_file(&session_id).is_some())
                        .await
                        .unwrap_or(false);
                    Message::OpenInTerminalChecked(id, saved)
                });
            }
            Message::OpenInTerminalChecked(id, saved) => {
                let Some(index) = self.chats.iter().position(|chat| chat.id == id) else {
                    return Task::none();
                };
                let chat = &self.chats[index];
                let pid = chat.pid();
                let cwd = chat.local_dir();
                let command = if chat.host.is_some() {
                    // Fechar o chat só desconecta; o terminal novo entra no mesmo tmux da VPS.
                    let mut copy = chat.reopen(0).expect("chat da VPS sempre reabre");
                    copy.command()
                } else {
                    let Some(session_id) = chat.session_id.clone().filter(|_| saved) else {
                        self.notice = Some("a sessão ainda não foi salva: mande uma mensagem antes.".into());
                        return Task::none();
                    };
                    chat.agent.command(&Start::Resume { session_id }, chat.model.as_deref(), None, chat.auto)
                };
                let close = self.close_chat(index);
                let open = cosmic::task::future(async move {
                    Message::OpenedInTerminal(open_in_terminal(pid, cwd, command).await)
                });
                return Task::batch([close, open]);
            }
            Message::OpenedInTerminal(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("não consegui abrir o terminal do Omarchy: {error}"));
                }
            }
            Message::ModeMenu(open) => self.mode_menu = open,
            Message::CheckTheme => {
                if !self.checking_theme {
                    self.checking_theme = true;
                    return cosmic::task::future(async {
                        let fingerprint = tokio::task::spawn_blocking(omarchy::theme_fingerprint)
                            .await
                            .unwrap_or_default();
                        Message::ThemeChecked(fingerprint)
                    });
                }
            }
            Message::ThemeChecked(fingerprint) => {
                self.checking_theme = false;
                if fingerprint != self.theme_fingerprint {
                    self.theme_fingerprint = fingerprint;
                    return self.apply_omarchy_theme();
                }
            }
            Message::SetMode(index) => {
                self.mode_menu = false;
                let Some(chat) = self.chats.get_mut(self.focused) else {
                    return Task::none();
                };
                let target = Mode::ALL[index.min(Mode::ALL.len() - 1)];
                chat.read_mode();
                if chat.agent != Agent::Claude || chat.mode == Some(target) || !chat.is_alive() {
                    return Task::none();
                }
                chat.mode_change = Some(ModeChange {
                    target,
                    start: chat.mode,
                    steps: 1,
                });
                chat.send(claude::NEXT_MODE_KEY);
                return mode_step_later(chat.id);
            }
            Message::ModeStep(id) => {
                let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == id) else {
                    return Task::none();
                };
                let Some(mut change) = chat.mode_change.take() else {
                    return Task::none();
                };
                chat.read_mode();
                let arrived = chat.mode == Some(change.target);
                let looped = change.start.is_some() && chat.mode == change.start;
                if arrived || looped || change.steps >= MODE_MAX_STEPS || !chat.is_alive() {
                    return Task::none();
                }
                change.steps += 1;
                chat.mode_change = Some(change);
                chat.send(claude::NEXT_MODE_KEY);
                return mode_step_later(id);
            }
            Message::ComposerAction(action) => {
                let Some(chat) = self.chats.get_mut(self.focused) else {
                    return Task::none();
                };
                chat.composer.perform(action);
                // Os comandos do usuário e do projeto são lidos na primeira `/`.
                if chat.agent == Agent::Claude && chat.commands.is_none() && chat.composer.text().starts_with('/') {
                    chat.commands = Some(Vec::new());
                    let (id, cwd) = (chat.id, chat.cwd.clone());
                    return cosmic::task::future(async move {
                        let list = tokio::task::spawn_blocking(move || commands::load(&cwd))
                            .await
                            .unwrap_or_default();
                        Message::CommandsLoaded(id, list)
                    });
                }
            }
            Message::CommandsLoaded(id, list) => {
                if let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == id) {
                    chat.commands = Some(list);
                }
            }
            Message::ComposerComplete(name) => {
                if let Some(chat) = self.chats.get_mut(self.focused) {
                    chat.composer = text_editor::Content::with_text(&format!("/{name} "));
                    chat.composer
                        .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                }
                return self.focus_chat();
            }
            Message::ComposerSend => {
                let Some(chat) = self.chats.get_mut(self.focused) else {
                    return Task::none();
                };
                let text = chat.composer.text().trim_end().to_owned();
                // Campo vazio: o Enter vai para a TUI (confirma a opção escolhida na tela).
                if text.trim().is_empty() {
                    chat.send(b"\r".as_slice());
                    return Task::none();
                }
                if !chat.is_alive() {
                    return Task::none();
                }
                chat.composer = text_editor::Content::new();
                chat.paste(text);
                let id = chat.id;
                return cosmic::task::future(async move {
                    tokio::time::sleep(SUBMIT_DELAY).await;
                    Message::SubmitEnter(id)
                });
            }
            Message::SubmitEnter(id) => {
                if let Some(chat) = self.chats.iter().find(|chat| chat.id == id) {
                    chat.send(b"\r".as_slice());
                }
            }
            Message::ComposerKey(bytes) => {
                if let Some(chat) = self.chats.get(self.focused) {
                    chat.send(bytes);
                }
            }
            Message::Poll => return self.poll(),
            Message::Polled(list) => {
                self.polling = false;
                self.apply_poll(list);
                return self.fetch_context_window();
            }
            Message::RemotePolled(host, result) => match result {
                Ok(list) => {
                    self.apply_remote_poll(&host, list);
                    return self.fetch_context_window();
                }
                // Conexão negada ou caída: fica o status vindo do título até reconectar.
                Err(error) => {
                    log::warn!("estado de {host}: {error}");
                    self.remote_poll_stopped.insert(host);
                }
            },
            Message::OpenLink(url) => {
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
            }
            Message::RefreshInfo => return self.refresh_usage(),
            Message::UsageFetched(result, codex_limits, fetched) => {
                self.fetching_usage = false;
                self.codex_usage = codex_limits;
                match result {
                    Ok(limits) => {
                        self.limits = limits;
                        self.usage_error = None;
                        self.usage_updated = Some(fetched.unwrap_or_else(now_hms));
                    }
                    Err(error) => self.usage_error = Some(error),
                }
            }
            Message::ContextWindowFetched(model, window) => {
                self.context_windows.insert(model, window);
            }
            Message::PaneResized(event) => {
                // Com o painel da esquerda visível, a divisão da raiz é a dele.
                let root = match self.panes.layout() {
                    pane_grid::Node::Split { id, .. } => Some(*id),
                    pane_grid::Node::Pane(_) => None,
                };
                if self.sidebar_visible && root == Some(event.split) {
                    self.sidebar_ratio = event.ratio;
                } else {
                    self.chat_ratio = event.ratio;
                }
                self.panes.resize(event.split, event.ratio);
            }
            Message::HideSection(section) => {
                // Chat e terminal não somem os dois ao mesmo tempo.
                match section {
                    Section::Sidebar => self.sidebar_visible = false,
                    Section::Chat if self.terminal_visible => self.chat_visible = false,
                    Section::Terminal if self.chat_visible => self.terminal_visible = false,
                    _ => {}
                }
                self.rebuild_panes();
            }
            Message::ShowSection(section) => {
                match section {
                    Section::Sidebar => self.sidebar_visible = true,
                    Section::Chat => self.chat_visible = true,
                    Section::Terminal => self.terminal_visible = true,
                }
                self.rebuild_panes();
            }
            Message::NewTerminalTab => {
                self.add_terminal_tab();
                // A aba nova entra no fim: rola até ela.
                return cosmic::iced::widget::scrollable::snap_to(
                    self.terminal_tabs_id.clone(),
                    scrollable::RelativeOffset { x: Some(1.0), y: None },
                );
            }
            Message::TerminalTabsWheel(delta) => {
                return cosmic::iced::widget::scrollable::scroll_by(
                    self.terminal_tabs_id.clone(),
                    scrollable::AbsoluteOffset { x: delta, y: 0.0 },
                );
            }
            Message::TerminalTabActivated(id) => self.terminal_tabs.activate(id),
            // Remover a aba descarta o `Terminal`, que encerra o shell.
            Message::TerminalTabClosed(id) => self.terminal_tabs.remove(id),
            Message::TermEventTx(sender) => {
                let first = self.term_events.is_none();
                self.term_events = Some(sender);
                if first {
                    if self.terminal_tabs.len() == 0 {
                        self.add_terminal_tab();
                    }
                    for index in 0..self.chats.len() {
                        self.spawn_chat_terminal(index);
                    }
                    return self.focus_chat();
                }
            }
            Message::TermEvent(id, event) => return self.handle_term_event(id, event),
            Message::Shortcut(shortcut) => return self.handle_shortcut(shortcut),
            Message::FocusSection(section) => {
                let show = match section {
                    Section::Chat if !self.chat_visible => self.update(Message::ShowSection(Section::Chat)),
                    Section::Terminal if !self.terminal_visible => {
                        self.update(Message::ShowSection(Section::Terminal))
                    }
                    _ => Task::none(),
                };
                let focus = match section {
                    Section::Terminal => {
                        if self.terminal_tabs.len() == 0 {
                            self.add_terminal_tab();
                        }
                        self.focus_terminal_tab()
                    }
                    _ => self.focus_chat(),
                };
                return Task::batch([show, focus]);
            }
        }
        Task::none()
    }

    /// Para conferir a interface por captura de tela, sem clicar: `CHUBE_DEBUG_PANEL=new` abre
    /// o painel de nova sessão (`new:<host>` já com o host do SSH), `sessions` o de sessões
    /// desta máquina e `sessions:<host>` o de um host.
    fn debug_panel(&mut self) -> Task<Message> {
        let Ok(value) = std::env::var("CHUBE_DEBUG_PANEL") else {
            return Task::none();
        };
        let (panel, host) = value.split_once(':').map_or((value.as_str(), None), |(panel, host)| (panel, Some(host.to_owned())));
        match panel {
            "new" => {
                let task = self.update(Message::NewChat);
                self.new_chat_host = host;
                task
            }
            "sessions" => {
                self.panel = Panel::Sessions;
                self.update(Message::SessionsHost(host))
            }
            _ => Task::none(),
        }
    }

    /// O agente e o modo de uma escolha do painel de nova sessão; `None` se o agente padrão do
    /// Omarchy não é o Claude Code nem o Codex.
    fn launch_agent(&self, launch: Launch) -> Option<(Agent, bool)> {
        match launch {
            Launch::Omarchy => self.omarchy_agent.map(|agent| (agent, true)),
            Launch::Claude => Some((Agent::Claude, false)),
            Launch::Codex => Some((Agent::Codex, false)),
        }
    }

    /// Pasta do chat novo nesta máquina. Vazio é a pasta do chat em foco ou, no modo do Omarchy,
    /// a mesma do `omarchy agent` (`~/Work`).
    fn new_chat_dir(&self) -> Result<PathBuf, String> {
        if self.new_chat_input.trim().is_empty() {
            return Ok(if self.new_chat_launch == Launch::Omarchy {
                omarchy::work_dir()
            } else {
                self.focused_local_dir()
            });
        }
        dirs::resolve(&self.new_chat_input, &self.focused_local_dir())
    }

    /// Pasta do chat em foco, se ele é desta máquina; senão a padrão.
    fn focused_local_dir(&self) -> PathBuf {
        match self.chats.get(self.focused) {
            Some(chat) if chat.host.is_none() => chat.cwd.clone(),
            _ => self.default_dir.clone(),
        }
    }

    /// Carrega as sessões do lugar escolhido no painel (desta máquina ou da VPS).
    fn load_sessions(&mut self) -> Task<Message> {
        match self.sessions_host.clone() {
            None => {
                self.saved_sessions = None;
                cosmic::task::future(async {
                    let list = tokio::task::spawn_blocking(|| {
                        let mut list = sessions::list();
                        list.extend(codex::list());
                        list.sort_by(|a, b| b.modified.cmp(&a.modified));
                        list
                    })
                    .await
                    .unwrap_or_default();
                    Message::SessionsLoaded(list)
                })
            }
            Some(host) => {
                self.remote_sessions.insert(host.clone(), RemoteState::Loading);
                cosmic::task::future(async move {
                    let result = remote::list(host.clone()).await;
                    Message::RemoteListed(host, result)
                })
            }
        }
    }

    /// Abre uma sessão salva (daqui ou da VPS), ou foca o chat que já a tem aberta.
    fn resume_saved(&mut self, saved: SavedSession) -> Task<Message> {
        self.panel = Panel::None;
        if let Some(open) = self.chat_index_for_session(saved.agent, &saved.id) {
            self.focused = open;
            return self.focus_chat();
        }
        let id = self.take_terminal_id();
        let chat = Chat::resume(id, &saved, self.new_model(saved.agent));
        self.open_chat(self.chats.len(), chat)
    }

    /// Relê o tema do Omarchy e o aplica na interface e nos terminais abertos.
    fn apply_omarchy_theme(&mut self) -> Task<Message> {
        let palette = themes::reload();
        let cosmic_theme = palette.cosmic();
        let chats = self.chats.iter().filter_map(|chat| chat.terminal.as_ref());
        let tabs = self
            .terminal_tabs
            .iter()
            .filter_map(|entity| self.terminal_tabs.data::<Mutex<Terminal>>(entity));
        for terminal in chats.chain(tabs) {
            if let Ok(mut terminal) = terminal.lock() {
                term::apply_theme(&mut terminal, &cosmic_theme);
            }
        }
        cosmic::command::set_theme(cosmic_theme)
    }

    /// Tira o chat da lista. Descartar o `Terminal` fecha o PTY, e o `claude` recebe SIGHUP:
    /// sai como no `/exit`, com a sessão salva.
    fn close_chat(&mut self, index: usize) -> Task<Message> {
        if index >= self.chats.len() {
            return Task::none();
        }
        self.chats.remove(index);
        if index < self.focused {
            self.focused -= 1;
        }
        self.focused = self.focused.min(self.chats.len().saturating_sub(1));
        if self.chats.is_empty() {
            Task::none()
        } else {
            self.focus_chat()
        }
    }

    fn take_terminal_id(&mut self) -> term::TabId {
        self.next_terminal_id += 1;
        self.next_terminal_id
    }

    /// Lê os limites que o Omarchy mantém (`omarchy agent usage`) ou, sem eles, roda o
    /// `/usage`, se já não houver uma busca em andamento.
    fn refresh_usage(&mut self) -> Task<Message> {
        if self.fetching_usage {
            return Task::none();
        }
        self.fetching_usage = true;
        self.account = omarchy::active_account("claude");
        cosmic::task::future(async {
            let (claude, codex) = tokio::task::spawn_blocking(|| (omarchy::usage("claude"), omarchy::usage("codex")))
                .await
                .unwrap_or((None, None));
            let codex = codex.map(|usage| usage.limits);
            match claude {
                Some(usage) => {
                    let fetched = usage.fetched.map(|time| {
                        let time = time.format("%H:%M").to_string();
                        if usage.stale { format!("{time} (desatualizado)") } else { time }
                    });
                    Message::UsageFetched(Ok(usage.limits), codex, fetched)
                }
                None => Message::UsageFetched(usage::fetch().await, codex, None),
            }
        })
    }

    /// Busca a janela de contexto do modelo do chat em foco, se ainda não foi buscada.
    fn fetch_context_window(&mut self) -> Task<Message> {
        if self.chats.get(self.focused).is_none_or(|chat| chat.agent != Agent::Claude) {
            return Task::none();
        }
        let Some(model) = self.chats.get(self.focused).and_then(|chat| chat.active_model.clone())
        else {
            return Task::none();
        };
        if self.context_windows.contains_key(&model) {
            return Task::none();
        }
        self.context_windows.insert(model.clone(), None);
        cosmic::task::future(async move {
            let window = usage::context_window(model.clone()).await;
            if let Err(error) = &window {
                eprintln!("chube: {error}");
            }
            Message::ContextWindowFetched(model, window.ok())
        })
    }

    /// Refaz o layout com as seções visíveis, mantendo as proporções.
    fn rebuild_panes(&mut self) {
        use pane_grid::{Axis, Configuration};

        let split = |ratio, a, b| Configuration::Split {
            axis: Axis::Vertical,
            ratio,
            a: Box::new(a),
            b: Box::new(b),
        };
        let main = match (self.chat_visible, self.terminal_visible) {
            (true, true) => split(
                self.chat_ratio,
                Configuration::Pane(Section::Chat),
                Configuration::Pane(Section::Terminal),
            ),
            (false, true) => Configuration::Pane(Section::Terminal),
            _ => Configuration::Pane(Section::Chat),
        };
        let layout = if self.sidebar_visible {
            split(self.sidebar_ratio, Configuration::Pane(Section::Sidebar), main)
        } else {
            main
        };
        self.panes = pane_grid::State::with_configuration(layout);
    }

    /// Uma chamada externa cria chats novos; não retoma outra sessão da mesma pasta.
    fn open_directories(&mut self, flags: Flags) -> Task<Message> {
        if flags.directories.is_empty() {
            return Task::none();
        }
        self.notice = None;
        self.panel = Panel::None;
        self.chat_visible = true;
        self.rebuild_panes();
        let mut tasks = Vec::new();
        for directory in flags.directories {
            let id = self.take_terminal_id();
            let chat = Chat::new(id, flags.agent, PathBuf::from(directory), flags.model.clone(), None)
                .with_place(flags.auto, None);
            tasks.push(self.open_chat(self.chats.len(), chat));
        }
        Task::batch(tasks)
    }

    // --model se aplica apenas ao agente escolhido na linha de comando.
    fn new_model(&self, agent: Agent) -> Option<String> {
        (agent == self.default_agent).then(|| self.model.clone()).flatten()
    }

    /// Pasta do chat em foco nesta máquina (a padrão, se ele é da VPS).
    fn focused_dir(&self) -> PathBuf {
        self.focused_local_dir()
    }

    /// Põe o chat na lista, em `index`, abre o `claude` dele e o deixa em foco.
    fn open_chat(&mut self, index: usize, chat: Chat) -> Task<Message> {
        self.chats.insert(index, chat);
        self.focused = index;
        self.spawn_chat_terminal(index);
        self.focus_chat()
    }

    /// Abre o terminal do chat rodando o `claude`, se o canal de eventos já existe.
    fn spawn_chat_terminal(&mut self, index: usize) {
        let Some(events) = self.term_events.clone() else {
            return;
        };
        let chat = &mut self.chats[index];
        if chat.terminal.is_some() || !chat.is_alive() || chat.status == Status::Suspended {
            return;
        }
        let command = chat.command();
        if let Some(host) = &chat.host {
            self.remote_poll_stopped.remove(host);
        }
        match term::spawn(chat.id, events, chat.local_dir(), Some(command), &theme::active()) {
            Ok(terminal) => chat.terminal = Some(terminal),
            Err(err) => {
                log::error!("não consegui abrir {}: {err}", chat.agent.label());
                chat.status = Status::Exited;
            }
        }
    }

    /// Abre o `claude` do chat em foco se ele estava em espera (restaurado ao abrir o app).
    fn wake_focused(&mut self) {
        let Some(chat) = self.chats.get_mut(self.focused) else {
            return;
        };
        if chat.status == Status::Suspended {
            chat.status = Status::Starting;
            self.spawn_chat_terminal(self.focused);
        }
    }

    /// Grava a lista de chats se ela mudou. Arquivo pequeno e só quando muda: direto, sem
    /// `spawn_blocking`, para as gravações não trocarem de ordem.
    fn save_open_chats(&mut self) {
        // Sub-chat cujo id ainda não apareceu fica de fora até aparecer.
        let with_id: Vec<(usize, &Chat)> = self
            .chats
            .iter()
            .enumerate()
            .filter(|(_, chat)| {
                chat.session_id.is_some() || matches!(chat.start, Start::New { .. }) || chat.attach_only
            })
            .collect();
        let open = OpenChats {
            focused: with_id
                .iter()
                .position(|(index, _)| *index == self.focused)
                .unwrap_or(0),
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

    /// Foco do teclado no terminal do chat em foco (tira dos outros widgets).
    fn focus_chat(&self) -> Task<Message> {
        cosmic::iced::widget::operation::focus(self.chat_terminal_id.clone())
    }

    fn focus_terminal_tab(&self) -> Task<Message> {
        cosmic::iced::widget::operation::focus(self.tab_terminal_id.clone())
    }

    fn handle_shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        let toggle = |visible: bool, section| {
            if visible {
                Message::HideSection(section)
            } else {
                Message::ShowSection(section)
            }
        };
        match shortcut {
            Shortcut::NewChat => self.update(Message::NewChat),
            Shortcut::ToggleSidebar => self.update(toggle(self.sidebar_visible, Section::Sidebar)),
            Shortcut::ToggleChat => {
                let task = self.update(toggle(self.chat_visible, Section::Chat));
                // Escondido o chat, o teclado vai para o terminal; mostrado, volta a ele.
                let focus = if self.chat_visible { self.focus_chat() } else { self.focus_terminal_tab() };
                Task::batch([task, focus])
            }
            Shortcut::ToggleTerminal => {
                let task = self.update(toggle(self.terminal_visible, Section::Terminal));
                let focus = if self.terminal_visible { self.focus_terminal_tab() } else { self.focus_chat() };
                Task::batch([task, focus])
            }
            Shortcut::PreviousChat | Shortcut::NextChat if !self.chats.is_empty() => {
                // Dá a volta nas pontas, como as abas do tmux.
                let len = self.chats.len();
                let index = match shortcut {
                    Shortcut::PreviousChat => (self.focused + len - 1) % len,
                    _ => (self.focused + 1) % len,
                };
                self.update(Message::SelectChat(index))
            }
            Shortcut::PreviousChat | Shortcut::NextChat => Task::none(),
            Shortcut::SwitchFocus => {
                // Com o terminal do chat em foco, vai para o integrado; em qualquer outro caso
                // (terminal integrado, nada em foco), vai para o chat.
                cosmic::iced::widget::operation::is_focused(self.chat_terminal_id.clone()).map(|chat_focused| {
                    cosmic::Action::App(Message::FocusSection(if chat_focused {
                        Section::Terminal
                    } else {
                        Section::Chat
                    }))
                })
            }
        }
    }

    /// Lê o status de cada `claude` (e o contexto do chat em foco) fora da thread da UI.
    fn poll(&mut self) -> Task<Message> {
        if self.polling {
            return Task::none();
        }
        let focused = self.chats.get(self.focused).map(|chat| chat.id);
        let targets: Vec<(term::TabId, Agent, Option<u32>, Option<String>, Option<String>)> = self
            .chats
            .iter()
            // Os da VPS não têm arquivos aqui: o status vem do título do terminal.
            .filter(|chat| chat.is_alive() && chat.host.is_none())
            .map(|chat| (chat.id, chat.agent, chat.pid(), chat.session_id.clone(), chat.codex_thread_hint.clone()))
            .collect();
        self.polling = true;
        cosmic::task::future(async move {
            let list = tokio::task::spawn_blocking(move || {
                targets
                    .into_iter()
                    .map(|(chat, agent, pid, session_id, hint)| {
                        if agent == Agent::Codex {
                            let session_id = hint.as_deref().and_then(codex::resolve_hint).or(session_id);
                            let snapshot = session_id.as_deref().and_then(codex::snapshot);
                            let running = snapshot.as_ref().and_then(|_| session_id.clone()).map(|session_id| Running {
                                session_id, status: String::new(),
                            });
                            return Polled { chat, running, stats: None, codex: snapshot, codex_hint: hint };
                        }
                        let running = pid.and_then(sessions::running);
                        let session_id = running
                            .as_ref()
                            .map(|running| running.session_id.clone())
                            .or(session_id);
                        let stats = (Some(chat) == focused)
                            .then(|| session_id.as_deref().and_then(sessions::stats))
                            .flatten();
                        Polled { chat, running, stats, codex: None, codex_hint: None }
                    })
                    .collect()
            })
            .await
            .unwrap_or_default();
            Message::Polled(list)
        })
    }

    /// Hosts que precisam da conexão de estado: com um chat do Claude vivo, cujo agente já subiu
    /// (a chave já foi liberada; antes disso, a conexão de estado pediria a senha de novo).
    fn remote_poll_hosts(&self) -> HashSet<String> {
        self.chats
            .iter()
            .filter(|chat| {
                chat.is_alive()
                    && chat.agent == Agent::Claude
                    && chat.terminal.is_some()
                    && !matches!(chat.status, Status::Starting | Status::Suspended)
            })
            .filter_map(|chat| chat.host.clone())
            .filter(|host| !self.remote_poll_stopped.contains(host))
            .collect()
    }

    fn apply_remote_poll(&mut self, host: &str, list: Vec<RemoteStatus>) {
        for polled in list {
            let Some(chat) = self.chats.iter_mut().find(|chat| {
                chat.is_alive() && chat.host.as_deref() == Some(host) && chat.tmux.as_deref() == Some(polled.tmux.as_str())
            }) else {
                continue;
            };
            if let Some(session_id) = polled.session_id {
                // `/clear` troca a sessão (o contexto zera); num sub-chat, o id aparece aqui.
                if chat.session_id.as_ref().is_some_and(|id| *id != session_id) {
                    chat.context_tokens = 0;
                }
                chat.session_id = Some(session_id);
            }
            if let Some(status) = polled.status.as_deref().and_then(Status::from_claude) {
                chat.status = status;
            }
            if polled.model.is_some() {
                chat.active_model = polled.model;
            }
            if polled.context_tokens > 0 {
                chat.context_tokens = polled.context_tokens;
            }
        }
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
                if chat
                    .session_id
                    .as_ref()
                    .is_some_and(|id| *id != running.session_id)
                {
                    chat.context_tokens = 0;
                }
                chat.session_id = Some(running.session_id);
                if let Some(status) = Status::from_claude(&running.status) {
                    chat.status = status;
                }
            }
            if let Some(snapshot) = polled.codex {
                if let Some(status) = snapshot.status { chat.status = status; }
                else if chat.status == Status::Starting { chat.status = Status::Ready; }
                chat.context_tokens = snapshot.context_tokens;
                chat.context_window = snapshot.context_window;
                if snapshot.model.is_some() { chat.active_model = snapshot.model; }
                if let Some(title) = snapshot.title { chat.title = title; }
                if !snapshot.limits.is_empty() { chat.codex_limits = snapshot.limits; }
            }
            if let Some(stats) = polled.stats {
                chat.context_tokens = stats.context_tokens;
                if stats.model.is_some() {
                    chat.active_model = stats.model;
                }
            }
        }
    }

    fn chat_index_for_session(&self, agent: Agent, session_id: &str) -> Option<usize> {
        self.chats
            .iter()
            .position(|chat| chat.agent == agent && chat.is_alive() && chat.session_id.as_deref() == Some(session_id))
    }

    /// Abre um shell numa aba nova, na pasta do chat em foco.
    fn add_terminal_tab(&mut self) {
        let Some(events) = self.term_events.clone() else {
            return;
        };
        let id = self.take_terminal_id();
        let dir = self.focused_dir();
        let entity = self
            .terminal_tabs
            .insert()
            .text("Terminal")
            .closable()
            .data(id)
            .data(TabDir(dir.clone()))
            .activate()
            .id();
        match term::spawn(id, events, dir, None, &theme::active()) {
            Ok(terminal) => self.terminal_tabs.data_set::<Mutex<Terminal>>(entity, terminal),
            Err(err) => {
                log::error!("não consegui abrir o terminal: {err}");
                self.terminal_tabs.text_set(entity, "Erro ao abrir");
            }
        }
    }

    /// Nome de cada aba, na ordem: a última pasta do shell; a segunda aba na mesma pasta vira
    /// `chube 2`, a terceira `chube 3`…
    fn terminal_tab_names(&self) -> Vec<String> {
        let mut seen: HashMap<String, usize> = HashMap::new();
        self.terminal_tabs
            .iter()
            .map(|entity| {
                let name = self
                    .terminal_tabs
                    .data::<TabDir>(entity)
                    .map_or_else(|| "terminal".to_owned(), |dir| dir_name(&dir.0));
                let count = seen.entry(name.clone()).or_default();
                *count += 1;
                if *count == 1 { name } else { format!("{name} {count}") }
            })
            .collect()
    }

    fn terminal_entity(&self, id: term::TabId) -> Option<segmented_button::Entity> {
        self.terminal_tabs
            .iter()
            .find(|entity| self.terminal_tabs.data::<term::TabId>(*entity) == Some(&id))
    }

    /// Eventos do emulador, como no `Message::TermEvent` do cosmic-term.
    fn handle_term_event(&mut self, id: term::TabId, event: term::Event) -> Task<Message> {
        if let Some(chat) = self.chats.iter_mut().find(|chat| chat.id == id) {
            match event {
                term::Event::Title(title) if chat.host.is_some() => {
                    // Na VPS, o título do agente (repassado pelo tmux) dá o assunto e uma resposta
                    // rápida do status; a leitura periódica (`poll_remote`) traz o "precisa de você",
                    // que o título não mostra e que ele não desfaz.
                    match claude::status_from_title(&title) {
                        Some(Status::Working) => chat.status = Status::Working,
                        Some(status) if chat.status != Status::Waiting => chat.status = status,
                        Some(_) => {}
                        None if chat.status == Status::Starting => chat.status = Status::Ready,
                        None => {}
                    }
                    // Antes do agente pôr o título dele, o tmux manda o do painel (o hostname).
                    if claude::status_from_title(&title).is_some() {
                        if let Some(title) = claude::title_from_terminal(&title) {
                            chat.title = short_title(&title);
                        }
                    }
                }
                term::Event::Title(title) => {
                    if chat.agent == Agent::Codex {
                        if let Some(hint) = codex::title_hint(&title) {
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
                            if chat.status == Status::Starting { chat.status = Status::Ready; }
                        }
                    }
                    if let Some(title) = (chat.agent == Agent::Claude).then(|| claude::title_from_terminal(&title)).flatten() {
                        chat.title = short_title(&title);
                    }
                }
                // O `claude` saiu: o terminal fica na tela com a última saída.
                term::Event::Exit => chat.status = Status::Exited,
                event => {
                    if let Some(terminal) = &chat.terminal {
                        return terminal_io(terminal, event);
                    }
                }
            }
            return Task::none();
        }

        let Some(entity) = self.terminal_entity(id) else {
            return Task::none();
        };
        match event {
            // O zsh (oh-my-zsh) muda o título a cada prompt, inclusive depois de um `cd`: é a
            // hora de reler a pasta do shell, que dá o nome da aba.
            term::Event::Title(_) | term::Event::ResetTitle => {
                let pid = self
                    .terminal_tabs
                    .data::<Mutex<Terminal>>(entity)
                    .and_then(|terminal| terminal.lock().ok()?.pid());
                if let Some(dir) = pid.and_then(|pid| std::fs::read_link(format!("/proc/{pid}/cwd")).ok()) {
                    self.terminal_tabs.data_set(entity, TabDir(dir));
                }
            }
            // O shell saiu (`exit`): fecha a aba.
            term::Event::Exit => self.terminal_tabs.remove(entity),
            event => {
                if let Some(terminal) = self.terminal_tabs.data::<Mutex<Terminal>>(entity) {
                    return terminal_io(terminal, event);
                }
            }
        }
        Task::none()
    }

    fn section_view(&self, section: Section) -> Element<'_, Message> {
        match section {
            Section::Sidebar => widget::Column::new()
                .push(self.chats_card())
                .push(self.info_card())
                .spacing(theme::spacing().space_xxs)
                .height(Length::Fill)
                .into(),
            Section::Chat => match self.panel {
                Panel::NewChat => self.new_chat_section(),
                Panel::Sessions => self.sessions_section(),
                Panel::None => self.chat_section(),
            },
            Section::Terminal => self.terminal_section(),
        }
    }

    fn chats_card(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let palette = themes::current();

        let mut list = widget::Column::new().spacing(space.space_xxxs);
        for (index, chat) in self.chats.iter().enumerate() {
            let selected = index == self.focused;
            let (glyph, color) = status_glyph(chat.status);
            let title = if selected {
                bold(chat.title.as_str()).class(theme::Text::Color(palette.accent))
            } else {
                mono(chat.title.as_str())
            };
            let close_tip = if chat.host.is_some() {
                "Desconectar (o agente continua na VPS)"
            } else {
                "Fechar este chat (como /exit)"
            };
            let first = widget::Row::new()
                .push(tinted(format!("{glyph} "), color))
                // Cortado no container: sem ele, o texto sem quebra passa por baixo do botão.
                .push(widget::container(title.wrapping(Wrapping::None)).width(Length::Fill).clip(true))
                .push(icon_button(ICON_CLOSE, close_tip, Some(Message::CloseChat(index))))
                .align_y(Alignment::Center);
            // Trabalhando / precisa de você ficam na cor do status; os outros, apagados.
            let status_color = match chat.status {
                Status::Working | Status::Waiting => color,
                _ => palette.dim,
            };
            let mut second = widget::Row::new().push(
                tinted(format!("  {} · {} · ", chat.agent_label(), chat.place_label()), palette.dim)
                    .wrapping(Wrapping::None),
            );
            second = second.push(tinted(chat.status.label(), status_color).wrapping(Wrapping::None));
            let item = widget::container(widget::Column::new().push(first).push(second))
                .width(Length::Fill)
                .clip(true);
            list = list.push(
                row_button(item, selected)
                    .padding([space.space_xxxs, space.space_xxs])
                    .width(Length::Fill)
                    .on_press(Message::SelectChat(index)),
            );
        }
        if self.chats.is_empty() {
            list = list.push(tinted("nenhum chat aberto", palette.dim));
        }

        let header = widget::Row::new()
            .push(heading(ICON_CHATS, "chats"))
            .push(tinted(format!(" {}", self.chats.len()), palette.dim))
            .push(horizontal_fill())
            .push(icon_button(ICON_NEW, "Nova sessão (Ctrl+Shift+N)", Some(Message::NewChat)))
            .push(icon_button(ICON_HISTORY, "Sessões salvas", Some(Message::ShowSessions)))
            .push(shortcuts_help())
            .push(icon_button(
                ICON_HIDE_LEFT,
                "Esconder o painel (Ctrl+Shift+1)",
                Some(Message::HideSection(Section::Sidebar)),
            ))
            .spacing(space.space_xxxs)
            .align_y(Alignment::Center);

        card(
            widget::Column::new()
                .push(section_header(header))
                .push(thin_scrollable(list))
                .spacing(space.space_xxs),
        )
        .height(Length::FillPortion(3))
        .into()
    }

    fn info_card(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let palette = themes::current();
        let focused = self.chats.get(self.focused);
        let context_tokens = focused.map_or(0, |chat| chat.context_tokens);
        let is_codex = focused.is_some_and(|chat| chat.agent == Agent::Codex);
        let limits: &[Limit] = if is_codex {
            match &self.codex_usage {
                Some(limits) => limits,
                None => focused.map(|chat| chat.codex_limits.as_slice()).unwrap_or_default(),
            }
        } else {
            &self.limits
        };
        let known_window = if is_codex { focused.and_then(|chat| chat.context_window) } else {
            focused.and_then(|chat| chat.active_model.as_ref())
                .and_then(|model| self.context_windows.get(model).copied().flatten())
        };
        let working = self
            .chats
            .iter()
            .filter(|chat| chat.status == Status::Working)
            .count();

        let header = widget::Row::new()
            .push(heading(ICON_USAGE, "uso"))
            .push(horizontal_fill())
            .push(icon_button(
                ICON_REFRESH,
                "Atualizar agora",
                (!self.fetching_usage).then_some(Message::RefreshInfo),
            ))
            .align_y(Alignment::Center);

        let context = if context_tokens == 0 || (is_codex && known_window.is_none()) {
            meter("contexto", None, None, None)
        } else {
            // Enquanto o `/context` não responde, chuta 200 mil; acima disso, só pode ser 1 milhão
            // (vale também para um `[1m]` opcional, que o nome do modelo no histórico não mostra).
            let guess: u32 = if context_tokens > 200_000 { 1_000_000 } else { 200_000 };
            let window = if is_codex { known_window.unwrap_or(1).max(1) } else { known_window.map_or(guess, |window| window.max(guess)) };
            meter(
                "contexto",
                Some(context_tokens as f32 / window as f32),
                Some(format!("{} / {}", short_tokens(context_tokens), short_tokens(window))),
                None,
            )
        };

        let mut body = widget::Column::new().push(context).spacing(space.space_xs);
        if limits.is_empty() {
            body = body
                .push(meter("sessão (5h)", None, None, None))
                .push(meter("semana", None, None, None));
        }
        for limit in limits {
            let label = match &limit.window {
                Window::Session => "sessão (5h)".to_owned(),
                Window::Week(models) if models == "all models" => "semana".to_owned(),
                Window::Week(models) => format!("semana · {}", models.to_lowercase()),
            };
            body = body.push(meter(
                label,
                Some(limit.used),
                None,
                limit.resets.map(usage::describe_reset),
            ));
        }

        let status = if is_codex && self.codex_usage.is_none() {
            tinted("codex · limites do histórico; /status na sessão", palette.dim)
        } else if self.fetching_usage {
            tinted("atualizando…", palette.dim)
        } else if let Some(error) = &self.usage_error {
            tinted(format!("erro ao atualizar: {error}"), palette.danger)
        } else if let Some(time) = &self.usage_updated {
            tinted(format!("limites de {time}"), palette.dim)
        } else {
            tinted("ainda não atualizado", palette.dim)
        };
        let mut footer = widget::Column::new().push(tinted(
            format!("{} chats · {working} trabalhando", self.chats.len()),
            palette.dim,
        ));
        if let Some(account) = self.account.as_ref().filter(|_| !is_codex) {
            footer = footer.push(tinted(format!("conta {account}"), palette.dim));
        }
        body = body.push(footer.push(status));

        card(
            widget::Column::new()
                .push(section_header(header))
                // Rola em vez de cortar quando a janela é baixa.
                .push(thin_scrollable(body.padding([0, space.space_xs, 0, 0])))
                .spacing(space.space_xxs),
        )
        .height(Length::FillPortion(2))
        .into()
    }

    fn chat_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let palette = themes::current();
        let Some(chat) = self.chats.get(self.focused) else {
            return self.no_chat_section();
        };

        let mut details = vec![chat.agent_label(), chat.place_label()];
        if let Some(model) = chat.active_model.as_ref().or(chat.model.as_ref()) {
            details.push(model.clone());
        }
        if let Some(parent) = &chat.parent {
            details.push(format!("sub-chat de \"{parent}\""));
        }

        let mut header = widget::Row::new();
        if !self.sidebar_visible {
            header = header.push(show_sidebar_button());
        }
        let can_fork = chat.session_id.is_some() && chat.status != Status::Starting && !chat.attach_only;
        let (glyph, color) = status_glyph(chat.status);
        header = header
            .push(tinted(format!("{glyph} "), color))
            .push(
                widget::container(bold(chat.title.as_str()).wrapping(Wrapping::None))
                    .width(Length::Fill)
                    .clip(true),
            )
            .push(icon_button(
                ICON_FORK,
                "Criar sub-chat com o contexto deste chat",
                can_fork.then_some(Message::SubChat),
            ))
            .push(icon_button(
                ICON_TERMINAL,
                "Continuar no terminal do Omarchy (fecha o chat aqui)",
                (can_fork || chat.host.is_some()).then_some(Message::OpenInTerminal),
            ));
        if chat.host.is_some() {
            header = header
                .push(icon_button(ICON_STOP, "Encerrar o agente na VPS (mata o tmux)", Some(Message::KillRemote)))
                .push(icon_button(
                    ICON_CLOSE,
                    "Desconectar (o agente continua na VPS)",
                    Some(Message::CloseChat(self.focused)),
                ));
        } else {
            header = header.push(icon_button(
                ICON_CLOSE,
                "Fechar este chat (como /exit)",
                Some(Message::CloseChat(self.focused)),
            ));
        }
        header = header.push(self.chat_toggle_button());

        let mut second = widget::Row::new().push(
            widget::container(tinted(format!("  {}", details.join(" · ")), palette.dim).wrapping(Wrapping::None))
                .width(Length::Fill)
                .clip(true),
        );
        // Também na VPS: o modo é lido no rodapé da TUI e trocado com Shift+Tab, pelo tmux.
        if chat.is_alive() && chat.agent == Agent::Claude && !chat.attach_only {
            second = second.push(self.mode_selector(chat.mode));
        }
        let header = widget::Column::new()
            .push(header.spacing(space.space_xxxs).align_y(Alignment::Center))
            .push(second.align_y(Alignment::Center));

        let mut column = widget::Column::new().push(section_header(header));
        if let Some(notice) = &self.notice {
            column = column.push(tinted(format!("{ICON_WARNING} {notice}"), palette.warning));
        }
        if !chat.is_alive() {
            let (text, action) = match (chat.terminal.is_some(), chat.host.is_some()) {
                (true, true) => ("conexão com a VPS encerrada.", "reconectar"),
                (true, false) => ("o agente desta sessão foi encerrado.", "reabrir sessão"),
                (false, _) => ("não consegui abrir o agente.", "tentar de novo"),
            };
            column = column.push(
                widget::Row::new()
                    .push(tinted(format!("{ICON_EXITED} {text}"), palette.warning).width(Length::Fill))
                    .push(text_button(action, "Retomar ou reiniciar esta sessão", Some(Message::ReopenChat)))
                    .align_y(Alignment::Center),
            );
        }

        let body: Element<'_, Message> = match &chat.terminal {
            Some(terminal) => terminal_view(terminal, self.chat_terminal_id.clone()),
            None => widget::container(tinted(
                if chat.status == Status::Suspended {
                    "em espera"
                } else if chat.is_alive() {
                    "iniciando o agente…"
                } else {
                    ""
                },
                palette.dim,
            ))
            .center(Length::Fill)
            .into(),
        };

        card(column.push(body).spacing(space.space_xxs)).into()
    }

    /// Modo de permissão, como no rodapé da TUI; Shift+Tab no terminal também troca.
    fn mode_selector(&self, mode: Option<Mode>) -> Element<'_, Message> {
        let palette = themes::current();
        let label = match mode {
            Some(mode) => format!("{} {} {ICON_DOWN}", mode_glyph(mode), Mode::LABELS[mode.index()].to_lowercase()),
            None => format!("modo {ICON_DOWN}"),
        };
        let button = row_button(mono(label), self.mode_menu)
            .padding([0, 4])
            .on_press(Message::ModeMenu(!self.mode_menu));
        let mut popover = widget::popover(button)
            .position(widget::popover::Position::Bottom)
            .on_close(Message::ModeMenu(false));
        if self.mode_menu {
            let mut menu = widget::Column::new().spacing(2);
            for (index, candidate) in Mode::ALL.iter().enumerate() {
                let current = mode == Some(*candidate);
                let text = format!("{} {}", mode_glyph(*candidate), Mode::LABELS[index].to_lowercase());
                let label = if current { tinted(text, palette.accent) } else { mono(text) };
                menu = menu.push(
                    row_button(label, current)
                        .padding([2, 6])
                        .width(Length::Fill)
                        .on_press(Message::SetMode(index)),
                );
            }
            popover = popover.popup(menu_card(menu).width(Length::Fixed(220.0)));
        }
        popover.into()
    }

    /// Campo de mensagem do chat: cresce com o texto; Enter envia, Shift+Enter quebra a linha.
    fn composer<'a>(&'a self, chat: &'a Chat) -> Element<'a, Message> {
        let space = theme::spacing();
        let text = chat.composer.text();
        let suggestions = chat
            .commands
            .as_deref()
            .map(|list| commands::suggestions(list, &text))
            .unwrap_or_default();
        let first = suggestions.first().map(|command| command.name.clone());
        let empty = text.is_empty();

        let editor = text_editor::TextEditor::new(&chat.composer)
            .id(self.composer_id.clone())
            .placeholder("Mensagem ou /comando")
            .padding(space.space_xs)
            .max_height(COMPOSER_MAX_HEIGHT)
            .on_action(Message::ComposerAction)
            .key_binding(move |press| composer_key(press, empty, first.as_deref()));

        let mut column = widget::Column::new().spacing(space.space_xxs);
        for command in suggestions {
            let row = widget::Row::new()
                .push(widget::text::body(format!("/{}", command.name)).width(Length::FillPortion(1)))
                .push(widget::text::caption(command.description.as_str()).width(Length::FillPortion(3)))
                .spacing(space.space_s)
                .align_y(Alignment::Center);
            column = column.push(
                widget::button::custom(row)
                    .class(theme::Button::ListItem([8.0; 4]))
                    .padding([space.space_xxxs, space.space_xs])
                    .width(Length::Fill)
                    .on_press(Message::ComposerComplete(command.name.clone())),
            );
        }
        column.push(editor).into()
    }

    /// Seção do chat sem nenhum chat aberto.
    fn no_chat_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let palette = themes::current();
        let mut header = widget::Row::new();
        if !self.sidebar_visible {
            header = header.push(show_sidebar_button());
        }
        header = header
            .push(heading(ICON_CHATS, "nenhum chat aberto"))
            .push(horizontal_fill())
            .push(self.chat_toggle_button())
            .spacing(space.space_xxxs)
            .align_y(Alignment::Center);
        let notice = self
            .notice
            .as_ref()
            .map(|notice| tinted(format!("{ICON_WARNING} {notice}"), palette.warning));
        let body = widget::container(
            widget::Column::new()
                .push_maybe(notice)
                .push(text_button("nova sessão", "Abrir o agente do Omarchy, o Claude Code ou o Codex", Some(Message::NewChat)))
                .push(text_button("sessões salvas", "Retomar uma sessão daqui ou da VPS", Some(Message::ShowSessions)))
                .spacing(space.space_xxs)
                .align_x(Alignment::Center),
        )
        .center(Length::Fill);
        card(
            widget::Column::new()
                .push(section_header(header))
                .push(body)
                .spacing(space.space_xxs),
        )
        .into()
    }

    /// Esconde o chat ou, com o terminal escondido, mostra o terminal.
    fn chat_toggle_button(&self) -> Element<'_, Message> {
        if self.terminal_visible {
            icon_button(ICON_HIDE_LEFT, "Esconder o chat (Ctrl+Shift+2)", Some(Message::HideSection(Section::Chat)))
        } else {
            icon_button(
                ICON_TERMINAL,
                "Mostrar o terminal (Ctrl+Shift+3)",
                Some(Message::ShowSection(Section::Terminal)),
            )
        }
    }

    /// Cabeçalho dos painéis (nova sessão, sessões salvas), no lugar do do chat.
    fn panel_header<'a>(&self, icon: &'static str, title: &'static str, extra: Option<Element<'a, Message>>) -> Element<'a, Message> {
        let space = theme::spacing();
        let mut row = widget::Row::new();
        if !self.sidebar_visible {
            row = row.push(show_sidebar_button());
        }
        row = row.push(heading(icon, title)).push(horizontal_fill());
        if let Some(extra) = extra {
            row = row.push(extra);
        }
        row = row.push(icon_button(ICON_CLOSE, "Voltar ao chat", Some(Message::ClosePanel)));
        section_header(row.spacing(space.space_xxxs).align_y(Alignment::Center))
    }

    /// Painel de nova sessão, no estilo dos menus do Omarchy: agente, lugar e pasta, cada um com
    /// o rótulo em cima e as opções quebrando a linha numa seção estreita.
    fn new_chat_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let palette = themes::current();
        let label = |text: &'static str| tinted(text, palette.dim);
        let options = |items: Vec<Element<'static, Message>>| {
            widget::flex_row(items).spacing(space.space_xxs)
        };

        let omarchy_label = match self.omarchy_agent {
            Some(agent) => format!("{ICON_OMARCHY} omarchy ({})", agent.key()),
            None => format!("{ICON_OMARCHY} omarchy"),
        };
        let agents = options(vec![
            choice(omarchy_label, self.new_chat_launch == Launch::Omarchy, Message::NewChatLaunch(Launch::Omarchy)),
            choice("claude".into(), self.new_chat_launch == Launch::Claude, Message::NewChatLaunch(Launch::Claude)),
            choice("codex".into(), self.new_chat_launch == Launch::Codex, Message::NewChatLaunch(Launch::Codex)),
        ]);

        let mut place_items = vec![choice(
            format!("{ICON_LAPTOP} local"),
            self.new_chat_host.is_none(),
            Message::NewChatHost(None),
        )];
        for host in &self.hosts {
            let selected = self.new_chat_host.as_ref() == Some(host);
            place_items.push(choice(format!("{ICON_SERVER} {host}"), selected, Message::NewChatHost(Some(host.clone()))));
        }

        let remote = self.new_chat_host.is_some();
        let placeholder = match (remote, self.new_chat_launch) {
            (true, _) => "pasta na VPS (vazio = ~)",
            (false, Launch::Omarchy) => "pasta, atalho do fshort ou . (vazio = ~/Work)",
            (false, _) => "pasta, atalho do fshort ou . (vazio = a do chat em foco)",
        };
        let input = widget::text_input(placeholder, &self.new_chat_input)
            .id(self.new_chat_input_id.clone())
            .font(mono_font())
            .size(text_size())
            .on_input(Message::NewChatInput)
            .on_submit(|_| Message::NewChatConfirm);

        let resolved: Result<String, String> = match &self.new_chat_host {
            Some(host) => Ok(format!("{host}:{}", remote_dir(&self.new_chat_input))),
            None => self.new_chat_dir().map(|path| claude::display_path(&path)),
        };
        let launchable = self.launch_agent(self.new_chat_launch).is_some();
        let (preview, preview_color) = match (&resolved, launchable) {
            (_, false) => ("o agente padrão do Omarchy não é o Claude Code nem o Codex".to_owned(), palette.danger),
            (Ok(path), true) => (format!("{ICON_RIGHT} {path}"), palette.dim),
            (Err(error), true) => (error.clone(), palette.danger),
        };

        let group = |title: &'static str, content: Element<'static, Message>| -> Element<'static, Message> {
            widget::Column::new().push(label(title)).push(content).spacing(space.space_xxxs).into()
        };
        let mut folder = widget::Column::new()
            .push(label("pasta"))
            .push(input)
            .push(tinted(preview, preview_color))
            .spacing(space.space_xxxs);
        if !remote {
            let names: Vec<String> = dirs::shortcuts().into_iter().map(|(name, _)| name).collect();
            if !names.is_empty() {
                folder = folder.push(tinted(format!("atalhos: {}", names.join(", ")), palette.dim));
            }
        } else {
            folder = folder.push(tinted(
                "roda num tmux da VPS: fechar o chat só desconecta. A senha da chave é pedida no terminal.",
                palette.dim,
            ));
        }
        let mut actions = vec![
            text_button(
                "criar",
                "Abrir o chat (Enter)",
                (launchable && resolved.is_ok()).then_some(Message::NewChatConfirm),
            ),
            text_button("cancelar", "Voltar ao chat", Some(Message::ClosePanel)),
        ];
        if !remote {
            actions.push(text_button("escolher pasta…", "Abrir o seletor de pastas", Some(Message::NewChatBrowse)));
        }

        let form = widget::Column::new()
            .push(group("agente", agents.into()))
            .push(group("onde", options(place_items).into()))
            .push(folder)
            .push(options(actions))
            .spacing(space.space_s)
            .max_width(640);

        card(
            widget::Column::new()
                .push(self.panel_header(ICON_NEW, "nova sessão", None))
                .push(thin_scrollable(form.padding([space.space_xs, space.space_xs])))
                .spacing(space.space_xxs),
        )
        .into()
    }

    /// Painel das sessões salvas: desta máquina ou de um host do SSH.
    fn sessions_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let mut tabs = widget::Row::new()
            .push(choice(format!("{ICON_LAPTOP} local"), self.sessions_host.is_none(), Message::SessionsHost(None)))
            .spacing(space.space_xxs)
            .align_y(Alignment::Center);
        for host in &self.hosts {
            let selected = self.sessions_host.as_ref() == Some(host);
            tabs = tabs.push(choice(format!("{ICON_SERVER} {host}"), selected, Message::SessionsHost(Some(host.clone()))));
        }
        let refresh = icon_button(ICON_REFRESH, "Recarregar", Some(Message::SessionsHost(self.sessions_host.clone())));
        let body: Element<'_, Message> = match &self.sessions_host {
            None => self.local_sessions_list(),
            Some(host) => self.remote_sessions_list(host),
        };
        card(
            widget::Column::new()
                .push(self.panel_header(ICON_HISTORY, "sessões", Some(refresh)))
                .push(tabs)
                .push(thin_scrollable(body))
                .spacing(space.space_xs),
        )
        .into()
    }

    fn local_sessions_list(&self) -> Element<'_, Message> {
        let palette = themes::current();
        let Some(list) = &self.saved_sessions else {
            return tinted("carregando…", palette.dim).into();
        };
        if list.is_empty() {
            return tinted("nenhuma sessão salva encontrada.", palette.dim).into();
        }

        // Agrupa por diretório; os grupos seguem a ordem da sessão mais recente de cada um.
        let mut groups: Vec<(&PathBuf, Vec<usize>)> = Vec::new();
        for (index, saved) in list.iter().enumerate() {
            match groups.iter_mut().find(|(dir, _)| **dir == saved.cwd) {
                Some((_, indices)) => indices.push(index),
                None => groups.push((&saved.cwd, vec![index])),
            }
        }

        let mut column = widget::Column::new().spacing(theme::spacing().space_xxxs);
        for (dir, indices) in groups {
            let expanded = self.expanded_dirs.contains(dir);
            let header = widget::Row::new()
                .push(tinted(format!("{} ", if expanded { ICON_DOWN } else { ICON_RIGHT }), palette.accent))
                .push(
                    widget::container(bold(claude::display_path(dir)).wrapping(Wrapping::None))
                        .width(Length::Fill)
                        .clip(true),
                )
                .push(tinted(format!(" {}", indices.len()), palette.dim));
            column = column.push(
                row_button(header, false)
                    .padding([theme::spacing().space_xxs, theme::spacing().space_xxs])
                    .width(Length::Fill)
                    .on_press(Message::ToggleDirGroup(dir.clone())),
            );
            if expanded {
                for index in indices {
                    column = column.push(self.saved_session_item(&list[index], Message::ResumeSession(index)));
                }
            }
        }
        column.into()
    }

    fn remote_sessions_list<'a>(&'a self, host: &'a str) -> Element<'a, Message> {
        let space = theme::spacing();
        let palette = themes::current();
        let listing = match self.remote_sessions.get(host) {
            None | Some(RemoteState::Loading) => return tinted(format!("conectando a {host}…"), palette.dim).into(),
            Some(RemoteState::Failed(error)) => {
                return widget::Column::new()
                    .push(tinted(format!("{ICON_WARNING} {error}"), palette.warning))
                    .push(text_button("tentar de novo", "Conectar de novo", Some(Message::SessionsHost(Some(host.to_owned())))))
                    .spacing(space.space_xxs)
                    .into();
            }
            Some(RemoteState::Loaded(listing)) => listing,
        };

        let mut column = widget::Column::new()
            .push(tinted(format!("tmux abertos em {host}"), palette.dim))
            .spacing(space.space_xxxs);
        if listing.tmux.is_empty() {
            column = column.push(tinted("  nenhum", palette.dim));
        }
        for (index, tmux) in listing.tmux.iter().enumerate() {
            let here = self
                .chats
                .iter()
                .any(|chat| chat.host.as_deref() == Some(host) && chat.tmux.as_deref() == Some(tmux.name.as_str()) && chat.is_alive());
            let title = claude::title_from_terminal(&tmux.title).unwrap_or_else(|| tmux.title.clone());
            let mut details = vec![tmux.name.clone(), tmux.path.clone()];
            if here {
                details.push("aberto aqui".into());
            } else if tmux.attached {
                details.push("conectado em outro lugar".into());
            }
            let (glyph, color) = match claude::status_from_title(&tmux.title) {
                Some(status) => status_glyph(status),
                None => (ICON_TMUX, palette.dim),
            };
            let item = widget::Column::new()
                .push(
                    widget::Row::new()
                        .push(tinted(format!("{glyph} "), color))
                        .push(mono(title).wrapping(Wrapping::None)),
                )
                .push(tinted(format!("  {}", details.join(" · ")), palette.dim).wrapping(Wrapping::None));
            column = column.push(
                row_button(widget::container(item).clip(true), false)
                    .padding([space.space_xxxs, space.space_xxs])
                    .width(Length::Fill)
                    .on_press(Message::AttachTmux(host.to_owned(), index)),
            );
        }

        column = column.push(widget::Space::new().height(Length::Fixed(space.space_xs.into())));
        column = column.push(tinted(format!("sessões salvas em {host}"), palette.dim));
        if listing.sessions.is_empty() {
            column = column.push(tinted("  nenhuma", palette.dim));
        }
        for (index, saved) in listing.sessions.iter().enumerate() {
            column = column.push(self.saved_session_item(saved, Message::ResumeRemote(host.to_owned(), index)));
        }
        column.into()
    }

    fn saved_session_item<'a>(&self, saved: &'a SavedSession, on_press: Message) -> Element<'a, Message> {
        let palette = themes::current();
        let open_here = self.chat_index_for_session(saved.agent, &saved.id).is_some();
        let mut details = vec![saved.agent.key().to_owned(), sessions::relative_time(saved.modified)];
        if saved.host.is_some() {
            details.push(saved.cwd.display().to_string());
        }
        let blocked = match (&saved.running, open_here) {
            (_, true) => {
                details.push("aberta aqui".into());
                false
            }
            (Some(_), false) => {
                details.push("aberta em outro terminal".into());
                true
            }
            (None, false) => false,
        };
        let running = self
            .chat_index_for_session(saved.agent, &saved.id)
            .map(|index| self.chats[index].status)
            .or_else(|| saved.running.as_deref().and_then(Status::from_claude));
        let (glyph, color) = match running {
            Some(status) => status_glyph(status),
            None => (ICON_IDLE, palette.dim),
        };
        let item = widget::Column::new()
            .push(
                widget::Row::new()
                    .push(tinted(format!("{glyph} "), color))
                    .push(mono(saved.title.as_str()).wrapping(Wrapping::None)),
            )
            .push(tinted(format!("  {}", details.join(" · ")), palette.dim).wrapping(Wrapping::None));
        row_button(widget::container(item).clip(true), false)
            .padding([theme::spacing().space_xxxs, theme::spacing().space_xxs])
            .width(Length::Fill)
            .on_press_maybe((!blocked).then_some(on_press))
            .into()
    }

    fn terminal_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let palette = themes::current();

        let toggle = if self.chat_visible {
            icon_button(ICON_HIDE_RIGHT, "Esconder o terminal (Ctrl+Shift+3)", Some(Message::HideSection(Section::Terminal)))
        } else {
            icon_button(ICON_HIDE_LEFT, "Mostrar o chat (Ctrl+Shift+2)", Some(Message::ShowSection(Section::Chat)))
        };

        // Abas com o nome da pasta do shell e um × para fechar.
        let active = self.terminal_tabs.active();
        let mut tabs = widget::Row::new().spacing(space.space_xxxs);
        for (entity, name) in self.terminal_tabs.iter().zip(self.terminal_tab_names()) {
            let selected = entity == active;
            let title = format!("{ICON_TERMINAL} {}", short_text(&name, TAB_TITLE_MAX_CHARS));
            let label = if selected { tinted(title, palette.accent) } else { mono(title) }.wrapping(Wrapping::None);
            let tab = widget::Row::new()
                .push(row_button(label, selected).padding([1, 6]).on_press(Message::TerminalTabActivated(entity)))
                .push(
                    row_button(tinted(ICON_CLOSE, palette.dim), selected)
                        .padding([1, 4])
                        .on_press(Message::TerminalTabClosed(entity)),
                );
            tabs = tabs.push(tab);
        }

        let mut header = widget::Row::new();
        if !self.sidebar_visible && !self.chat_visible {
            header = header.push(show_sidebar_button());
        }
        let header = header
            .push(toggle)
            .push(
                // A roda do mouse (vertical) rola as abas na horizontal; Shift+roda o scrollable
                // já trata sozinho.
                widget::mouse_area(
                    widget::scrollable(tabs)
                        .id(self.terminal_tabs_id.clone())
                        .direction(scrollable::Direction::Horizontal(
                            scrollable::Scrollbar::new().width(2).scroller_width(2),
                        ))
                        .width(Length::Fill),
                )
                .on_scroll(|delta| {
                    Message::TerminalTabsWheel(match delta {
                        cosmic::iced::mouse::ScrollDelta::Lines { y, .. } => -y * 60.0,
                        cosmic::iced::mouse::ScrollDelta::Pixels { y, .. } => -y,
                    })
                }),
            )
            .push(icon_button(ICON_NEW, "Nova aba do terminal", Some(Message::NewTerminalTab)))
            .spacing(space.space_xxs)
            .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.terminal_tabs.data::<Mutex<Terminal>>(active) {
            Some(terminal) => terminal_view(terminal, self.tab_terminal_id.clone()),
            None => widget::container(
                widget::Column::new()
                    .push(tinted("nenhum terminal aberto.", palette.dim))
                    .push(text_button("abrir terminal", "Abrir um shell", Some(Message::NewTerminalTab)))
                    .spacing(space.space_xxs)
                    .align_x(Alignment::Center),
            )
            .center(Length::Fill)
            .into(),
        };

        card(
            widget::Column::new()
                .push(section_header(header))
                .push(body)
                .spacing(space.space_xxs),
        )
        .into()
    }
}

/// O que o painel de nova sessão mostra para um agente escolhido sem o modo do Omarchy.
fn launch_of(agent: Agent) -> Launch {
    match agent {
        Agent::Claude => Launch::Claude,
        Agent::Codex => Launch::Codex,
    }
}

/// Pasta da VPS como digitada; vazio é a home de lá.
fn remote_dir(input: &str) -> String {
    let input = input.trim();
    if input.is_empty() { "~".into() } else { input.to_owned() }
}

// Ícones da Nerd Font (a JetBrainsMono Nerd Font do Omarchy tem todos), como na barra do Omarchy.
const ICON_NEW: &str = "\u{f067}";
const ICON_HISTORY: &str = "\u{f02da}";
const ICON_HELP: &str = "\u{f059}";
const ICON_HIDE_LEFT: &str = "\u{f053}";
const ICON_HIDE_RIGHT: &str = "\u{f054}";
const ICON_RIGHT: &str = "\u{f0da}";
const ICON_DOWN: &str = "\u{f0d7}";
const ICON_FORK: &str = "\u{e725}";
const ICON_TERMINAL: &str = "\u{f489}";
const ICON_CLOSE: &str = "\u{f0156}";
const ICON_REFRESH: &str = "\u{f0450}";
const ICON_SERVER: &str = "\u{f048b}";
const ICON_LAPTOP: &str = "\u{f0322}";
const ICON_OMARCHY: &str = "\u{f303}";
const ICON_STOP: &str = "\u{f04d}";
const ICON_CHATS: &str = "\u{f0b79}";
const ICON_USAGE: &str = "\u{f080}";
const ICON_WARNING: &str = "\u{f071}";
const ICON_TMUX: &str = "\u{ebc8}";
const ICON_IDLE: &str = "\u{f444}";
const ICON_EXITED: &str = "\u{f057}";

/// Rolagem vertical com a barra fina, como a dos menus do Omarchy.
fn thin_scrollable<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(3).scroller_width(3),
        ))
        .height(Length::Fill)
        .into()
}

fn mono_font() -> Font {
    Font::MONOSPACE
}

/// Seção: cantos retos e uma borda fina e apagada, como a das janelas inativas do Hyprland. Sem
/// fundo próprio: o da janela (translúcido no modo vidro) aparece por baixo; pintar de novo
/// dobraria o alfa.
fn card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(theme::spacing().space_xxs)
        .width(Length::Fill)
        .height(Length::Fill)
        .class(theme::Container::custom(|_| {
            let palette = themes::current();
            widget::container::Style {
                border: Border {
                    color: Color { a: 0.35, ..palette.dim },
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        }))
}

/// Menu suspenso, como os popups do shell do Omarchy: fundo opaco e borda no acento.
fn menu_card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(theme::spacing().space_xxs)
        .class(theme::Container::custom(|_| {
            let palette = themes::current();
            widget::container::Style {
                background: Some(palette.background.into()),
                border: Border {
                    color: palette.accent,
                    width: 2.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        }))
}

/// Cabeçalho de seção com uma linha embaixo, todos da mesma altura (as linhas ficam alinhadas).
fn section_header<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let line = widget::container(widget::Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .class(theme::Container::custom(|_| {
            let palette = themes::current();
            widget::container::Style {
                background: Some(Color { a: 0.35, ..palette.dim }.into()),
                ..Default::default()
            }
        }));
    widget::Column::new()
        .push(
            widget::container(content)
                .width(Length::Fill)
                .height(Length::Fixed(header_height()))
                .align_y(Alignment::Center),
        )
        .push(line)
        .spacing(theme::spacing().space_xxs)
        .into()
}

fn mono<'a>(text: impl Into<Cow<'a, str>> + 'a) -> widget::Text<'a, cosmic::Theme, cosmic::Renderer> {
    widget::text(text).font(mono_font()).size(text_size())
}

fn bold<'a>(text: impl Into<Cow<'a, str>> + 'a) -> widget::Text<'a, cosmic::Theme, cosmic::Renderer> {
    mono(text).font(Font {
        weight: Weight::Bold,
        ..mono_font()
    })
}

fn tinted<'a>(text: impl Into<Cow<'a, str>> + 'a, color: Color) -> widget::Text<'a, cosmic::Theme, cosmic::Renderer> {
    mono(text).class(theme::Text::Color(color))
}

/// Título de seção: o ícone no acento e o nome em negrito.
fn heading<'a>(icon: &'static str, text: &'a str) -> Element<'a, Message> {
    widget::Row::new()
        .push(tinted(format!("{icon} "), themes::current().accent))
        .push(bold(text))
        .into()
}

/// Linha clicável, como as dos menus do Omarchy: sem fundo; ao passar o mouse, o texto a 8%;
/// selecionada, o mesmo fundo e uma borda do texto a 25%.
fn row_button<'a>(content: impl Into<Element<'a, Message>>, selected: bool) -> widget::Button<'a, Message> {
    let style = move |hover: f32, disabled: bool| {
        let palette = themes::current();
        let text = if disabled { Color { a: 0.4, ..palette.text } } else { palette.text };
        let fill = if selected { 0.08 } else { 0.0 } + hover;
        widget::button::Style {
            background: (fill > 0.0).then(|| palette.text_alpha(fill).into()),
            border_radius: 0.0.into(),
            border_width: if selected { 1.0 } else { 0.0 },
            border_color: palette.text_alpha(0.25),
            text_color: Some(text),
            icon_color: Some(text),
            ..widget::button::Style::new()
        }
    };
    widget::button::custom(content)
        .padding([0, 2])
        .class(theme::Button::Custom {
            active: Box::new(move |_, _| style(0.0, false)),
            hovered: Box::new(move |_, _| style(0.08, false)),
            pressed: Box::new(move |_, _| style(0.16, false)),
            disabled: Box::new(move |_| style(0.0, true)),
        })
}

/// Dica ao passar o mouse, como as do shell do Omarchy.
fn tip<'a>(content: impl Into<Element<'a, Message>>, text: impl Into<Cow<'a, str>> + 'a) -> Element<'a, Message> {
    let bubble = widget::container(mono(text))
        .padding([2, 6])
        .class(theme::Container::custom(|_| {
            let palette = themes::current();
            widget::container::Style {
                background: Some(Color { a: 0.97, ..palette.background }.into()),
                text_color: Some(palette.text),
                border: Border {
                    color: palette.text,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        }));
    tooltip::tooltip(content, bubble, tooltip::Position::Bottom).into()
}

/// Botão de ícone (Nerd Font), apagado até passar o mouse; sem mensagem, fica desabilitado.
fn icon_button<'a>(icon: &'static str, help: &'static str, on_press: Option<Message>) -> Element<'a, Message> {
    let palette = themes::current();
    let color = if on_press.is_some() { palette.dim } else { Color { a: 0.35, ..palette.dim } };
    tip(
        row_button(tinted(icon, color), false).padding([1, 5]).on_press_maybe(on_press),
        help,
    )
}

/// Botão de texto com borda, como os controles do shell do Omarchy (borda do texto a 40%).
fn text_button<'a>(label: &'static str, help: &'static str, on_press: Option<Message>) -> Element<'a, Message> {
    let enabled = on_press.is_some();
    let style = move |hover: f32| {
        let palette = themes::current();
        let text = if enabled { palette.text } else { palette.text_alpha(0.4) };
        widget::button::Style {
            background: Some(palette.text_alpha(0.04 + hover).into()),
            border_radius: 0.0.into(),
            border_width: 1.0,
            border_color: palette.text_alpha(if enabled { 0.4 } else { 0.15 }),
            text_color: Some(text),
            icon_color: Some(text),
            ..widget::button::Style::new()
        }
    };
    tip(
        widget::button::custom(mono(label))
            .padding([2, 10])
            .class(theme::Button::Custom {
                active: Box::new(move |_, _| style(0.0)),
                hovered: Box::new(move |_, _| style(0.04)),
                pressed: Box::new(move |_, _| style(0.14)),
                disabled: Box::new(move |_| style(0.0)),
            })
            .on_press_maybe(on_press),
        help,
    )
}

/// Opção de um grupo (agente, lugar): a escolhida com o texto no acento e a borda da seleção.
fn choice<'a>(label: String, selected: bool, on_press: Message) -> Element<'a, Message> {
    let palette = themes::current();
    let text = if selected { tinted(label, palette.accent) } else { mono(label) };
    row_button(text, selected).padding([2, 8]).on_press(on_press).into()
}

/// `?` com a lista de atalhos de teclado na dica.
fn shortcuts_help<'a>() -> Element<'a, Message> {
    let palette = themes::current();
    let mut list = widget::Column::new();
    for (keys, shortcut) in shortcuts::LIST {
        list = list.push(mono(format!("{keys:<14}{}", shortcut.label())));
    }
    let bubble = widget::container(list)
        .padding([4, 8])
        .class(theme::Container::custom(|_| {
            let palette = themes::current();
            widget::container::Style {
                background: Some(Color { a: 0.97, ..palette.background }.into()),
                text_color: Some(palette.text),
                border: Border {
                    color: palette.text,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        }));
    tooltip::tooltip(
        widget::container(tinted(ICON_HELP, palette.dim)).padding([1, 5]),
        bubble,
        tooltip::Position::Bottom,
    )
    .into()
}

fn show_sidebar_button<'a>() -> Element<'a, Message> {
    icon_button(ICON_HIDE_RIGHT, "Mostrar o painel (Ctrl+Shift+1)", Some(Message::ShowSection(Section::Sidebar)))
}

/// Última pasta do caminho (`~/projects/chube` → `chube`); a home vira `~`.
fn dir_name(path: &std::path::Path) -> String {
    if path == claude::home_dir() {
        return "~".into();
    }
    path.file_name()
        .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// Ícone e cor de cada status, com as cores do tema do Omarchy.
fn status_glyph(status: Status) -> (&'static str, Color) {
    let palette = themes::current();
    match status {
        Status::Suspended => ("\u{f04c}", palette.dim),
        Status::Starting => ("\u{f10c}", palette.dim),
        Status::Working => ("\u{f110}", palette.accent),
        Status::Waiting => ("\u{f0f3}", palette.warning),
        Status::Ready => ("\u{f111}", palette.success),
        Status::Exited => (ICON_EXITED, palette.dim),
    }
}

/// Ícone do modo, como o símbolo do rodapé da TUI (⏸ / ⏵⏵).
fn mode_glyph(mode: Mode) -> &'static str {
    match mode {
        Mode::Manual | Mode::Plan => "\u{f04c}",
        Mode::AcceptEdits | Mode::Auto | Mode::Bypass => "\u{f04e}",
    }
}

/// O terminal de um chat ou de uma aba, sem cantos arredondados.
/// Opacidade 0 no fundo padrão: fica o da janela (translúcido no modo vidro); células com
/// cor de fundo própria continuam pintadas.
fn terminal_view(terminal: &Mutex<Terminal>, id: widget::Id) -> Element<'_, Message> {
    term::terminal_box(terminal, &term::KEY_BINDS)
        .id(id)
        .padding(theme::spacing().space_xxxs)
        .pane_border_radius(Some(0.0.into()))
        .opacity(0.0)
        .on_open_hyperlink(Some(Box::new(Message::OpenLink)))
        .into()
}

/// Rótulo, detalhe e uma barra fina, como os medidores do shell do Omarchy:
///
/// ```text
/// contexto            84,2k / 200k  42%
/// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// └ reinicia em 3 h 1 min
/// ```
fn meter<'a>(
    label: impl Into<Cow<'a, str>> + 'a,
    value: Option<f32>,
    detail: Option<String>,
    note: Option<String>,
) -> Element<'a, Message> {
    let palette = themes::current();
    let value = value.map(|value| value.clamp(0.0, 1.0));
    let color = meter_color(value.unwrap_or(0.0));
    let mut head = widget::Row::new().push(mono(label).width(Length::Fill));
    if let Some(detail) = detail {
        head = head.push(tinted(format!("{detail}  "), palette.dim));
    }
    head = head.push(match value {
        Some(value) => tinted(format!("{:.0}%", value * 100.0), color),
        None => tinted("—", palette.dim),
    });
    // A quantidade de traços depende da largura disponível.
    let bar = widget::responsive(move |size| line_bar(value, size.width)).height(Length::Shrink);
    let mut column = widget::Column::new().push(head).push(bar);
    if let Some(note) = note {
        column = column.push(tinted(format!("└ {note}"), palette.dim));
    }
    column.into()
}

fn meter_color(ratio: f32) -> Color {
    let palette = themes::current();
    if ratio >= 0.9 {
        palette.danger
    } else if ratio >= 0.7 {
        palette.warning
    } else {
        palette.accent
    }
}

/// Barra de traços: a parte usada na cor do nível, o resto na cor de seleção do tema.
fn line_bar<'a>(value: Option<f32>, width: f32) -> Element<'a, Message> {
    let palette = themes::current();
    let cells = ((width / (text_size() * MONO_ADVANCE)) as usize).saturating_sub(1).max(8);
    let ratio = value.unwrap_or(0.0);
    let mut filled = ((ratio * cells as f32).round() as usize).min(cells);
    if ratio > 0.0 {
        filled = filled.max(1);
    }
    widget::Row::new()
        .push(tinted("━".repeat(filled), meter_color(ratio)).wrapping(Wrapping::None))
        .push(tinted("━".repeat(cells - filled), palette.selection).wrapping(Wrapping::None))
        .into()
}

/// Teclas do campo de mensagem. Enter envia; Esc e Shift+Tab vão direto para a TUI
/// (interromper, trocar de modo); Tab completa o comando `/` sugerido; com o campo vazio,
/// as setas também vão para a TUI (escolher uma opção num pedido de permissão).
fn composer_key(
    press: text_editor::KeyPress,
    empty: bool,
    suggestion: Option<&str>,
) -> Option<text_editor::Binding<Message>> {
    use text_editor::Binding;

    if !matches!(press.status, text_editor::Status::Focused { .. }) {
        return None;
    }
    let shift = press.modifiers.shift();
    let custom = |message| Some(Binding::Custom(message));
    match press.key.as_ref() {
        Key::Named(Named::Enter) if !shift => custom(Message::ComposerSend),
        Key::Named(Named::Escape) => custom(Message::ComposerKey(b"\x1b")),
        Key::Named(Named::Tab) if shift => custom(Message::ComposerKey(claude::NEXT_MODE_KEY)),
        Key::Named(Named::Tab) => suggestion.map(|name| Binding::Custom(Message::ComposerComplete(name.to_owned()))),
        Key::Named(Named::ArrowUp) if empty => custom(Message::ComposerKey(b"\x1b[A")),
        Key::Named(Named::ArrowDown) if empty => custom(Message::ComposerKey(b"\x1b[B")),
        _ => Binding::from_key_press(press),
    }
}

/// Espera o `claude` do chat fechado sair (ao receber o SIGHUP, ele grava a sessão) e abre
/// a sessão no terminal do Omarchy, numa janela de agente (`org.omarchy.agent`, como as do
/// `omarchy agent`), na pasta do chat.
async fn open_in_terminal(
    pid: Option<u32>,
    cwd: PathBuf,
    (program, args): (String, Vec<String>),
) -> Result<(), String> {
    if let Some(pid) = pid {
        let step = Duration::from_millis(50);
        let mut waited = Duration::ZERO;
        while waited < EXIT_WAIT && process_running(pid) {
            tokio::time::sleep(step).await;
            waited += step;
        }
    }
    // O `omarchy-launch-tui` passa pelo `uwsm-app`, que não leva a pasta atual: o `sh` entra nela.
    let mut child = tokio::process::Command::new("omarchy-launch-tui")
        .arg("--app-id=org.omarchy.agent")
        .args(["sh", "-c", "cd \"$0\" && exec \"$@\""])
        .arg(&cwd)
        .arg(program)
        .args(args)
        .current_dir(&cwd)
        .spawn()
        .map_err(|error| error.to_string())?;
    // O lançador se separa e sai logo; o `wait` evita deixar um processo zumbi.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}

/// O processo existe e não é um zumbi esperando ser recolhido.
fn process_running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        // O estado vem depois do nome, que fica entre parênteses e pode ter espaços.
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|state| state != "Z")
    })
}

/// Relê o modo na tela um pouco depois do Shift+Tab.
fn mode_step_later(id: term::TabId) -> Task<Message> {
    cosmic::task::future(async move {
        tokio::time::sleep(MODE_STEP_DELAY).await;
        Message::ModeStep(id)
    })
}

/// Recebe os eventos de todos os terminais (vêm das threads do PTY).
fn term_events() -> impl cosmic::iced::futures::Stream<Item = Message> {
    cosmic::iced::stream::channel(100, async |mut output| {
        use cosmic::iced::futures::SinkExt;

        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let _ = output.send(Message::TermEventTx(sender)).await;
        while let Some((id, event)) = receiver.recv().await {
            let _ = output.send(Message::TermEvent(id, event)).await;
        }
    })
}

/// Eventos do emulador comuns aos terminais dos chats e às abas.
fn terminal_io(terminal: &Mutex<Terminal>, event: term::Event) -> Task<Message> {
    use alacritty_terminal::term::ClipboardType;

    match event {
        term::Event::Wakeup | term::Event::MouseCursorDirty => {
            terminal.lock().unwrap().needs_update = true;
        }
        term::Event::PtyWrite(text) => {
            terminal.lock().unwrap().input_no_scroll(text.into_bytes());
        }
        term::Event::ColorRequest(index, format) => {
            let terminal = terminal.lock().unwrap();
            let text = format(terminal.effective_color(index));
            terminal.input_no_scroll(text.into_bytes());
        }
        term::Event::TextAreaSizeRequest(format) => {
            let terminal = terminal.lock().unwrap();
            let text = format(terminal.size().into());
            terminal.input_no_scroll(text.into_bytes());
        }
        term::Event::ClipboardStore(ClipboardType::Clipboard, data) => {
            return cosmic::iced::clipboard::write(data);
        }
        term::Event::ClipboardLoad(ClipboardType::Clipboard, format) => {
            return cosmic::iced::clipboard::read().map(move |data| {
                let _ = format(&data.unwrap_or_default());
                cosmic::action::none()
            });
        }
        _ => {}
    }
    Task::none()
}

fn horizontal_fill() -> widget::Space {
    widget::Space::new().width(Length::Fill)
}

fn now_hms() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn short_title(text: &str) -> String {
    short_text(text.lines().next().unwrap_or_default(), TITLE_MAX_CHARS)
}

fn short_text(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max).collect::<String>())
    } else {
        text.to_owned()
    }
}

/// `84,2k`, `200k`, `1m`.
fn short_tokens(tokens: u32) -> String {
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
    fn process_running_ve_processos_vivos_e_zumbis() {
        assert!(process_running(std::process::id()));
        assert!(!process_running(u32::MAX));

        // Um filho que saiu e ainda não foi recolhido é zumbi: conta como encerrado.
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!process_running(pid));
        child.wait().unwrap();
        assert!(!process_running(pid));
    }

    #[test]
    fn short_tokens_abrevia() {
        assert_eq!(short_tokens(950), "950");
        assert_eq!(short_tokens(84_230), "84,2k");
        assert_eq!(short_tokens(200_000), "200k");
        assert_eq!(short_tokens(1_000_000), "1m");
    }
}
