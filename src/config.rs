use serde::{Serialize, Deserialize};

#[cfg(target_os = "linux")]
use evdev;
#[cfg(target_os = "windows")]
use crate::evdev;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Config {
    pub files: Files,
    pub appearance: Appearance,
    pub input: Input,
    pub hotkeys: Hotkeys,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Files {
    pub punctuation_file: String,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Appearance {
    pub show_candidates: bool,
    pub show_modern_candidates: bool,
    pub show_notifications: bool,
    pub show_keystrokes: bool,
    pub page_size: usize,
    pub show_tone_hint: bool,
    pub show_en_hint: bool,
    pub candidate_anchor: String,
    pub candidate_font_size: u32,
    pub candidate_hint_font_size: u32,
    pub candidate_margin_x: i32,
    pub candidate_margin_y: i32,
    pub candidate_bg_color: String,
    pub candidate_text_color: String,
    pub candidate_highlight_color: String,
    pub pinyin_color: String,
    pub pinyin_font_size: u32,
    pub pinyin_font_family: String,
    pub candidate_font_family: String,
    pub corner_radius: f32,
    pub modern_cand_anchor: String,
    pub modern_cand_font_size: u32,
    pub modern_cand_hint_font_size: u32,
    pub modern_cand_margin_x: i32,
    pub modern_cand_margin_y: i32,
    pub modern_cand_text_color: String,
    pub modern_cand_highlight_color: String,
    pub modern_cand_bg_color: String,
    pub keystroke_anchor: String,
    pub keystroke_font_size: u32,
    pub keystroke_timeout_ms: u64,
    pub keystroke_bg_color: String,
    pub keystroke_margin_x: i32,
    pub keystroke_margin_y: i32,
    pub learning_mode: bool,
    pub learning_anchor: String,
    pub learning_interval_sec: u64,
    pub learning_font_size: u32,
    pub learning_margin_x: i32,
    pub learning_margin_y: i32,
    pub learning_bg_color: String,
    pub learning_dict_path: String,
    pub preview_mode: String,
    pub learning_interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Input {
    pub autostart: bool,
    pub commit_mode: String,
    pub default_profile: String,
    pub paste_method: String,
    pub clipboard_delay_ms: u64,
    pub enable_anti_typo: bool,
    pub enable_double_tap: bool,
    pub double_tap_timeout_ms: u64,
    pub double_taps: Vec<DoubleTap>,
    pub enable_long_press: bool,
    pub long_press_timeout_ms: u64,
    pub long_press_mappings: Vec<LongPressMapping>,
    pub auto_commit_unique_en_fuzhuma: bool,
    pub auto_commit_unique_full_match: bool,
    pub enable_prefix_matching: bool,
    pub prefix_matching_limit: usize,
    pub enable_abbreviation_matching: bool,
    pub filter_proper_nouns_by_case: bool,
    pub active_profiles: Vec<String>,
    pub profile_keys: Vec<ProfileKey>,
    pub page_flipping_keys: Vec<String>,
    pub swap_arrow_keys: bool,
    pub enable_error_sound: bool,
    pub enable_english_filter: bool,
    pub enable_caps_selection: bool,
    pub enable_number_selection: bool,
    pub enable_user_dict: bool,
    pub enable_fixed_first_candidate: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DoubleTap {
    pub trigger_key: String,
    pub insert_text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LongPressMapping {
    pub trigger_key: String,
    pub insert_text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProfileKey {
    pub key: String,
    pub profile: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Hotkeys {
    pub switch_language: Hotkey,
    pub switch_language_alt: Hotkey,
    pub switch_dictionary: Hotkey,
    pub cycle_preview_mode: Hotkey,
    pub toggle_notifications: Hotkey,
    pub cycle_paste_method: Hotkey,
    pub toggle_traditional_gui: Hotkey,
    pub toggle_modern_gui: Hotkey,
    pub toggle_keystrokes: Hotkey,
    pub switch_commit_mode: Hotkey,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Hotkey {
    pub key: String,
    pub description: String,
}

impl Config {
    pub fn default_config() -> Self {
        Config {
            files: Files {
                punctuation_file: "dicts/chinese/punctuation.json".to_string(),
                profiles: vec![
                    Profile { name: "chinese".to_string(), path: "data/chinese/trie".to_string() },
                    Profile { name: "english".to_string(), path: "data/english/trie".to_string() },
                    Profile { name: "japanese".to_string(), path: "data/japanese/trie".to_string() },
                ],
            },
            appearance: Appearance {
                show_candidates: true,
                show_modern_candidates: false,
                show_notifications: true,
                show_keystrokes: false,
                page_size: 5,
                show_tone_hint: false,
                show_en_hint: true,
                candidate_anchor: "bottom".to_string(),
                candidate_font_size: 18,
                candidate_hint_font_size: 10,
                candidate_margin_x: 0,
                candidate_margin_y: 50,
                candidate_bg_color: "#ffffff".to_string(),
                candidate_text_color: "#24292e".to_string(), // Dark grey
                candidate_highlight_color: "#0969da".to_string(), // GitHub Blue
                pinyin_color: "#586069".to_string(), // Light grey
                pinyin_font_size: 18,
                pinyin_font_family: "SimHei".to_string(),
                candidate_font_family: "SimHei".to_string(),
                corner_radius: 10.0, // Smoother corners
                modern_cand_anchor: "bottom_left".to_string(),
                modern_cand_font_size: 16,
                modern_cand_hint_font_size: 10,
                modern_cand_margin_x: 40,
                modern_cand_margin_y: 200,
                modern_cand_text_color: "#0969da".to_string(),
                modern_cand_highlight_color: "#0969da".to_string(),
                modern_cand_bg_color: "rgba(255, 255, 255, 0.95)".to_string(),
                keystroke_anchor: "bottom_right".to_string(),
                keystroke_font_size: 24,
                keystroke_timeout_ms: 1500,
                keystroke_bg_color: "rgba(0, 0, 0, 0.7)".to_string(),
                keystroke_margin_x: 20,
                keystroke_margin_y: 20,
                learning_mode: false,
                learning_anchor: "top_right".to_string(),
                learning_interval_sec: 10,
                learning_font_size: 24,
                learning_margin_x: 20,
                learning_margin_y: 40,
                learning_bg_color: "rgba(20, 20, 20, 0.85)".to_string(),
                learning_dict_path: "dicts/chinese/words/words.json".to_string(),
                preview_mode: "pinyin".to_string(),
                learning_interval: 10,
            },
            input: Input {
                autostart: false,
                commit_mode: "single".to_string(),
                default_profile: "chinese".to_string(),
                paste_method: "shift_insert".to_string(),
                clipboard_delay_ms: 10,
                enable_anti_typo: true,
                enable_double_tap: true,
                double_tap_timeout_ms: 250, // 默认 250ms
                double_taps: vec![
                    DoubleTap { trigger_key: "i".to_string(), insert_text: "ing".to_string() },
                    DoubleTap { trigger_key: "u".to_string(), insert_text: "sh".to_string() },
                    DoubleTap { trigger_key: "l".to_string(), insert_text: "uang".to_string() },
                    DoubleTap { trigger_key: "o".to_string(), insert_text: "ong".to_string() },
                    DoubleTap { trigger_key: "p".to_string(), insert_text: "un".to_string() },
                    DoubleTap { trigger_key: "j".to_string(), insert_text: "iang".to_string() },
                    DoubleTap { trigger_key: "k".to_string(), insert_text: "uai".to_string() },
                    DoubleTap { trigger_key: "n".to_string(), insert_text: "ian".to_string() },
                    DoubleTap { trigger_key: "m".to_string(), insert_text: "ian".to_string() },
                    DoubleTap { trigger_key: "a".to_string(), insert_text: "ch".to_string() },
                    DoubleTap { trigger_key: "e".to_string(), insert_text: "zh".to_string() },
                    DoubleTap { trigger_key: "z".to_string(), insert_text: "ou".to_string() },
                    DoubleTap { trigger_key: "c".to_string(), insert_text: "ao".to_string() },
                    DoubleTap { trigger_key: "s".to_string(), insert_text: "ai".to_string() },
                ],
                enable_long_press: true,
                long_press_timeout_ms: 400,
                long_press_mappings: vec![
                    LongPressMapping { trigger_key: "q".to_string(), insert_text: "Q".to_string() },
                    LongPressMapping { trigger_key: "w".to_string(), insert_text: "W".to_string() },
                    LongPressMapping { trigger_key: "e".to_string(), insert_text: "E".to_string() },
                    LongPressMapping { trigger_key: "r".to_string(), insert_text: "R".to_string() },
                    LongPressMapping { trigger_key: "t".to_string(), insert_text: "T".to_string() },
                    LongPressMapping { trigger_key: "y".to_string(), insert_text: "Y".to_string() },
                    LongPressMapping { trigger_key: "u".to_string(), insert_text: "U".to_string() },
                    LongPressMapping { trigger_key: "i".to_string(), insert_text: "I".to_string() },
                    LongPressMapping { trigger_key: "o".to_string(), insert_text: "O".to_string() },
                    LongPressMapping { trigger_key: "p".to_string(), insert_text: "P".to_string() },
                    LongPressMapping { trigger_key: "a".to_string(), insert_text: "A".to_string() },
                    LongPressMapping { trigger_key: "s".to_string(), insert_text: "S".to_string() },
                    LongPressMapping { trigger_key: "d".to_string(), insert_text: "D".to_string() },
                    LongPressMapping { trigger_key: "f".to_string(), insert_text: "F".to_string() },
                    LongPressMapping { trigger_key: "g".to_string(), insert_text: "G".to_string() },
                    LongPressMapping { trigger_key: "h".to_string(), insert_text: "H".to_string() },
                    LongPressMapping { trigger_key: "j".to_string(), insert_text: "J".to_string() },
                    LongPressMapping { trigger_key: "k".to_string(), insert_text: "K".to_string() },
                    LongPressMapping { trigger_key: "l".to_string(), insert_text: "L".to_string() },
                    LongPressMapping { trigger_key: "z".to_string(), insert_text: "Z".to_string() },
                    LongPressMapping { trigger_key: "x".to_string(), insert_text: "X".to_string() },
                    LongPressMapping { trigger_key: "c".to_string(), insert_text: "C".to_string() },
                    LongPressMapping { trigger_key: "v".to_string(), insert_text: "V".to_string() },
                    LongPressMapping { trigger_key: "b".to_string(), insert_text: "B".to_string() },
                    LongPressMapping { trigger_key: "n".to_string(), insert_text: "N".to_string() },
                    LongPressMapping { trigger_key: "m".to_string(), insert_text: "M".to_string() },
                ],
                auto_commit_unique_en_fuzhuma: false,
                auto_commit_unique_full_match: false,
                enable_prefix_matching: true,
                prefix_matching_limit: 20,
                enable_abbreviation_matching: true,
                filter_proper_nouns_by_case: true,
                active_profiles: vec!["chinese".to_string()],
                profile_keys: vec![],
                page_flipping_keys: vec!["arrow".to_string()],
                swap_arrow_keys: false,
                enable_error_sound: true,
                enable_english_filter: true,
                enable_caps_selection: true,
                enable_number_selection: true,
                enable_user_dict: true,
                enable_fixed_first_candidate: false,
            },
            hotkeys: Hotkeys {
                switch_language: Hotkey { key: "tab".to_string(), description: "核心: 切换中/英文模式".to_string() },
                switch_language_alt: Hotkey { key: "ctrl+space".to_string(), description: "核心: 切换中/英文模式 (备选)".to_string() },
                switch_dictionary: Hotkey { key: "ctrl+alt+s".to_string(), description: "功能: 切换输入方案/词库".to_string() },
                cycle_preview_mode: Hotkey { key: "ctrl+alt+p".to_string(), description: "界面: 切换屏幕预览模式".to_string() },
                toggle_notifications: Hotkey { key: "ctrl+alt+n".to_string(), description: "界面: 开启/关闭系统通知".to_string() },
                cycle_paste_method: Hotkey { key: "ctrl+alt+v".to_string(), description: "兼容: 切换粘贴注入方式".to_string() },
                toggle_traditional_gui: Hotkey { key: "ctrl+alt+g".to_string(), description: "界面: 显示/隐藏传统候选窗".to_string() },
                toggle_modern_gui: Hotkey { key: "ctrl+alt+h".to_string(), description: "界面: 显示/隐藏卡片候选词".to_string() },
                toggle_keystrokes: Hotkey { key: "ctrl+alt+k".to_string(), description: "功能: 开启/关闭按键显示".to_string() },
                switch_commit_mode: Hotkey { key: "ctrl+alt+m".to_string(), description: "模式: 切换单/双空格上屏".to_string() },
            },
        }
    }
}

#[allow(dead_code)]
pub fn parse_key(s: &str) -> Vec<Vec<Vec<evdev::Key>>> {
    let mut combinations = Vec::new();
    for combo_str in s.split('|') {
        let mut requirements = Vec::new();
        for part in combo_str.split('+') {
            let k = part.to_lowercase().trim().to_string();
            let mut keys = Vec::new();
            match k.as_str() {
                "ctrl" => { keys.push(evdev::Key::KEY_LEFTCTRL); keys.push(evdev::Key::KEY_RIGHTCTRL); }
                "shift" => { keys.push(evdev::Key::KEY_LEFTSHIFT); keys.push(evdev::Key::KEY_RIGHTSHIFT); }
                "alt" => { keys.push(evdev::Key::KEY_LEFTALT); keys.push(evdev::Key::KEY_RIGHTALT); }
                "meta" | "win" => { keys.push(evdev::Key::KEY_LEFTMETA); keys.push(evdev::Key::KEY_RIGHTMETA); }
                _ => { if let Some(key) = string_to_key(&k) { keys.push(key); } }
            }
            if !keys.is_empty() { requirements.push(keys); }
        }
        if !requirements.is_empty() { combinations.push(requirements); }
    }
    combinations
}

#[allow(dead_code)]
fn string_to_key(s: &str) -> Option<evdev::Key> {
    match s {
        "ctrl" | "lctrl" | "rctrl" => Some(evdev::Key::KEY_LEFTCTRL),
        "shift" | "lshift" | "rshift" => Some(evdev::Key::KEY_LEFTSHIFT),
        "alt" | "lalt" | "ralt" => Some(evdev::Key::KEY_LEFTALT),
        "meta" | "win" | "command" => Some(evdev::Key::KEY_LEFTMETA),
        "space" => Some(evdev::Key::KEY_SPACE),
        "enter" => Some(evdev::Key::KEY_ENTER),
        "tab" => Some(evdev::Key::KEY_TAB),
        "backspace" => Some(evdev::Key::KEY_BACKSPACE),
        "esc" | "escape" => Some(evdev::Key::KEY_ESC),
        "caps_lock" | "caps" => Some(evdev::Key::KEY_CAPSLOCK),
        "a" => Some(evdev::Key::KEY_A), "b" => Some(evdev::Key::KEY_B), "c" => Some(evdev::Key::KEY_C),
        "d" => Some(evdev::Key::KEY_D), "e" => Some(evdev::Key::KEY_E), "f" => Some(evdev::Key::KEY_F),
        "g" => Some(evdev::Key::KEY_G), "h" => Some(evdev::Key::KEY_H), "i" => Some(evdev::Key::KEY_I),
        "j" => Some(evdev::Key::KEY_J), "k" => Some(evdev::Key::KEY_K), "l" => Some(evdev::Key::KEY_L),
        "m" => Some(evdev::Key::KEY_M), "n" => Some(evdev::Key::KEY_N), "o" => Some(evdev::Key::KEY_O),
        "p" => Some(evdev::Key::KEY_P), "q" => Some(evdev::Key::KEY_Q), "r" => Some(evdev::Key::KEY_R),
        "s" => Some(evdev::Key::KEY_S), "t" => Some(evdev::Key::KEY_T), "u" => Some(evdev::Key::KEY_U),
        "v" => Some(evdev::Key::KEY_V), "w" => Some(evdev::Key::KEY_W), "x" => Some(evdev::Key::KEY_X),
        "y" => Some(evdev::Key::KEY_Y), "z" => Some(evdev::Key::KEY_Z),
        "0" => Some(evdev::Key::KEY_0), "1" => Some(evdev::Key::KEY_1), "2" => Some(evdev::Key::KEY_2),
        "3" => Some(evdev::Key::KEY_3), "4" => Some(evdev::Key::KEY_4), "5" => Some(evdev::Key::KEY_5),
        "6" => Some(evdev::Key::KEY_6), "7" => Some(evdev::Key::KEY_7), "8" => Some(evdev::Key::KEY_8),
        "9" => Some(evdev::Key::KEY_9),
        _ => None,
    }
}
