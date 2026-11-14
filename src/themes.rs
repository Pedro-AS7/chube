//! Cores do Chube: as do tema atual do Omarchy (`omarchy.rs`), relidas quando o tema muda.
//!
//! A paleta fica num `static` para ser lida de qualquer parte da interface (CSS e terminais).
//! Os tokens seguem o `shell.toml` do Omarchy (menus, controles e popups do shell): linhas
//! retas, seleção com fundo do texto a 8% e texto no acento, bordas do texto a 25–40%.

use std::sync::{Arc, RwLock};

use crate::omarchy::{Rgb, ThemeColors};

/// Cor RGBA com componentes de 0 a 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// `rgba(…)` para o CSS do GTK.
    pub fn css(self) -> String {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("rgba({}, {}, {}, {:.3})", byte(self.r), byte(self.g), byte(self.b), self.a)
    }

    pub fn rgba(self) -> gtk4::gdk::RGBA {
        gtk4::gdk::RGBA::new(self.r, self.g, self.b, self.a)
    }
}

/// Paleta em uso.
pub struct Palette {
    /// Fundo da janela, opaco; `alpha` é a opacidade do modo vidro.
    pub background: Color,
    pub alpha: f32,
    pub selection: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    /// Tamanho da fonte do terminal (a do foot do Omarchy), em pixels.
    pub font_px: f32,
    pub colors: ThemeColors,
}

impl Palette {
    pub fn new(colors: ThemeColors, font_px: f32) -> Self {
        Self {
            background: rgb(colors.background),
            alpha: colors.opacity,
            selection: rgb(colors.selection),
            text: rgb(colors.foreground),
            dim: rgb(colors.muted),
            accent: rgb(colors.accent),
            danger: rgb(colors.ansi[1]),
            success: rgb(colors.ansi[2]),
            warning: rgb(colors.ansi[3]),
            font_px,
            colors,
        }
    }

    /// Fundo da janela com a opacidade do modo vidro (o Hyprland desfoca o que fica atrás).
    pub fn window(&self) -> Color {
        Color { a: self.alpha, ..self.background }
    }
}

static CURRENT: RwLock<Option<Arc<Palette>>> = RwLock::new(None);

/// Paleta em uso; lê o tema do Omarchy na primeira chamada.
pub fn current() -> Arc<Palette> {
    if let Some(palette) = CURRENT.read().ok().and_then(|current| current.clone()) {
        return palette;
    }
    reload()
}

/// Relê o tema do Omarchy (depois de um `omarchy theme set` ou do modo vidro).
pub fn reload() -> Arc<Palette> {
    let palette = Arc::new(Palette::new(ThemeColors::load(), crate::omarchy::terminal_font_px()));
    if let Ok(mut current) = CURRENT.write() {
        *current = Some(palette.clone());
    }
    palette
}

pub const fn rgb(hex: Rgb) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paleta_segue_o_tema() {
        let colors = ThemeColors { accent: 0x7aa2f7, opacity: 0.65, ..ThemeColors::default() };
        let palette = Palette::new(colors, 12.0);
        assert_eq!(palette.accent, rgb(0x7aa2f7));
        assert_eq!(palette.window().a, 0.65);
        assert_eq!(palette.background.a, 1.0);
        // O verde do tema (no Starship, laranja) marca o "pronto".
        assert_eq!(palette.success, rgb(0xff9f0a));
    }
}
