//! Interface em GTK4: lista de chats | terminal do chat | terminal integrado.
//!
//! O `App` guarda o estado; `Ui::sync` deixa os widgets iguais a ele depois de cada mensagem.
//! Os terminais (VTE) nunca são recriados: cada um vira uma página de um `Stack` e só troca qual
//! aparece. A lista e as abas só são refeitas quando o que elas mostram muda, e o GTK só redesenha
//! o que mudou (o hover de uma linha não redesenha os terminais).

use std::cell::RefCell;
use std::collections::HashSet;

use async_channel::Sender;
use gtk4::prelude::*;
use gtk4::{gdk, glib, pango};

use crate::agent::Agent;
use crate::app::{self, App, Focus, Message, Panel, Section};
use crate::chat::{Chat, Status, TermId};
use crate::claude::Mode;
use crate::usage::{self, Window};
use crate::groups::Category;
use crate::panels::{NewChatPanel, SessionsPanel};
use crate::shortcuts;
use crate::themes;

// Ícones da Nerd Font (a JetBrainsMono Nerd Font do Omarchy tem todos), como na barra do Omarchy.
const ICON_NEW: &str = "\u{f067}";
const ICON_HIDE_LEFT: &str = "\u{f053}";
const ICON_HIDE_RIGHT: &str = "\u{f054}";
const ICON_TERMINAL: &str = "\u{f489}";
const ICON_CLOSE: &str = "\u{f0156}";
pub const ICON_SERVER: &str = "\u{f048b}";
pub const ICON_LAPTOP: &str = "\u{f0322}";
pub const ICON_OMARCHY: &str = "\u{f303}";
const ICON_HISTORY: &str = "\u{f02da}";
const ICON_FORK: &str = "\u{e725}";
const ICON_STOP: &str = "\u{f04d}";
const ICON_USAGE: &str = "\u{f080}";
const ICON_REFRESH: &str = "\u{f0450}";
const ICON_CHATS: &str = "\u{f0b79}";
const ICON_EXITED: &str = "\u{f057}";
const ICON_HELP: &str = "\u{f059}";
const TAB_TITLE_MAX_CHARS: usize = 24;

/// Linha horizontal com os widgets dados.
macro_rules! row {
    ($($child:expr),* $(,)?) => {{
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.set_valign(gtk4::Align::Center);
        $(row.append($child);)*
        row
    }};
}
pub(crate) use row;

pub struct Ui {
    window: gtk4::ApplicationWindow,
    css: gtk4::CssProvider,
    tx: Sender<Message>,
    sidebar: gtk4::Box,
    chat_card: gtk4::Box,
    terminal_card: gtk4::Box,
    chats_count: gtk4::Label,
    list: gtk4::Box,
    /// O que a lista mostra; ela só é refeita quando isto muda.
    list_key: RefCell<Vec<String>>,
    show_sidebar: gtk4::Button,
    chat_glyph: gtk4::Label,
    chat_title: gtk4::Label,
    chat_details: gtk4::Label,
    reopen: gtk4::Button,
    fork: gtk4::Button,
    open_terminal: gtk4::Button,
    kill_remote: gtk4::Button,
    mode: gtk4::MenuButton,
    mode_items: Vec<gtk4::Button>,
    close_panel: gtk4::Button,
    usage_body: gtk4::Box,
    usage_key: RefCell<Vec<String>>,
    refresh: gtk4::Button,
    new_chat: NewChatPanel,
    sessions: SessionsPanel,
    close_chat: gtk4::Button,
    show_terminal: gtk4::Button,
    notice: gtk4::Label,
    chat_stack: gtk4::Stack,
    /// Chats com página no `chat_stack`.
    chat_pages: RefCell<HashSet<TermId>>,
    show_chat: gtk4::Button,
    tabs: gtk4::Box,
    tabs_key: RefCell<Vec<String>>,
    tab_stack: gtk4::Stack,
    tab_pages: RefCell<HashSet<TermId>>,
}

impl Ui {
    pub fn new(application: &gtk4::Application, app: &App, tx: Sender<Message>) -> Self {
        let css = gtk4::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(&display, &css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        css.load_from_string(&stylesheet());

        // Lista de chats.
        let chats_count = label("", &["dim"]);
        let new_chat = icon_button(ICON_NEW, "Nova sessão (Ctrl+Shift+N)", &tx, || Message::NewChat);
        let show_sessions = icon_button(ICON_HISTORY, "Sessões salvas", &tx, || Message::ShowSessions);
        let hide_sidebar = icon_button(ICON_HIDE_LEFT, "Esconder o painel (Ctrl+Shift+1)", &tx, || {
            Message::HideSection(Section::Sidebar)
        });
        let help = label(ICON_HELP, &["dim", "icon-label"]);
        help.set_tooltip_text(Some(&shortcuts_help()));
        let header = row!(&heading(ICON_CHATS, "chats"), &chats_count, &fill(), &new_chat, &show_sessions, &help, &hide_sidebar);
        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        let chats_card = card(&section_header(&header), &scroll);
        chats_card.set_vexpand(true);

        // Uso do plano e contexto do chat em foco.
        let refresh = icon_button(ICON_REFRESH, "Atualizar agora", &tx, || Message::RefreshInfo);
        let header = row!(&heading(ICON_USAGE, "uso"), &fill(), &refresh);
        let usage_body = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let usage_scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&usage_body)
            .build();
        let usage_card = card(&section_header(&header), &usage_scroll);
        let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        sidebar.append(&chats_card);
        sidebar.append(&usage_card);
        sidebar.set_size_request(220, -1);

        // Chat em foco.
        let show_sidebar = icon_button(ICON_HIDE_RIGHT, "Mostrar o painel (Ctrl+Shift+1)", &tx, || {
            Message::ShowSection(Section::Sidebar)
        });
        let chat_glyph = label("", &[]);
        let chat_title = label("", &["bold"]);
        chat_title.set_ellipsize(pango::EllipsizeMode::End);
        chat_title.set_hexpand(true);
        let chat_details = label("", &["dim"]);
        chat_details.set_ellipsize(pango::EllipsizeMode::Middle);
        let reopen = text_button("reabrir", "Retomar ou reiniciar esta sessão", &tx, || Message::ReopenChat);
        let fork = icon_button(ICON_FORK, "Criar sub-chat com o contexto deste chat", &tx, || Message::SubChat);
        let open_terminal = icon_button(ICON_TERMINAL, "Continuar no terminal do Omarchy (fecha o chat aqui)", &tx, || {
            Message::OpenInTerminal
        });
        let kill_remote = icon_button(ICON_STOP, "Encerrar o agente na VPS (mata o tmux)", &tx, || Message::KillRemote);
        let (mode, mode_items) = mode_selector(&tx);
        let close_panel = icon_button(ICON_CLOSE, "Voltar ao chat (Esc)", &tx, || Message::ClosePanel);
        let close_chat = gtk4::Button::with_label(ICON_CLOSE);
        close_chat.add_css_class("icon");
        let show_terminal = icon_button(ICON_HIDE_LEFT, "Mostrar o terminal (Ctrl+Shift+3)", &tx, || {
            Message::ShowSection(Section::Terminal)
        });
        let title_line = row!(&chat_glyph, &chat_title, &reopen, &fork, &open_terminal, &kill_remote, &close_chat, &close_panel, &show_terminal);
        let titles = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        titles.set_hexpand(true);
        titles.append(&title_line);
        chat_details.set_hexpand(true);
        titles.append(&row!(&chat_details, &mode));
        let header = row!(&show_sidebar, &titles);
        let notice = label("", &["warning"]);
        notice.set_wrap(true);
        let chat_stack = gtk4::Stack::new();
        chat_stack.set_vexpand(true);
        let empty = label("nenhum chat aberto — Ctrl+Shift+N abre um na pasta atual", &["dim"]);
        empty.set_xalign(0.5);
        chat_stack.add_named(&empty, Some("vazio"));
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        body.append(&notice);
        body.append(&chat_stack);
        let chat_card = card(&section_header(&header), &body);

        // Terminal integrado.
        let hide_terminal = icon_button(ICON_HIDE_RIGHT, "Esconder o terminal (Ctrl+Shift+3)", &tx, || {
            Message::HideSection(Section::Terminal)
        });
        let show_chat = icon_button(ICON_HIDE_LEFT, "Mostrar o chat (Ctrl+Shift+2)", &tx, || {
            Message::ShowSection(Section::Chat)
        });
        let tabs = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        let tabs_scroll = gtk4::ScrolledWindow::builder()
            .vscrollbar_policy(gtk4::PolicyType::Never)
            .hexpand(true)
            .child(&tabs)
            .build();
        let new_tab = icon_button(ICON_NEW, "Nova aba do terminal", &tx, || Message::NewTerminalTab);
        let header = row!(&show_chat, &hide_terminal, &tabs_scroll, &new_tab);
        let tab_stack = gtk4::Stack::new();
        tab_stack.set_vexpand(true);
        tab_stack.add_named(&label("nenhum terminal aberto.", &["dim"]), Some("vazio"));
        let terminal_card = card(&section_header(&header), &tab_stack);

        let inner = gtk4::Paned::new(gtk4::Orientation::Horizontal);
        inner.set_start_child(Some(&chat_card));
        inner.set_end_child(Some(&terminal_card));
        inner.set_position(760);
        inner.set_shrink_start_child(false);
        inner.set_shrink_end_child(false);
        let outer = gtk4::Paned::new(gtk4::Orientation::Horizontal);
        outer.set_start_child(Some(&sidebar));
        outer.set_end_child(Some(&inner));
        outer.set_position(300);
        outer.set_resize_start_child(false);
        outer.set_shrink_start_child(false);
        outer.set_shrink_end_child(false);

        // Sem a barra de título, como as janelas do Omarchy: o Hyprland move, fecha e
        // redimensiona (Super+W fecha).
        let window = gtk4::ApplicationWindow::builder()
            .application(application)
            .title("C-hub")
            .icon_name(app::APP_ID)
            .default_width(1400)
            .default_height(860)
            .decorated(false)
            .child(&outer)
            .build();
        window.add_css_class("chube");
        let new_chat_panel = NewChatPanel::new(app, &window, &tx);
        let sessions_panel = SessionsPanel::new(app, &tx);
        chat_stack.add_named(&new_chat_panel.root, Some("nova"));
        chat_stack.add_named(&sessions_panel.root, Some("sessoes"));

        // Atalhos na fase de captura: chegam antes do terminal em foco e não vão para o PTY.
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let sender = tx.clone();
        keys.connect_key_pressed(move |_, keyval, keycode, state| match shortcuts::find(keyval, keycode, state) {
            Some(shortcut) => {
                let _ = sender.try_send(Message::Shortcut(shortcut));
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        });
        window.add_controller(keys);

        let ui = Ui {
            window,
            css,
            tx,
            sidebar,
            chat_card,
            terminal_card,
            chats_count,
            list,
            list_key: RefCell::new(vec!["(nunca)".into()]),
            show_sidebar,
            chat_glyph,
            chat_title,
            chat_details,
            reopen,
            fork,
            open_terminal,
            kill_remote,
            mode,
            mode_items,
            close_panel,
            usage_body,
            usage_key: RefCell::new(vec!["(nunca)".into()]),
            refresh,
            new_chat: new_chat_panel,
            sessions: sessions_panel,
            close_chat,
            show_terminal,
            notice,
            chat_stack,
            chat_pages: RefCell::new(HashSet::new()),
            show_chat,
            tabs,
            tabs_key: RefCell::new(vec!["(nunca)".into()]),
            tab_stack,
            tab_pages: RefCell::new(HashSet::new()),
        };
        let sender = ui.tx.clone();
        ui.close_chat.connect_clicked(move |button| {
            // O índice do chat em foco fica no widget (muda a cada `sync`).
            let index: usize = button.widget_name().parse().unwrap_or(usize::MAX);
            let _ = sender.try_send(Message::CloseChat(index));
        });
        ui
    }

    pub fn present(&self) {
        self.window.present();
    }

    /// Deixa a interface igual ao estado do app.
    pub fn sync(&self, app: &mut App) {
        if std::mem::take(&mut app.theme_changed) {
            self.css.load_from_string(&stylesheet());
            // As cores dos glifos de status estão nas classes; refaz a lista com o tema novo.
            self.list_key.borrow_mut().clear();
        }
        self.sidebar.set_visible(app.sidebar_visible);
        self.chat_card.set_visible(app.chat_visible);
        self.terminal_card.set_visible(app.terminal_visible);
        self.show_sidebar.set_visible(!app.sidebar_visible);
        self.show_terminal.set_visible(!app.terminal_visible);
        self.show_chat.set_visible(!app.chat_visible);

        self.sync_list(app);
        self.sync_chat(app);
        self.sync_usage(app);
        self.sync_tabs(app);

        if let Some(focus) = app.focus.take() {
            let target = match focus {
                Focus::NewChatInput => {
                    let entry = self.new_chat.entry.clone();
                    glib::idle_add_local_once(move || {
                        entry.grab_focus();
                    });
                    None
                }
                Focus::Chat if app.chat_visible => app.chats.get(app.focused).and_then(|chat| chat.terminal.clone()),
                _ => app.active_tab.and_then(|id| app.tabs.iter().find(|tab| tab.id == id)).map(|tab| tab.terminal.clone()),
            };
            // No ocioso: a página acabou de entrar no `Stack` e ainda não está na tela.
            if let Some(terminal) = target {
                glib::idle_add_local_once(move || {
                    terminal.grab_focus();
                });
            }
        }
    }

    /// Ctrl+Tab: com o terminal do chat em foco, vai para o integrado; em qualquer outro caso,
    /// para o chat.
    pub fn switch_focus(&self, app: &App) {
        let chat = app.chats.get(app.focused).and_then(|chat| chat.terminal.clone());
        let tab = app.active_tab.and_then(|id| app.tabs.iter().find(|tab| tab.id == id)).map(|tab| tab.terminal.clone());
        let chat_focused = chat.as_ref().is_some_and(|terminal| terminal.has_focus());
        let target = if chat_focused && app.terminal_visible { tab } else if app.chat_visible { chat } else { tab };
        if let Some(terminal) = target {
            terminal.grab_focus();
        }
    }

    fn sync_list(&self, app: &App) {
        let groups = app.chat_groups();
        let mut key = Vec::new();
        for group in &groups {
            key.push(format!("g{:?}", group.category));
            for dir in &group.dirs {
                key.push(format!("d{}", dir.dir));
                for &index in &dir.chats {
                    let chat = &app.chats[index];
                    key.push(format!(
                        "c{index}\t{}\t{:?}\t{}\t{}",
                        chat.title,
                        chat.status,
                        index == app.focused,
                        chat.agent_label()
                    ));
                }
            }
        }
        if *self.list_key.borrow() == key {
            return;
        }
        *self.list_key.borrow_mut() = key;
        self.chats_count.set_text(&format!(" {}", app.chats.len()));

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for group in groups {
            let icon = match group.category {
                Category::Omarchy => ICON_OMARCHY,
                Category::Local => ICON_LAPTOP,
                Category::Vps => ICON_SERVER,
            };
            let count: usize = group.dirs.iter().map(|dir| dir.chats.len()).sum();
            let header = row!(&heading(icon, group.category.label()), &label(&format!(" {count}"), &["dim"]));
            header.add_css_class("group");
            self.list.append(&header);
            for dir in group.dirs {
                let dir_label = label(&format!("  {}", dir.dir), &["dim"]);
                dir_label.set_ellipsize(pango::EllipsizeMode::Middle);
                self.list.append(&dir_label);
                for index in dir.chats {
                    self.list.append(&self.chat_row(app, index));
                }
            }
        }
        if app.chats.is_empty() {
            self.list.append(&label("nenhum chat aberto", &["dim"]));
        }
    }

    /// Um chat da lista: status e título; embaixo, o agente e o status por extenso (a pasta e o
    /// lugar já estão nos cabeçalhos do grupo).
    fn chat_row(&self, app: &App, index: usize) -> gtk4::Box {
        let chat = &app.chats[index];
        let (glyph, class) = status_glyph(chat.status);
        let title = label(&chat.title, &["title"]);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_hexpand(true);
        let close = icon_button(ICON_CLOSE, close_tip(chat), &self.tx, move || Message::CloseChat(index));
        let first = row!(&label(&format!("{glyph} "), &[class]), &title, &close);
        // Trabalhando / precisa de você ficam na cor do status; os outros, apagados.
        let status_class = match chat.status {
            Status::Working | Status::Waiting => class,
            _ => "dim",
        };
        // No grupo do Omarchy, o "omarchy · " do rótulo seria repetido.
        let agent = if chat.auto && !chat.attach_only { chat.agent.key().to_owned() } else { chat.agent_label() };
        let second = row!(&label(&format!("  {agent} · "), &["dim"]), &label(chat.status.label(), &[status_class]));

        let item = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        if index == app.focused {
            item.add_css_class("selected");
        }
        item.append(&first);
        item.append(&second);
        let row = clickable_row(&item, &self.tx, move || Message::SelectChat(index));
        row.set_margin_start(12);
        row
    }

    fn sync_chat(&self, app: &mut App) {
        // Páginas novas; as dos chats que saíram da lista vão embora (o terminal junto).
        let live: HashSet<TermId> = app.chats.iter().filter(|chat| chat.terminal.is_some()).map(|chat| chat.id).collect();
        for chat in app.chats.iter().filter(|chat| !self.chat_pages.borrow().contains(&chat.id)) {
            if let Some(terminal) = &chat.terminal {
                self.chat_stack.add_named(&terminal_page(terminal), Some(&page_name(chat.id)));
                self.chat_pages.borrow_mut().insert(chat.id);
            }
        }
        let gone: Vec<TermId> = self.chat_pages.borrow().difference(&live).copied().collect();
        for id in gone {
            if let Some(page) = self.chat_stack.child_by_name(&page_name(id)) {
                self.chat_stack.remove(&page);
            }
            self.chat_pages.borrow_mut().remove(&id);
        }

        match app.panel {
            Panel::NewChat => self.new_chat.sync(app),
            Panel::Sessions => self.sessions.sync(app, &self.tx),
            Panel::None => {}
        }
        let chat = app.chats.get(app.focused);
        let page = match app.panel {
            Panel::NewChat => "nova".to_owned(),
            Panel::Sessions => "sessoes".to_owned(),
            Panel::None => chat.filter(|chat| chat.terminal.is_some()).map_or_else(|| "vazio".to_owned(), |chat| page_name(chat.id)),
        };
        self.chat_stack.set_visible_child_name(&page);
        self.close_panel.set_visible(app.panel != Panel::None);
        let panel_title = match app.panel {
            Panel::NewChat => Some((ICON_NEW, "nova sessão")),
            Panel::Sessions => Some((ICON_HISTORY, "sessões")),
            Panel::None => None,
        };
        if let Some((icon, title)) = panel_title {
            self.chat_glyph.set_text(&format!("{icon} "));
            self.chat_glyph.set_css_classes(&["accent"]);
            self.chat_title.set_text(title);
            self.chat_details.set_text("Esc volta ao chat");
            self.reopen.set_visible(false);
            self.fork.set_visible(false);
            self.open_terminal.set_visible(false);
            self.kill_remote.set_visible(false);
            self.mode.set_visible(false);
            self.close_chat.set_visible(false);
            self.notice.set_visible(app.notice.is_some());
            self.notice.set_text(app.notice.as_deref().unwrap_or_default());
            return;
        }

        match chat {
            Some(chat) => {
                let (glyph, class) = status_glyph(chat.status);
                self.chat_glyph.set_text(&format!("{glyph} "));
                self.chat_glyph.set_css_classes(&[class]);
                self.chat_title.set_text(&chat.title);
                self.chat_details.set_text(&chat_details(chat));
                self.reopen.set_visible(chat.status == Status::Exited);
                let can_fork = chat.session_id.is_some() && chat.status != Status::Starting && !chat.attach_only;
                self.fork.set_visible(true);
                self.fork.set_sensitive(can_fork);
                self.open_terminal.set_visible(true);
                self.open_terminal.set_sensitive(can_fork || chat.host.is_some());
                self.kill_remote.set_visible(chat.host.is_some() && !chat.attach_only);
                // Também na VPS: o modo é lido no rodapé da TUI e trocado com Shift+Tab, pelo tmux.
                self.mode.set_visible(chat.is_alive() && chat.agent == Agent::Claude && !chat.attach_only);
                for (index, item) in self.mode_items.iter().enumerate() {
                    if chat.mode.is_some_and(|mode| mode.index() == index) {
                        item.add_css_class("selected");
                    } else {
                        item.remove_css_class("selected");
                    }
                }
                self.mode.set_label(&match chat.mode {
                    Some(mode) => format!("{} {}", mode_glyph(mode), Mode::LABELS[mode.index()].to_lowercase()),
                    None => "modo".to_owned(),
                });
                self.close_chat.set_visible(true);
                self.close_chat.set_widget_name(&app.focused.to_string());
                self.close_chat.set_tooltip_text(Some(close_tip(chat)));
            }
            None => {
                self.chat_glyph.set_text("");
                self.chat_title.set_text("C-hub");
                self.chat_details.set_text("");
                self.reopen.set_visible(false);
                self.fork.set_visible(false);
                self.open_terminal.set_visible(false);
                self.kill_remote.set_visible(false);
                self.mode.set_visible(false);
                self.close_chat.set_visible(false);
            }
        }
        self.notice.set_visible(app.notice.is_some());
        self.notice.set_text(app.notice.as_deref().unwrap_or_default());
    }

    fn sync_tabs(&self, app: &App) {
        for tab in app.tabs.iter().filter(|tab| !self.tab_pages.borrow().contains(&tab.id)) {
            self.tab_stack.add_named(&terminal_page(&tab.terminal), Some(&page_name(tab.id)));
            self.tab_pages.borrow_mut().insert(tab.id);
        }
        let live: HashSet<TermId> = app.tabs.iter().map(|tab| tab.id).collect();
        let gone: Vec<TermId> = self.tab_pages.borrow().difference(&live).copied().collect();
        for id in gone {
            if let Some(page) = self.tab_stack.child_by_name(&page_name(id)) {
                self.tab_stack.remove(&page);
            }
            self.tab_pages.borrow_mut().remove(&id);
        }
        let page = app.active_tab.map_or_else(|| "vazio".to_owned(), page_name);
        self.tab_stack.set_visible_child_name(&page);

        let names = app.tab_names();
        let key: Vec<String> = app
            .tabs
            .iter()
            .zip(&names)
            .map(|(tab, name)| format!("{}\t{name}\t{}", tab.id, Some(tab.id) == app.active_tab))
            .collect();
        if *self.tabs_key.borrow() == key {
            return;
        }
        *self.tabs_key.borrow_mut() = key;
        while let Some(child) = self.tabs.first_child() {
            self.tabs.remove(&child);
        }
        for (tab, name) in app.tabs.iter().zip(names) {
            let id = tab.id;
            let title = format!("{ICON_TERMINAL} {}", app::short_text(&name, TAB_TITLE_MAX_CHARS));
            let open = gtk4::Button::with_label(&title);
            open.add_css_class("tab");
            let close = gtk4::Button::with_label(ICON_CLOSE);
            close.add_css_class("tab");
            close.add_css_class("dim");
            if Some(id) == app.active_tab {
                open.add_css_class("selected");
                close.add_css_class("selected");
            }
            let sender = self.tx.clone();
            open.connect_clicked(move |_| {
                let _ = sender.try_send(Message::TerminalTabActivated(id));
            });
            let sender = self.tx.clone();
            close.connect_clicked(move |_| {
                let _ = sender.try_send(Message::TerminalTabClosed(id));
            });
            self.tabs.append(&row!(&open, &close));
        }
    }
}

impl Ui {
    /// Painel "uso": contexto do chat em foco e limites do plano (do Claude ou, num chat do
    /// Codex, os dele). Refeito só quando o que ele mostra muda.
    fn sync_usage(&self, app: &App) {
        let focused = app.chats.get(app.focused);
        let context_tokens = focused.map_or(0, |chat| chat.context_tokens);
        let is_codex = focused.is_some_and(|chat| chat.agent == Agent::Codex);
        let limits: &[usage::Limit] = if is_codex {
            match &app.codex_usage {
                Some(limits) => limits,
                None => focused.map(|chat| chat.codex_limits.as_slice()).unwrap_or_default(),
            }
        } else {
            &app.limits
        };
        let known_window = if is_codex {
            focused.and_then(|chat| chat.context_window)
        } else {
            focused
                .and_then(|chat| chat.active_model.as_ref())
                .and_then(|model| app.context_windows.get(model).copied().flatten())
        };

        let mut meters: Vec<Meter> = Vec::new();
        if context_tokens == 0 || (is_codex && known_window.is_none()) {
            meters.push(Meter::new("contexto", None, None, None));
        } else {
            // Enquanto o `/context` não responde, chuta 200 mil; acima disso, só pode ser 1 milhão.
            let guess: u32 = if context_tokens > 200_000 { 1_000_000 } else { 200_000 };
            let window = if is_codex {
                known_window.unwrap_or(1).max(1)
            } else {
                known_window.map_or(guess, |window| window.max(guess))
            };
            meters.push(Meter::new(
                "contexto",
                Some(context_tokens as f32 / window as f32),
                Some(format!("{} / {}", app::short_tokens(context_tokens), app::short_tokens(window))),
                None,
            ));
        }
        if limits.is_empty() {
            meters.push(Meter::new("sessão (5h)", None, None, None));
            meters.push(Meter::new("semana", None, None, None));
        }
        for limit in limits {
            let label = match &limit.window {
                Window::Session => "sessão (5h)".to_owned(),
                Window::Week(models) if models == "all models" => "semana".to_owned(),
                Window::Week(models) => format!("semana · {}", models.to_lowercase()),
            };
            meters.push(Meter::new(&label, Some(limit.used), None, limit.resets.map(usage::describe_reset)));
        }

        let working = app.chats.iter().filter(|chat| chat.status == Status::Working).count();
        let mut footer = vec![(format!("{} chats · {working} trabalhando", app.chats.len()), "dim")];
        if let Some(account) = app.account.as_ref().filter(|_| !is_codex) {
            footer.push((format!("conta {account}"), "dim"));
        }
        footer.push(if is_codex && app.codex_usage.is_none() {
            ("codex · limites do histórico; /status na sessão".to_owned(), "dim")
        } else if app.fetching_usage {
            ("atualizando…".to_owned(), "dim")
        } else if let Some(error) = &app.usage_error {
            (format!("erro ao atualizar: {error}"), "danger")
        } else if let Some(time) = &app.usage_updated {
            (format!("limites de {time}"), "dim")
        } else {
            ("ainda não atualizado".to_owned(), "dim")
        });
        self.refresh.set_sensitive(!app.fetching_usage);

        let key: Vec<String> = meters
            .iter()
            .map(|meter| format!("{meter:?}"))
            .chain(footer.iter().map(|(text, class)| format!("{text}{class}")))
            .collect();
        if *self.usage_key.borrow() == key {
            return;
        }
        *self.usage_key.borrow_mut() = key;
        while let Some(child) = self.usage_body.first_child() {
            self.usage_body.remove(&child);
        }
        for meter in meters {
            self.usage_body.append(&meter.widget());
        }
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        for (text, class) in footer {
            let line = label(&text, &[class]);
            line.set_wrap(true);
            lines.append(&line);
        }
        self.usage_body.append(&lines);
    }
}

/// Uma linha do painel "uso": rótulo, detalhe e porcentagem; embaixo, a barra e uma nota.
#[derive(Debug)]
struct Meter {
    label: String,
    value: Option<f32>,
    detail: Option<String>,
    note: Option<String>,
}

impl Meter {
    fn new(label: &str, value: Option<f32>, detail: Option<String>, note: Option<String>) -> Self {
        Self { label: label.to_owned(), value: value.map(|value| value.clamp(0.0, 1.0)), detail, note }
    }

    fn widget(&self) -> gtk4::Box {
        let level = match self.value {
            Some(value) if value >= 0.9 => "danger",
            Some(value) if value >= 0.7 => "warning",
            _ => "accent",
        };
        let name = label(&self.label, &[]);
        name.set_hexpand(true);
        let head = row!(&name);
        if let Some(detail) = &self.detail {
            head.append(&label(&format!("{detail}  "), &["dim"]));
        }
        head.append(&match self.value {
            Some(value) => label(&format!("{:.0}%", value * 100.0), &[level]),
            None => label("—", &["dim"]),
        });
        let bar = gtk4::ProgressBar::new();
        bar.set_fraction(f64::from(self.value.unwrap_or(0.0)));
        bar.add_css_class("meter");
        bar.add_css_class(level);
        let meter = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        meter.append(&head);
        meter.append(&bar);
        if let Some(note) = &self.note {
            meter.append(&label(&format!("└ {note}"), &["dim"]));
        }
        meter
    }
}

/// Botão do modo de permissão com o menu dos modos, como o seletor da TUI.
fn mode_selector(tx: &Sender<Message>) -> (gtk4::MenuButton, Vec<gtk4::Button>) {
    let menu = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    let popover = gtk4::Popover::new();
    popover.set_has_arrow(false);
    popover.add_css_class("menu-card");
    let mut items = Vec::new();
    for (index, mode) in Mode::ALL.iter().enumerate() {
        let item = gtk4::Button::new();
        item.set_child(Some(&label(&format!("{} {}", mode_glyph(*mode), Mode::LABELS[index].to_lowercase()), &[])));
        item.add_css_class("choice");
        item.set_halign(gtk4::Align::Fill);
        items.push(item.clone());
        let sender = tx.clone();
        let parent = popover.clone();
        item.connect_clicked(move |_| {
            parent.popdown();
            let _ = sender.try_send(Message::SetMode(index));
        });
        menu.append(&item);
    }
    popover.set_child(Some(&menu));
    let button = gtk4::MenuButton::builder().popover(&popover).label("modo").always_show_arrow(true).build();
    button.add_css_class("mode");
    button.set_tooltip_text(Some("Modo de permissão (Shift+Tab no terminal)"));
    (button, items)
}

/// Ícone do modo, como o símbolo do rodapé da TUI (⏸ / ⏵⏵).
fn mode_glyph(mode: Mode) -> &'static str {
    match mode {
        Mode::Manual | Mode::Plan => "\u{f04c}",
        Mode::AcceptEdits | Mode::Auto | Mode::Bypass => "\u{f04e}",
    }
}

fn page_name(id: TermId) -> String {
    format!("t{id}")
}

/// O terminal numa rolagem com a barra fina (sobreposta) do GTK.
fn terminal_page(terminal: &vte4::Terminal) -> gtk4::ScrolledWindow {
    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(terminal)
        .build()
}

/// Segunda linha do cabeçalho do chat: agente · pasta · modelo · contexto.
fn chat_details(chat: &Chat) -> String {
    let mut parts = vec![chat.agent_label(), chat.place_label()];
    if let Some(model) = chat.active_model.as_ref().or(chat.model.as_ref()) {
        parts.push(model.clone());
    }
    if let Some(parent) = &chat.parent {
        parts.push(format!("sub-chat de \"{parent}\""));
    }
    parts.join(" · ")
}

fn close_tip(chat: &Chat) -> &'static str {
    if chat.host.is_some() { "Desconectar (o agente continua na VPS)" } else { "Fechar este chat (como /exit)" }
}

/// Ícone e classe de cor de cada status, com as cores do tema do Omarchy.
pub fn status_glyph(status: Status) -> (&'static str, &'static str) {
    match status {
        Status::Suspended => ("\u{f04c}", "dim"),
        Status::Starting => ("\u{f10c}", "dim"),
        Status::Working => ("\u{f110}", "accent"),
        Status::Waiting => ("\u{f0f3}", "warning"),
        Status::Ready => ("\u{f111}", "success"),
        Status::Exited => (ICON_EXITED, "dim"),
    }
}

fn shortcuts_help() -> String {
    shortcuts::LIST
        .iter()
        .map(|(keys, shortcut)| format!("{keys:<14}{}", shortcut.label()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn label(text: &str, classes: &[&str]) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    for class in classes {
        label.add_css_class(class);
    }
    label
}

/// Título de seção: o ícone no acento e o nome em negrito.
fn heading(icon: &str, text: &str) -> gtk4::Box {
    row!(&label(&format!("{icon} "), &["accent"]), &label(text, &["bold"]))
}



fn fill() -> gtk4::Box {
    let fill = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    fill.set_hexpand(true);
    fill
}

/// Seção com borda reta, como os cards do shell do Omarchy.
fn card(header: &impl IsA<gtk4::Widget>, body: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    card.add_css_class("card");
    card.append(header);
    card.append(body);
    card
}

fn section_header(content: &gtk4::Box) -> gtk4::Box {
    content.add_css_class("section-header");
    content.set_valign(gtk4::Align::Start);
    content.clone()
}

/// Botão de ícone (Nerd Font), apagado até passar o mouse.
fn icon_button(icon: &str, help: &str, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) -> gtk4::Button {
    let button = gtk4::Button::with_label(icon);
    button.add_css_class("icon");
    button.set_tooltip_text(Some(help));
    button.set_valign(gtk4::Align::Center);
    let sender = tx.clone();
    button.connect_clicked(move |_| {
        let _ = sender.try_send(message());
    });
    button
}

/// Linha clicável, como as dos menus do Omarchy: ao passar o mouse, um fundo do texto a 8%.
pub fn clickable_row(content: &gtk4::Box, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) -> gtk4::Box {
    content.add_css_class("chat-row");
    let click = gtk4::GestureClick::new();
    let sender = tx.clone();
    click.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let _ = sender.try_send(message());
    });
    content.add_controller(click);
    content.set_cursor_from_name(Some("pointer"));
    content.clone()
}

/// Opção de um grupo (agente, lugar): a escolhida com o texto no acento e a borda da seleção.
pub fn choice(text: &str, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) -> gtk4::Button {
    let button = gtk4::Button::with_label(text);
    button.add_css_class("choice");
    button_message(&button, tx, message);
    button
}

/// Manda a mensagem a cada clique.
pub fn button_message(button: &gtk4::Button, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) {
    let sender = tx.clone();
    button.connect_clicked(move |_| {
        let _ = sender.try_send(message());
    });
}

/// Botão de texto com borda, como os controles do shell do Omarchy.
pub fn text_button(text: &str, help: &str, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) -> gtk4::Button {
    let button = gtk4::Button::with_label(text);
    button.add_css_class("text");
    button.set_tooltip_text(Some(help));
    button.set_valign(gtk4::Align::Center);
    let sender = tx.clone();
    button.connect_clicked(move |_| {
        let _ = sender.try_send(message());
    });
    button
}

/// CSS da janela com as cores do tema do Omarchy. Linhas retas, seleção com fundo do texto a 8%
/// e texto no acento, bordas do texto a 25–40%, como o `shell.toml` do Omarchy.
fn stylesheet() -> String {
    let palette = themes::current();
    let family = crate::omarchy::monospace_family().unwrap_or_else(|| "monospace".into());
    let text = |alpha: f32| palette.text.with_alpha(alpha).css();
    format!(
        "
window.chube {{ background: {window}; color: {fg}; font-family: '{family}'; font-size: {px}px; }}
window.chube * {{ border-radius: 0; }}
popover.menu-card {{ font-family: '{family}'; font-size: {px}px; color: {fg}; }}
.card {{ border: 1px solid {border}; margin: 4px; padding: 4px; }}
.section-header {{ border-bottom: 1px solid {line}; padding-bottom: 4px; }}
.bold {{ font-weight: bold; }}
.dim {{ color: {dim}; }}
.accent {{ color: {accent}; }}
.success {{ color: {success}; }}
.warning {{ color: {warning}; }}
.danger {{ color: {danger}; }}
.group {{ margin-top: 4px; }}
.chat-row {{ padding: 2px 4px; border: 1px solid transparent; }}
.chat-row:hover {{ background: {hover}; }}
.chat-row.selected {{ background: {hover}; border-color: {select_border}; }}
.chat-row.selected .title {{ color: {accent}; font-weight: bold; }}
button.icon, button.tab {{ background: none; border: none; box-shadow: none; padding: 1px 6px; min-height: 0; min-width: 1em; color: {dim}; }}
button.icon:hover, button.tab:hover {{ background: {hover}; color: {fg}; }}
button.tab {{ color: {fg}; }}
button.tab.dim {{ color: {dim}; }}
button.tab.selected {{ background: {hover}; color: {accent}; }}
button.text {{ background: {button}; border: 1px solid {button_border}; box-shadow: none; padding: 2px 10px; min-height: 0; color: {fg}; }}
button.text:hover {{ background: {hover}; }}
paned > separator {{ background: transparent; min-width: 2px; }}
scrollbar, scrollbar trough {{ background: transparent; border: none; }}
scrollbar slider {{ background: {line}; min-width: 3px; min-height: 3px; }}
tooltip {{ background: {tooltip}; color: {fg}; border: 1px solid {fg}; }}
tooltip label {{ font-family: '{family}'; }}
vte-terminal, vte-terminal.view, scrolledwindow, viewport, stack {{ background: transparent; }}
vte-terminal {{ padding: 2px 4px; }}
.panel {{ padding: 8px; }}
button.choice {{ background: none; border: 1px solid transparent; box-shadow: none; padding: 2px 8px; min-height: 0; color: {fg}; }}
button.choice:hover {{ background: {hover}; }}
button.choice.selected {{ background: {hover}; border-color: {select_border}; color: {accent}; }}
flowboxchild {{ padding: 0; }}
entry {{ background: {button}; border: 1px solid {button_border}; box-shadow: none; color: {fg}; padding: 2px 6px; min-height: 0; }}
entry:focus-within {{ border-color: {accent}; outline: none; }}
.blocked {{ opacity: 0.45; }}
button:disabled {{ opacity: 0.4; }}
progressbar.meter trough {{ min-height: 3px; background: {selection}; border: none; box-shadow: none; }}
progressbar.meter progress {{ min-height: 3px; background: {accent}; border: none; }}
progressbar.meter.warning progress {{ background: {warning}; }}
progressbar.meter.danger progress {{ background: {danger}; }}
menubutton.mode > button {{ background: none; border: none; box-shadow: none; padding: 0 4px; min-height: 0; color: {fg}; }}
menubutton.mode > button:hover {{ background: {hover}; }}
popover.menu-card > contents {{ background: {menu_bg}; border: 1px solid {accent}; padding: 4px; box-shadow: none; }}
.section-header button {{ margin-left: 2px; }}
",
        window = palette.window().css(),
        fg = palette.text.css(),
        px = palette.font_px,
        border = palette.dim.with_alpha(0.35).css(),
        line = text(0.15),
        dim = palette.dim.css(),
        accent = palette.accent.css(),
        success = palette.success.css(),
        warning = palette.warning.css(),
        danger = palette.danger.css(),
        hover = text(0.08),
        select_border = text(0.25),
        button = text(0.04),
        button_border = text(0.4),
        tooltip = palette.background.with_alpha(0.97).css(),
        selection = palette.selection.css(),
        menu_bg = palette.background.css(),
    )
}
