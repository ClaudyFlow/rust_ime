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
}
