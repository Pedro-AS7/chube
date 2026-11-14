//! Cores do Chube: as do tema atual do Omarchy (`omarchy.rs`), relidas quando o tema muda.
//!
//! A paleta fica num `static` porque as funções de estilo dos widgets não recebem o `App`.
//! Os tokens seguem o `shell.toml` do Omarchy (menus, controles e popups do shell): linhas
//! retas, seleção com fundo do texto a 8% e texto no acento, bordas do texto a 25–40%.

use std::sync::{Arc, RwLock};

use cosmic::cosmic_theme::ThemeBuilder;
use cosmic::cosmic_theme::palette::Srgb;
use cosmic::iced::Color;

use crate::omarchy::{Rgb, ThemeColors};

/// Paleta em uso.
pub struct Palette {
    pub light: bool,
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
            light: colors.light,
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

    /// Cor do texto com outra opacidade (os tokens `*-alpha` do shell do Omarchy).
    pub fn text_alpha(&self, alpha: f32) -> Color {
        Color { a: alpha, ..self.text }
    }

    /// Tema do libcosmic com as mesmas cores, para os widgets dele (campos de texto, dicas,
    /// barras de rolagem). Sem cantos arredondados, como tudo no Omarchy.
    pub fn cosmic(&self) -> cosmic::Theme {
        let builder = if self.light { ThemeBuilder::light() } else { ThemeBuilder::dark() };
        let square = cosmic::cosmic_theme::CornerRadii {
            radius_0: [0.0; 4],
            radius_xs: [0.0; 4],
            radius_s: [0.0; 4],
            radius_m: [0.0; 4],
            radius_l: [0.0; 4],
            radius_xl: [0.0; 4],
        };
        let builder = builder
            .bg_color(srgb(self.background).into())
            .text_tint(srgb(self.text))
            .accent(srgb(self.accent))
            .success(srgb(self.success))
            .warning(srgb(self.warning))
            .destructive(srgb(self.danger))
            .neutral_tint(srgb(self.dim))
            .corner_radii(square);
        cosmic::Theme::custom(Arc::new(builder.build()))
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
    // A interface e os terminais usam a família "monospace" do cosmic-text, que por padrão não é
    // a do fontconfig: sem isto, sairia a Noto/DejaVu Sans Mono no lugar da fonte do Omarchy.
    if let Some(family) = crate::omarchy::monospace_family() {
        if let Ok(mut fonts) = cosmic::iced::advanced::graphics::text::font_system().write() {
            fonts.raw().db_mut().set_monospace_family(family);
        }
    }
    let palette = Arc::new(Palette::new(ThemeColors::load(), crate::omarchy::terminal_font_px()));
    if let Ok(mut current) = CURRENT.write() {
        *current = Some(palette.clone());
    }
    palette
}

pub const fn rgb(hex: Rgb) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn srgb(color: Color) -> Srgb {
    Srgb::new(color.r, color.g, color.b)
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
