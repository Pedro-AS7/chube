//! Temas próprios do C-hub, para qualquer distro: opacos (sem o vidro do Omarchy) e com as
//! cores oficiais de cada esquema. O seletor fica no cabeçalho "# chats" e a escolha vai para
//! `config.rs`.

use crate::omarchy::ThemeColors;

/// Um tema embutido: id gravado na configuração, nome no seletor e cores.
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: ThemeColors,
}

/// Tema usado fora do Omarchy quando nada foi escolhido.
pub const DEFAULT: &str = "tokyo-night";

pub const BUILTINS: [Builtin; 5] = [
    Builtin {
        id: "tokyo-night",
        name: "Tokyo Night",
        colors: ThemeColors {
            light: false,
            accent: 0x7aa2f7,
            selection: 0x283457,
            // O `comment` (#565f89) some sobre o fundo nas linhas apagadas da interface.
            muted: 0x737aa2,
            background: 0x1a1b26,
            lighter_background: 0x24283b,
            foreground: 0xc0caf5,
            bright_foreground: 0xc0caf5,
            ansi: [
                0x15161e, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xa9b1d6, //
                0x414868, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5,
            ],
            opacity: 1.0,
        },
    },
    Builtin {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        colors: ThemeColors {
            light: false,
            accent: 0xcba6f7,
            selection: 0x45475a,
            muted: 0x7f849c,
            background: 0x1e1e2e,
            lighter_background: 0x313244,
            foreground: 0xcdd6f4,
            bright_foreground: 0xcdd6f4,
            ansi: [
                0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de, //
                0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xa6adc8,
            ],
            opacity: 1.0,
        },
    },
    Builtin {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        colors: ThemeColors {
            light: true,
            accent: 0x8839ef,
            selection: 0xbcc0cc,
            // O overlay1 (#8c8fa1) fica com contraste 2,8 sobre o fundo claro; o overlay2 dá 3,5.
            muted: 0x7c7f93,
            background: 0xeff1f5,
            lighter_background: 0xe6e9ef,
            foreground: 0x4c4f69,
            bright_foreground: 0x4c4f69,
            ansi: [
                0x5c5f77, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xacb0be, //
                0x6c6f85, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xbcc0cc,
            ],
            opacity: 1.0,
        },
    },
    Builtin {
        id: "gruvbox-dark",
        name: "Gruvbox",
        colors: ThemeColors {
            light: false,
            accent: 0xfabd2f,
            selection: 0x504945,
            muted: 0x928374,
            background: 0x282828,
            lighter_background: 0x3c3836,
            foreground: 0xebdbb2,
            bright_foreground: 0xfbf1c7,
            ansi: [
                0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984, //
                0x928374, 0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xebdbb2,
            ],
            opacity: 1.0,
        },
    },
    Builtin {
        id: "nord",
        name: "Nord",
        colors: ThemeColors {
            light: false,
            accent: 0x88c0d0,
            selection: 0x434c5e,
            // O nord3 (#4c566a) quase some sobre o fundo; este é o tom dos comentários do Nord.
            muted: 0x7b88a1,
            background: 0x2e3440,
            lighter_background: 0x3b4252,
            foreground: 0xd8dee9,
            bright_foreground: 0xeceff4,
            ansi: [
                0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0, //
                0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
            ],
            opacity: 1.0,
        },
    },
];

pub fn find(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Luminância relativa (WCAG) de `0xRRGGBB`.
    fn luminance(hex: u32) -> f64 {
        let channel = |shift: u32| {
            let value = f64::from((hex >> shift) & 0xff) / 255.0;
            if value <= 0.03928 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (high, low) = if luminance(a) > luminance(b) { (luminance(a), luminance(b)) } else { (luminance(b), luminance(a)) };
        (high + 0.05) / (low + 0.05)
    }

    #[test]
    fn temas_tem_contraste_para_ler() {
        assert!(find(DEFAULT).is_some());
        for builtin in &BUILTINS {
            let colors = &builtin.colors;
            assert_eq!(colors.opacity, 1.0, "{}: os temas próprios são opacos", builtin.id);
            // Texto normal: o mínimo do WCAG AA. Apagado e acento: legíveis como texto grande.
            assert!(contrast(colors.foreground, colors.background) >= 4.5, "{}: texto", builtin.id);
            assert!(contrast(colors.muted, colors.background) >= 3.0, "{}: apagado", builtin.id);
            assert!(contrast(colors.accent, colors.background) >= 3.0, "{}: acento", builtin.id);
            assert_eq!(colors.light, luminance(colors.background) > 0.5, "{}: claro/escuro", builtin.id);
        }
    }

    #[test]
    fn ids_unicos() {
        for (index, builtin) in BUILTINS.iter().enumerate() {
            assert!(BUILTINS[index + 1..].iter().all(|other| other.id != builtin.id));
        }
    }
}
