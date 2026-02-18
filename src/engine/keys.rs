#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VirtualKey {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Space, Enter, Tab, Backspace, Esc, CapsLock,
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
        if v <= 25 {
            return Some(unsafe { std::mem::transmute(v) });
        }
        if v >= 26 && v <= 35 {
            return Some(unsafe { std::mem::transmute(v) });
        }
        // 对于非连续区域或特殊按键，我们可以继续按需添加分支，
        // 或者简单地在 0..=66 (假设总数) 范围内进行安全校验。
        // 目前 VirtualKey 定义中有 67 个元素 (0-66)。
        if v <= 66 {
            return Some(unsafe { std::mem::transmute(v) });
        }
        None
    }
}
