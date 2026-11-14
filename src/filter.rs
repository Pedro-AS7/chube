//! Filtro de cores entre o `claude` e o terminal: `chube --filtro-cores -- <comando>`.
//!
//! A TUI do `claude` pinta a faixa das mensagens do usuário com um cinza fixo (`rgb(55,55,55)` no
//! tema escuro dela, `rgb(240,240,240)` no claro), que destoa dos temas. O VTE não deixa trocar
//! uma cor RGB na hora de desenhar, então a troca é feita no fluxo de bytes: o filtro roda o
//! comando num PTY próprio (como o `script`), repassa o teclado e a saída, e troca essas duas cores
//! pelas entradas `MESSAGE_BG` e `MESSAGE_FG` da paleta de 256 cores, que o C-hub define com o tema
//! (`terminal::apply_theme`). Trocar de tema recolore até as faixas que já estão na tela.
//!
//! O filtro também atende o OSC 52 (`ESC ] 52 ; <seleção> ; <base64> BEL`), que o VTE ignora: é
//! por ele que o `/copy` do `claude` numa VPS (dentro do tmux) chega à área de transferência daqui.
//! A sequência sai do fluxo e o texto vai para o `wl-copy`.

use std::ffi::{CString, OsString};
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};

/// Primeiro argumento do modo filtro.
pub const ARG: &str = "--filtro-cores";
/// Entrada da paleta de 256 cores usada no fundo da faixa (no xterm, `#3a3a3a`: perto do cinza
/// original, então outro uso dela quase não muda).
pub const MESSAGE_BG: u8 = 237;
/// Entrada usada no texto da faixa, quando a TUI não define outro (no xterm, `#eeeeee`).
pub const MESSAGE_FG: u8 = 255;
/// Sequências de escape maiores que isto não são de cor: passam sem ser olhadas.
const MAX_SEQUENCE: usize = 256;
/// Começo do OSC 52 (gravar na área de transferência).
const OSC52: &[u8] = b"\x1b]52;";
/// Um OSC 52 maior que isto (em base64) passa como veio, sem ser guardado inteiro na memória.
const MAX_CLIPBOARD: usize = 16 * 1024 * 1024;

/// Texto que o comando pediu para pôr na área de transferência (OSC 52).
#[derive(Debug, PartialEq, Eq)]
pub struct Clip {
    /// Só a seleção primária (`p`), a do botão do meio; senão, a área de transferência.
    pub primary: bool,
    pub text: Vec<u8>,
}

enum Osc52 {
    /// Não é OSC 52 (ou está malformado): passa como veio.
    Not,
    /// Pode ser, mas o resto ainda não chegou.
    Incomplete,
    /// Sequência inteira de `length` bytes; `copy` é `None` para consultas (`?`) e base64 inválido.
    Done { length: usize, copy: Option<Clip> },
}

/// Comando do terminal de um chat do Claude, passando pelo filtro.
pub fn wrap(argv: Vec<String>) -> Vec<String> {
    let exe = std::env::current_exe().map_or_else(|_| "chube".to_owned(), |path| path.display().to_string());
    [exe, ARG.to_owned(), "--".to_owned()].into_iter().chain(argv).collect()
}

/// Pid do comando que o filtro `pid` roda (o primeiro filho dele).
pub fn inner_pid(pid: u32) -> Option<u32> {
    std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Troca as cores da faixa nas sequências SGR (`ESC [ … m`). Guarda o fim de uma sequência
/// cortada entre duas leituras para a próxima.
#[derive(Default)]
pub struct Rewriter {
    pending: Vec<u8>,
    /// Dentro da faixa: do fundo dela até outro fundo ou um reset.
    in_band: bool,
    /// Pedidos de OSC 52 tirados do fluxo, para quem chamou entregar (`run` usa o `wl-copy`).
    pub copies: Vec<Clip>,
}

impl Rewriter {
    pub fn feed(&mut self, input: &[u8], output: &mut Vec<u8>) {
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(input);
        let mut index = 0;
        while index < data.len() {
            let Some(offset) = data[index..].iter().position(|&byte| byte == 0x1b) else {
                output.extend_from_slice(&data[index..]);
                return;
            };
            output.extend_from_slice(&data[index..index + offset]);
            index += offset;
            // `ESC` no fim: pode ser o começo de uma sequência.
            let Some(&next) = data.get(index + 1) else {
                self.pending = data[index..].to_vec();
                return;
            };
            if next == b']' {
                match parse_osc52(&data[index..]) {
                    Osc52::Not => {}
                    Osc52::Incomplete if data.len() - index <= MAX_CLIPBOARD => {
                        self.pending = data[index..].to_vec();
                        return;
                    }
                    // Grande demais: passa como veio (o VTE ignora o OSC 52).
                    Osc52::Incomplete => {
                        output.extend_from_slice(&data[index..]);
                        return;
                    }
                    Osc52::Done { length, copy } => {
                        self.copies.extend(copy);
                        index += length;
                        continue;
                    }
                }
            }
            if next != b'[' {
                output.push(0x1b);
                index += 1;
                continue;
            }
            let start = index + 2;
            match data[start..].iter().position(|byte| (0x40..=0x7e).contains(byte)) {
                Some(length) => {
                    let end = start + length;
                    if data[end] == b'm' {
                        output.extend_from_slice(b"\x1b[");
                        output.extend_from_slice(&rewrite_sgr(&data[start..end], &mut self.in_band));
                        output.push(b'm');
                    } else {
                        output.extend_from_slice(&data[index..=end]);
                    }
                    index = end + 1;
                }
                None if data.len() - index < MAX_SEQUENCE => {
                    self.pending = data[index..].to_vec();
                    return;
                }
                // Grande demais para ser cor: passa como veio.
                None => {
                    output.extend_from_slice(&data[index..]);
                    return;
                }
            }
        }
    }
}

/// Parâmetros de um SGR com o fundo da faixa trocado pela entrada `MESSAGE_BG`. Dentro da faixa
/// (`in_band`, do fundo dela até outro fundo ou um reset), toda cor de texto vira a `MESSAGE_FG`,
/// como o terminal antigo fazia célula por célula: a TUI define o texto da faixa numa sequência
/// separada, depois do fundo.
fn rewrite_sgr(params: &[u8], in_band: &mut bool) -> Vec<u8> {
    // Sequências privadas (`ESC [ > 4 ; 2 m`) não são cores.
    if params.first().is_some_and(|byte| !byte.is_ascii_digit() && *byte != b';') {
        return params.to_vec();
    }
    let Ok(text) = std::str::from_utf8(params) else {
        return params.to_vec();
    };
    let tokens: Vec<&str> = text.split(';').collect();
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    // O fundo da faixa começou neste SGR e ainda não veio cor de texto depois dele.
    let mut needs_foreground = false;
    let message_foreground = |out: &mut Vec<String>| out.extend(["38".to_owned(), "5".to_owned(), MESSAGE_FG.to_string()]);
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let rgb = |at: usize| -> Option<(u8, u8, u8)> {
            Some((tokens.get(at)?.parse().ok()?, tokens.get(at + 1)?.parse().ok()?, tokens.get(at + 2)?.parse().ok()?))
        };
        let width = match (token, tokens.get(index + 1).copied()) {
            ("38" | "48", Some("2")) => 5,
            ("38" | "48", Some("5")) => 3,
            _ => 1,
        };
        let copy = |out: &mut Vec<String>| {
            out.extend(tokens[index..(index + width).min(tokens.len())].iter().map(|token| (*token).to_owned()));
        };
        let band = (token == "48" && width == 5 && rgb(index + 2).is_some_and(is_message_bg))
            || (token.starts_with("48:2:") && colon_rgb(token).is_some_and(is_message_bg));
        let background = token == "48" || token.starts_with("48:") || matches!(token.parse::<u8>(), Ok(40..=47 | 49 | 100..=107));
        let foreground = token == "38" || token.starts_with("38:") || matches!(token.parse::<u8>(), Ok(30..=37 | 39 | 90..=97));
        if band {
            out.extend(["48".to_owned(), "5".to_owned(), MESSAGE_BG.to_string()]);
            *in_band = true;
            needs_foreground = true;
        } else if token.is_empty() || token == "0" || background {
            *in_band = false;
            needs_foreground = false;
            copy(&mut out);
        } else if foreground && *in_band {
            message_foreground(&mut out);
            needs_foreground = false;
        } else {
            copy(&mut out);
        }
        index += width;
    }
    if needs_foreground {
        message_foreground(&mut out);
    }
    out.join(";").into_bytes()
}

/// Lê um OSC 52 no começo de `data` (que começa com `ESC ]`). Termina em BEL ou `ESC \\`.
fn parse_osc52(data: &[u8]) -> Osc52 {
    let known = data.len().min(OSC52.len());
    if data[..known] != OSC52[..known] {
        return Osc52::Not;
    }
    if data.len() < OSC52.len() {
        return Osc52::Incomplete;
    }
    let body = &data[OSC52.len()..];
    let mut at = 0;
    while at < body.len() {
        let terminator = match body[at] {
            0x07 => 1,
            0x1b => match body.get(at + 1) {
                Some(b'\\') => 2,
                Some(_) => return Osc52::Not,
                None => return Osc52::Incomplete,
            },
            _ => {
                at += 1;
                continue;
            }
        };
        let content = &body[..at];
        let (selection, payload) = match content.iter().position(|&byte| byte == b';') {
            Some(split) => (&content[..split], &content[split + 1..]),
            None => (&b""[..], content),
        };
        let copy = (payload != b"?")
            .then(|| decode_base64(payload))
            .flatten()
            .map(|text| Clip { primary: selection == b"p", text });
        return Osc52::Done { length: OSC52.len() + at + terminator, copy };
    }
    Osc52::Incomplete
}

/// Base64 padrão (`A-Z a-z 0-9 + /`, com ou sem `=`); quebras de linha são ignoradas.
fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for &byte in input {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(output)
}

/// Entrega um pedido de OSC 52 ao `wl-copy`, numa thread para não travar o fluxo do terminal.
fn set_clipboard(copy: Clip) {
    std::thread::spawn(move || {
        let mut command = std::process::Command::new("wl-copy");
        if copy.primary {
            command.arg("--primary");
        }
        let child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(&copy.text);
            }
            let _ = child.wait();
        }
    });
}

fn colon_rgb(token: &str) -> Option<(u8, u8, u8)> {
    let parts: Vec<&str> = token.split(':').collect();
    // `48:2:<espaço de cor>:r:g:b` ou `48:2:r:g:b`.
    let values = if parts.len() == 6 { &parts[3..] } else if parts.len() == 5 { &parts[2..] } else { return None };
    Some((values[0].parse().ok()?, values[1].parse().ok()?, values[2].parse().ok()?))
}

fn is_message_bg(rgb: (u8, u8, u8)) -> bool {
    matches!(rgb, (55, 55, 55) | (240, 240, 240))
}

static RESIZED: AtomicBool = AtomicBool::new(false);
static HANGUP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(signal: libc::c_int) {
    if signal == libc::SIGWINCH {
        RESIZED.store(true, Ordering::Relaxed);
    } else {
        HANGUP.store(true, Ordering::Relaxed);
    }
}

/// Modo filtro: roda `args` (depois de `--`) num PTY próprio e devolve o código de saída dele.
pub fn run(args: &[OsString]) -> i32 {
    let args: Vec<&OsString> = args.iter().skip_while(|arg| *arg != "--").skip(1).collect();
    let Ok(argv) = args.iter().map(|arg| CString::new(arg.as_bytes())).collect::<Result<Vec<_>, _>>() else {
        eprintln!("chube: argumento com byte nulo");
        return 2;
    };
    if argv.is_empty() {
        eprintln!("chube: uso: chube {ARG} -- <comando> [argumentos]");
        return 2;
    }
    // SAFETY: chamadas de sistema com descritores e estruturas locais; o filho só chama funções
    // seguras depois do `fork` (setsid, ioctl, dup2, close, execvp, _exit).
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        libc::ioctl(0, libc::TIOCGWINSZ, &mut size);
        let (mut master, mut slave) = (0, 0);
        if libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size) != 0 {
            eprintln!("chube: não consegui abrir o PTY do filtro");
            return 1;
        }
        let pointers: Vec<*const libc::c_char> =
            argv.iter().map(|arg| arg.as_ptr()).chain(std::iter::once(std::ptr::null())).collect();
        let child = libc::fork();
        if child == 0 {
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY, 0);
            for fd in 0..3 {
                libc::dup2(slave, fd);
            }
            libc::close(master);
            if slave > 2 {
                libc::close(slave);
            }
            libc::execvp(pointers[0], pointers.as_ptr());
            libc::_exit(127);
        }
        libc::close(slave);
        if child < 0 {
            eprintln!("chube: fork falhou");
            return 1;
        }

        // O terminal de fora em modo cru: as teclas passam direto, quem ecoa e edita é o comando.
        let mut mode: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut mode) == 0 {
            libc::cfmakeraw(&mut mode);
            libc::tcsetattr(0, libc::TCSANOW, &mode);
        }
        for signal in [libc::SIGWINCH, libc::SIGHUP, libc::SIGTERM] {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as *const () as usize;
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }

        let mut rewriter = Rewriter::default();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut output = Vec::with_capacity(64 * 1024);
        let mut input_open = true;
        // Fim da entrada só quer dizer "o terminal fechou" quando a entrada é um terminal.
        let input_is_terminal = libc::isatty(0) == 1;
        let mut hung_up = false;
        loop {
            if RESIZED.swap(false, Ordering::Relaxed) && libc::ioctl(0, libc::TIOCGWINSZ, &mut size) == 0 {
                libc::ioctl(master, libc::TIOCSWINSZ, &size);
            }
            // O terminal de fora fechou (chat fechado, app saiu): o comando recebe SIGHUP, como
            // receberia sem o filtro, e sai gravando a sessão.
            if (HANGUP.load(Ordering::Relaxed) || (!input_open && input_is_terminal)) && !hung_up {
                libc::kill(child, libc::SIGHUP);
                hung_up = true;
            }
            let mut fds = [
                libc::pollfd { fd: if input_open { 0 } else { -1 }, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: master, events: libc::POLLIN, revents: 0 },
            ];
            if libc::poll(fds.as_mut_ptr(), 2, -1) < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            if fds[0].revents != 0 {
                let read = libc::read(0, buffer.as_mut_ptr().cast(), buffer.len());
                if read <= 0 {
                    input_open = false;
                } else {
                    write_all(master, &buffer[..read as usize]);
                }
            }
            if fds[1].revents != 0 {
                let read = libc::read(master, buffer.as_mut_ptr().cast(), buffer.len());
                // EIO: o comando saiu e o PTY fechou.
                if read <= 0 {
                    break;
                }
                output.clear();
                rewriter.feed(&buffer[..read as usize], &mut output);
                write_all(1, &output);
                for copy in rewriter.copies.drain(..) {
                    set_clipboard(copy);
                }
            }
        }
        let mut status = 0;
        libc::waitpid(child, &mut status, 0);
        if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { 128 + libc::WTERMSIG(status) }
    }
}

fn write_all(fd: libc::c_int, mut data: &[u8]) {
    while !data.is_empty() {
        // SAFETY: escreve do slice válido no descritor dado.
        let written = unsafe { libc::write(fd, data.as_ptr().cast(), data.len()) };
        if written < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        data = &data[written as usize..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(chunks: &[&[u8]]) -> Vec<u8> {
        let mut rewriter = Rewriter::default();
        let mut output = Vec::new();
        for chunk in chunks {
            rewriter.feed(chunk, &mut output);
        }
        output
    }

    #[test]
    fn troca_o_fundo_da_faixa_e_poe_o_texto() {
        assert_eq!(filter(&[b"a\x1b[48;2;55;55;55mtexto"]), b"a\x1b[48;5;237;38;5;255mtexto");
        assert_eq!(filter(&[b"\x1b[48;2;240;240;240m"]), b"\x1b[48;5;237;38;5;255m");
        // A cor de texto de antes da faixa é trocada pela do tema; a de fora fica.
        assert_eq!(filter(&[b"\x1b[1;38;2;1;2;3;48;2;55;55;55m"]), b"\x1b[1;38;2;1;2;3;48;5;237;38;5;255m");
        assert_eq!(filter(&[b"\x1b[48;2;55;55;55;38;2;9;9;9m"]), b"\x1b[48;5;237;38;5;255m");
        assert_eq!(filter(&[b"\x1b[48:2::55:55:55m"]), b"\x1b[48;5;237;38;5;255m");
    }

    #[test]
    fn texto_dentro_da_faixa_ate_o_fundo_mudar() {
        assert_eq!(
            filter(&[b"\x1b[48;2;55;55;55m\x1b[38;2;255;255;255mx\x1b[49m\x1b[38;2;255;255;255my"]),
            b"\x1b[48;5;237;38;5;255m\x1b[38;5;255mx\x1b[49m\x1b[38;2;255;255;255my"
        );
        assert_eq!(
            filter(&[b"\x1b[48;2;55;55;55m\x1b[31mx\x1b[0m\x1b[31my"]),
            b"\x1b[48;5;237;38;5;255m\x1b[38;5;255mx\x1b[0m\x1b[31my"
        );
    }

    #[test]
    fn nao_mexe_no_resto() {
        for input in [
            &b"\x1b[48;2;56;55;55mx"[..],
            b"\x1b[38;2;55;55;55m",
            b"\x1b[2J\x1b[H",
            b"\x1b]0;titulo\x07",
            b"\x1b[>4;2m",
            b"texto puro \xc3\xa7",
            b"\x1b[0m\x1b[1m",
        ] {
            assert_eq!(filter(&[input]), input, "{input:?}");
        }
    }

    fn copies(chunks: &[&[u8]]) -> (Vec<u8>, Vec<Clip>) {
        let mut rewriter = Rewriter::default();
        let mut output = Vec::new();
        for chunk in chunks {
            rewriter.feed(chunk, &mut output);
        }
        (output, rewriter.copies)
    }

    fn clipboard(text: &str) -> Clip {
        Clip { primary: false, text: text.as_bytes().to_vec() }
    }

    #[test]
    fn osc52_sai_do_fluxo_e_vira_copia() {
        // "olá mundo" em base64, com os dois terminadores e a seleção vazia (como o tmux manda).
        assert_eq!(copies(&[b"a\x1b]52;c;b2zDoSBtdW5kbw==\x07b"]), (b"ab".to_vec(), vec![clipboard("olá mundo")]));
        assert_eq!(copies(&[b"\x1b]52;;b2zDoSBtdW5kbw\x1b\\"]), (Vec::new(), vec![clipboard("olá mundo")]));
        assert_eq!(copies(&[b"\x1b]52;p;eA==\x07"]).1, vec![Clip { primary: true, text: b"x".to_vec() }]);
        // Consulta e base64 inválido: saem do fluxo sem copiar nada.
        assert_eq!(copies(&[b"\x1b]52;c;?\x07x"]), (b"x".to_vec(), Vec::new()));
        assert_eq!(copies(&[b"\x1b]52;c;@@\x07x"]), (b"x".to_vec(), Vec::new()));
        // Outros OSC e um 52 malformado passam como vieram.
        assert_eq!(copies(&[b"\x1b]5;x\x07"]), (b"\x1b]5;x\x07".to_vec(), Vec::new()));
        assert_eq!(copies(&[b"\x1b]52;c;eA\x1b[0m"]).1, Vec::new());
    }

    #[test]
    fn osc52_cortado_entre_leituras() {
        let whole = b"x\x1b]52;c;b2zDoSBtdW5kbw==\x1b\\\x1b[48;2;55;55;55my".as_slice();
        for cut in 0..=whole.len() {
            let (first, second) = whole.split_at(cut);
            assert_eq!(
                copies(&[first, second]),
                (b"x\x1b[48;5;237;38;5;255my".to_vec(), vec![clipboard("olá mundo")]),
                "corte em {cut}"
            );
        }
    }

    #[test]
    fn base64() {
        assert_eq!(decode_base64(b"").unwrap(), b"");
        assert_eq!(decode_base64(b"YQ==").unwrap(), b"a");
        assert_eq!(decode_base64(b"YWI=").unwrap(), b"ab");
        assert_eq!(decode_base64(b"YWJj\nZA").unwrap(), b"abcd");
        assert!(decode_base64(b"YW!j").is_none());
    }

    #[test]
    fn sequencia_cortada_entre_leituras() {
        let whole = b"x\x1b[48;2;55;55;55my".as_slice();
        for cut in 0..=whole.len() {
            let (first, second) = whole.split_at(cut);
            assert_eq!(filter(&[first, second]), b"x\x1b[48;5;237;38;5;255my", "corte em {cut}");
        }
    }
}
