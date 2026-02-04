use serde::{Serialize, Deserialize};

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
    pub candidate_margin_x: i32,
    pub candidate_margin_y: i32,
    pub candidate_bg_color: String,
    pub candidate_text_color: String,
    pub modern_cand_anchor: String,
    pub modern_cand_font_size: u32,
    pub modern_cand_margin_x: i32,
    pub modern_cand_margin_y: i32,
    pub modern_cand_text_color: String,
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
    pub enable_quick_rime: bool,
    pub quick_rimes: Vec<QuickRime>,
    pub auto_commit_unique_en_fuzhuma: bool,
    pub auto_commit_unique_full_match: bool,
    pub enable_prefix_matching: bool,
    pub prefix_matching_limit: usize,
    pub enable_abbreviation_matching: bool,
    pub filter_proper_nouns_by_case: bool,
    pub active_profiles: Vec<String>,
    pub profile_keys: Vec<ProfileKey>,
    pub page_flipping_keys: Vec<String>,
    pub enable_error_sound: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct QuickRime {
    pub trigger: String,
    pub insert: String,
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
                candidate_margin_x: 0,
                candidate_margin_y: 50,
                candidate_bg_color: "#ffffff".to_string(),
                candidate_text_color: "#000000".to_string(),
                modern_cand_anchor: "bottom_left".to_string(),
                modern_cand_font_size: 16,
                modern_cand_margin_x: 40,
                modern_cand_margin_y: 200,
                modern_cand_text_color: "#0071e3".to_string(),
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
                enable_quick_rime: true,
                quick_rimes: vec![
                    QuickRime { trigger: "caps+j".to_string(), insert: "iang".to_string() },
                    QuickRime { trigger: "caps+k".to_string(), insert: "uai".to_string() },
                    QuickRime { trigger: "caps+l".to_string(), insert: "uang".to_string() },
                    QuickRime { trigger: "caps+i".to_string(), insert: "ing".to_string() },
                    QuickRime { trigger: "caps+o".to_string(), insert: "ong".to_string() },
                    QuickRime { trigger: "caps+p".to_string(), insert: "un".to_string() },
                    QuickRime { trigger: "caps+n".to_string(), insert: "ian".to_string() },
                    QuickRime { trigger: "caps+m".to_string(), insert: "ian".to_string() },
                    QuickRime { trigger: "caps+u".to_string(), insert: "sh".to_string() },
                    QuickRime { trigger: "caps+a".to_string(), insert: "ch".to_string() },
                    QuickRime { trigger: "caps+e".to_string(), insert: "zh".to_string() },
                    QuickRime { trigger: "caps+z".to_string(), insert: "ou".to_string() },
                    QuickRime { trigger: "caps+c".to_string(), insert: "ao".to_string() },
                    QuickRime { trigger: "caps+s".to_string(), insert: "ai".to_string() },
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
                enable_error_sound: true,
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
