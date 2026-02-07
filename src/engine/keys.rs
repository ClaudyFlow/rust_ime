#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VirtualKey {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, VirtualDigit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Space, Enter, Tab, Backspace, Esc, CapsLock,
    Left, Right, Up, Down,
    PageUp, PageDown, Home, End, Delete,
    Grave, Minus, Equal, LeftBrace, RightBrace, Backslash, Semicolon, Apostrophe, Comma, Dot, Slash,
}
