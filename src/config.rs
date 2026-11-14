//! Preferências do usuário em `$XDG_CONFIG_HOME/chube/config.json` (padrão `~/.config/chube`):
//! por enquanto, só o tema. A lista de chats abertos é estado, não preferência (`restore.rs`).

use std::path::{Path, PathBuf};

fn path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| crate::claude::home_dir().join(".config"))
        .join("chube/config.json")
}

/// Tema gravado (`omarchy` ou o id de um tema próprio); `None` se nada foi escolhido.
pub fn theme() -> Option<String> {
    theme_in(&path())
}

/// Grava o tema escolhido, mantendo as outras chaves do arquivo.
pub fn set_theme(id: &str) -> std::io::Result<()> {
    set_theme_in(&path(), id)
}

fn theme_in(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("theme")?.as_str().map(str::to_owned)
}

fn set_theme_in(path: &Path, id: &str) -> std::io::Result<()> {
    let mut value = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    value["theme"] = serde_json::Value::String(id.to_owned());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Arquivo temporário + `rename`: um desligamento no meio não deixa o arquivo pela metade.
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_string_pretty(&value).unwrap_or_default())?;
    std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grava_e_le_o_tema_sem_perder_outras_chaves() {
        let dir = std::env::temp_dir().join(format!("chube-config-{}", uuid::Uuid::new_v4()));
        let file = dir.join("chube/config.json");
        assert_eq!(theme_in(&file), None);
        set_theme_in(&file, "nord").unwrap();
        assert_eq!(theme_in(&file).as_deref(), Some("nord"));
        std::fs::write(&file, r#"{"outra": 1, "theme": "nord"}"#).unwrap();
        set_theme_in(&file, "gruvbox-dark").unwrap();
        assert_eq!(theme_in(&file).as_deref(), Some("gruvbox-dark"));
        assert!(std::fs::read_to_string(&file).unwrap().contains("outra"));
        // Arquivo estragado: volta a não ter escolha, e a próxima gravação o refaz.
        std::fs::write(&file, "{ não é json").unwrap();
        assert_eq!(theme_in(&file), None);
        set_theme_in(&file, "nord").unwrap();
        assert_eq!(theme_in(&file).as_deref(), Some("nord"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
