//! Argumentos da linha de comando. Com o app já aberto, o GApplication manda a linha de
//! comando à instância aberta pelo D-Bus, e ela é lida lá (`connect_command_line`).
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::agent::Agent;

#[derive(Clone, Debug, Default)]
pub struct Flags {
    pub agent: Agent,
    /// `--agent omarchy`: o agente padrão do Omarchy, no modo do `omarchy agent`.
    pub auto: bool,
    pub model: Option<String>,
    /// Caminhos absolutos, já validados no processo que recebeu a chamada do Files.
    pub directories: Vec<String>,
}

pub fn parse(args: impl IntoIterator<Item = impl Into<OsString>>) -> Result<Flags, String> {
    parse_in(args, None)
}

/// Como `parse`, com os caminhos relativos resolvidos em `base`: a pasta de quem chamou, quando a
/// linha de comando chega de outro processo (o app já aberto recebe a chamada do Files).
pub fn parse_in(args: impl IntoIterator<Item = impl Into<OsString>>, base: Option<&Path>) -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut args = args.into_iter().map(|arg| {
        arg.into().into_string().map_err(|_| "O caminho precisa ser UTF-8".to_owned())
    });
    while let Some(arg) = args.next() {
        let arg = arg?;
        match arg.as_str() {
            "--agent" => {
                let value = args.next().ok_or("Falta o agente depois de --agent")??;
                if value == "omarchy" {
                    flags.auto = true;
                    flags.agent = crate::omarchy::default_agent()
                        .and_then(|name| Agent::parse(&name))
                        .ok_or("O agente padrão do Omarchy não é o Claude Code nem o Codex")?;
                } else {
                    flags.agent = Agent::parse(&value).ok_or_else(|| format!("Agente inválido: {value}"))?;
                }
            }
            "--model" => flags.model = Some(args.next().ok_or("Falta o modelo depois de --model")??),
            "--open-directory" => {
                flags.directories.push(args.next().ok_or("Falta a pasta depois de --open-directory")??);
            }
            "--" => {
                for directory in args { flags.directories.push(directory?); }
                break;
            }
            _ => return Err(format!("Argumento desconhecido: {arg}")),
        }
    }
    for directory in &mut flags.directories {
        let path = PathBuf::from(&*directory);
        let path = match base {
            Some(base) if path.is_relative() => base.join(path),
            _ => path,
        };
        let path = path.canonicalize()
            .map_err(|error| format!("Não consegui abrir {directory}: {error}"))?;
        if !path.is_dir() { return Err(format!("{directory} não é uma pasta")); }
        *directory = path.into_os_string().into_string()
            .map_err(|_| "O caminho precisa ser UTF-8".to_owned())?;
    }
    Ok(flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_does_not_create_a_session() {
        let flags = parse(Vec::<String>::new()).unwrap();
        assert!(flags.directories.is_empty());
        assert_eq!(flags.agent, Agent::Claude);
    }

    #[test]
    fn directory_with_odd_characters_and_agent() {
        let base = std::env::temp_dir().join(format!("chube-cli-{}", uuid::Uuid::new_v4()));
        let folder = base.join("pasta com espaço, acentuação e $() ' \"");
        std::fs::create_dir_all(&folder).unwrap();
        let input = folder.to_str().unwrap();
        let flags = parse(["--agent", "codex", "--model", "modelo", "--open-directory", input]).unwrap();
        let received = flags;
        assert_eq!(received.agent, Agent::Codex);
        assert_eq!(received.model.as_deref(), Some("modelo"));
        assert_eq!(received.directories, vec![input]);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn resolves_relative_paths_before_sending_to_other_process() {
        let flags = parse(["--open-directory", "."]).unwrap();
        assert_eq!(PathBuf::from(&flags.directories[0]), std::env::current_dir().unwrap().canonicalize().unwrap());
    }

    #[test]
    fn relative_paths_use_the_caller_directory() {
        let base = std::env::temp_dir();
        let flags = parse_in(["--open-directory", "."], Some(&base)).unwrap();
        assert_eq!(PathBuf::from(&flags.directories[0]), base.canonicalize().unwrap());
    }

    #[test]
    fn accepts_multiple_directories_and_explicit_claude() {
        let flags = parse(["--agent", "claude", "--", ".", "src"]).unwrap();
        assert_eq!(flags.agent, Agent::Claude);
        assert_eq!(flags.directories.len(), 2);
        assert!(flags.directories.iter().all(|path| PathBuf::from(path).is_absolute()));
    }

    #[test]
    fn rejects_invalid_requests_before_starting_an_app() {
        for input in [vec!["--agent"], vec!["--agent", "invalid"], vec!["--model"],
            vec!["--open-directory"], vec!["--open-directory", "Cargo.toml"],
            vec!["--open-directory", "/pasta-chube-que-nao-existe"], vec!["--unknown"]] {
            assert!(parse(input).is_err());
        }
    }
}
