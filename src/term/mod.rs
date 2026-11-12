//! Terminal integrado, adaptado do cosmic-term (© System76, GPL-3.0-only).
//!
//! `terminal.rs` (PTY + emulação com alacritty_terminal + texto com cosmic-text),
//! `terminal_box.rs` (o widget) e `mouse_reporter.rs` vêm de lá com poucas mudanças;
//! este módulo substitui o que eles usavam do resto do app: configuração, menu de contexto
//! e atalhos.

mod echo_guard;
mod mouse_reporter;
#[allow(dead_code)]
mod terminal;
#[allow(dead_code)]
mod terminal_box;
mod terminal_theme;

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use alacritty_terminal::term::color::Colors;
use cosmic::cosmic_theme::palette::Srgba;
use cosmic_text::{Metrics, Stretch};

use crate::themes::Theme;

pub use alacritty_terminal::event::Event;
pub use terminal::Terminal;
pub use terminal_box::terminal_box;

/// Identifica a aba do terminal nos eventos que vêm da thread do PTY.
pub type TabId = u64;

pub type EventSender = tokio::sync::mpsc::UnboundedSender<(TabId, Event)>;

/// No cosmic-term, os perfis permitem comando e tema próprios; aqui não há perfis.
pub type ProfileId = u64;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorSchemeKind {
    Dark,
    Light,
}

/// Menu de contexto: o widget guarda o link sob o clique direito. O Chube não mostra menu.
#[derive(Clone, Debug)]
pub struct MenuState {
    pub link: Option<String>,
}

/// No cosmic-term, as ações dos atalhos do app. Aqui são os atalhos do Chube: o terminal
/// não manda essas teclas ao PTY, e o app as trata numa subscription (copiar e colar
/// continuam direto no widget).
pub use crate::shortcuts::{KEY_BINDS, Shortcut as Action};

/// Substitui a `Config` do cosmic-term, com os valores padrão dela.
pub struct Config {
    pub font_size: u16,
    pub font_weight: u16,
    pub dim_font_weight: u16,
    pub bold_font_weight: u16,
    pub use_bright_bold: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_size: 14,
            font_weight: cosmic_text::Weight::NORMAL.0,
            dim_font_weight: cosmic_text::Weight::NORMAL.0,
            bold_font_weight: cosmic_text::Weight::BOLD.0,
            use_bright_bold: false,
        }
    }
}

impl Config {
    pub fn metrics(&self, zoom_adj: i8) -> Metrics {
        let font_size = f32::from(self.font_size.saturating_add_signed(i16::from(zoom_adj)).max(1));
        Metrics::new(font_size, (font_size * 1.4).ceil())
    }

    pub fn typed_font_stretch(&self) -> Stretch {
        Stretch::Normal
    }

    /// Paleta do tema do Chube em uso; no do sistema, a COSMIC Dark ou Light.
    pub fn syntax_theme(
        &self,
        kind: ColorSchemeKind,
        _profile_id: Option<ProfileId>,
    ) -> (String, ColorSchemeKind) {
        match Theme::current() {
            Theme::System => (theme_name(kind).to_owned(), kind),
            theme => (theme.label().to_owned(), ColorSchemeKind::Dark),
        }
    }
}

fn theme_name(kind: ColorSchemeKind) -> &'static str {
    match kind {
        ColorSchemeKind::Dark => "COSMIC Dark",
        ColorSchemeKind::Light => "COSMIC Light",
    }
}

pub fn themes() -> HashMap<(String, ColorSchemeKind), Colors> {
    let mut themes = HashMap::from([
        ((theme_name(ColorSchemeKind::Dark).to_owned(), ColorSchemeKind::Dark), terminal_theme::cosmic_dark()),
        ((theme_name(ColorSchemeKind::Light).to_owned(), ColorSchemeKind::Light), terminal_theme::cosmic_light()),
    ]);
    for theme in Theme::ALL {
        if let Some(palette) = theme.palette() {
            themes.insert(
                (theme.label().to_owned(), ColorSchemeKind::Dark),
                terminal_theme::from_palette(&palette.terminal),
            );
        }
    }
    themes
}

pub fn color_scheme_kind(theme: &cosmic::Theme) -> ColorSchemeKind {
    if theme.theme_type.is_dark() {
        ColorSchemeKind::Dark
    } else {
        ColorSchemeKind::Light
    }
}

/// Fundo padrão do terminal (o "sem cor" das aplicações): o do tema do Chube ou, no do
/// sistema, o do COSMIC.
pub fn set_background(theme: &cosmic::Theme) {
    let rgb = match Theme::current().palette() {
        Some(palette) => palette.terminal.background,
        None => {
            let color: Srgba = theme.cosmic().background(theme.transparent).base;
            let bytes = cosmic::iced::Color::from(color).into_rgba8();
            u32::from(bytes[2]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[0]) << 16)
        }
    };
    terminal::WINDOW_BG_COLOR.store(rgb | 0xFF00_0000, Ordering::SeqCst);
}

/// Troca as cores de um terminal aberto pelas do tema em uso (`theme` é o do libcosmic).
pub fn apply_theme(terminal: &mut Terminal, theme: &cosmic::Theme) {
    set_background(theme);
    terminal.set_config(&Config::default(), color_scheme_kind(theme), &themes());
}

/// Abre um terminal novo em `cwd`, rodando `command` (programa e argumentos) ou,
/// sem comando, o shell do usuário.
pub fn spawn(
    id: TabId,
    events: EventSender,
    cwd: std::path::PathBuf,
    command: Option<(String, Vec<String>)>,
    theme: &cosmic::Theme,
) -> std::io::Result<Mutex<Terminal>> {
    set_background(theme);
    let config = Config::default();
    let kind = color_scheme_kind(theme);
    let themes = themes();
    let colors = themes[&config.syntax_theme(kind, None)];
    let options = alacritty_terminal::tty::Options {
        shell: command.map(|(program, args)| alacritty_terminal::tty::Shell::new(program, args)),
        working_directory: Some(cwd),
        ..Default::default()
    };
    let mut terminal = Terminal::new(
        id,
        events,
        alacritty_terminal::term::Config::default(),
        options,
        &config,
        colors,
        None,
        None,
    )?;
    terminal.set_config(&config, kind, &themes);
    Ok(Mutex::new(terminal))
}

/// Texto das últimas `count` linhas da tela (sem o histórico nem a rolagem).
pub fn screen_tail(terminal: &Terminal, count: usize) -> Vec<String> {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};

    let term = terminal.term.lock();
    let grid = term.grid();
    let rows = grid.screen_lines();
    (rows.saturating_sub(count)..rows)
        .map(|row| {
            let line = &grid[Line(row as i32)];
            (0..grid.columns()).map(|col| line[Column(col)].c).collect::<String>()
        })
        .collect()
}
