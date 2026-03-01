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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum AuxMode {
    None,
    English,
    Stroke,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Appearance {
    pub show_candidates: bool,
    pub page_size: usize,
    pub show_tone_hint: bool,
    pub aux_mode: AuxMode,
    pub candidate_anchor: String,
    pub candidate_layout: String, // "horizontal" 或 "vertical"
    
    // Window Style
    pub corner_radius: f32,
    pub window_bg_color: String,
    pub window_highlight_color: String,
    pub window_border_color: String,
    pub window_padding_x: i32,
    pub window_padding_y: i32,
    pub item_spacing: f32,
    pub row_spacing: f32,

    // Text Styles
    pub pinyin_text: TextStyle,
    pub candidate_text: TextStyle,
    pub hint_text: TextStyle,
    pub comment_text: TextStyle, // For extra info like "User", "Emoji"

    pub preview_mode: String,
    pub show_english_aux: bool,
    pub show_stroke_aux: bool,
    pub show_pinyin_hint: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TextStyle {
    pub font_family: String,
    pub font_size: u32,
    pub color: String,
    pub alpha: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum AntiTypoMode {
    None,
    Strict,
    Smart,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Input {
    pub autostart: bool,
    pub commit_mode: String,
    pub default_profile: String,
    pub paste_method: String,
    pub clipboard_delay_ms: u64,
    pub anti_typo_mode: AntiTypoMode,
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
    pub enable_smart_backspace: bool,
    pub enable_double_pinyin: bool,
    pub double_pinyin_scheme: DoublePinyinScheme,
    pub enable_fuzzy_pinyin: bool,
    pub fuzzy_config: FuzzyPinyinConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FuzzyPinyinConfig {
    pub z_zh: bool,
    pub c_ch: bool,
    pub s_sh: bool,
    pub n_l: bool,
    pub r_l: bool,
    pub f_h: bool,
    pub an_ang: bool,
    pub en_eng: bool,
    pub in_ing: bool,
    pub ian_iang: bool,
    pub uan_uang: bool,
    pub u_v: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DoublePinyinScheme {
    pub name: String,
    pub initials: std::collections::HashMap<String, String>, // v -> zh
    pub rimes: std::collections::HashMap<String, String>,    // q -> iu
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
    pub cycle_paste_method: Hotkey,
    pub toggle_traditional_gui: Hotkey,
    pub switch_commit_mode: Hotkey,
    pub toggle_double_pinyin: Hotkey,
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
                page_size: 5,
                show_tone_hint: false,
                aux_mode: AuxMode::English,
                candidate_anchor: "bottom".to_string(),
                candidate_layout: "horizontal".to_string(),
                
                corner_radius: 10.0,
                window_bg_color: "#efefef".to_string(),
                window_highlight_color: "#0969da".to_string(),
                window_border_color: "rgba(0, 0, 0, 0.2)".to_string(),
                window_padding_x: 18,
                window_padding_y: 14,
                item_spacing: 16.0,
                row_spacing: 8.0,

                pinyin_text: TextStyle {
                    font_family: "Microsoft YaHei".to_string(),
                    font_size: 18,
                    color: "#586069".to_string(),
                    alpha: 1.0,
                },
                candidate_text: TextStyle {
                    font_family: "Microsoft YaHei".to_string(),
                    font_size: 18,
                    color: "#24292e".to_string(),
                    alpha: 1.0,
                },
                hint_text: TextStyle {
                    font_family: "Arial".to_string(),
                    font_size: 14,
                    color: "#6e7781".to_string(),
                    alpha: 0.8,
                },
                comment_text: TextStyle {
                    font_family: "Segoe UI Emoji".to_string(),
                    font_size: 12,
                    color: "#0969da".to_string(),
                    alpha: 0.7,
                },

                preview_mode: "pinyin".to_string(),
                show_english_aux: true,
                show_stroke_aux: true,
                show_pinyin_hint: true,
            },
            input: Input {
                autostart: false,
                commit_mode: "single".to_string(),
                default_profile: "chinese".to_string(),
                paste_method: "shift_insert".to_string(),
                clipboard_delay_ms: 10,
                anti_typo_mode: AntiTypoMode::None,
                enable_double_tap: false,
                double_tap_timeout_ms: 250,
                double_taps: vec![
                    DoubleTap { trigger_key: "i".into(), insert_text: "ing".into() },
                    DoubleTap { trigger_key: "l".into(), insert_text: "uang".into() },
                    DoubleTap { trigger_key: "o".into(), insert_text: "ong".into() },
                    DoubleTap { trigger_key: "j".into(), insert_text: "an".into() },
                    DoubleTap { trigger_key: "k".into(), insert_text: "uai".into() },
                    DoubleTap { trigger_key: "n".into(), insert_text: "ian".into() },
                    DoubleTap { trigger_key: "m".into(), insert_text: "ian".into() },
                    DoubleTap { trigger_key: "a".into(), insert_text: "ang".into() },
                    DoubleTap { trigger_key: "f".into(), insert_text: "en".into() },
                    DoubleTap { trigger_key: "d".into(), insert_text: "ai".into() },
                    DoubleTap { trigger_key: "w".into(), insert_text: "ei".into() },
                    DoubleTap { trigger_key: "g".into(), insert_text: "ao".into() },
                    DoubleTap { trigger_key: "h".into(), insert_text: "ou".into() },
                    DoubleTap { trigger_key: "p".into(), insert_text: "iong".into() },
                    DoubleTap { trigger_key: "u".into(), insert_text: "ui".into() },
                    DoubleTap { trigger_key: "x".into(), insert_text: "ua".into() },
                ],
                enable_long_press: false,
                long_press_timeout_ms: 400,
                long_press_mappings: vec![
                    LongPressMapping { trigger_key: "q".into(), insert_text: "Q".into() },
                    LongPressMapping { trigger_key: "w".into(), insert_text: "W".into() },
                    LongPressMapping { trigger_key: "e".into(), insert_text: "E".into() },
                    LongPressMapping { trigger_key: "r".into(), insert_text: "R".into() },
                    LongPressMapping { trigger_key: "t".into(), insert_text: "T".into() },
                    LongPressMapping { trigger_key: "y".into(), insert_text: "Y".into() },
                    LongPressMapping { trigger_key: "u".into(), insert_text: "U".into() },
                    LongPressMapping { trigger_key: "i".into(), insert_text: "I".into() },
                    LongPressMapping { trigger_key: "o".into(), insert_text: "O".into() },
                    LongPressMapping { trigger_key: "p".into(), insert_text: "P".into() },
                    LongPressMapping { trigger_key: "a".into(), insert_text: "A".into() },
                    LongPressMapping { trigger_key: "s".into(), insert_text: "S".into() },
                    LongPressMapping { trigger_key: "d".into(), insert_text: "D".into() },
                    LongPressMapping { trigger_key: "f".into(), insert_text: "F".into() },
                    LongPressMapping { trigger_key: "g".into(), insert_text: "G".into() },
                    LongPressMapping { trigger_key: "h".into(), insert_text: "H".into() },
                    LongPressMapping { trigger_key: "j".into(), insert_text: "J".into() },
                    LongPressMapping { trigger_key: "k".into(), insert_text: "K".into() },
                    LongPressMapping { trigger_key: "l".into(), insert_text: "L".into() },
                    LongPressMapping { trigger_key: "z".into(), insert_text: "Z".into() },
                    LongPressMapping { trigger_key: "x".into(), insert_text: "X".into() },
                    LongPressMapping { trigger_key: "c".into(), insert_text: "C".into() },
                    LongPressMapping { trigger_key: "v".into(), insert_text: "V".into() },
                    LongPressMapping { trigger_key: "b".into(), insert_text: "B".into() },
                    LongPressMapping { trigger_key: "n".into(), insert_text: "N".into() },
                    LongPressMapping { trigger_key: "m".into(), insert_text: "M".into() },
                ],
                auto_commit_unique_en_fuzhuma: false,
                auto_commit_unique_full_match: false,
                enable_prefix_matching: true,
                prefix_matching_limit: 20,
                enable_abbreviation_matching: true,
                filter_proper_nouns_by_case: true,
                active_profiles: vec!["chinese".to_string()],
                profile_keys: vec![],
                page_flipping_keys: vec!["arrow".to_string(), "minus_equal".to_string(), "comma_dot".to_string()],
                swap_arrow_keys: false,
                enable_error_sound: true,
                enable_english_filter: true,
                enable_caps_selection: true,
                enable_number_selection: true,
                enable_user_dict: true,
                enable_fixed_first_candidate: false,
                enable_smart_backspace: false,
                enable_double_pinyin: false,
                double_pinyin_scheme: DoublePinyinScheme {
                    name: "小鹤双拼".to_string(),
                    initials: [
                        ("v", "zh"), ("u", "sh"), ("i", "ch")
                    ].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                    rimes: [
                        ("p", "ie"), ("b", "in"), ("m", "ian"),  ("q", "iu"),
                        ("r", "uan"), ("x", "ia"), ("k", "ao"), ("f", "en"),
                        ("d", "ai"), ("j", "an"), ("t", "ue"), ("c", "ao"), ("s", "ong"),
                        ("z", "ou"), ("y", "un"), ("w", "ei"), ("l", "iang")
                    ].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                },
                enable_fuzzy_pinyin: false,
                fuzzy_config: FuzzyPinyinConfig {
                    z_zh: true, c_ch: true, s_sh: true, n_l: false, r_l: false, f_h: false,
                    an_ang: false, en_eng: false, in_ing: false, ian_iang: false, uan_uang: false, u_v: false,
                },
            },
            hotkeys: Hotkeys {
                switch_language: Hotkey { key: "tab".to_string(), description: "核心: 切换中/英文模式".to_string() },
                switch_language_alt: Hotkey { key: "ctrl+space".to_string(), description: "核心: 切换中/英文模式 (备选)".to_string() },
                switch_dictionary: Hotkey { key: "ctrl+alt+s".to_string(), description: "功能: 切换输入方案/词库".to_string() },
                cycle_preview_mode: Hotkey { key: "ctrl+alt+p".to_string(), description: "界面: 切换屏幕预览模式".to_string() },
                cycle_paste_method: Hotkey { key: "ctrl+alt+v".to_string(), description: "兼容: 切换粘贴注入方式".to_string() },
                toggle_traditional_gui: Hotkey { key: "ctrl+alt+g".to_string(), description: "界面: 显示/隐藏候选窗".to_string() },
                switch_commit_mode: Hotkey { key: "ctrl+alt+m".to_string(), description: "模式: 切换单/双空格上屏".to_string() },
                toggle_double_pinyin: Hotkey { key: "ctrl+alt+d".to_string(), description: "模式: 开启/关闭双拼模式".to_string() },
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