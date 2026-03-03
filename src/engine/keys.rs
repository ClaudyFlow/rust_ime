#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VirtualKey {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Space, Enter, Tab, Backspace, Esc, CapsLock, Shift, Control, Alt,
    Left, Right, Up, Down,
    PageUp, PageDown, Home, End, Delete,
    Grave, Minus, Equal, LeftBrace, RightBrace, Backslash, Semicolon, Apostrophe, Comma, Dot, Slash,
}

impl std::fmt::Display for VirtualKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl VirtualKey {
    pub fn from_u32(v: u32) -> Option<Self> {
        const ALL: &[VirtualKey] = &[
            VirtualKey::A, VirtualKey::B, VirtualKey::C, VirtualKey::D, VirtualKey::E, VirtualKey::F, VirtualKey::G, VirtualKey::H, VirtualKey::I, VirtualKey::J, VirtualKey::K, VirtualKey::L, VirtualKey::M, VirtualKey::N, VirtualKey::O, VirtualKey::P, VirtualKey::Q, VirtualKey::R, VirtualKey::S, VirtualKey::T, VirtualKey::U, VirtualKey::V, VirtualKey::W, VirtualKey::X, VirtualKey::Y, VirtualKey::Z,
            VirtualKey::Digit0, VirtualKey::Digit1, VirtualKey::Digit2, VirtualKey::Digit3, VirtualKey::Digit4, VirtualKey::Digit5, VirtualKey::Digit6, VirtualKey::Digit7, VirtualKey::Digit8, VirtualKey::Digit9,
            VirtualKey::Space, VirtualKey::Enter, VirtualKey::Tab, VirtualKey::Backspace, VirtualKey::Esc, VirtualKey::CapsLock,
            VirtualKey::Shift, VirtualKey::Control, VirtualKey::Alt,
            VirtualKey::Left, VirtualKey::Right, VirtualKey::Up, VirtualKey::Down,
            VirtualKey::PageUp, VirtualKey::PageDown, VirtualKey::Home, VirtualKey::End, VirtualKey::Delete,
            VirtualKey::Grave, VirtualKey::Minus, VirtualKey::Equal, VirtualKey::LeftBrace, VirtualKey::RightBrace, VirtualKey::Backslash, VirtualKey::Semicolon, VirtualKey::Apostrophe, VirtualKey::Comma, VirtualKey::Dot, VirtualKey::Slash,
        ];
        ALL.get(v as usize).copied()
    }
}
