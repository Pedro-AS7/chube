//! O que o Chube lê do Omarchy: as cores do tema atual, o modo vidro, a fonte do terminal,
//! o agente padrão e os limites de uso que o próprio Omarchy mantém atualizados.
//!
//! Tudo vem de arquivos gerados pelo Omarchy em `~/.local/state/omarchy` e `~/.config`; nada é
//! gravado lá. Sem o Omarchy (outro desktop), as funções devolvem os padrões do tema Starship.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use serde_json::Value;

use crate::claude::home_dir;
use crate::usage::{Limit, Window};

fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home_dir().join(".local/state"))
        .join("omarchy")
}

/// Pasta do tema em uso (`omarchy theme set` regenera os arquivos dela).
fn theme_dir() -> PathBuf {
    state_dir().join("current/theme")
}

/// Cor `#rrggbb` em `0xRRGGBB`.
pub type Rgb = u32;

/// As chaves do `colors.toml` de um tema do Omarchy.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeColors {
    pub light: bool,
    pub accent: Rgb,
    pub selection: Rgb,
    pub muted: Rgb,
    pub background: Rgb,
    pub lighter_background: Rgb,
    pub foreground: Rgb,
    pub bright_foreground: Rgb,
    /// Preto, vermelho, verde, amarelo, azul, magenta, ciano, branco e as versões claras,
    /// na ordem e com o mesmo mapeamento do `alacritty.toml` que o Omarchy gera.
    pub ansi: [Rgb; 16],
    /// Opacidade das janelas de terminal no modo vidro; 1 sem ele.
    pub opacity: f32,
}

impl Default for ThemeColors {
    /// O Starship de fábrica.
    fn default() -> Self {
        Self {
            light: false,
            accent: 0xff9f0a,
            selection: 0x3a3a3c,
            muted: 0x8e8e93,
            background: 0x1c1c1e,
            lighter_background: 0x2c2c2e,
            foreground: 0xf5f5f7,
            bright_foreground: 0xffffff,
            ansi: [
                0x1c1c1e, 0xff5f4a, 0xff9f0a, 0xffc27a, 0x8fb8de, 0xff8fa3, 0xa6dcef, 0xf5f5f7, //
                0x8e8e93, 0xff8373, 0xffb340, 0xffd8a8, 0xb5d0ea, 0xffb3c1, 0xcdeefa, 0xffffff,
            ],
            opacity: 1.0,
        }
    }
}

impl ThemeColors {
    /// Lê o tema atual; o que faltar fica com o padrão.
    pub fn load() -> Self {
        let dir = theme_dir();
        let mut colors = fs::read_to_string(dir.join("colors.toml"))
            .map(|text| Self::parse(&text))
            .unwrap_or_default();
        colors.opacity = fs::read_to_string(dir.join("alacritty.toml"))
            .ok()
            .and_then(|text| glass_opacity(&text))
            .unwrap_or(1.0);
        colors
    }

    pub fn parse(text: &str) -> Self {
        let mut colors = Self::default();
        let mut keys = std::collections::HashMap::new();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim().trim_matches('"');
            keys.insert(key.trim().to_owned(), value.to_owned());
        }
        let color = |key: &str| keys.get(key).and_then(|value| parse_hex(value));
        let set = |target: &mut Rgb, key: &str| {
            if let Some(value) = color(key) {
                *target = value;
            }
        };
        colors.light = keys.get("mode").is_some_and(|mode| mode == "light");
        set(&mut colors.accent, "accent");
        set(&mut colors.selection, "selection");
        set(&mut colors.muted, "muted");
        set(&mut colors.background, "background");
        set(&mut colors.lighter_background, "lighter_background");
        set(&mut colors.foreground, "foreground");
        set(&mut colors.bright_foreground, "bright_foreground");
        // Mesmo mapeamento do alacritty.toml gerado: preto é o fundo, branco é o texto,
        // preto claro é o `muted` e branco claro é o `bright_foreground`.
        let names = ["red", "green", "yellow", "blue", "magenta", "cyan"];
        colors.ansi[0] = colors.background;
        colors.ansi[7] = colors.foreground;
        colors.ansi[8] = colors.muted;
        colors.ansi[15] = colors.bright_foreground;
        for (index, name) in names.iter().enumerate() {
            set(&mut colors.ansi[index + 1], name);
            let bright = format!("bright_{name}");
            colors.ansi[index + 9] = color(&bright).unwrap_or(colors.ansi[index + 1]);
        }
        colors
    }
}

/// `#rrggbb` (com ou sem `#`).
fn parse_hex(value: &str) -> Option<Rgb> {
    let hex = value.trim().trim_start_matches('#');
    (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten()
}

/// A opacidade que o modo vidro do Omarchy põe no `alacritty.toml`:
/// `# omarchy-glass begin` / `[window]` / `opacity = 0.65` / `# omarchy-glass end`.
fn glass_opacity(alacritty: &str) -> Option<f32> {
    let mut inside = false;
    for line in alacritty.lines() {
        let line = line.trim();
        if line.starts_with("# omarchy-glass begin") {
            inside = true;
        } else if line.starts_with("# omarchy-glass end") {
            inside = false;
        } else if inside {
            if let Some(value) = line.strip_prefix("opacity").and_then(|rest| rest.trim().strip_prefix('=')) {
                return value.trim().parse::<f32>().ok().map(|value| value.clamp(0.2, 1.0));
            }
        }
    }
    None
}

/// Marca de mudança do tema: muda com `omarchy theme set` e com o modo vidro.
pub fn theme_fingerprint() -> String {
    let dir = theme_dir();
    let generation = fs::read_to_string(state_dir().join("current/theme.generation")).unwrap_or_default();
    let modified = |name: &str| {
        fs::metadata(dir.join(name))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_millis())
    };
    format!("{} {} {}", generation.trim(), modified("colors.toml"), modified("alacritty.toml"))
}

/// Tamanho da fonte do terminal do Omarchy (o foot), em pixels lógicos: `font=Nome:size=9`
/// em pontos, como no foot (9 pt = 12 px).
pub fn terminal_font_px() -> f32 {
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("foot/foot.ini");
    fs::read_to_string(path)
        .ok()
        .and_then(|text| foot_font_points(&text))
        .map_or(12.0, |points| (points * 96.0 / 72.0).round())
}

fn foot_font_points(text: &str) -> Option<f32> {
    let line = text.lines().map(str::trim).find(|line| line.starts_with("font="))?;
    let size = line.split(':').find_map(|part| part.trim().strip_prefix("size="))?;
    size.trim().parse::<f32>().ok().filter(|points| *points > 3.0 && *points < 40.0)
}

/// Fonte mono do sistema, que o `omarchy font set` troca no fontconfig.
pub fn monospace_family() -> Option<String> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let family = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!family.is_empty()).then_some(family)
}

/// Agente padrão do Omarchy (`omarchy default agent`); `None` se não foi escolhido.
pub fn default_agent() -> Option<String> {
    let output = std::process::Command::new("omarchy-default-agent")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// Pasta onde o `omarchy agent` abre: `~/Work`, se existir (os agentes não guardam a
/// confiança na home), senão a home.
pub fn work_dir() -> PathBuf {
    let work = home_dir().join("Work");
    if work.is_dir() { work } else { home_dir() }
}

/// Limites do plano que o Omarchy grava em `agents/usage/<agente>.json` (atualizados pelo
/// `omarchy agent usage update`, a cada poucos minutos), com a hora da leitura.
#[derive(Clone, Debug)]
pub struct Usage {
    pub limits: Vec<Limit>,
    /// Quando o Omarchy buscou os limites.
    pub fetched: Option<chrono::DateTime<chrono::Local>>,
    pub stale: bool,
}

pub fn usage(agent: &str) -> Option<Usage> {
    let text = fs::read_to_string(state_dir().join(format!("agents/usage/{agent}.json"))).ok()?;
    parse_usage(&text)
}

fn parse_usage(text: &str) -> Option<Usage> {
    let value: Value = serde_json::from_str(text).ok()?;
    let limits: Vec<Limit> = value["limits"]
        .as_array()?
        .iter()
        .filter_map(|limit| {
            let label = limit["label"].as_str()?;
            let window = if label.contains("5-hour") || label.starts_with("Session") {
                Window::Session
            } else if label.starts_with("Weekly") {
                Window::Week("all models".into())
            } else {
                Window::Week(limit["title"].as_str().unwrap_or(label).trim_end_matches(" Weekly").to_owned())
            };
            let resets = limit["resetsAt"]
                .as_str()
                .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
                .map(|time| time.with_timezone(&chrono::Local));
            Some(Limit { window, used: limit["percent"].as_f64()?.clamp(0.0, 1.0) as f32, resets })
        })
        .collect();
    if limits.is_empty() {
        return None;
    }
    let fetched = value["limitsFetchedAt"]
        .as_i64()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|time| time.with_timezone(&chrono::Local));
    Some(Usage { limits, fetched, stale: value["limitsStale"] == true })
}

/// Conta ativa do agente no `omarchy agent account` (nome e plano), se houver o registro.
pub fn active_account(agent: &str) -> Option<String> {
    let text = fs::read_to_string(state_dir().join(format!("agents/accounts/{agent}.json"))).ok()?;
    parse_account(&text)
}

fn parse_account(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let active = value["active"].as_str()?;
    let account = value["accounts"].as_array()?.iter().find(|account| account["id"] == active)?;
    let label = account["label"].as_str().unwrap_or(active);
    Some(match account["plan"].as_str().filter(|plan| !plan.is_empty()) {
        Some(plan) => format!("{label} ({plan})"),
        None => label.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STARSHIP: &str = r##"mode = "dark"

accent = "#7aa2f7"
selection = "#3a3a3c"
muted = "#8e8e93"

background = "#1c1c1e"
lighter_background = "#2c2c2e"

foreground = "#f5f5f7"
bright_foreground = "#ffffff"

red = "#ff5f4a"
green = "#ff9f0a"
blue = "#8fb8de"
bright_red = "#ff8373"
"##;

    #[test]
    fn le_o_colors_toml() {
        let colors = ThemeColors::parse(STARSHIP);
        assert!(!colors.light);
        assert_eq!(colors.accent, 0x7aa2f7);
        assert_eq!(colors.background, 0x1c1c1e);
        assert_eq!(colors.ansi[0], 0x1c1c1e);
        assert_eq!(colors.ansi[1], 0xff5f4a);
        assert_eq!(colors.ansi[9], 0xff8373);
        // Sem `bright_green`, a clara repete a normal.
        assert_eq!(colors.ansi[10], 0xff9f0a);
        assert_eq!(colors.ansi[7], 0xf5f5f7);
        assert_eq!(colors.ansi[8], 0x8e8e93);
        assert!(ThemeColors::parse("mode = \"light\"").light);
        // Chave estragada fica com o padrão.
        assert_eq!(ThemeColors::parse("accent = \"azul\"").accent, ThemeColors::default().accent);
    }

    #[test]
    fn opacidade_do_vidro() {
        let text = "[colors.bright]\nwhite = \"#ffffff\"\n# omarchy-glass begin\n[window]\nopacity = 0.65\n# omarchy-glass end\n";
        assert_eq!(glass_opacity(text), Some(0.65));
        assert_eq!(glass_opacity("[window]\nopacity = 0.5\n"), None);
    }

    #[test]
    fn fonte_do_foot() {
        assert_eq!(foot_font_points("[main]\nfont=JetBrainsMono Nerd Font:size=9\n"), Some(9.0));
        assert_eq!(foot_font_points("font=Mono\n"), None);
    }

    #[test]
    fn limites_do_omarchy() {
        let usage = parse_usage(r#"{"limits":[
            {"label":"Session (5-hour)","percent":0.49,"resetsAt":"2026-11-14T01:00:00.398331+00:00"},
            {"label":"Weekly (7-day)","percent":0.37,"resetsAt":"2026-11-18T08:00:00+00:00"},
            {"label":"Fable Weekly","percent":0.23,"title":"Fable Weekly"}],
            "limitsFetchedAt":1794617482525,"limitsStale":false}"#)
        .unwrap();
        assert_eq!(usage.limits.len(), 3);
        assert_eq!(usage.limits[0].window, Window::Session);
        assert_eq!(usage.limits[1].window, Window::Week("all models".into()));
        assert_eq!(usage.limits[2].window, Window::Week("Fable".into()));
        assert!((usage.limits[0].used - 0.49).abs() < 1e-6);
        assert!(usage.limits[0].resets.is_some() && usage.fetched.is_some());
        assert!(parse_usage(r#"{"limits":[]}"#).is_none());
    }

    #[test]
    fn conta_ativa() {
        let text = r#"{"active":"main","accounts":[{"id":"main","label":"Main","plan":"Pro"},{"id":"b","label":"B"}]}"#;
        assert_eq!(parse_account(text).as_deref(), Some("Main (Pro)"));
        assert_eq!(parse_account(&text.replace("\"main\",\"acc", "\"b\",\"acc")).as_deref(), Some("B"));
    }
}
