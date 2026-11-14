use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::core::Auto;
use cosmic::iced::keyboard::{Key, key::Named};
use cosmic::iced::font::Weight;
use cosmic::iced::widget::scrollable;
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Alignment, Background, Border, Color, Font, Length, Radians, Shadow, Subscription};
use cosmic::widget::{self, pane_grid, segmented_button, text_editor, tooltip};
use cosmic::{Application, ApplicationExt, Element, theme};

use crate::agent::Agent;
use crate::codex;
use crate::chat::{Chat, ModeChange, Status};
use crate::claude::{self, Mode, Start};
use crate::commands::{self, Command};
use crate::dirs;
use crate::restore::{self, OpenChat, OpenChats};
use crate::sessions::{self, Running, SavedSession, Stats};
use crate::shortcuts::{self, Shortcut};
use crate::term::{self, Terminal};
use crate::themes::{self, Theme};
use crate::usage::{self, Limit, Window};

/// Intervalo de atualização automática do painel de uso.
const REFRESH_EVERY: Duration = Duration::from_secs(180);
/// Intervalo da leitura do status das sessões (`~/.claude/sessions`).
const POLL_EVERY: Duration = Duration::from_secs(1);
const TITLE_MAX_CHARS: usize = 40;
/// Espera entre colar a mensagem e mandar o Enter: junto da colagem, a TUI trataria o
/// Enter como parte do texto.
const SUBMIT_DELAY: Duration = Duration::from_millis(80);
/// Espera entre um Shift+Tab e a leitura do modo na tela.
const MODE_STEP_DELAY: Duration = Duration::from_millis(250);
/// Tempo máximo esperando o `claude` de um chat sair antes de abrir a sessão no COSMIC Terminal.
const EXIT_WAIT: Duration = Duration::from_secs(5);
/// Mais Shift+Tab do que isso e o modo pedido não está no ciclo.
const MODE_MAX_STEPS: u8 = 6;
/// Altura máxima do campo de mensagem; acima disso, ele rola.
const COMPOSER_MAX_HEIGHT: f32 = 360.0;
/// Tamanho do texto da interface, toda em fonte mono, como no terminal.
const TEXT_SIZE: f32 = 13.0;
/// Largura de um caractere da fonte mono, em relação ao tamanho (0,6 na Noto Sans Mono e na
/// maioria das mono); usada para saber quantos caracteres cabem numa barra.
const MONO_ADVANCE: f32 = 0.6;
const TAB_TITLE_MAX_CHARS: usize = 24;
/// Altura do cabeçalho das seções: duas linhas, como o do chat (título e pasta/modo).
const HEADER_HEIGHT: f32 = 2.0 * TEXT_SIZE * 1.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Sidebar,
    Chat,
    Terminal,
}

#[derive(Clone, Debug)]
pub enum Message {
    SelectChat(usize),
    NewChat,
    NewChatInput(String),
    NewChatAgent(usize),
    NewChatConfirm,
    NewChatCancel,
    NewChatBrowse,
    NewChatPicked(Option<PathBuf>),
    ToggleDirGroup(PathBuf),
    ShowSessions,
    CloseSessions,
    SessionsLoaded(Vec<SavedSession>),
    ResumeSession(usize),
    SubChat,
    ReopenChat,
    CloseChat(usize),
    /// Leva a sessão do chat em foco para o COSMIC Terminal.
    OpenInTerminal,
    /// Se a sessão do chat já tem `.jsonl` (sem ele, o `--resume` falha).
    OpenInTerminalChecked(term::TabId, bool),
    OpenedInTerminal(Result<(), String>),
    SetMode(usize),
    ModeMenu(bool),
    /// Tema escolhido (índice em `Theme::ALL`).
    SetTheme(usize),
    ThemeMenu(bool),
    /// Liga/desliga o desfoque do fundo.
    ToggleBlur,
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
    OpenLink(String),
    RefreshInfo,
    UsageFetched(Result<Vec<Limit>, String>),
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
    DragWindow,
    ToggleMaximize,
    Minimize,
    CloseWindow,
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

/// Opções da linha de comando.
#[derive(Clone, Debug, Default)]
pub struct Flags {
    pub agent: Agent,
    /// Modelo usado nos chats novos (padrão: o do Claude Code).
    pub model: Option<String>,
}

pub struct App {
    core: Core,
    chats: Vec<Chat>,
    focused: usize,
    default_dir: PathBuf,
    model: Option<String>,
    default_agent: Agent,
    new_chat_agent: Agent,
    /// Id do campo de mensagem do chat (hoje fora da tela).
    composer_id: widget::Id,
    /// Ids dos terminais na tela (o do chat em foco e o da aba ativa), para dar o foco a eles.
    chat_terminal_id: widget::Id,
    tab_terminal_id: widget::Id,
    /// Id da rolagem horizontal das abas do terminal.
    terminal_tabs_id: widget::Id,
    polling: bool,
    panes: pane_grid::State<Section>,
    sidebar_visible: bool,
    chat_visible: bool,
    terminal_visible: bool,
    /// Proporções das divisões, guardadas para refazer o layout ao esconder/mostrar seções.
    sidebar_ratio: f32,
    chat_ratio: f32,
    /// Texto do diálogo de nova sessão; `None` com o diálogo fechado.
    new_chat_input: Option<String>,
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
    /// Sessões salvas mostradas no painel lateral; `None` enquanto carrega.
    saved_sessions: Option<Vec<SavedSession>>,
    /// Diretórios abertos na lista de sessões salvas.
    expanded_dirs: HashSet<PathBuf>,
    /// Menu do seletor de modo aberto.
    mode_menu: bool,
    /// Menu de temas aberto.
    theme_menu: bool,
    /// Aviso mostrado abaixo do cabeçalho do chat (ex.: falha ao abrir no COSMIC Terminal).
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

        let mut app = App {
            core,
            chats: Vec::new(),
            focused: 0,
            default_dir,
            model: flags.model,
            default_agent: flags.agent,
            new_chat_agent: flags.agent,
            composer_id: widget::Id::unique(),
            chat_terminal_id: widget::Id::unique(),
            tab_terminal_id: widget::Id::unique(),
            terminal_tabs_id: widget::Id::unique(),
            polling: false,
            panes: pane_grid::State::new(Section::Chat).0,
            sidebar_visible: true,
            chat_visible: true,
            terminal_visible: true,
            sidebar_ratio: 0.22,
            chat_ratio: 0.64,
            new_chat_input: None,
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
            expanded_dirs: HashSet::new(),
            mode_menu: false,
            theme_menu: false,
            notice: None,
            open_chats: OpenChats::default(),
        };
        // Sem a barra de título: o conteúdo ocupa a janela toda. Os cabeçalhos das seções
        // arrastam a janela, e os botões dela ficam no cabeçalho da seção da direita.
        app.core.window.show_headerbar = false;
        app.core.window.content_container = false;
        app.rebuild_panes();
        // Volta a lista de chats da última execução, em espera; o do foco abre quando o canal de
        // eventos dos terminais chegar (`Message::TermEventTx`).
        let open = restore::load();
        for saved in &open.chats {
            let id = app.take_terminal_id();
            let has_file = saved.agent.session_file(&saved.session_id).is_some();
            app.chats.push(Chat::restore(id, saved, has_file));
        }
        app.focused = open.focused.min(app.chats.len().saturating_sub(1));
        app.open_chats = open;

        let title = match app.core.main_window_id() {
            Some(id) => app.set_window_title("C-hub".into(), id),
            None => Task::none(),
        };
        let usage = app.refresh_usage();
        // O tema escolhido vale antes de abrir qualquer terminal (as cores vêm dele).
        let theme = Theme::load();
        theme.make_current();
        themes::set_blur(themes::load_blur());
        app.apply_blur();
        let theme = match theme {
            Theme::System => Task::none(),
            theme => cosmic::command::set_theme(theme.cosmic()),
        };
        (app, Task::batch([title, usage, theme]))
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
        if self.chats.iter().any(Chat::is_alive) {
            subscriptions.push(cosmic::iced::time::every(POLL_EVERY).map(|_| Message::Poll));
        }
        Subscription::batch(subscriptions)
    }

    fn context_drawer(&self) -> Option<cosmic::app::ContextDrawer<'_, Message>> {
        if !self.core.window.show_context {
            return None;
        }
        Some(
            cosmic::app::context_drawer(self.sessions_list(), Message::CloseSessions)
                .title("Sessões salvas"),
        )
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        let input = self.new_chat_input.as_ref()?;
        let space = theme::spacing();
        let resolved = dirs::resolve(input, &self.focused_dir());

        let preview = match &resolved {
            Ok(path) => format!("→ {}", path.display()),
            Err(error) => error.clone(),
        };
        let names: Vec<String> = dirs::shortcuts().into_iter().map(|(name, _)| name).collect();
        let mut control = widget::Column::new()
            .push(widget::dropdown(&Agent::LABELS, Some(self.new_chat_agent.index()), Message::NewChatAgent))
            .push(
                widget::text_input("Diretório, atalho do fshort ou .", input)
                    .id(self.new_chat_input_id.clone())
                    .on_input(Message::NewChatInput)
                    .on_submit(|_| Message::NewChatConfirm),
            )
            .push(widget::text::caption(preview))
            .spacing(space.space_xs);
        if !names.is_empty() {
            control = control.push(widget::text::caption(format!(
                "Atalhos: {}",
                names.join(", ")
            )));
        }

        Some(
            widget::dialog()
                .title("Nova sessão")
                .body(format!(
                    "\".\" usa a pasta do chat em foco ({}).",
                    claude::display_path(&self.focused_dir())
                ))
                .control(control)
                .primary_action(
                    widget::button::suggested("Criar")
                        .on_press_maybe(resolved.is_ok().then_some(Message::NewChatConfirm)),
                )
                .secondary_action(widget::button::standard("Cancelar").on_press(Message::NewChatCancel))
                .tertiary_action(
                    widget::button::text("Escolher pasta…").on_press(Message::NewChatBrowse),
                )
                .into(),
        )
    }

    /// Nos temas próprios, a cor padrão do texto é a da paleta. O fundo fica transparente
    /// (é pintado no `view`), também com a janela maximizada.
    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        let palette = Theme::current().palette()?;
        Some(cosmic::iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: palette.text,
            icon_color: palette.text,
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let sharp = self.core.window.sharp_corners;

        widget::container(
            pane_grid::PaneGrid::new(&self.panes, |_pane, section, _maximized| {
                pane_grid::Content::new(self.section_view(*section))
            })
            .spacing(space.space_xxs)
            .min_size(180)
            .on_resize(10, Message::PaneResized),
        )
        .padding(space.space_xxs)
        // Sem o `content_container` do libcosmic, o fundo da janela é daqui. Com os mesmos cantos
        // que o libcosmic dá à janela: sem eles, o fundo sai quadrado para fora da borda redonda.
        .class(theme::Container::custom(move |theme| widget::container::Style {
            background: Some(window_background(theme)),
            border: Border {
                radius: if sharp { 0.0 } else { window_corner_radius(theme) }.into(),
                ..Default::default()
            },
            ..Default::default()
        }))
        .into()
    }
}

impl App {
    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {            Message::SelectChat(index) => {
                self.notice = None;
                if index < self.chats.len() {
                    self.focused = index;
                    return self.focus_chat();
                }
            }
            Message::NewChat => {
                self.new_chat_agent = self.chats.get(self.focused).map_or(self.default_agent, |chat| chat.agent);
                self.new_chat_input = Some(String::new());
                return widget::text_input::focus(self.new_chat_input_id.clone());
            }
            Message::NewChatInput(text) => self.new_chat_input = Some(text),
            Message::NewChatAgent(index) => {
                if let Some(agent) = Agent::ALL.get(index) { self.new_chat_agent = *agent; }
            },
            Message::NewChatCancel => self.new_chat_input = None,
            Message::NewChatConfirm => {
                let Some(input) = &self.new_chat_input else {
                    return Task::none();
                };
                if let Ok(folder) = dirs::resolve(input, &self.focused_dir()) {
                    self.new_chat_input = None;
                    let id = self.take_terminal_id();
                    return self.open_chat(self.chats.len(), Chat::new(id, self.new_chat_agent, folder, self.new_model(self.new_chat_agent), None));
                }
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
                self.new_chat_input = Some(folder.display().to_string());
            }
            Message::NewChatPicked(None) => {}
            Message::ToggleDirGroup(dir) => {
                if !self.expanded_dirs.remove(&dir) {
                    self.expanded_dirs.insert(dir);
                }
            }
            Message::ShowSessions => {
                self.saved_sessions = None;
                self.expanded_dirs = HashSet::from([self.focused_dir()]);
                self.set_show_context(true);
                return cosmic::task::future(async {
                    let list = tokio::task::spawn_blocking(|| {
                        let mut list = sessions::list();
                        list.extend(codex::list());
                        list.sort_by(|a,b| b.modified.cmp(&a.modified));
                        list
                    })
                        .await
                        .unwrap_or_default();
                    Message::SessionsLoaded(list)
                });
            }
            Message::CloseSessions => self.set_show_context(false),
            Message::SessionsLoaded(list) => self.saved_sessions = Some(list),
            Message::ResumeSession(index) => {
                let Some(saved) = self
                    .saved_sessions
                    .as_ref()
                    .and_then(|list| list.get(index))
                    .cloned()
                else {
                    return Task::none();
                };
                self.set_show_context(false);
                if let Some(open) = self.chat_index_for_session(saved.agent, &saved.id) {
                    self.focused = open;
                    return self.focus_chat();
                }
                let id = self.take_terminal_id();
                return self.open_chat(self.chats.len(), Chat::resume(id, &saved, self.new_model(saved.agent)));
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
                let Some(session_id) = chat.session_id.clone() else {
                    return Task::none();
                };
                let id = chat.id;
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
                let Some(session_id) = chat.session_id.clone().filter(|_| saved) else {
                    self.notice = Some("a sessão ainda não foi salva: mande uma mensagem antes.".into());
                    return Task::none();
                };
                let pid = chat.pid();
                let cwd = chat.cwd.clone();
                let command = chat.agent.command(&Start::Resume { session_id }, chat.model.as_deref(), None);
                let close = self.close_chat(index);
                let open = cosmic::task::future(async move {
                    Message::OpenedInTerminal(open_in_terminal(pid, cwd, command).await)
                });
                return Task::batch([close, open]);
            }
            Message::OpenedInTerminal(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("não consegui abrir o COSMIC Terminal: {error}"));
                }
            }
            Message::ModeMenu(open) => self.mode_menu = open,
            Message::ThemeMenu(open) => self.theme_menu = open,
            Message::SetTheme(index) => {
                self.theme_menu = false;
                let Some(chosen) = Theme::ALL.get(index).copied() else {
                    return Task::none();
                };
                chosen.make_current();
                if let Err(error) = chosen.save() {
                    log::error!("não consegui gravar o tema: {error}");
                }
                // Os terminais abertos trocam de cor na hora; os novos já nascem com o tema.
                let cosmic_theme = chosen.cosmic();
                let chats = self.chats.iter().filter_map(|chat| chat.terminal.as_ref());
                let tabs = self
                    .terminal_tabs
                    .iter()
                    .filter_map(|entity| self.terminal_tabs.data::<Mutex<Terminal>>(entity));
                for terminal in chats.chain(tabs) {
                    term::apply_theme(&mut terminal.lock().unwrap(), &cosmic_theme);
                }
                // Tema opaco não pede desfoque à extensão do GNOME (gastaria a GPU à toa).
                self.apply_blur();
                return cosmic::command::set_theme(cosmic_theme);
            }
            Message::ToggleBlur => {
                let on = !themes::blur();
                themes::set_blur(on);
                if let Err(error) = themes::save_blur(on) {
                    log::error!("não consegui gravar o desfoque: {error}");
                }
                self.apply_blur();
                // Reaplicar o tema faz o libcosmic refazer o pedido de blur da janela.
                return cosmic::command::set_theme(Theme::current().cosmic());
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
            Message::OpenLink(url) => {
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
            }
            Message::RefreshInfo => return self.refresh_usage(),
            Message::UsageFetched(result) => {
                self.fetching_usage = false;
                match result {
                    Ok(limits) => {
                        self.limits = limits;
                        self.usage_error = None;
                        self.usage_updated = Some(now_hms());
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
            Message::DragWindow => return self.core.drag(None),
            Message::ToggleMaximize => return self.core.toggle_maximize(None),
            Message::Minimize => return self.core.minimize(None),
            Message::CloseWindow => {
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::close(id);
                }
            }
        }
        Task::none()
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

    /// Busca os limites com o `/usage`, se já não houver uma busca em andamento.
    fn refresh_usage(&mut self) -> Task<Message> {
        if self.chats.get(self.focused).map_or(self.default_agent, |chat| chat.agent) == Agent::Codex {
            return self.poll();
        }
        if self.fetching_usage {
            return Task::none();
        }
        self.fetching_usage = true;
        cosmic::task::future(async { Message::UsageFetched(usage::fetch().await) })
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

    // --model se aplica apenas ao agente escolhido na linha de comando.
    fn new_model(&self, agent: Agent) -> Option<String> {
        (agent == self.default_agent).then(|| self.model.clone()).flatten()
    }

    fn focused_dir(&self) -> PathBuf {
        self.chats
            .get(self.focused)
            .map_or_else(|| self.default_dir.clone(), |chat| chat.cwd.clone())
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
        let command = chat.agent.command(&chat.start, chat.model.as_deref(), chat.prompt.take().as_deref());
        match term::spawn(chat.id, events, chat.cwd.clone(), Some(command), &theme::active()) {
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
            .filter(|(_, chat)| chat.session_id.is_some() || matches!(chat.start, Start::New { .. }))
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
                    session_id: chat.session_id.clone().unwrap_or_else(|| format!("draft:{}", chat.id)),
                    cwd: chat.cwd.clone(),
                    title: chat.title.clone(),
                    model: chat.model.clone(),
                    parent: chat.parent.clone(),
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
            .filter(|chat| chat.is_alive())
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
            Section::Chat => self.chat_section(),
            Section::Terminal => self.terminal_section(),
        }
    }

    fn chats_card(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let colors = Colors::get();

        let mut list = widget::Column::new();
        for (index, chat) in self.chats.iter().enumerate() {
            let selected = index == self.focused;
            let (glyph, color) = status_glyph(chat.status, &colors);
            let title = if selected {
                bold(chat.title.as_str()).class(theme::Text::Color(colors.accent))
            } else {
                mono(chat.title.as_str())
            };
            let first = widget::Row::new()
                .push(tinted(if selected { "> " } else { "  " }, colors.accent))
                .push(tinted(format!("{glyph} "), color))
                // Cortado no container: sem ele, o texto sem quebra passa por baixo do `[x]`.
                .push(widget::container(title.wrapping(Wrapping::None)).width(Length::Fill).clip(true))
                .push(ascii_button("[x]", "Fechar este chat (como /exit)", Some(Message::CloseChat(index))))
                .align_y(Alignment::Center);
            // Trabalhando / precisa de você ficam na cor do status; os outros, apagados.
            let status_color = match chat.status {
                Status::Working | Status::Waiting => color,
                _ => colors.dim,
            };
            let second = widget::Row::new()
                .push(tinted(format!("    {} · {} · ", chat.agent.label(), claude::display_path(&chat.cwd)), colors.dim).wrapping(Wrapping::None))
                .push(tinted(chat.status.label(), status_color).wrapping(Wrapping::None));
            let item = widget::container(widget::Column::new().push(first).push(second))
                .width(Length::Fill)
                .clip(true);
            list = list.push(
                flat_button(item, selected)
                    .padding([space.space_xxxs, 0])
                    .width(Length::Fill)
                    .on_press(Message::SelectChat(index)),
            );
        }

        let header = widget::Row::new()
            .push(heading("chats"))
            .push(tinted(format!(" [{}]", self.chats.len()), colors.dim))
            .push(horizontal_fill())
            .push(ascii_button("[+]", "Nova sessão (Ctrl+Shift+N)", Some(Message::NewChat)))
            .push(ascii_button("[≡]", "Sessões salvas", Some(Message::ShowSessions)))
            .push(shortcuts_help())
            .push(ascii_button(
                "[«]",
                "Esconder o painel (Ctrl+Shift+1)",
                Some(Message::HideSection(Section::Sidebar)),
            ))
            .spacing(space.space_xxxs)
            .align_y(Alignment::Center);

        card(
            widget::Column::new()
                .push(section_header(header))
                .push(widget::scrollable(list).height(Length::Fill))
                .spacing(space.space_xxs),
        )
        .height(Length::FillPortion(3))
        .into()
    }

    fn info_card(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let colors = Colors::get();
        let focused = self.chats.get(self.focused);
        let context_tokens = focused.map_or(0, |chat| chat.context_tokens);
        let is_codex = focused.is_some_and(|chat| chat.agent == Agent::Codex);
        let limits = if is_codex { focused.map(|chat| chat.codex_limits.as_slice()).unwrap_or_default() } else { &self.limits };
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
            .push(heading("uso"))
            .push(horizontal_fill())
            // Aqui e não em "# chats", que corta os últimos botões com o painel estreito.
            .push(self.theme_selector())
            .push(ascii_button(
                "[↻]",
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
                .push(meter("limite 5h", None, None, None))
                .push(meter("limite semanal", None, None, None));
        }
        for limit in limits {
            let label = match &limit.window {
                Window::Session => "limite 5h".to_owned(),
                Window::Week(models) if models == "all models" => "limite semanal".to_owned(),
                Window::Week(models) => format!("semanal ({models})"),
            };
            body = body.push(meter(
                label,
                Some(limit.used),
                None,
                limit.resets.map(usage::describe_reset),
            ));
        }

        let status = if is_codex {
            tinted("Codex · limites do histórico; /status na sessão", colors.dim)
        } else if self.fetching_usage {
            tinted("atualizando…", colors.dim)
        } else if let Some(error) = &self.usage_error {
            tinted(format!("erro ao atualizar: {error}"), colors.danger)
        } else if let Some(time) = &self.usage_updated {
            tinted(format!("atualizado {time} · a cada 3 min"), colors.dim)
        } else {
            tinted("ainda não atualizado", colors.dim)
        };
        body = body.push(
            widget::Column::new()
                .push(tinted(
                    format!("{} chats · {working} trabalhando", self.chats.len()),
                    colors.dim,
                ))
                .push(status),
        );

        card(
            widget::Column::new()
                .push(section_header(header))
                // Rola em vez de cortar quando a janela é baixa.
                .push(
                    widget::scrollable(body.padding([0, space.space_xs, 0, 0]))
                        .height(Length::Fill),
                )
                .spacing(space.space_xxs),
        )
        .height(Length::FillPortion(2))
        .into()
    }

    fn chat_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let colors = Colors::get();
        let Some(chat) = self.chats.get(self.focused) else {
            return self.no_chat_section();
        };

        let mut details = vec![chat.agent.label().to_owned(), claude::display_path(&chat.cwd)];
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
        let can_fork = chat.session_id.is_some() && chat.status != Status::Starting;
        let (glyph, color) = status_glyph(chat.status, &colors);
        header = header
            .push(tinted(format!("{glyph} "), color))
            .push(
                widget::container(bold(chat.title.as_str()).wrapping(Wrapping::None))
                    .width(Length::Fill)
                    .clip(true),
            )
            .push(ascii_button(
                "[fork]",
                "Criar sub-chat com o contexto deste chat",
                can_fork.then_some(Message::SubChat),
            ))
            .push(ascii_button(
                "[term]",
                "Continuar esta sessão no COSMIC Terminal (fecha o chat aqui)",
                can_fork.then_some(Message::OpenInTerminal),
            ))
            .push(ascii_button(
                "[exit]",
                "Fechar este chat (como /exit)",
                Some(Message::CloseChat(self.focused)),
            ))
            .push(self.chat_toggle_button());
        if !self.terminal_visible {
            header = header.push(window_controls());
        }
        let mut second = widget::Row::new().push(
            widget::container(tinted(format!("  {}", details.join(" · ")), colors.dim).wrapping(Wrapping::None))
                .width(Length::Fill)
                .clip(true),
        );
        if chat.is_alive() && chat.agent == Agent::Claude {
            second = second.push(self.mode_selector(chat.mode));
        }
        let header = widget::Column::new()
            .push(header.spacing(space.space_xxxs).align_y(Alignment::Center))
            .push(second.align_y(Alignment::Center));

        let mut column = widget::Column::new().push(section_header(header));
        if let Some(notice) = &self.notice {
            column = column.push(tinted(format!("! {notice}"), colors.warning));
        }
        if !chat.is_alive() {
            let text = if chat.terminal.is_some() {
                "o agente desta sessão foi encerrado."
            } else {
                "não consegui abrir o agente."
            };
            column = column.push(
                widget::Row::new()
                    .push(tinted(format!("× {text}"), colors.warning).width(Length::Fill))
                    .push(ascii_button(
                        "[reabrir sessão]",
                        "Retomar ou reiniciar esta sessão",
                        Some(Message::ReopenChat),
                    ))
                    .align_y(Alignment::Center),
            );
        }

        let body: Element<'_, Message> = match &chat.terminal {
            Some(terminal) => terminal_view(terminal, self.chat_terminal_id.clone()),
            None => widget::container(tinted(
                if chat.is_alive() { "iniciando o agente…" } else { "" },
                colors.dim,
            ))
            .center(Length::Fill)
            .into(),
        };

        card(column.push(body).spacing(space.space_xxs)).into()
    }

    /// Modo de permissão, como no rodapé da TUI; Shift+Tab no terminal também troca.
    fn mode_selector(&self, mode: Option<Mode>) -> Element<'_, Message> {
        let colors = Colors::get();
        let label = match mode {
            Some(mode) => format!("[{} {} ▾]", mode_glyph(mode), Mode::LABELS[mode.index()].to_lowercase()),
            None => "[modo ▾]".to_owned(),
        };
        let button = flat_button(mono(label), self.mode_menu)
            .on_press(Message::ModeMenu(!self.mode_menu));
        let mut popover = widget::popover(button)
            .position(widget::popover::Position::Bottom)
            .on_close(Message::ModeMenu(false));
        if self.mode_menu {
            let mut menu = widget::Column::new();
            for (index, candidate) in Mode::ALL.iter().enumerate() {
                let current = mode == Some(*candidate);
                let row = widget::Row::new()
                    .push(tinted(if current { "> " } else { "  " }, colors.accent))
                    .push(mono(format!("{} {}", mode_glyph(*candidate), Mode::LABELS[index].to_lowercase())));
                menu = menu.push(
                    flat_button(row, current)
                        .width(Length::Fill)
                        .on_press(Message::SetMode(index)),
                );
            }
            popover = popover.popup(menu_card(menu).width(Length::Fixed(220.0)));
        }
        popover.into()
    }

    /// Sem nenhum `Auto`, o libcosmic não pede o blur ao compositor nem usa as cores
    /// translúcidas do tema, mesmo com o "frosted" ligado nele. No GNOME, o pedido vai para a
    /// extensão `desfoque-chube@pedro`, por arquivo, com a intensidade do tema.
    fn apply_blur(&mut self) {
        self.core.set_auto_blur(if themes::blur() {
            Auto::System | Auto::Popup | Auto::Window
        } else {
            Default::default()
        });
        let request = match Theme::current().palette() {
            Some(palette) => palette.blur,
            None => {
                let theme = theme::active();
                (theme.cosmic().background(theme.transparent).base.alpha < 1.0).then_some(themes::DOCK_BLUR)
            }
        };
        // Com a janela maximizada, o libcosmic deixa os cantos retos, e a extensão também.
        let corner = window_corner_radius(&Theme::current().cosmic());
        if let Err(error) = themes::save_blur_request(request.filter(|_| themes::blur()), corner) {
            log::error!("não consegui gravar o pedido de desfoque: {error}");
        }
    }

    /// `[◐]` com o menu de temas e a opção do desfoque.
    fn theme_selector(&self) -> Element<'_, Message> {
        let colors = Colors::get();
        let current = Theme::current();
        let button = tooltip::tooltip(
            flat_button(mono("[◐]"), self.theme_menu).on_press(Message::ThemeMenu(!self.theme_menu)),
            widget::text::body(format!("Tema: {}", current.label())),
            tooltip::Position::Bottom,
        );
        let mut popover = widget::popover(button)
            .position(widget::popover::Position::Bottom)
            .on_close(Message::ThemeMenu(false));
        if self.theme_menu {
            let mut menu = widget::Column::new().push(tinted("tema", colors.dim));
            for (index, theme) in Theme::ALL.iter().enumerate() {
                let selected = *theme == current;
                let row = widget::Row::new()
                    .push(tinted(if selected { "> " } else { "  " }, colors.accent))
                    .push(mono(theme.label()));
                menu = menu.push(
                    flat_button(row, selected)
                        .width(Length::Fill)
                        .on_press(Message::SetTheme(index)),
                );
            }
            let blur = if themes::blur() { "[x] desfoque" } else { "[ ] desfoque" };
            menu = menu
                .push(tinted("fundo", colors.dim))
                .push(flat_button(mono(blur), false).width(Length::Fill).on_press(Message::ToggleBlur));
            popover = popover.popup(menu_card(menu).width(Length::Fixed(160.0)));
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

    /// Seção do chat depois de fechar o último.
    fn no_chat_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let colors = Colors::get();
        let mut header = widget::Row::new();
        if !self.sidebar_visible {
            header = header.push(show_sidebar_button());
        }
        header = header
            .push(heading("nenhum chat aberto"))
            .push(horizontal_fill())
            .push(self.chat_toggle_button())
            .spacing(space.space_xxxs)
            .align_y(Alignment::Center);
        if !self.terminal_visible {
            header = header.push(window_controls());
        }
        let notice = self
            .notice
            .as_ref()
            .map(|notice| tinted(format!("! {notice}"), colors.warning));
        let body = widget::container(
            widget::Column::new()
                .push_maybe(notice)
                .push(tinted("$ chube --novo", colors.dim))
                .push(ascii_button("[+] nova sessão", "Abrir Claude Code ou Codex numa pasta", Some(Message::NewChat)))
                .push(ascii_button("[≡] sessões salvas", "Retomar uma sessão", Some(Message::ShowSessions)))
                .spacing(space.space_xxs),
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
            ascii_button("[«]", "Esconder o chat (Ctrl+Shift+2)", Some(Message::HideSection(Section::Chat)))
        } else {
            ascii_button(
                "[>_]",
                "Mostrar o terminal (Ctrl+Shift+3)",
                Some(Message::ShowSection(Section::Terminal)),
            )
        }
    }

    fn sessions_list(&self) -> Element<'_, Message> {
        let colors = Colors::get();
        let Some(list) = &self.saved_sessions else {
            return tinted("carregando…", colors.dim).into();
        };
        if list.is_empty() {
            return tinted("nenhuma sessão salva encontrada.", colors.dim).into();
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
                .push(tinted(if expanded { "▾ " } else { "▸ " }, colors.accent))
                .push(bold(claude::display_path(dir)).width(Length::Fill))
                .push(tinted(format!("[{}]", indices.len()), colors.dim));
            column = column.push(
                flat_button(header, false)
                    .padding([theme::spacing().space_xxs, 0])
                    .width(Length::Fill)
                    .on_press(Message::ToggleDirGroup(dir.clone())),
            );
            if expanded {
                for index in indices {
                    column = column.push(self.saved_session_item(index, &list[index]));
                }
            }
        }
        column.into()
    }

    fn saved_session_item<'a>(&self, index: usize, saved: &'a SavedSession) -> Element<'a, Message> {
        let colors = Colors::get();
        let open_here = self.chat_index_for_session(saved.agent, &saved.id).is_some();
        let mut details = format!("{} · {}", saved.agent.label(), sessions::relative_time(saved.modified));
        let blocked = match (&saved.running, open_here) {
            (_, true) => {
                details.push_str(" · aberta aqui");
                false
            }
            (Some(_), false) => {
                details.push_str(" · aberta em outro terminal");
                true
            }
            (None, false) => false,
        };
        let running = self
            .chat_index_for_session(saved.agent, &saved.id)
            .map(|index| self.chats[index].status)
            .or_else(|| saved.running.as_deref().and_then(Status::from_claude));
        let (glyph, color) = match running {
            Some(status) => status_glyph(status, &colors),
            None => ("·", colors.dim),
        };
        let item = widget::Column::new()
            .push(
                widget::Row::new()
                    .push(tinted(format!("  {glyph} "), color))
                    .push(mono(saved.title.as_str())),
            )
            .push(tinted(format!("    {details}"), colors.dim));
        flat_button(item, false)
            .padding([theme::spacing().space_xxxs, 0])
            .width(Length::Fill)
            .on_press_maybe((!blocked).then_some(Message::ResumeSession(index)))
            .into()
    }

    fn terminal_section(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let colors = Colors::get();

        let toggle = if self.chat_visible {
            ascii_button("[»]", "Esconder o terminal (Ctrl+Shift+3)", Some(Message::HideSection(Section::Terminal)))
        } else {
            ascii_button("[«]", "Mostrar o chat (Ctrl+Shift+2)", Some(Message::ShowSection(Section::Chat)))
        };

        // Abas com o nome da pasta do shell e um × para fechar.
        let active = self.terminal_tabs.active();
        let mut tabs = widget::Row::new();
        for (entity, name) in self.terminal_tabs.iter().zip(self.terminal_tab_names()) {
            let selected = entity == active;
            let title = short_text(&name, TAB_TITLE_MAX_CHARS);
            let label = if selected {
                bold(title).class(theme::Text::Color(colors.accent))
            } else {
                mono(title)
            }
            .wrapping(Wrapping::None);
            tabs = tabs
                .push(flat_button(label, selected).on_press(Message::TerminalTabActivated(entity)))
                .push(
                    flat_button(tinted("×", colors.dim), selected)
                        .on_press(Message::TerminalTabClosed(entity)),
                )
                .push(mono(" "));
        }

        let mut header = widget::Row::new();
        if !self.sidebar_visible && !self.chat_visible {
            header = header.push(show_sidebar_button());
        }
        let header = header
            .push(toggle)
            .push(mono(" "))
            .push(
                // A roda do mouse (vertical) rola as abas na horizontal; Shift+roda o scrollable
                // já trata sozinho. Este `mouse_area` captura o clique esquerdo, que assim não
                // chega ao do `section_header`: o arrasto e o clique duplo são repetidos aqui.
                widget::mouse_area(
                    widget::scrollable(tabs)
                        .id(self.terminal_tabs_id.clone())
                        .direction(scrollable::Direction::Horizontal(
                            scrollable::Scrollbar::new().width(2).scroller_width(2),
                        ))
                        .width(Length::Fill),
                )
                .on_drag(Message::DragWindow)
                .on_double_click(Message::ToggleMaximize)
                .on_scroll(|delta| {
                    Message::TerminalTabsWheel(match delta {
                        cosmic::iced::mouse::ScrollDelta::Lines { y, .. } => -y * 60.0,
                        cosmic::iced::mouse::ScrollDelta::Pixels { y, .. } => -y,
                    })
                }),
            )
            .push(ascii_button("[+]", "Nova aba do terminal", Some(Message::NewTerminalTab)))
            .push(window_controls())
            .spacing(space.space_xxxs)
            .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.terminal_tabs.data::<Mutex<Terminal>>(active) {
            Some(terminal) => terminal_view(terminal, self.tab_terminal_id.clone()),
            None => widget::container(
                widget::Column::new()
                    .push(tinted("nenhum terminal aberto.", colors.dim))
                    .push(ascii_button("[+] abrir terminal", "Abrir um shell", Some(Message::NewTerminalTab)))
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

/// Cores do texto "de terminal": as da paleta do tema do Chube ou, no do sistema, as do COSMIC.
#[derive(Clone, Copy)]
struct Colors {
    text: Color,
    dim: Color,
    accent: Color,
    success: Color,
    warning: Color,
    danger: Color,
    /// Fundo dos botões ao passar o mouse e selecionados (com alfa do botão).
    highlight: Color,
}

impl Colors {
    fn get() -> Self {
        if let Some(palette) = Theme::current().palette() {
            return Colors {
                text: palette.text,
                dim: palette.dim,
                accent: palette.accent,
                success: palette.success,
                warning: palette.warning,
                danger: palette.danger,
                highlight: palette.highlight,
            };
        }
        let theme = theme::active();
        let cosmic = theme.cosmic();
        let text = Color::from(cosmic.on_bg_color());
        Colors {
            text,
            dim: Color { a: 0.5, ..text },
            accent: cosmic.accent_text_color().into(),
            success: cosmic.success_color().into(),
            warning: cosmic.warning_color().into(),
            danger: cosmic.destructive_color().into(),
            highlight: text,
        }
    }
}

/// Fundo da janela. No tema do sistema, o mesmo do terminal, que assim não destoa; nos
/// próprios, o da paleta (no neon e no vidro, um gradiente na diagonal). Com o desfoque
/// desligado, opaco.
fn window_background(theme: &cosmic::Theme) -> Background {
    let Some(palette) = Theme::current().palette() else {
        return Color::from(theme.cosmic().background(theme.transparent).base).into();
    };
    let solid = |color: Color| if themes::blur() { color } else { palette.solid(color) };
    match palette.window_end {
        Some(end) => cosmic::iced::gradient::Linear::new(Radians(2.4))
            .add_stop(0.0, solid(palette.window))
            .add_stop(1.0, solid(end))
            .into(),
        None => solid(palette.window).into(),
    }
}

/// Raio dos cantos da janela, com o mesmo cálculo do libcosmic (`window_corner_radius`, em
/// `app/mod.rs`), que assim alinha o conteúdo com a borda.
fn window_corner_radius(theme: &cosmic::Theme) -> f32 {
    let radius = theme.cosmic().radius_s()[0];
    if radius < 4.0 { radius } else { radius + 4.0 }
}

/// Cor das bordas e linhas: a da paleta ou um cinza mais claro que o fundo.
fn border_color(theme: &cosmic::Theme) -> Color {
    match Theme::current().palette() {
        Some(palette) => palette.border,
        None => Color { a: 0.22, ..Color::from(theme.cosmic().on_bg_color()) },
    }
}

/// Seção: cantos retos e borda fina. Sem fundo próprio: o da janela (translúcido quando o
/// tema tem blur) aparece por baixo, como nos espaços entre as seções. Pintar de novo
/// dobraria o alfa e a seção ficaria opaca. Exceção: o tema neon, que tinge as seções de leve
/// e acende um brilho em volta delas.
fn card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(theme::spacing().space_xxs)
        .width(Length::Fill)
        .height(Length::Fill)
        .class(theme::Container::custom(|theme| {
            let palette = Theme::current().palette();
            widget::container::Style {
                background: palette.and_then(|palette| palette.card).map(Background::from),
                border: Border {
                    color: border_color(theme),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                shadow: palette.and_then(|palette| palette.glow).map_or_else(Shadow::default, |color| Shadow {
                    color,
                    blur_radius: 16.0,
                    ..Default::default()
                }),
                ..Default::default()
            }
        }))
}

/// Menu suspenso (modo, tema): como uma seção, mas com fundo opaco, porque abre por cima
/// do conteúdo.
fn menu_card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message, cosmic::Theme> {
    card(content).height(Length::Shrink).class(theme::Container::custom(|theme| {
        let background = match Theme::current().palette() {
            Some(palette) => palette.solid(palette.window),
            None => theme.cosmic().background(false).base.into(),
        };
        widget::container::Style {
            background: Some(background.into()),
            border: Border {
                color: border_color(theme),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    }))
}

/// Cabeçalho de seção com uma linha embaixo. Arrasta a janela (que não tem barra de título)
/// e maximiza com clique duplo; os botões dentro dele continuam funcionando.
fn section_header<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let line = widget::container(widget::Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .class(theme::Container::custom(|theme| widget::container::Style {
            background: Some(border_color(theme).into()),
            ..Default::default()
        }));
    widget::Column::new()
        .push(
            widget::mouse_area(
                // Mesma altura em todas as seções, para as linhas de baixo ficarem alinhadas.
                widget::container(content)
                    .width(Length::Fill)
                    .height(Length::Fixed(HEADER_HEIGHT)),
            )
                .on_drag(Message::DragWindow)
                .on_double_click(Message::ToggleMaximize),
        )
        .push(line)
        .spacing(theme::spacing().space_xxs)
        .into()
}

fn mono<'a>(text: impl Into<Cow<'a, str>> + 'a) -> widget::Text<'a, cosmic::Theme, cosmic::Renderer> {
    widget::text(text).font(cosmic::font::mono()).size(TEXT_SIZE)
}

fn bold<'a>(text: impl Into<Cow<'a, str>> + 'a) -> widget::Text<'a, cosmic::Theme, cosmic::Renderer> {
    mono(text).font(Font {
        weight: Weight::Bold,
        ..cosmic::font::mono()
    })
}

fn tinted<'a>(text: impl Into<Cow<'a, str>> + 'a, color: Color) -> widget::Text<'a, cosmic::Theme, cosmic::Renderer> {
    mono(text).class(theme::Text::Color(color))
}

/// Título de seção, como um cabeçalho de markdown: `# chats`.
fn heading<'a>(text: &'a str) -> Element<'a, Message> {
    widget::Row::new()
        .push(tinted("# ", Colors::get().accent))
        .push(bold(text))
        .into()
}

/// Botão sem fundo nem cantos; o fundo aparece ao passar o mouse e quando selecionado.
fn flat_button<'a>(content: impl Into<Element<'a, Message>>, selected: bool) -> widget::Button<'a, Message> {
    let style = |_theme: &cosmic::Theme, alpha: f32, disabled: bool| {
        let colors = Colors::get();
        let text = if disabled { Color { a: 0.35, ..colors.text } } else { colors.text };
        widget::button::Style {
            background: (alpha > 0.0).then(|| Color { a: alpha, ..colors.highlight }.into()),
            border_radius: 0.0.into(),
            text_color: Some(text),
            icon_color: Some(text),
            ..widget::button::Style::new()
        }
    };
    let base = if selected { 0.10 } else { 0.0 };
    widget::button::custom(content)
        .padding([0, 2])
        .class(theme::Button::Custom {
            active: Box::new(move |_, theme| style(theme, base, false)),
            hovered: Box::new(move |_, theme| style(theme, base + 0.08, false)),
            pressed: Box::new(move |_, theme| style(theme, base + 0.16, false)),
            disabled: Box::new(move |theme| style(theme, 0.0, true)),
        })
}

/// Botão de texto como `[+]`, com dica; sem mensagem, fica apagado.
fn ascii_button<'a>(
    label: &'static str,
    tip: &'static str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    tooltip::tooltip(
        flat_button(mono(label), false).on_press_maybe(on_press),
        widget::text::body(tip),
        tooltip::Position::Bottom,
    )
    .into()
}

/// `[?]` com a lista de atalhos de teclado na dica.
fn shortcuts_help<'a>() -> Element<'a, Message> {
    let mut list = widget::Column::new();
    for (keys, shortcut) in shortcuts::LIST {
        list = list.push(widget::text::body(format!("{keys:<14}{}", shortcut.label())).font(cosmic::font::mono()));
    }
    tooltip::tooltip(
        widget::container(tinted("[?]", Colors::get().dim)).padding([0, 2]),
        list,
        tooltip::Position::Bottom,
    )
    .into()
}

/// Minimizar, maximizar e fechar, que ficavam na barra de título.
fn window_controls<'a>() -> Element<'a, Message> {
    widget::Row::new()
        .spacing(theme::spacing().space_xxxs)
        .align_y(Alignment::Center)
        .push(tinted("│", Colors::get().dim))
        .push(ascii_button("–", "Minimizar", Some(Message::Minimize)))
        .push(ascii_button("□", "Maximizar", Some(Message::ToggleMaximize)))
        .push(ascii_button("×", "Fechar o C-hub", Some(Message::CloseWindow)))
        .into()
}

fn show_sidebar_button<'a>() -> Element<'a, Message> {
    ascii_button("[»]", "Mostrar o painel (Ctrl+Shift+1)", Some(Message::ShowSection(Section::Sidebar)))
}

/// Última pasta do caminho (`~/projects/chube` → `chube`); a home vira `~`.
fn dir_name(path: &std::path::Path) -> String {
    if path == claude::home_dir() {
        return "~".into();
    }
    path.file_name()
        .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// Símbolo e cor de cada status, no estilo da TUI do Claude Code.
fn status_glyph(status: Status, colors: &Colors) -> (&'static str, Color) {
    match status {
        Status::Suspended => ("-", colors.dim),
        Status::Starting => ("○", colors.dim),
        Status::Working => ("✻", colors.accent),
        Status::Waiting => ("?", colors.warning),
        Status::Ready => ("●", colors.success),
        Status::Exited => ("×", colors.dim),
    }
}

/// Símbolo do modo, como no rodapé da TUI.
fn mode_glyph(mode: Mode) -> &'static str {
    match mode {
        Mode::Manual | Mode::Plan => "⏸",
        Mode::AcceptEdits | Mode::Auto | Mode::Bypass => "⏵⏵",
    }
}

/// O terminal de um chat ou de uma aba, sem cantos arredondados.
/// Opacidade 0 no fundo padrão: fica o da janela (com blur, se o tema tiver); células com
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

/// Rótulo, detalhe e uma barra ASCII colorida, como os medidores do htop:
///
/// ```text
/// contexto                 84,2k / 200k
/// [||||||||||||||||                 42%]
/// └ reinicia em 3 h 1 min
/// ```
fn meter<'a>(
    label: impl Into<Cow<'a, str>> + 'a,
    value: Option<f32>,
    detail: Option<String>,
    note: Option<String>,
) -> Element<'a, Message> {
    let colors = Colors::get();
    let mut head = widget::Row::new().push(mono(label).width(Length::Fill));
    if let Some(detail) = detail {
        head = head.push(tinted(detail, colors.dim));
    }
    // A quantidade de caracteres da barra depende da largura disponível.
    let bar = widget::responsive(move |size| ascii_bar(value, size.width)).height(Length::Shrink);
    let mut column = widget::Column::new().push(head).push(bar);
    if let Some(note) = note {
        column = column.push(tinted(format!("└ {note}"), colors.dim));
    }
    column.into()
}

fn ascii_bar<'a>(value: Option<f32>, width: f32) -> Element<'a, Message> {
    let colors = Colors::get();
    let value = value.map(|value| value.clamp(0.0, 1.0));
    let percent = match value {
        Some(value) => format!("{:.0}%", value * 100.0),
        None => "—".to_owned(),
    };
    // Colchetes e um caractere de folga, porque a largura da fonte é estimada.
    let columns = (width / (TEXT_SIZE * MONO_ADVANCE)) as usize;
    let cells = columns.saturating_sub(3).max(8);
    let ratio = value.unwrap_or(0.0);
    let mut filled = ((ratio * cells as f32).round() as usize).min(cells);
    if ratio > 0.0 {
        filled = filled.max(1);
    }
    // A porcentagem fica dentro da barra, no fim; perto de 100%, cobre o fim das barras.
    let label_len = percent.chars().count();
    let bars = filled.min(cells - label_len);
    let blank = cells - bars - label_len;
    let color = if ratio >= 0.9 {
        colors.danger
    } else if ratio >= 0.7 {
        colors.warning
    } else {
        colors.success
    };
    let percent = if value.is_some() { mono(percent) } else { tinted(percent, colors.dim) };
    widget::Row::new()
        .push(tinted("[", colors.dim))
        .push(tinted("|".repeat(bars), color).wrapping(Wrapping::None))
        .push(mono(" ".repeat(blank)).wrapping(Wrapping::None))
        .push(percent.wrapping(Wrapping::None))
        .push(tinted("]", colors.dim))
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
/// a sessão no COSMIC Terminal, na pasta do chat.
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
    let mut child = tokio::process::Command::new("cosmic-term")
        .arg("--working-directory")
        .arg(&cwd)
        .arg("-e")
        .arg(program)
        .args(args)
        .spawn()
        .map_err(|error| error.to_string())?;
    // O cosmic-term se separa e sai logo; o `wait` evita deixar um processo zumbi.
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
