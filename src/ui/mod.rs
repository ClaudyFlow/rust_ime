pub mod tray;
pub mod web;
pub mod gui_slint;
pub use gui_slint as gui;

use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub chinese_enabled: bool,
    pub active_profile: String,
    pub show_status_bar_pref: bool,
    pub show_candidates_pref: bool,
    pub is_ime_active: bool, // 窗口是否获得焦点/输入法是否激活
    pub pinyin: String,
    pub candidates: Vec<String>,
    pub hints: Vec<String>,
    pub selected_index: usize,
    pub status_text: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum GuiEvent {
    SyncState(AppState), // 单一数据源同步
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
    UpdateStatusBarVisible(bool), // 手动更新状态栏显隐
    SetVisible(bool),         // 窗口显隐 (用于输入法激活/停用)
    OpenTrayMenu { x: i32, y: i32, chinese_enabled: bool, active_profile: String },
    Exit,
}
    