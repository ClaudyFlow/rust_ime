pub mod tray;
pub mod web;
pub mod gui_slint;
pub use gui_slint as gui;

use crate::config::Config;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum GuiEvent {
    Update {
        pinyin: String,
        candidates: Vec<String>,
        hints: Vec<String>,
        selected: usize,
        sentence: String,
        cursor_pos: usize,
        commit_mode: String,
    },
    MoveTo { x: i32, y: i32 },
    ApplyConfig(Box<Config>),
    ShowStatus(String, bool), // 显示文字, 是否激活
    OpenTrayMenu { x: i32, y: i32, chinese_enabled: bool, active_profile: String },
    Exit,
}
    