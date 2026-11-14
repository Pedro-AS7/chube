//! Painéis mostrados no lugar do terminal do chat: nova sessão e sessões salvas.
//!
//! O de nova sessão é montado uma vez (o campo de texto não pode ser recriado a cada mensagem,
//! senão perde o cursor); `sync` só troca seleção, avisos e o que está habilitado. O de sessões é
//! uma lista de linhas clicáveis, refeita quando `App::sessions_version` muda.

use std::cell::Cell;
use std::path::PathBuf;

use async_channel::Sender;
use gtk4::prelude::*;
use gtk4::{gio, pango};

use crate::app::{App, Launch, Message, RemoteState};
use crate::chat::Status;
use crate::claude;
use crate::dirs;
use crate::sessions::{self, SavedSession};
use crate::ui::{
    ICON_LAPTOP, ICON_OMARCHY, ICON_SERVER, button_message, choice, clickable_row, label, row, status_glyph, text_button,
};

const ICON_RIGHT: &str = "\u{f0da}";
const ICON_DOWN: &str = "\u{f0d7}";
const ICON_REFRESH: &str = "\u{f0450}";
const ICON_WARNING: &str = "\u{f071}";
const ICON_TMUX: &str = "\u{ebc8}";
const ICON_IDLE: &str = "\u{f444}";

/// Painel de nova sessão, no estilo dos menus do Omarchy: agente, lugar e pasta, cada um com o
/// rótulo em cima.
pub struct NewChatPanel {
    pub root: gtk4::ScrolledWindow,
    launches: [(Launch, gtk4::Button); 3],
    places: Vec<(Option<String>, gtk4::Button)>,
    pub entry: gtk4::Entry,
    preview: gtk4::Label,
    shortcuts: gtk4::Label,
    remote_note: gtk4::Label,
    create: gtk4::Button,
    browse: gtk4::Button,
}

impl NewChatPanel {
    pub fn new(app: &App, window: &gtk4::ApplicationWindow, tx: &Sender<Message>) -> Self {
        let omarchy_label = match app.omarchy_agent {
            Some(agent) => format!("{ICON_OMARCHY} omarchy ({})", agent.key()),
            None => format!("{ICON_OMARCHY} omarchy"),
        };
        let launches = [
            (Launch::Omarchy, choice(&omarchy_label, tx, move || Message::NewChatLaunch(Launch::Omarchy))),
            (Launch::Claude, choice("claude", tx, move || Message::NewChatLaunch(Launch::Claude))),
            (Launch::Codex, choice("codex", tx, move || Message::NewChatLaunch(Launch::Codex))),
        ];
        let agents = flow();
        for (_, button) in &launches {
            agents.append(button);
        }

        let mut places = vec![(None, choice(&format!("{ICON_LAPTOP} local"), tx, || Message::NewChatHost(None)))];
        for host in &app.hosts {
            let value = host.clone();
            let button = choice(&format!("{ICON_SERVER} {host}"), tx, move || Message::NewChatHost(Some(value.clone())));
            places.push((Some(host.clone()), button));
        }
        let place_box = flow();
        for (_, button) in &places {
            place_box.append(button);
        }

        let entry = gtk4::Entry::new();
        entry.set_hexpand(true);
        let sender = tx.clone();
        entry.connect_changed(move |entry| {
            let _ = sender.try_send(Message::NewChatInput(entry.text().to_string()));
        });
        let sender = tx.clone();
        entry.connect_activate(move |_| {
            let _ = sender.try_send(Message::NewChatConfirm);
        });
        let preview = label("", &["dim"]);
        preview.set_ellipsize(pango::EllipsizeMode::Middle);
        let names: Vec<String> = dirs::shortcuts().into_iter().map(|(name, _)| name).collect();
        let shortcuts = label(&format!("atalhos: {}", names.join(", ")), &["dim"]);
        shortcuts.set_wrap(true);
        shortcuts.set_visible(!names.is_empty());
        let remote_note = label(
            "roda num tmux da VPS: fechar o chat só desconecta. A senha da chave é pedida no terminal.",
            &["dim"],
        );
        remote_note.set_wrap(true);

        let create = text_button("criar", "Abrir o chat (Enter)", tx, || Message::NewChatConfirm);
        let cancel = text_button("cancelar", "Voltar ao chat (Esc)", tx, || Message::ClosePanel);
        let browse = gtk4::Button::with_label("escolher pasta…");
        browse.add_css_class("text");
        browse.set_tooltip_text(Some("Abrir o seletor de pastas"));
        let sender = tx.clone();
        let parent = window.clone();
        browse.connect_clicked(move |_| {
            // O seletor do GTK (pelo portal do sistema, quando há um).
            let dialog = gtk4::FileDialog::builder().title("Pasta do novo chat").modal(true).build();
            let sender = sender.clone();
            dialog.select_folder(Some(&parent), None::<&gio::Cancellable>, move |result| {
                let folder: Option<PathBuf> = result.ok().and_then(|file| file.path());
                let _ = sender.try_send(Message::NewChatPicked(folder));
            });
        });
        let actions = flow();
        actions.append(&create);
        actions.append(&cancel);
        actions.append(&browse);

        let form = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        form.add_css_class("panel");
        form.set_width_request(-1);
        form.append(&group("agente", &agents));
        form.append(&group("onde", &place_box));
        let folder = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        folder.append(&label("pasta", &["dim"]));
        folder.append(&entry);
        folder.append(&preview);
        folder.append(&shortcuts);
        folder.append(&remote_note);
        form.append(&folder);
        form.append(&actions);
        let root = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&form)
            .build();
        close_on_escape(&root, tx);
        Self { root, launches, places, entry, preview, shortcuts, remote_note, create, browse }
    }

    pub fn sync(&self, app: &mut App) {
        for (launch, button) in &self.launches {
            set_selected(button, *launch == app.new_chat_launch);
        }
        let omarchy = app.new_chat_launch == Launch::Omarchy;
        for (host, button) in &self.places {
            set_selected(button, *host == app.new_chat_host);
            // Com o agente do Omarchy, só "local": ele é do sistema deste notebook.
            button.set_visible(host.is_none() || !omarchy);
        }
        if std::mem::take(&mut app.new_chat_input_reset) {
            self.entry.set_text(&app.new_chat_input);
            self.entry.set_position(-1);
        }
        let remote = app.new_chat_host.is_some();
        let placeholder = match (remote, omarchy) {
            (true, _) => "pasta na VPS (vazio = ~)",
            (false, true) => "pasta, atalho do fshort ou . (vazio = ~/Work)",
            (false, false) => "pasta, atalho do fshort ou . (vazio = a do chat em foco)",
        };
        self.entry.set_placeholder_text(Some(placeholder));
        let preview = app.new_chat_preview();
        match &preview {
            Ok(path) => {
                self.preview.set_text(&format!("{ICON_RIGHT} {path}"));
                self.preview.set_css_classes(&["dim"]);
            }
            Err(error) => {
                self.preview.set_text(error);
                self.preview.set_css_classes(&["danger"]);
            }
        }
        self.create.set_sensitive(preview.is_ok());
        self.shortcuts.set_visible(!remote && self.shortcuts.text().len() > "atalhos: ".len());
        self.remote_note.set_visible(remote);
        self.browse.set_visible(!remote);
    }
}

/// Painel das sessões salvas: desta máquina ou de um host do SSH.
pub struct SessionsPanel {
    pub root: gtk4::Box,
    places: Vec<(Option<String>, gtk4::Button)>,
    list: gtk4::Box,
    version: Cell<u64>,
}

impl SessionsPanel {
    pub fn new(app: &App, tx: &Sender<Message>) -> Self {
        let mut places = vec![(None, choice(&format!("{ICON_LAPTOP} local"), tx, || Message::SessionsHost(None)))];
        for host in &app.hosts {
            let value = host.clone();
            let button = choice(&format!("{ICON_SERVER} {host}"), tx, move || Message::SessionsHost(Some(value.clone())));
            places.push((Some(host.clone()), button));
        }
        let tabs = flow();
        for (_, button) in &places {
            tabs.append(button);
        }
        let refresh = gtk4::Button::with_label(ICON_REFRESH);
        refresh.add_css_class("icon");
        refresh.set_tooltip_text(Some("Recarregar"));
        refresh.set_valign(gtk4::Align::Start);
        button_message(&refresh, tx, || Message::ReloadSessions);
        tabs.set_hexpand(true);
        let header = row!(&tabs, &refresh);

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.add_css_class("panel");
        root.append(&header);
        root.append(&scroll);
        close_on_escape(&root, tx);
        Self { root, places, list, version: Cell::new(u64::MAX) }
    }

    pub fn sync(&self, app: &App, tx: &Sender<Message>) {
        for (host, button) in &self.places {
            set_selected(button, *host == app.sessions_host);
        }
        // Os marcadores "aberta aqui" dependem dos chats abertos: a lista de chats entra na versão.
        let version = app.sessions_version.wrapping_mul(1_000).wrapping_add(app.chats.len() as u64);
        if self.version.replace(version) == version {
            return;
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        match &app.sessions_host {
            None => self.local(app, tx),
            Some(host) => self.remote(app, host, tx),
        }
    }

    fn local(&self, app: &App, tx: &Sender<Message>) {
        let Some(list) = &app.saved_sessions else {
            self.list.append(&label("carregando…", &["dim"]));
            return;
        };
        if list.is_empty() {
            self.list.append(&label("nenhuma sessão salva encontrada.", &["dim"]));
            return;
        }
        // Agrupa por pasta; os grupos seguem a ordem da sessão mais recente de cada um.
        let mut groups: Vec<(&PathBuf, Vec<usize>)> = Vec::new();
        for (index, saved) in list.iter().enumerate() {
            match groups.iter_mut().find(|(dir, _)| **dir == saved.cwd) {
                Some((_, indices)) => indices.push(index),
                None => groups.push((&saved.cwd, vec![index])),
            }
        }
        for (dir, indices) in groups {
            let expanded = app.expanded_dirs.contains(dir);
            let path = label(&claude::display_path(dir), &["bold"]);
            path.set_ellipsize(pango::EllipsizeMode::Middle);
            path.set_hexpand(true);
            let arrow = if expanded { ICON_DOWN } else { ICON_RIGHT };
            let header = row!(&label(&format!("{arrow} "), &["accent"]), &path, &label(&format!(" {}", indices.len()), &["dim"]));
            let dir = dir.clone();
            self.list.append(&clickable(header, tx, move || Message::ToggleDirGroup(dir.clone())));
            if expanded {
                for index in indices {
                    self.list.append(&saved_item(app, &list[index], tx, move || Message::ResumeSession(index)));
                }
            }
        }
    }

    fn remote(&self, app: &App, host: &str, tx: &Sender<Message>) {
        let listing = match app.remote_sessions.get(host) {
            None | Some(RemoteState::Loading) => {
                self.list.append(&label(&format!("conectando a {host}… (a senha da chave pode ser pedida)"), &["dim"]));
                return;
            }
            Some(RemoteState::Failed(error)) => {
                let message = label(&format!("{ICON_WARNING} {error}"), &["warning"]);
                message.set_wrap(true);
                self.list.append(&message);
                let host = host.to_owned();
                let retry = text_button("tentar de novo", "Conectar de novo", tx, move || Message::SessionsHost(Some(host.clone())));
                retry.set_halign(gtk4::Align::Start);
                self.list.append(&retry);
                return;
            }
            Some(RemoteState::Loaded(listing)) => listing,
        };

        self.list.append(&label(&format!("tmux abertos em {host}"), &["dim"]));
        if listing.tmux.is_empty() {
            self.list.append(&label("  nenhum", &["dim"]));
        }
        for (index, tmux) in listing.tmux.iter().enumerate() {
            let here = app.chats.iter().any(|chat| {
                chat.host.as_deref() == Some(host) && chat.tmux.as_deref() == Some(tmux.name.as_str()) && chat.is_alive()
            });
            let title = claude::title_from_terminal(&tmux.title).unwrap_or_else(|| tmux.title.clone());
            let mut details = vec![tmux.name.clone(), tmux.path.clone()];
            if here {
                details.push("aberto aqui".into());
            } else if tmux.attached {
                details.push("conectado em outro lugar".into());
            }
            let (glyph, class) = match claude::status_from_title(&tmux.title) {
                Some(status) => status_glyph(status),
                None => (ICON_TMUX, "dim"),
            };
            let item = two_lines(glyph, class, &title, &details.join(" · "));
            let host = host.to_owned();
            self.list.append(&clickable(item, tx, move || Message::AttachTmux(host.clone(), index)));
        }

        let gap = label(&format!("sessões salvas em {host}"), &["dim"]);
        gap.set_margin_top(8);
        self.list.append(&gap);
        if listing.sessions.is_empty() {
            self.list.append(&label("  nenhuma", &["dim"]));
        }
        for (index, saved) in listing.sessions.iter().enumerate() {
            let host = host.to_owned();
            self.list.append(&saved_item(app, saved, tx, move || Message::ResumeRemote(host.clone(), index)));
        }
    }
}

/// Uma sessão salva: status e título; embaixo, agente, quando e onde está aberta.
fn saved_item(app: &App, saved: &SavedSession, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) -> gtk4::Widget {
    let open_here = app.chat_index_for_session(saved.agent, &saved.id);
    let mut details = vec![saved.agent.key().to_owned(), sessions::relative_time(saved.modified)];
    if saved.host.is_some() {
        details.push(saved.cwd.display().to_string());
    }
    let blocked = match (&saved.running, open_here) {
        (_, Some(_)) => {
            details.push("aberta aqui".into());
            false
        }
        // Aberta em outro terminal: retomar aqui brigaria com ela.
        (Some(_), None) => {
            details.push("aberta em outro terminal".into());
            true
        }
        (None, None) => false,
    };
    let running = open_here
        .map(|index| app.chats[index].status)
        .or_else(|| saved.running.as_deref().and_then(Status::from_claude));
    let (glyph, class) = match running {
        Some(status) => status_glyph(status),
        None => (ICON_IDLE, "dim"),
    };
    let item = two_lines(glyph, class, &saved.title, &details.join(" · "));
    if blocked {
        item.add_css_class("blocked");
        item.add_css_class("chat-row");
        return item.upcast();
    }
    clickable(item, tx, message).upcast()
}

fn two_lines(glyph: &str, class: &str, title: &str, details: &str) -> gtk4::Box {
    let title = label(title, &[]);
    title.set_ellipsize(pango::EllipsizeMode::End);
    let details = label(&format!("  {details}"), &["dim"]);
    details.set_ellipsize(pango::EllipsizeMode::End);
    let item = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    item.append(&row!(&label(&format!("{glyph} "), &[class]), &title));
    item.append(&details);
    item
}

/// Linha clicável com o hover das linhas da lista de chats.
fn clickable(item: gtk4::Box, tx: &Sender<Message>, message: impl Fn() -> Message + 'static) -> gtk4::Box {
    clickable_row(&item, tx, message)
}

/// Uma linha de opções. (O GTK4 puro não tem caixa que quebra a linha; o `FlowBox` alinha as
/// opções numa grade, com espaços desiguais. Com poucos hosts, uma linha basta.)
fn flow() -> gtk4::Box {
    let flow = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    flow.set_halign(gtk4::Align::Start);
    flow
}

fn group(title: &str, content: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let group = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    group.append(&label(title, &["dim"]));
    group.append(content);
    group
}

fn set_selected(button: &gtk4::Button, selected: bool) {
    if selected {
        button.add_css_class("selected");
    } else {
        button.remove_css_class("selected");
    }
}

/// Esc volta ao chat (com o foco em qualquer widget do painel).
fn close_on_escape(widget: &impl IsA<gtk4::Widget>, tx: &Sender<Message>) {
    let keys = gtk4::EventControllerKey::new();
    let sender = tx.clone();
    keys.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk4::gdk::Key::Escape {
            let _ = sender.try_send(Message::ClosePanel);
            return gtk4::glib::Propagation::Stop;
        }
        gtk4::glib::Propagation::Proceed
    });
    widget.add_controller(keys);
}
