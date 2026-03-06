pub mod tray;
pub mod web;
pub mod gui_slint;
pub mod linux_notify;
pub mod slint_window;
pub use gui_slint as gui;

use crate::config::Config;

/// 核心显示接口：解耦 Slint 窗口与 Linux 桌面通知
pub trait CandidateDisplay {
    /// 更新候选词列表及拼音
    fn update_candidates(&mut self, pinyin: &str, candidates: Vec<String>, hints: Vec<String>, selected: usize);
    
    /// 更新状态栏显示（中/英模式文字）
    fn update_status(&mut self, text: &str, chinese_enabled: bool);
    
    /// 移动显示位置（通常仅对窗口 UI 有效）
    fn move_to(&mut self, x: i32, y: i32);
    
    /// 设置全局显隐状态
    fn set_visible(&mut self, visible: bool);
    
    /// 应用配置更新
    fn apply_config(&mut self, config: &Config);

    /// 销毁或关闭显示
    fn close(&mut self);
}

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
    ForceStatusVisible(bool), // 强制、独立的状态栏显隐控制 (不受任何焦点影响)
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
    