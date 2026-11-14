// Adaptado de cosmic-term (src/terminal_theme.rs), © System76, GPL-3.0-only.
// Só as paletas COSMIC Dark/Light (sem uso desde o tema do Omarchy); temas em arquivo e
// exportação ficaram de fora. `from_palette` (cores do tema do Omarchy) não existe no cosmic-term.

use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{NamedColor, Rgb},
};

fn auto_colors() -> Colors {
    let mut colors = Colors::default();

    // These colors come from `ransid`: https://gitlab.redox-os.org/redox-os/ransid/-/blob/master/src/color.rs
    /* Indexed colors */
    for value in 16..=231 {
        let convert = |value: u8| -> u8 {
            match value {
                0 => 0,
                _ => value * 0x28 + 0x28,
            }
        };

        let r = convert((value - 16) / 36 % 6);
        let g = convert((value - 16) / 6 % 6);
        let b = convert((value - 16) % 6);
        colors[value as usize] = Some(Rgb { r, g, b });
    }

    /* Grays */
    for value in 232..=255 {
        let gray = (value - 232) * 10 + 8;
        colors[value as usize] = Some(Rgb {
            r: gray,
            g: gray,
            b: gray,
        });
    }

    colors
}

#[allow(dead_code)]
pub fn cosmic_dark() -> Colors {
    let mut colors = auto_colors();

    let encode_rgb = |data: u32| -> Rgb {
        Rgb {
            r: (data >> 16) as u8,
            g: (data >> 8) as u8,
            b: data as u8,
        }
    };

    colors[NamedColor::Black] = Some(encode_rgb(0x1B1B1B));
    colors[NamedColor::Red] = Some(encode_rgb(0xF16161));
    colors[NamedColor::Green] = Some(encode_rgb(0x7CB987));
    colors[NamedColor::Yellow] = Some(encode_rgb(0xDDC74C));
    colors[NamedColor::Blue] = Some(encode_rgb(0x6296BE));
    colors[NamedColor::Magenta] = Some(encode_rgb(0xBE6DEE));
    colors[NamedColor::Cyan] = Some(encode_rgb(0x49BAC8));
    colors[NamedColor::White] = Some(encode_rgb(0xBEBEBE));

    colors[NamedColor::BrightBlack] = Some(encode_rgb(0x808080));
    colors[NamedColor::BrightRed] = Some(encode_rgb(0xFF8985));
    colors[NamedColor::BrightGreen] = Some(encode_rgb(0x97D5A0));
    colors[NamedColor::BrightYellow] = Some(encode_rgb(0xFAE365));
    colors[NamedColor::BrightBlue] = Some(encode_rgb(0x7DB1DA));
    colors[NamedColor::BrightMagenta] = Some(encode_rgb(0xD68EFF));
    colors[NamedColor::BrightCyan] = Some(encode_rgb(0x49BAC8));
    colors[NamedColor::BrightWhite] = Some(encode_rgb(0xC4C4C4));

    colors[NamedColor::DimBlack] = Some(encode_rgb(0x000000));
    colors[NamedColor::DimRed] = Some(encode_rgb(0xA04040));
    colors[NamedColor::DimGreen] = Some(encode_rgb(0x5D7D62));
    colors[NamedColor::DimYellow] = Some(encode_rgb(0x9E914A));
    colors[NamedColor::DimBlue] = Some(encode_rgb(0x486073));
    colors[NamedColor::DimMagenta] = Some(encode_rgb(0x7F46A1));
    colors[NamedColor::DimCyan] = Some(encode_rgb(0x3F7F87));
    colors[NamedColor::DimWhite] = Some(encode_rgb(0x898989));

    // Set special colors
    colors[NamedColor::Foreground] = colors[NamedColor::BrightWhite];
    // Background comes from theme settings: colors[NamedColor::Background] = colors[NamedColor::Black];
    colors[NamedColor::Cursor] = colors[NamedColor::BrightWhite];
    colors[NamedColor::BrightForeground] = colors[NamedColor::BrightWhite];
    colors[NamedColor::DimForeground] = colors[NamedColor::DimWhite];

    colors
}

#[allow(dead_code)]
pub fn cosmic_light() -> Colors {
    let mut colors = auto_colors();

    let encode_rgb = |data: u32| -> Rgb {
        Rgb {
            r: (data >> 16) as u8,
            g: (data >> 8) as u8,
            b: data as u8,
        }
    };

    colors[NamedColor::Black] = Some(encode_rgb(0x1B1B1B));
    colors[NamedColor::Red] = Some(encode_rgb(0xA24F50));
    colors[NamedColor::Green] = Some(encode_rgb(0x437E4F));
    colors[NamedColor::Yellow] = Some(encode_rgb(0x7D6E1E));
    colors[NamedColor::Blue] = Some(encode_rgb(0x516D94));
    colors[NamedColor::Magenta] = Some(encode_rgb(0x9A30CA));
    colors[NamedColor::Cyan] = Some(encode_rgb(0x227A85));
    colors[NamedColor::White] = Some(encode_rgb(0xD7D7D7));

    colors[NamedColor::BrightBlack] = Some(encode_rgb(0x000000));
    colors[NamedColor::BrightRed] = Some(encode_rgb(0x890418));
    colors[NamedColor::BrightGreen] = Some(encode_rgb(0x185529));
    colors[NamedColor::BrightYellow] = Some(encode_rgb(0x534800));
    colors[NamedColor::BrightBlue] = Some(encode_rgb(0x2E496D));
    colors[NamedColor::BrightMagenta] = Some(encode_rgb(0x6B0091));
    colors[NamedColor::BrightCyan] = Some(encode_rgb(0x00525A));
    colors[NamedColor::BrightWhite] = Some(encode_rgb(0xE8E8E8));

    colors[NamedColor::DimBlack] = Some(encode_rgb(0x262626));
    colors[NamedColor::DimRed] = Some(encode_rgb(0xB25D5E));
    colors[NamedColor::DimGreen] = Some(encode_rgb(0x528D5E));
    colors[NamedColor::DimYellow] = Some(encode_rgb(0x8C7D30));
    colors[NamedColor::DimBlue] = Some(encode_rgb(0x5F7CA4));
    colors[NamedColor::DimMagenta] = Some(encode_rgb(0xAA43DB));
    colors[NamedColor::DimCyan] = Some(encode_rgb(0x358994));
    colors[NamedColor::DimWhite] = Some(encode_rgb(0xC4C4C4));

    // Set special colors
    colors[NamedColor::Foreground] = colors[NamedColor::Black];
    // Background comes from theme settings: colors[NamedColor::Background] = colors[NamedColor::BrightWhite];
    colors[NamedColor::Cursor] = colors[NamedColor::Black];
    colors[NamedColor::BrightForeground] = colors[NamedColor::BrightBlack];
    colors[NamedColor::DimForeground] = colors[NamedColor::DimBlack];

    colors
}

/// Paleta do tema do Omarchy. As cores apagadas (`Dim*`) são as normais a 2/3.
pub fn from_palette(palette: &crate::omarchy::ThemeColors) -> Colors {
    let mut colors = auto_colors();

    let encode_rgb = |data: u32| -> Rgb {
        Rgb {
            r: (data >> 16) as u8,
            g: (data >> 8) as u8,
            b: data as u8,
        }
    };
    let dim = |data: u32| -> Rgb {
        let rgb = encode_rgb(data);
        Rgb {
            r: (u16::from(rgb.r) * 2 / 3) as u8,
            g: (u16::from(rgb.g) * 2 / 3) as u8,
            b: (u16::from(rgb.b) * 2 / 3) as u8,
        }
    };

    let normal = [
        NamedColor::Black,
        NamedColor::Red,
        NamedColor::Green,
        NamedColor::Yellow,
        NamedColor::Blue,
        NamedColor::Magenta,
        NamedColor::Cyan,
        NamedColor::White,
    ];
    let bright = [
        NamedColor::BrightBlack,
        NamedColor::BrightRed,
        NamedColor::BrightGreen,
        NamedColor::BrightYellow,
        NamedColor::BrightBlue,
        NamedColor::BrightMagenta,
        NamedColor::BrightCyan,
        NamedColor::BrightWhite,
    ];
    let dimmed = [
        NamedColor::DimBlack,
        NamedColor::DimRed,
        NamedColor::DimGreen,
        NamedColor::DimYellow,
        NamedColor::DimBlue,
        NamedColor::DimMagenta,
        NamedColor::DimCyan,
        NamedColor::DimWhite,
    ];
    for i in 0..8 {
        colors[normal[i]] = Some(encode_rgb(palette.ansi[i]));
        colors[bright[i]] = Some(encode_rgb(palette.ansi[i + 8]));
        colors[dimmed[i]] = Some(dim(palette.ansi[i]));
    }

    colors[NamedColor::Foreground] = Some(encode_rgb(palette.foreground));
    colors[NamedColor::Cursor] = Some(encode_rgb(palette.bright_foreground));
    colors[NamedColor::BrightForeground] = Some(encode_rgb(palette.bright_foreground));
    colors[NamedColor::DimForeground] = Some(dim(palette.foreground));

    colors
}
