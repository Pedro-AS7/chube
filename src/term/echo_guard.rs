//! Chube: evita que o mesmo caractere entre duas vezes no terminal.
//!
//! No GNOME em Wayland, toda tecla passa pelo IBus. Os caracteres que ele monta (ç, ã, á,
//! à, â...) chegam ao widget duas vezes: como texto da tecla pressionada e como "commit" do
//! método de entrada. Escrever os dois dava "çç". Aqui fica guardado o último texto escrito
//! por um dos caminhos; se o outro trouxer o mesmo texto logo em seguida, ele é descartado.

use std::time::{Duration, Instant};

/// Intervalo máximo entre a tecla e o commit do mesmo caractere. Os dois chegam no mesmo
/// ciclo de eventos (bem menos de 1 ms); 50 ms é folga e ainda é bem mais rápido que
/// alguém apertar a mesma tecla duas vezes.
const WINDOW: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Key,
    Commit,
}

#[derive(Debug, Default)]
pub struct EchoGuard {
    last: Option<(Source, String, Instant)>,
}

impl EchoGuard {
    /// Diz se `text`, vindo de `source`, deve ser escrito no terminal. Se for a repetição
    /// do que o outro caminho acabou de escrever, devolve `false` e esquece o par, para que
    /// a próxima tecla igual (outro aperto de verdade) passe.
    pub fn should_write(&mut self, source: Source, text: &str, now: Instant) -> bool {
        if let Some((last_source, last_text, at)) = &self.last {
            if *last_source != source
                && last_text == text
                && now.saturating_duration_since(*at) <= WINDOW
            {
                self.last = None;
                return false;
            }
        }
        self.last = Some((source, text.to_owned(), now));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tecla_depois_commit_igual_escreve_uma_vez() {
        let mut guard = EchoGuard::default();
        let t = Instant::now();
        assert!(guard.should_write(Source::Key, "ç", t));
        assert!(!guard.should_write(Source::Commit, "ç", t + Duration::from_millis(1)));
    }

    #[test]
    fn commit_depois_tecla_igual_escreve_uma_vez() {
        let mut guard = EchoGuard::default();
        let t = Instant::now();
        assert!(guard.should_write(Source::Commit, "ã", t));
        assert!(!guard.should_write(Source::Key, "ã", t));
    }

    #[test]
    fn dois_apertos_seguidos_escrevem_duas_vezes() {
        let mut guard = EchoGuard::default();
        let t = Instant::now();
        assert!(guard.should_write(Source::Key, "ç", t));
        assert!(!guard.should_write(Source::Commit, "ç", t));
        assert!(guard.should_write(Source::Key, "ç", t + Duration::from_millis(5)));
        assert!(!guard.should_write(Source::Commit, "ç", t + Duration::from_millis(5)));
    }

    #[test]
    fn mesmo_caminho_repetido_passa() {
        let mut guard = EchoGuard::default();
        let t = Instant::now();
        assert!(guard.should_write(Source::Key, "a", t));
        assert!(guard.should_write(Source::Key, "a", t));
    }

    #[test]
    fn texto_diferente_ou_tarde_passa() {
        let mut guard = EchoGuard::default();
        let t = Instant::now();
        assert!(guard.should_write(Source::Key, "a", t));
        assert!(guard.should_write(Source::Commit, "á", t));
        assert!(guard.should_write(Source::Key, "é", t));
        assert!(guard.should_write(Source::Commit, "é", t + Duration::from_millis(200)));
    }
}
