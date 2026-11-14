//! Atalhos de teclado do app. Valem também com o foco num terminal: a janela os recebe na fase
//! de captura, antes do VTE, e eles não chegam ao PTY.

use gtk4::gdk::{Key, ModifierType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shortcut {
    NewChat,
    ToggleSidebar,
    ToggleChat,
    ToggleTerminal,
    /// Passa o foco do teclado entre o chat em foco e o terminal integrado.
    SwitchFocus,
    /// Chat de cima / de baixo na lista "# chats".
    PreviousChat,
    NextChat,
}

impl Shortcut {
    pub fn label(self) -> &'static str {
        match self {
            Shortcut::NewChat => "nova sessão",
            Shortcut::ToggleSidebar => "mostrar/esconder o painel",
            Shortcut::ToggleChat => "mostrar/esconder o chat",
            Shortcut::ToggleTerminal => "mostrar/esconder o terminal",
            Shortcut::SwitchFocus => "teclado: chat ↔ terminal",
            Shortcut::PreviousChat => "chat de cima na lista",
            Shortcut::NextChat => "chat de baixo na lista",
        }
    }
}

/// Teclas e ações, na ordem mostrada na ajuda.
pub const LIST: [(&str, Shortcut); 7] = [
    ("Ctrl+Shift+N", Shortcut::NewChat),
    ("Ctrl+Shift+1", Shortcut::ToggleSidebar),
    ("Ctrl+Shift+2", Shortcut::ToggleChat),
    ("Ctrl+Shift+3", Shortcut::ToggleTerminal),
    ("Ctrl+Tab", Shortcut::SwitchFocus),
    ("Ctrl+Shift+↑", Shortcut::PreviousChat),
    ("Ctrl+Shift+↓", Shortcut::NextChat),
];

/// Copiar e colar num terminal. O VTE não traz atalho para isso (cada app define os seus). O
/// Super+C/V do Omarchy manda Ctrl+C/V para janelas que ele não marca como terminal (é o caso do
/// C-hub), então os dois também valem aqui, sem tirar o Ctrl+C de interromper nem o Ctrl+V de colar
/// imagem no `claude`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Clipboard {
    /// Ctrl+Shift+C: copia a seleção (sem seleção, não faz nada).
    Copy,
    /// Ctrl+C e Ctrl+Insert: copia se houver seleção; senão a tecla segue para o programa.
    CopyIfSelected,
    /// Ctrl+Shift+V e Shift+Insert: cola o texto da área de transferência.
    Paste,
    /// Ctrl+V: cola o texto, a não ser que a área de transferência tenha imagem; aí a tecla segue
    /// para o programa (o `claude` lê a imagem sozinho no Ctrl+V).
    PasteUnlessImage,
}

/// A ação de copiar/colar de uma tecla pressionada num terminal, se houver.
pub fn clipboard(keyval: Key, state: ModifierType) -> Option<Clipboard> {
    let state = state & (ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK);
    let ctrl = ModifierType::CONTROL_MASK;
    let shift = ModifierType::SHIFT_MASK;
    let ctrl_shift = ctrl | shift;
    match (keyval.to_lower(), state) {
        (Key::c, s) if s == ctrl_shift => Some(Clipboard::Copy),
        (Key::v, s) if s == ctrl_shift => Some(Clipboard::Paste),
        (Key::c, s) if s == ctrl => Some(Clipboard::CopyIfSelected),
        (Key::v, s) if s == ctrl => Some(Clipboard::PasteUnlessImage),
        (Key::Insert | Key::KP_Insert, s) if s == ctrl => Some(Clipboard::CopyIfSelected),
        (Key::Insert | Key::KP_Insert, s) if s == shift => Some(Clipboard::Paste),
        _ => None,
    }
}

/// Teclas físicas (código do evdev + 8, como o GDK entrega) dos números: com Shift, o caractere
/// muda conforme o layout, então os números valem pela tecla.
const KEYCODE_1: u32 = 10;
const KEYCODE_2: u32 = 11;
const KEYCODE_3: u32 = 12;

/// O atalho de uma tecla pressionada, se houver. Só Ctrl e Shift contam (Caps Lock, Num Lock
/// e afins ficam de fora).
pub fn find(keyval: Key, keycode: u32, state: ModifierType) -> Option<Shortcut> {
    let state = state & (ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK);
    let ctrl = ModifierType::CONTROL_MASK;
    let ctrl_shift = ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK;
    if state == ctrl_shift {
        match keycode {
            KEYCODE_1 => return Some(Shortcut::ToggleSidebar),
            KEYCODE_2 => return Some(Shortcut::ToggleChat),
            KEYCODE_3 => return Some(Shortcut::ToggleTerminal),
            _ => {}
        }
        return match keyval.to_lower() {
            Key::n => Some(Shortcut::NewChat),
            Key::Up | Key::KP_Up => Some(Shortcut::PreviousChat),
            Key::Down | Key::KP_Down => Some(Shortcut::NextChat),
            _ => None,
        };
    }
    if state == ctrl && keyval == Key::Tab {
        return Some(Shortcut::SwitchFocus);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_os_atalhos() {
        let ctrl_shift = ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK;
        assert_eq!(find(Key::n, 57, ctrl_shift), Some(Shortcut::NewChat));
        // Caps Lock ou Shift deixam a letra maiúscula.
        assert_eq!(find(Key::N, 57, ctrl_shift | ModifierType::LOCK_MASK), Some(Shortcut::NewChat));
        assert_eq!(find(Key::n, 57, ModifierType::CONTROL_MASK), None);
        // Ctrl+Shift+2 no layout br dá `@`: vale a tecla física.
        assert_eq!(find(Key::at, KEYCODE_2, ctrl_shift), Some(Shortcut::ToggleChat));
        assert_eq!(find(Key::Tab, 23, ModifierType::CONTROL_MASK), Some(Shortcut::SwitchFocus));
        assert_eq!(find(Key::ISO_Left_Tab, 23, ModifierType::SHIFT_MASK), None);
        assert_eq!(find(Key::Down, 116, ctrl_shift), Some(Shortcut::NextChat));
        assert_eq!(find(Key::Down, 116, ModifierType::CONTROL_MASK), None);
    }

    #[test]
    fn acha_copiar_e_colar() {
        let ctrl = ModifierType::CONTROL_MASK;
        let shift = ModifierType::SHIFT_MASK;
        assert_eq!(clipboard(Key::C, ctrl | shift), Some(Clipboard::Copy));
        assert_eq!(clipboard(Key::V, ctrl | shift | ModifierType::LOCK_MASK), Some(Clipboard::Paste));
        assert_eq!(clipboard(Key::c, ctrl), Some(Clipboard::CopyIfSelected));
        assert_eq!(clipboard(Key::v, ctrl), Some(Clipboard::PasteUnlessImage));
        assert_eq!(clipboard(Key::Insert, ctrl), Some(Clipboard::CopyIfSelected));
        assert_eq!(clipboard(Key::Insert, shift), Some(Clipboard::Paste));
        assert_eq!(clipboard(Key::c, ModifierType::empty()), None);
        assert_eq!(clipboard(Key::v, ctrl | ModifierType::ALT_MASK), None);
        // Não colide com os atalhos do app.
        for (keyval, state) in [(Key::c, ctrl | shift), (Key::v, ctrl | shift), (Key::c, ctrl), (Key::v, ctrl)] {
            assert_eq!(find(keyval, 0, state), None);
        }
    }
}
