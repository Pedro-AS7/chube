//! Terminais (VTE) dos chats e das abas: criação, tema do Omarchy e eventos.
//!
//! O VTE é o terminal do GNOME (o mesmo do Console e do Ptyxis). Ele cuida do PTY, do desenho,
//! da seleção, da área de transferência e dos links; o app só ouve o título e a saída do processo.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use async_channel::Sender;
use gtk4::{gio, glib, pango};
use vte4::prelude::*;

use crate::app::Message;
use crate::chat::TermId;
use crate::themes;

/// Termprop do título que o programa põe na janela (OSC 0/2), o mesmo do `vteglobals.h`.
const TITLE: &str = "xterm.title";
/// Linhas de histórico de cada terminal.
const SCROLLBACK_LINES: i64 = 10_000;

/// Terminal novo com o tema atual, que manda `TermTitle` e `TermExit` com o `id` dele.
pub fn new(id: TermId, tx: &Sender<Message>) -> vte4::Terminal {
    let terminal = vte4::Terminal::new();
    terminal.set_hexpand(true);
    terminal.set_vexpand(true);
    terminal.set_scrollback_lines(SCROLLBACK_LINES);
    terminal.set_mouse_autohide(true);
    terminal.set_bold_is_bright(false);
    terminal.set_scroll_on_keystroke(true);
    terminal.set_scroll_on_output(false);
    // Quem pinta o fundo é a janela (com a opacidade do modo vidro); o VTE só desenha o texto e
    // os fundos de célula que o programa pedir.
    terminal.set_clear_background(false);
    apply_theme(&terminal);

    let sender = tx.clone();
    terminal.connect_termprop_changed(Some(TITLE), move |terminal, _| {
        let title = terminal.termprop_string(TITLE).0.unwrap_or_default();
        let _ = sender.try_send(Message::TermTitle(id, title.to_string()));
    });
    let sender = tx.clone();
    terminal.connect_child_exited(move |_, _| {
        let _ = sender.try_send(Message::TermExit(id));
    });
    terminal
}

/// Roda `argv` (ou o shell do usuário, se vazio) em `dir`. O pid vai para `pid` quando o VTE
/// termina de abrir o processo; uma falha chega como `TermExit`.
pub fn spawn(
    terminal: &vte4::Terminal,
    id: TermId,
    dir: &Path,
    argv: &[String],
    pid: Rc<Cell<Option<u32>>>,
    tx: &Sender<Message>,
) {
    let shell;
    let argv: Vec<&str> = if argv.is_empty() {
        shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        vec![shell.as_str()]
    } else {
        argv.iter().map(String::as_str).collect()
    };
    let dir = dir.to_string_lossy();
    let sender = tx.clone();
    terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        Some(&dir),
        &argv,
        &[],
        glib::SpawnFlags::SEARCH_PATH,
        || {},
        -1,
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(child) => pid.set(u32::try_from(child.0).ok()),
            Err(error) => {
                log::error!("não consegui abrir o terminal: {error}");
                let _ = sender.try_send(Message::TermExit(id));
            }
        },
    );
}

/// Fonte e cores do tema do Omarchy. O fundo fica transparente: quem pinta é a janela, com a
/// opacidade do modo vidro.
pub fn apply_theme(terminal: &vte4::Terminal) {
    let palette = themes::current();
    let family = crate::omarchy::monospace_family().unwrap_or_else(|| "monospace".into());
    let mut font = pango::FontDescription::from_string(&family);
    font.set_absolute_size(f64::from(palette.font_px) * f64::from(pango::SCALE));
    terminal.set_font(Some(&font));

    let ansi: Vec<gtk4::gdk::RGBA> = palette.colors.ansi.iter().map(|hex| themes::rgb(*hex).rgba()).collect();
    let ansi: Vec<&gtk4::gdk::RGBA> = ansi.iter().collect();
    terminal.set_colors(
        Some(&palette.text.rgba()),
        Some(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0)),
        &ansi,
    );
    terminal.set_color_cursor(Some(&palette.text.rgba()));
    terminal.set_color_cursor_foreground(Some(&palette.background.rgba()));
    terminal.set_color_highlight(Some(&palette.selection.rgba()));
    terminal.set_color_highlight_foreground(Some(&palette.text.rgba()));
}
