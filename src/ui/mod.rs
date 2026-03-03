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
    ApplyConfig(Config),
    ShowStatus(String, bool), // 状态文字, 是否为中文模式 (用于更新文字)
    SetVisible(bool),         // 窗口显隐 (用于输入法激活/停用)
    OpenTrayMenu { x: i32, y: i32, chinese_enabled: bool, active_profile: String },
    Exit,
}
    