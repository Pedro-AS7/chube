//! Sessões em outra máquina por SSH (a VPS), cada uma dentro de um tmux de lá.
//!
//! O chat roda `ssh -t <host> … tmux new-session -A -s <nome> …`: o `-A` entra na sessão do
//! tmux se ela ainda existe e, se não, cria uma rodando o agente. Assim, fechar o chat, fechar o
//! Chube ou cair a rede só desconecta: o agente continua na VPS, e reabrir o chat volta a ele.
//! Também dá para entrar pelo celular (`tmux attach -t <nome>`).
//!
//! As conexões compartilham um mestre do SSH (`ControlMaster`, socket em `$XDG_RUNTIME_DIR`): a
//! senha da chave é pedida uma vez, no terminal do primeiro chat, e as consultas em segundo
//! plano (sessões salvas, tmux abertos) reaproveitam a conexão sem pedir nada. Sem o mestre e
//! sem um agente SSH com a chave, essas consultas falham com uma mensagem, e o chat pede a senha.
//!
//! Nada é gravado na VPS além do que o agente e o tmux já gravam. As opções do tmux valem só
//! para as sessões criadas pelo Chube (`set-option -t`), menos a de cor (`terminal-features`),
//! que é do servidor do tmux e só acrescenta o suporte a truecolor.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use crate::agent::Agent;
use crate::claude::home_dir;
use crate::sessions::SavedSession;

/// Tempo máximo de uma consulta em segundo plano.
const TIMEOUT: Duration = Duration::from_secs(20);
/// Prefixo das sessões do tmux criadas pelo Chube.
pub const TMUX_PREFIX: &str = "chube-";

/// Hosts do `~/.ssh/config` (só os nomes exatos, sem `*` nem `?`).
pub fn hosts() -> Vec<String> {
    std::fs::read_to_string(home_dir().join(".ssh/config"))
        .map(|text| parse_hosts(&text))
        .unwrap_or_default()
}

fn parse_hosts(config: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some((key, rest)) = line.split_once(char::is_whitespace) else { continue };
        if !key.eq_ignore_ascii_case("host") {
            continue;
        }
        for name in rest.split_whitespace() {
            let name = name.trim_matches('"');
            if !name.contains(['*', '?', '!']) && !hosts.iter().any(|host| host == name) {
                hosts.push(name.to_owned());
            }
        }
    }
    hosts
}

/// Nome novo para a sessão do tmux de um chat.
pub fn new_tmux_name() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{TMUX_PREFIX}{}", &id[..8])
}

/// Aspas simples do shell: o texto passa literal, mesmo com espaços, `$` ou aspas.
pub fn quote(text: &str) -> String {
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c)) {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Pasta da VPS para o `cd`: `~` e `~/…` viram `"$HOME"…` (o til entre aspas não expande).
fn quote_dir(dir: &str) -> String {
    match dir.strip_prefix('~') {
        Some("") => "\"$HOME\"".into(),
        Some(rest) if rest.starts_with('/') => format!("\"$HOME\"{}", quote(rest)),
        _ => quote(dir),
    }
}

/// Opções de SSH de todas as conexões do Chube: o mestre compartilhado e um aviso de vida, para
/// uma rede caída fechar o chat em vez de deixá-lo congelado.
fn ssh_options() -> Vec<String> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let socket = runtime.join("chube-ssh-%C");
    [
        "ControlMaster=auto".to_owned(),
        format!("ControlPath={}", socket.display()),
        "ControlPersist=15m".to_owned(),
        "ServerAliveInterval=15".to_owned(),
        "ServerAliveCountMax=4".to_owned(),
    ]
    .into_iter()
    .flat_map(|option| ["-o".to_owned(), option])
    .collect()
}

/// Linha de comando (para o shell da VPS) que roda o agente. Num login sem terminal, o PATH do
/// `bash -l` pode não ter `~/.local/bin`, onde o instalador do Claude Code põe o `claude`.
pub fn agent_shell_command(agent: Agent, args: &[String], fallback: Option<&[String]>) -> String {
    let (binary, unset) = match agent {
        Agent::Claude => ("claude", "-u ANTHROPIC_API_KEY"),
        Agent::Codex => ("codex", "-u OPENAI_API_KEY -u CODEX_API_KEY"),
    };
    let run = |args: &[String]| {
        let args: Vec<String> = args.iter().map(|arg| quote(arg)).collect();
        format!("env {unset} {binary} {}", args.join(" ")).trim_end().to_owned()
    };
    let mut command = format!("export PATH=\"$HOME/.local/bin:$PATH\"; {}", run(args));
    // Uma sessão nova cujo histórico não chegou a ser gravado não tem o que retomar: começa de
    // novo com o mesmo id.
    if let Some(fallback) = fallback {
        command = format!("{command} || {}", run(fallback));
    }
    command
}

/// Programa e argumentos do chat: `ssh -t` que entra na sessão do tmux (ou a cria rodando
/// `agent_command` na pasta `dir`).
pub fn command(host: &str, dir: &str, tmux: &str, agent_command: Option<&str>) -> (String, Vec<String>) {
    (
        "ssh".into(),
        ssh_args(host, true, &remote_script(dir, tmux, agent_command)),
    )
}

fn ssh_args(host: &str, tty: bool, script: &str) -> Vec<String> {
    let mut args = Vec::new();
    if tty {
        args.push("-t".to_owned());
    } else {
        args.extend(["-o".to_owned(), "BatchMode=yes".to_owned(), "-T".to_owned()]);
    }
    args.extend(ssh_options());
    args.extend(["--".to_owned(), host.to_owned(), script.to_owned()]);
    args
}

/// Script do shell da VPS. Sem `agent_command`, só entra numa sessão do tmux que já existe
/// (uma aberta pelo celular, por exemplo).
fn remote_script(dir: &str, tmux: &str, agent_command: Option<&str>) -> String {
    let name = quote(tmux);
    // Só nas sessões do Chube: sem a barra (o Chube já mostra o título), mouse para rolar o
    // histórico com a roda, e o título do agente repassado ao terminal (dá o status do chat).
    let options = [
        format!("set-option -t {name} status off"),
        format!("set-option -t {name} mouse on"),
        format!("set-option -t {name} set-titles on"),
        format!("set-option -t {name} set-titles-string '#{{pane_title}}'"),
        "set-option -as terminal-features ',xterm-256color:RGB'".to_owned(),
    ]
    .join(" \\; ");
    match agent_command {
        Some(agent) => format!(
            "cd {dir} 2>/dev/null || cd; exec tmux new-session -A -s {name} -e COLORTERM=truecolor bash -lc {agent} \\; {options}",
            dir = quote_dir(dir),
            agent = quote(agent),
        ),
        None => format!("exec tmux attach-session -t {name} \\; {options}"),
    }
}

/// Roda um script na VPS sem terminal e sem pedir senha (usa o mestre do SSH ou o agente SSH).
async fn run(host: &str, script: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("ssh")
        .args(ssh_args(host, false, script))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(TIMEOUT, output)
        .await
        .map_err(|_| format!("{host} não respondeu"))?
        .map_err(|error| format!("não consegui rodar o ssh: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(if stderr.contains("Permission denied") {
        format!("sem acesso a {host} sem senha: abra um chat lá (o terminal pede a senha da chave) e tente de novo")
    } else {
        stderr.lines().last().unwrap_or("erro no ssh").trim().to_owned()
    })
}

/// Encerra a sessão do tmux de um chat (e o agente dentro dela).
pub async fn kill(host: String, tmux: String) -> Result<(), String> {
    run(&host, &format!("tmux kill-session -t {}", quote(&tmux))).await.map(|_| ())
}

/// Estado de um chat da VPS, lido lá: o que o `~/.claude/sessions/<pid>.json` do `claude` do tmux
/// diz (id e status) e a última resposta do histórico (modelo e contexto).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemoteStatus {
    pub tmux: String,
    pub session_id: Option<String>,
    /// `busy`, `idle` ou `waiting`, como nesta máquina.
    pub status: Option<String>,
    pub model: Option<String>,
    pub context_tokens: u32,
}

/// Lê o estado dos chats do Claude Code nos tmux `names` da VPS, numa consulta só.
///
/// Só usa uma conexão mestre que já existe (a do terminal do chat): sem ela, não faz nada, para
/// uma chave trancada no agente SSH não abrir a janela de senha a cada poucos segundos.
pub async fn poll(host: String, names: Vec<String>) -> Result<Vec<RemoteStatus>, String> {
    let mut check = tokio::process::Command::new("ssh");
    check.args(["-O", "check"]).args(ssh_options()).args(["--", &host]);
    let master = check
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(Duration::from_secs(3), master).await {
        Ok(Ok(status)) if status.success() => {}
        _ => return Err("sem conexão aberta".into()),
    }
    let text = run(&host, &poll_script(&names)).await?;
    Ok(parse_poll(&text))
}

/// Para cada tmux: o `claude` é o processo do painel ou um filho dele (no chat do Chube, o `env`
/// que roda o `claude` é filho do `bash -lc`; num tmux de fora, o `claude` é filho do shell).
fn poll_script(names: &[String]) -> String {
    const FUNCTION: &str = r#"chube_poll() {
  t=$1; sid=; st=; l=
  pp=$(tmux display-message -p -t "$t" '#{pane_pid}' 2>/dev/null) || return 0
  f=
  for p in $pp $(pgrep -P "$pp" 2>/dev/null); do
    [ -f "$HOME/.claude/sessions/$p.json" ] && { f="$HOME/.claude/sessions/$p.json"; break; }
  done
  if [ -n "$f" ]; then
    sid=$(grep -o '"sessionId":"[^"]*"' "$f" | head -n 1 | cut -d '"' -f 4)
    st=$(grep -o '"status":"[^"]*"' "$f" | head -n 1 | cut -d '"' -f 4)
  fi
  if [ -n "$sid" ]; then
    jf=$(ls "$HOME"/.claude/projects/*/"$sid".jsonl 2>/dev/null | head -n 1)
    [ -n "$jf" ] && l=$(tail -c 524288 "$jf" | grep '"type":"assistant"' | grep -v '"isSidechain":true' | tail -n 1)
  fi
  m=$(printf '%s' "$l" | grep -o '"model":"[^"]*"' | head -n 1 | cut -d '"' -f 4)
  u=$(printf '%s' "$l" | grep -o '"usage":{[^}]*' | head -n 1)
  printf 'C	%s	%s	%s	%s	%s
' "$t" "$sid" "$st" "$m" "$u"
}
"#;
    let calls: Vec<String> = names.iter().map(|name| format!("chube_poll {}", quote(name))).collect();
    format!("{FUNCTION}{}
", calls.join("
"))
}

fn parse_poll(text: &str) -> Vec<RemoteStatus> {
    text.lines()
        .filter_map(|line| line.strip_prefix("C\t"))
        .filter_map(|rest| {
            let fields: Vec<&str> = rest.splitn(5, '\t').collect();
            let field = |index: usize| fields.get(index).map(|value| value.trim()).filter(|value| !value.is_empty());
            Some(RemoteStatus {
                tmux: field(0)?.to_owned(),
                session_id: field(1).map(str::to_owned),
                status: field(2).map(str::to_owned),
                model: field(3).map(str::to_owned),
                context_tokens: field(4).map_or(0, usage_tokens),
            })
        })
        .collect()
}

/// Tokens de contexto no começo do `usage` cortado pelo `grep -o` (até o primeiro `}`, o de um
/// objeto aninhado): a entrada, a criação e a leitura do cache, como nesta máquina.
fn usage_tokens(usage: &str) -> u32 {
    let body = usage.strip_prefix("\"usage\":{").unwrap_or(usage);
    body.split(',')
        .filter_map(|pair| pair.split_once(':'))
        .filter(|(key, _)| {
            matches!(
                key.trim().trim_matches('"'),
                "input_tokens" | "cache_creation_input_tokens" | "cache_read_input_tokens"
            )
        })
        .filter_map(|(_, value)| value.trim().parse::<u64>().ok())
        .sum::<u64>()
        .min(u64::from(u32::MAX)) as u32
}

/// Sessão do tmux aberta na VPS.
#[derive(Clone, Debug, PartialEq)]
pub struct TmuxSession {
    pub name: String,
    pub path: String,
    /// Título do painel (o agente põe o assunto da conversa).
    pub title: String,
    pub attached: bool,
}

/// O que a VPS tem: sessões salvas do Claude Code e sessões do tmux abertas.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub sessions: Vec<SavedSession>,
    pub tmux: Vec<TmuxSession>,
}

/// Lista as sessões da VPS. Só lê os campos usados na lista (pasta e título), com `grep -o`,
/// para não trazer o histórico inteiro pela rede.
pub async fn list(host: String) -> Result<Listing, String> {
    const SCRIPT: &str = r#"
printf 'H\t%s\n' "$HOME"
tmux list-sessions -F 'T	#{session_name}	#{session_attached}	#{pane_current_path}	#{pane_title}' 2>/dev/null
cd ~/.claude/projects 2>/dev/null || exit 0
ls -t -- */*.jsonl 2>/dev/null | head -n 150 | while IFS= read -r f; do
  printf 'F\t%s\t%s\n' "$(stat -c %Y -- "$f")" "$f"
  grep -m1 -o '"cwd":"[^"]*"' -- "$f"
  grep -o '"\(customTitle\|aiTitle\|lastPrompt\)":"[^"]*' -- "$f" | tail -n 3
done
"#;
    let text = run(&host, SCRIPT).await?;
    Ok(parse_listing(&host, &text))
}

fn parse_listing(host: &str, text: &str) -> Listing {
    let mut listing = Listing::default();
    let mut home = String::new();
    // Sessão salva em montagem: id, data, pasta e os títulos achados.
    let mut current: Option<(String, SystemTime, Option<String>, [Option<String>; 3])> = None;
    let finish = |current: Option<(String, SystemTime, Option<String>, [Option<String>; 3])>,
                      listing: &mut Listing| {
        let Some((id, modified, cwd, [custom, ai, prompt])) = current else { return };
        let (Some(cwd), Some(title)) = (cwd, custom.or(ai).or(prompt)) else { return };
        listing.sessions.push(SavedSession {
            agent: Agent::Claude,
            id,
            cwd: PathBuf::from(cwd),
            title: title.lines().next().unwrap_or_default().chars().take(120).collect(),
            modified,
            running: None,
            host: Some(host.to_owned()),
        });
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("H\t") {
            home = rest.to_owned();
        } else if let Some(rest) = line.strip_prefix("T\t") {
            let fields: Vec<&str> = rest.splitn(4, '\t').collect();
            if let [name, attached, path, title] = fields[..] {
                listing.tmux.push(TmuxSession {
                    name: name.to_owned(),
                    attached: attached != "0",
                    path: tilde(path, &home),
                    title: title.to_owned(),
                });
            }
        } else if let Some(rest) = line.strip_prefix("F\t") {
            finish(current.take(), &mut listing);
            let Some((time, path)) = rest.split_once('\t') else { continue };
            let Some(id) = Path::new(path).file_stem().and_then(|stem| stem.to_str()) else { continue };
            let seconds = time.parse::<u64>().unwrap_or(0);
            current = Some((id.to_owned(), SystemTime::UNIX_EPOCH + Duration::from_secs(seconds), None, Default::default()));
        } else if let Some((_, _, cwd, titles)) = current.as_mut() {
            let Some((key, value)) = line.split_once("\":\"") else { continue };
            let value = unescape(value.trim_end_matches('"'));
            match key.trim_start_matches('"') {
                "cwd" => *cwd = Some(tilde(&value, &home)),
                "customTitle" => titles[0] = Some(value),
                "aiTitle" => titles[1] = Some(value),
                "lastPrompt" => titles[2] = Some(value),
                _ => {}
            }
        }
    }
    finish(current.take(), &mut listing);
    listing
}

/// Os escapes do JSON que aparecem num título (o `grep` corta antes de uma aspa escapada).
fn unescape(text: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{}\"", text.trim_end_matches('\\')))
        .unwrap_or_else(|_| text.to_owned())
}

/// Caminho da VPS com `~` no lugar da home de lá.
fn tilde(path: &str, home: &str) -> String {
    if home.is_empty() {
        return path.to_owned();
    }
    match path.strip_prefix(home) {
        Some("") => "~".into(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_do_ssh_config() {
        let config = "Host vps-r1\n    HostName 1.2.3.4\nHost *\n  User x\nhost outro \"terceiro\" vps-r1\n";
        assert_eq!(parse_hosts(config), ["vps-r1", "outro", "terceiro"]);
    }

    #[test]
    fn aspas_do_shell() {
        assert_eq!(quote("simples/caminho-1"), "simples/caminho-1");
        assert_eq!(quote("com espaço"), "'com espaço'");
        assert_eq!(quote("it's $HOME"), r"'it'\''s $HOME'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote_dir("~"), "\"$HOME\"");
        assert_eq!(quote_dir("~/meus projetos"), "\"$HOME\"'/meus projetos'");
        assert_eq!(quote_dir("/srv/app"), "/srv/app");
    }

    #[test]
    fn comando_do_chat() {
        let args = vec!["--session-id".to_owned(), "abc".to_owned(), "--".to_owned(), "oi; rm -rf /".to_owned()];
        let agent = agent_shell_command(Agent::Claude, &args, None);
        assert_eq!(
            agent,
            "export PATH=\"$HOME/.local/bin:$PATH\"; env -u ANTHROPIC_API_KEY claude --session-id abc -- 'oi; rm -rf /'"
        );
        let (program, args) = command("vps-r1", "~/x", "chube-1234", Some(&agent));
        assert_eq!(program, "ssh");
        assert_eq!(args[0], "-t");
        let script = args.last().unwrap();
        assert_eq!(args[args.len() - 2], "vps-r1");
        assert!(script.starts_with("cd \"$HOME\"/x 2>/dev/null || cd; exec tmux new-session -A -s chube-1234 "));
        assert!(script.contains("bash -lc 'export PATH="));
        assert!(script.contains("set-option -t chube-1234 status off"));
        let (_, attach) = command("vps-r1", "~", "do celular", None);
        assert!(attach.last().unwrap().starts_with("exec tmux attach-session -t 'do celular' \\; "));
    }

    #[test]
    fn sessao_nova_volta_a_comecar_se_nao_houver_historico() {
        let resume = vec!["--resume".to_owned(), "abc".to_owned()];
        let new = vec!["--session-id".to_owned(), "abc".to_owned()];
        let command = agent_shell_command(Agent::Claude, &resume, Some(&new));
        assert!(command.ends_with("claude --resume abc || env -u ANTHROPIC_API_KEY claude --session-id abc"));
    }

    #[test]
    fn lista_da_vps() {
        let text = "H\t/home/pedro\n\
T\tchube-1234\t0\t/home/pedro/projects/x\t✳ Corrigir testes\n\
T\tcelular\t1\t/srv\tbash\n\
F\t1794617482\tprojeto/abc.jsonl\n\
\"cwd\":\"/home/pedro/projects/x\"\n\
\"aiTitle\":\"Título da IA\n\
\"customTitle\":\"Meu t\\u00edtulo\n\
F\t1794617000\toutro/sem-titulo.jsonl\n\
\"cwd\":\"/tmp\"\n";
        let listing = parse_listing("vps-r1", text);
        assert_eq!(listing.tmux.len(), 2);
        assert_eq!(listing.tmux[0].path, "~/projects/x");
        assert!(!listing.tmux[0].attached && listing.tmux[1].attached);
        assert_eq!(listing.sessions.len(), 1);
        let saved = &listing.sessions[0];
        assert_eq!(saved.id, "abc");
        assert_eq!(saved.cwd, PathBuf::from("~/projects/x"));
        assert_eq!(saved.title, "Meu título");
        assert_eq!(saved.host.as_deref(), Some("vps-r1"));
    }

    #[test]
    fn estado_dos_chats_da_vps() {
        let text = "lixo\n\
C\tchube-1\tabc\twaiting\tclaude-opus-5-5\t\"usage\":{\"input_tokens\":3,\"cache_creation_input_tokens\":1000,\"cache_read_input_tokens\":80000,\"cache_creation\":{\"ephemeral_5m_input_tokens\":0\n\
C\tcelular\t\t\t\t\n";
        let list = parse_poll(text);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].tmux, "chube-1");
        assert_eq!(list[0].session_id.as_deref(), Some("abc"));
        assert_eq!(list[0].status.as_deref(), Some("waiting"));
        assert_eq!(list[0].model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(list[0].context_tokens, 81_003);
        assert_eq!(list[1], RemoteStatus { tmux: "celular".into(), ..Default::default() });
    }

    #[test]
    fn script_de_estado_com_nomes_entre_aspas() {
        let script = poll_script(&["chube-1".into(), "do celular".into()]);
        assert!(script.starts_with("chube_poll() {"));
        assert!(script.ends_with("chube_poll chube-1\nchube_poll 'do celular'\n"));
    }
}
