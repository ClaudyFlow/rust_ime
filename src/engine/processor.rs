use std::collections::HashMap;
use crate::engine::trie::Trie;
use crate::engine::keys::VirtualKey;
use serde_json::Value;
use std::time::{Instant, Duration};

#[derive(Debug, Clone, PartialEq)]
pub enum ImeState {
    Direct,
    Composing,
    NoMatch,
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Emit(String),
    DeleteAndEmit { delete: usize, insert: String },
    PassThrough,
    Consume,
    Alert,
    Notify(String, String), // Summary, Body
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhantomMode {
    None,
    Pinyin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterMode {
    None,
    Global, // Shift + 字母 (全局筛选)
    Page,   // Caps + 字母 (当前页筛选)
}

struct ParsedPart {
    pinyin: String,
    aux_code: Option<String>,
    specified_idx: Option<usize>,
    raw: String,
}

pub struct Processor {
    pub state: ImeState,
    pub buffer: String,
    pub tries: HashMap<String, Trie>, 
    pub active_profiles: Vec<String>,
    pub punctuation: HashMap<String, Vec<String>>,
    pub candidates: Vec<String>,
    pub candidate_hints: Vec<String>, 
    pub selected: usize,
    pub page: usize,
    pub chinese_enabled: bool,
    pub best_segmentation: Vec<String>,
    pub joined_sentence: String,
    
    pub show_candidates: bool,
    pub show_modern_candidates: bool,
    pub show_notifications: bool,
    pub show_keystrokes: bool,
    pub phantom_mode: PhantomMode,
    pub phantom_text: String,
    pub preview_selected_candidate: bool,
    pub enable_anti_typo: bool,
    pub commit_mode: String,
    pub switch_mode: bool,
    pub cursor_pos: usize,
    pub profile_keys: Vec<(String, String)>,
    pub page_size: usize,
    pub show_tone_hint: bool,
    pub show_en_hint: bool,
    pub auto_commit_unique_en_fuzhuma: bool,
    pub auto_commit_unique_full_match: bool,
    pub enable_prefix_matching: bool,
    pub prefix_matching_limit: usize,
    pub enable_abbreviation_matching: bool,
    pub filter_proper_nouns_by_case: bool,
    pub enable_error_sound: bool,
    pub has_dict_match: bool,
    pub page_flipping_styles: Vec<String>,
    pub swap_arrow_keys: bool,
    
    // 筛选模式相关
    pub aux_filter: String,
    pub filter_mode: FilterMode,
    pub page_snapshot: Vec<(String, String)>, // (candidate, hint)
    
    pub enable_english_filter: bool,
    pub enable_caps_selection: bool,
    pub enable_number_selection: bool,
    
    // 双击相关
    pub enable_double_tap: bool,
    pub double_tap_timeout: Duration,
    pub double_taps: HashMap<String, String>,
    pub last_tap_key: Option<VirtualKey>,
    pub last_tap_time: Option<Instant>,

    // 长按相关
    pub enable_long_press: bool,
    pub long_press_timeout: Duration,
    pub long_press_mappings: HashMap<String, String>,
    pub key_press_info: Option<(VirtualKey, Instant)>,
    pub long_press_triggered: bool,

    pub nav_mode: bool,

    // 用户个人词库相关
    pub enable_user_dict: bool,
    pub enable_fixed_first_candidate: bool,
    pub user_dict: HashMap<String, Vec<(String, u32)>>, // 拼音 -> Vec<(词组, 词频)>
    pub last_lookup_pinyin: String, // 记录最近一次检索的拼音串
    
    // 连续选词记忆
    pub commit_history: Vec<(String, String)>, // 最近上屏的 (拼音, 词组)
    pub last_commit_time: Instant,
}

impl Processor {
    fn parse_buffer(&self) -> Vec<ParsedPart> {
        let buffer_normalized = strip_tones(&self.buffer);
        let parts: Vec<&str> = buffer_normalized.split(' ').filter(|s| !s.is_empty()).collect();
        let mut result = Vec::new();

        for part in parts {
            let split_pos = part.char_indices().find(|(i, c)| {
                c.is_ascii_digit() || (*i > 0 && c.is_ascii_uppercase())
            }).map(|(i, _)| i);
            
            let (pinyin, aux, idx) = if let Some(pos) = split_pos {
                let (p, suffix) = part.split_at(pos);
                let digit_start = suffix.find(|c: char| c.is_ascii_digit());
                
                let (a, d) = if let Some(ds) = digit_start {
                    let (alpha, digits) = suffix.split_at(ds);
                    let aux_str = if alpha.is_empty() { None } else { Some(alpha.to_string()) };
                    let end_of_digits = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
                    let idx_val = digits[..end_of_digits].parse::<usize>().ok();
                    (aux_str, idx_val)
                } else {
                    (Some(suffix.to_string()), None)
                };
                (p.to_string(), a, d)
            } else {
                (part.to_string(), None, None)
            };

            result.push(ParsedPart {
                pinyin,
                aux_code: aux,
                specified_idx: idx,
                raw: part.to_string(),
            });
        }
        result
    }

    pub fn new(
        tries: HashMap<String, Trie>, 
        initial_profile: String, 
        punctuation_raw: HashMap<String, Value>, 
    ) -> Self {
        let mut punctuation = HashMap::new();
        for (k, v) in punctuation_raw {
            if let Some(arr) = v.as_array() {
                let chars: Vec<String> = arr.iter().filter_map(|item| item.get("char").and_then(|c| c.as_str())).map(|s| s.to_string()).collect();
                punctuation.insert(k, chars);
            }
        }

        let phantom_mode = if cfg!(target_os = "windows") { PhantomMode::None } else { PhantomMode::Pinyin };

        Self {
            state: ImeState::Direct, buffer: String::new(), tries, 
            active_profiles: vec![initial_profile],
            punctuation, candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, 
            chinese_enabled: false, best_segmentation: vec![],
            joined_sentence: String::new(),
            show_candidates: true, show_modern_candidates: false, show_notifications: true, show_keystrokes: true,
            phantom_mode,
            phantom_text: String::new(),
            preview_selected_candidate: false,
            enable_anti_typo: true,
            commit_mode: "single".to_string(),
            switch_mode: false,
            cursor_pos: 0,
            profile_keys: Vec::new(),
            auto_commit_unique_en_fuzhuma: false,
            auto_commit_unique_full_match: false,
            enable_prefix_matching: true,
            prefix_matching_limit: 20,
            enable_abbreviation_matching: true,
            filter_proper_nouns_by_case: true,
            enable_error_sound: true,
            has_dict_match: false,
            page_size: 5,
            show_tone_hint: false,
            show_en_hint: true,
            page_flipping_styles: vec!["arrow".to_string()],
            swap_arrow_keys: false,
            aux_filter: String::new(),
            filter_mode: FilterMode::None,
            page_snapshot: Vec::new(),
            
            enable_english_filter: true,
            enable_caps_selection: true,
            enable_number_selection: true,
            
            enable_double_tap: true,
            double_tap_timeout: Duration::from_millis(250),
            double_taps: HashMap::new(),
            last_tap_key: None,
            last_tap_time: None,

            enable_long_press: true,
            long_press_timeout: Duration::from_millis(400),
            long_press_mappings: HashMap::new(),
            key_press_info: None,
            long_press_triggered: false,
            nav_mode: false,
            enable_user_dict: true,
            enable_fixed_first_candidate: false,
            user_dict: HashMap::new(),
            last_lookup_pinyin: String::new(),
            commit_history: Vec::new(),
            last_commit_time: Instant::now(),
        }
    }

    pub fn apply_config(&mut self, conf: &crate::config::Config) {
        self.enable_user_dict = conf.input.enable_user_dict;
        self.enable_fixed_first_candidate = conf.input.enable_fixed_first_candidate;
        // 如果是初次加载或切换，可以从文件读取
        if self.enable_user_dict && self.user_dict.is_empty() {
            self.load_user_dict();
        }
        self.show_candidates = conf.appearance.show_candidates;
        self.show_modern_candidates = conf.appearance.show_modern_candidates;
        self.show_notifications = conf.appearance.show_notifications;
        self.show_keystrokes = conf.appearance.show_keystrokes;
        self.page_size = conf.appearance.page_size;
        self.show_tone_hint = conf.appearance.show_tone_hint;
        self.show_en_hint = conf.appearance.show_en_hint;
        self.enable_anti_typo = conf.input.enable_anti_typo;
        self.commit_mode = conf.input.commit_mode.clone();
        self.auto_commit_unique_en_fuzhuma = conf.input.auto_commit_unique_en_fuzhuma;
        self.auto_commit_unique_full_match = conf.input.auto_commit_unique_full_match;
        self.enable_error_sound = conf.input.enable_error_sound;
        self.enable_prefix_matching = conf.input.enable_prefix_matching;
        self.prefix_matching_limit = conf.input.prefix_matching_limit;
        self.enable_abbreviation_matching = conf.input.enable_abbreviation_matching;
        self.filter_proper_nouns_by_case = conf.input.filter_proper_nouns_by_case;
        self.profile_keys = conf.input.profile_keys.iter().map(|pk| (pk.key.to_lowercase(), pk.profile.to_lowercase())).collect();
        
        self.page_flipping_styles = conf.input.page_flipping_keys.iter().map(|s| s.to_lowercase()).collect();
        self.swap_arrow_keys = conf.input.swap_arrow_keys;
        
        self.enable_english_filter = conf.input.enable_english_filter;
        self.enable_caps_selection = conf.input.enable_caps_selection;
        self.enable_number_selection = conf.input.enable_number_selection;

        self.enable_double_tap = conf.input.enable_double_tap;
        self.double_tap_timeout = Duration::from_millis(conf.input.double_tap_timeout_ms);
        self.double_taps.clear();
        for dt in &conf.input.double_taps {
            self.double_taps.insert(dt.trigger_key.to_lowercase(), dt.insert_text.clone());
        }

        self.enable_long_press = conf.input.enable_long_press;
        self.long_press_timeout = Duration::from_millis(conf.input.long_press_timeout_ms);
        self.long_press_mappings.clear();
        for lm in &conf.input.long_press_mappings {
            self.long_press_mappings.insert(lm.trigger_key.to_lowercase(), lm.insert_text.clone());
        }

        if !conf.input.active_profiles.is_empty() {
            self.active_profiles = conf.input.active_profiles.iter().map(|p| p.to_lowercase()).collect();
        } else {
            let new_profile = conf.input.default_profile.to_lowercase();
            if !new_profile.is_empty() && self.tries.contains_key(&new_profile) {
                self.active_profiles = vec![new_profile];
            }
        }

        self.phantom_mode = if cfg!(target_os = "windows") {
            PhantomMode::None
        } else {
            match conf.appearance.preview_mode.as_str() {
                "pinyin" => PhantomMode::Pinyin,
                _ => PhantomMode::None,
            }
        };

        if self.buffer.is_empty() {
            self.reset();
        } else {
            let _ = self.lookup();
        }
    }

    pub fn toggle(&mut self) -> Action {
        self.chinese_enabled = !self.chinese_enabled;
        self.reset();
        Action::Consume
    }

    #[allow(dead_code)]
    pub fn inject_text(&mut self, text: &str) -> Action {
        self.buffer.push_str(text);
        if self.state == ImeState::Direct { self.state = ImeState::Composing; }
        self.preview_selected_candidate = false;
        if let Some(act) = self.lookup() { return act; }
        if let Some(act) = self.check_auto_commit() { return act; }
        self.update_phantom_action()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.candidates.clear();
        self.candidate_hints.clear();
        self.best_segmentation.clear();
        self.joined_sentence.clear();
        self.selected = 0;
        self.page = 0;
        self.state = ImeState::Direct;
        self.phantom_text.clear();
        self.preview_selected_candidate = false;
        self.switch_mode = false;
        self.cursor_pos = 0;
        self.aux_filter.clear();
        self.filter_mode = FilterMode::None;
        self.page_snapshot.clear();
        self.nav_mode = false;
    }

    pub fn handle_key(&mut self, key: VirtualKey, val: i32, shift_pressed: bool) -> Action {
        if !self.chinese_enabled {
            return Action::PassThrough;
        }
        let _is_press = val != 0;
        let is_repeat = val == 2;
        let is_release = val == 0;
        let now = Instant::now();

        // 处理长按逻辑
        if self.enable_long_press && is_letter(key) && !shift_pressed {
            if val == 1 {
                self.key_press_info = Some((key, now));
                self.long_press_triggered = false;
            } else if is_repeat {
                if !self.long_press_triggered {
                    if let Some((press_key, press_time)) = self.key_press_info {
                        if press_key == key && now.duration_since(press_time) >= self.long_press_timeout {
                            if let Some(c) = key_to_char(key, false) {
                                if let Some(replacement) = self.long_press_mappings.get(&c.to_string()).cloned() {
                                    self.long_press_triggered = true;
                                    if !self.buffer.is_empty() && self.buffer.ends_with(c) {
                                        self.buffer.pop();
                                    }
                                    return self.inject_text(&replacement);
                                }
                            }
                        }
                    }
                }
                return Action::Consume; 
            } else if is_release {
                self.key_press_info = None;
                if self.long_press_triggered {
                    return Action::Consume; 
                }
            }
        }

        if is_release {
            if self.buffer.is_empty() { return Action::PassThrough; }
            if key == VirtualKey::Grave { return Action::Consume; }
            return Action::Consume;
        }

        if key == VirtualKey::Grave {
            if self.buffer.is_empty() {
                self.switch_mode = !self.switch_mode;
                return if self.switch_mode { Action::Notify("快捷切换".into(), "已进入方案切换模式".into()) } else { Action::Notify("快捷切换".into(), "已退出".into()) };
            } else {
                self.nav_mode = !self.nav_mode;
                return if self.nav_mode { Action::Notify("导航模式".into(), "已开启 (HJKL)".into()) } else { Action::Notify("导航模式".into(), "已退出".into()) };
            }
        }

        if self.switch_mode {
            match key {
                VirtualKey::Esc | VirtualKey::Space | VirtualKey::Enter => { self.switch_mode = false; return Action::Notify("快捷切换".into(), "已退出".into()); }
                VirtualKey::T => {
                    self.switch_mode = false;
                    return Action::Notify("位置切换".into(), "窗口已移至顶部".into());
                }
                VirtualKey::B => {
                    self.switch_mode = false;
                    return Action::Notify("位置切换".into(), "窗口已移至底部".into());
                }
                _ if is_letter(key) => {
                    let k = key_to_char(key, false).unwrap_or(' ').to_string();
                    let mut target_profile = None;
                    for (trigger_key, profile_name) in &self.profile_keys {
                        if trigger_key == &k { target_profile = Some(profile_name.clone()); break; }
                    }

                    if let Some(p_str) = target_profile {
                        let profiles: Vec<String> = p_str.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty() && self.tries.contains_key(s)).collect();
                        if !profiles.is_empty() {
                            self.active_profiles = profiles;
                            let display = self.get_current_profile_display();
                            let _ = self.lookup();
                            self.switch_mode = false;
                            return Action::Notify("输入方案".into(), format!("已切换至: {}", display));
                        }
                    }
                }
                _ => {} 
            }
            return Action::Consume;
        }

        if !self.buffer.is_empty() { return self.handle_composing(key, shift_pressed); }
        match self.state {
            ImeState::Direct => self.handle_direct(key, shift_pressed),
            _ => self.handle_composing(key, shift_pressed)
        }
    }

    fn handle_direct(&mut self, key: VirtualKey, shift_pressed: bool) -> Action {
        if is_letter(key) {
            if let Some(c) = key_to_char(key, shift_pressed) {
                let old_buffer = self.buffer.clone();
                self.buffer.push(c);
                self.state = ImeState::Composing;
                if let Some(act) = self.lookup() { return act; }
                if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; let _ = self.lookup(); return Action::Alert; }
                return self.update_phantom_action();
            }
        }

        if let Some(punc_key) = get_punctuation_key(key, shift_pressed) {
            if let Some(zh_puncs) = self.punctuation.get(punc_key) { if let Some(first) = zh_puncs.first() { return Action::Emit(first.clone()); } }
            return Action::Emit(punc_key.to_string());
        }

        Action::PassThrough
    }

    fn handle_composing(&mut self, mut key: VirtualKey, shift_pressed: bool) -> Action {
        let has_cand = !self.candidates.is_empty();
        let now = Instant::now();

        // 如果处于导航模式，映射 HJKL 为方向键
        if self.nav_mode {
            match key {
                VirtualKey::H => key = VirtualKey::Left,
                VirtualKey::L => key = VirtualKey::Right,
                VirtualKey::K => key = VirtualKey::Up,
                VirtualKey::J => key = VirtualKey::Down,
                VirtualKey::Left | VirtualKey::Right | VirtualKey::Up | VirtualKey::Down 
                | VirtualKey::PageUp | VirtualKey::PageDown | VirtualKey::Home | VirtualKey::End 
                | VirtualKey::Space | VirtualKey::Enter | VirtualKey::Grave => { /* 保持模式 */ }
                _ => { self.nav_mode = false; } // 按其他键退出导航模式
            }
        }

        // 1. 字母键优先处理 (筛选 或 双击)
        if is_letter(key) {
            // A. 如果已经处于筛选模式，直接追加筛选码 (忽略双击)
            if self.filter_mode != FilterMode::None {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    self.aux_filter.push(c);
                    self.selected = 0;
                    if self.filter_mode == FilterMode::Global { self.page = 0; }
                    if let Some(act) = self.lookup() { return act; }
                    return self.update_phantom_action();
                }
            }
            
            // B. 尝试双击逻辑 (仅在非 Shift 且开启时)
            if !shift_pressed && self.enable_double_tap {
                if let Some(last_k) = self.last_tap_key {
                    if last_k == key {
                        if let Some(last_t) = self.last_tap_time {
                            if now.duration_since(last_t) <= self.double_tap_timeout {
                                if let Some(c) = key_to_char(key, false) {
                                    if let Some(replacement) = self.double_taps.get(&c.to_string()) {
                                        if self.buffer.ends_with(c) {
                                            self.buffer.pop();
                                            self.buffer.push_str(replacement);
                                            self.last_tap_key = None;
                                            self.last_tap_time = None;
                                            if let Some(act) = self.lookup() { return act; }
                                            return self.update_phantom_action();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // 更新状态供下次双击判断
                self.last_tap_key = Some(key);
                self.last_tap_time = Some(now);
            } else {
                self.last_tap_key = None;
                self.last_tap_time = None;
            }
        } else {
            self.last_tap_key = None;
            self.last_tap_time = None;
        }

        let styles = &self.page_flipping_styles;
        let flip_me = styles.contains(&"minus_equal".to_string());
        let flip_cd = styles.contains(&"comma_dot".to_string());
        let flip_arrow = styles.contains(&"arrow".to_string());

        match key {
            VirtualKey::Backspace => {
                if self.filter_mode != FilterMode::None {
                    self.aux_filter.pop();
                    if self.aux_filter.is_empty() {
                        self.filter_mode = FilterMode::None;
                        self.page_snapshot.clear();
                        self.page = 0; 
                    } else {
                        self.selected = 0;
                        if self.filter_mode == FilterMode::Global { self.page = 0; }
                    }
                    if let Some(act) = self.lookup() { return act; }
                    return self.update_phantom_action();
                }

                if self.buffer.is_empty() {
                    self.commit_history.clear(); // 连续退格清空历史
                    return Action::PassThrough;
                }

                self.buffer.pop();
                if self.buffer.is_empty() {
                    let del = self.phantom_text.chars().count(); self.reset();
                    if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
                } else { if let Some(act) = self.lookup() { return act; } self.update_phantom_action() }
            }
            VirtualKey::Minus if flip_me && has_cand => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            VirtualKey::Equal if flip_me && has_cand => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }
            VirtualKey::Comma if flip_cd && has_cand => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            VirtualKey::Dot if flip_cd && has_cand => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }

            VirtualKey::Left | VirtualKey::Right | VirtualKey::Up | VirtualKey::Down => {
                let (move_prev, move_next, page_prev, page_next) = if self.swap_arrow_keys {
                    (VirtualKey::Up, VirtualKey::Down, VirtualKey::Left, VirtualKey::Right)
                } else {
                    (VirtualKey::Left, VirtualKey::Right, VirtualKey::Up, VirtualKey::Down)
                };

                if key == move_prev {
                    if has_cand { self.preview_selected_candidate = true; if self.selected > 0 { self.selected -= 1; } self.page = (self.selected / self.page_size) * self.page_size; self.update_phantom_action() } else { Action::PassThrough }
                } else if key == move_next {
                    if has_cand { self.preview_selected_candidate = true; if self.selected + 1 < self.candidates.len() { self.selected += 1; } self.page = (self.selected / self.page_size) * self.page_size; self.update_phantom_action() } else { Action::PassThrough }
                } else if key == page_prev && flip_arrow {
                    self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume
                } else if key == page_next && flip_arrow {
                    if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume
                } else {
                    Action::PassThrough
                }
            }

            VirtualKey::PageUp => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            VirtualKey::PageDown => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }
            VirtualKey::Home => { if shift_pressed { self.selected = 0; self.page = 0; } else { self.selected = self.page; } Action::Consume }
            VirtualKey::End => { if has_cand { if shift_pressed { self.selected = self.candidates.len() - 1; self.page = (self.selected / self.page_size) * self.page_size; } else { self.selected = (self.page + self.page_size - 1).min(self.candidates.len() - 1); } } Action::Consume }

            VirtualKey::Space => {
                if shift_pressed {
                    if let Some(hint) = self.candidate_hints.get(self.selected) {
                        if !hint.is_empty() {
                            return self.commit_candidate(hint.clone());
                        }
                    }
                }
                if self.preview_selected_candidate || self.commit_mode == "single" { if let Some(word) = self.candidates.get(self.selected) { return self.commit_candidate(word.clone()); } }
                if self.buffer.ends_with(' ') && !self.joined_sentence.is_empty() { return self.commit_candidate(self.joined_sentence.clone()); }
                self.buffer.push(' '); self.preview_selected_candidate = false; if let Some(act) = self.lookup() { return act; } self.update_phantom_action()
            }
            VirtualKey::Enter => {
                self.commit_history.clear(); // 强制上屏原始拼音，中断组词历史
                self.last_lookup_pinyin.clear(); // 清空检索记录，确保不触发学习
                if self.commit_mode == "single" { let out = self.buffer.clone(); return self.commit_candidate(out); }
                if self.preview_selected_candidate { if let Some(word) = self.candidates.get(self.selected) { return self.commit_candidate(word.clone()); } }
                if !self.joined_sentence.is_empty() { self.commit_candidate(self.joined_sentence.clone()) } else { let out = self.buffer.clone(); self.commit_candidate(out) }
            }
            VirtualKey::Esc | VirtualKey::Delete => { 
                self.commit_history.clear(); // 取消输入，清空历史
                let del = self.phantom_text.chars().count(); self.reset(); if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume } 
            }

            VirtualKey::Slash if !self.buffer.is_empty() => {
                let mut new_buffer = self.buffer.clone();
                // 找到最后一个音节的起始位置（通常是空格后的部分，或者是整个 buffer）
                let last_part_start = new_buffer.rfind(' ').map(|i| i + 1).unwrap_or(0);
                let last_part = &new_buffer[last_part_start..];
                
                let transformed = if last_part.starts_with("zh") {
                    last_part.replacen("zh", "z", 1)
                } else if last_part.starts_with("ch") {
                    last_part.replacen("ch", "c", 1)
                } else if last_part.starts_with("sh") {
                    last_part.replacen("sh", "s", 1)
                } else if last_part.starts_with("z") {
                    last_part.replacen("z", "zh", 1)
                } else if last_part.starts_with("c") {
                    last_part.replacen("c", "ch", 1)
                } else if last_part.starts_with("s") {
                    last_part.replacen("s", "sh", 1)
                } else {
                    last_part.to_string()
                };

                if transformed != last_part {
                    new_buffer.replace_range(last_part_start.., &transformed);
                    self.buffer = new_buffer;
                    if let Some(act) = self.lookup() { return act; }
                    return self.update_phantom_action();
                }
                Action::PassThrough
            }

            _ if is_digit(key) => {
                let digit = key_to_digit(key).unwrap_or(0);
                if self.enable_number_selection && self.commit_mode == "single" && digit >= 1 && digit <= self.page_size {
                    let abs_idx = self.page + digit - 1;
                    if let Some(word) = self.candidates.get(abs_idx) { return self.commit_candidate(word.clone()); }
                }
                let old_buffer = self.buffer.clone(); self.buffer.push_str(&digit.to_string()); if let Some(act) = self.lookup() { return act; }
                if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; let _ = self.lookup(); return Action::Alert; }
                if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
            }
            _ => {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    let old_buffer = self.buffer.clone(); self.buffer.push(c); self.preview_selected_candidate = false; if let Some(act) = self.lookup() { return act; }
                    if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; let _ = self.lookup(); return Action::Alert; }
                    if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
                } else if get_punctuation_key(key, shift_pressed).is_some() {
                    self.handle_punctuation(key, shift_pressed)
                } else { Action::PassThrough }
            }
        }
    }

    fn handle_punctuation(&mut self, key: VirtualKey, shift_pressed: bool) -> Action {
        let punc_key = get_punctuation_key(key, shift_pressed).unwrap();
        let zh_punc = self.punctuation.get(punc_key).and_then(|v| v.first()).cloned().unwrap_or_else(|| punc_key.to_string());
        let mut commit_text = if !self.joined_sentence.is_empty() { self.joined_sentence.clone() } else if !self.candidates.is_empty() { self.candidates[0].clone() } else { self.buffer.clone() };
        commit_text.push_str(&zh_punc);
        let del_len = self.phantom_text.chars().count();
        self.reset();
        self.commit_history.clear(); // 标点断句，清空历史
        Action::DeleteAndEmit { delete: del_len, insert: commit_text }
    }

    fn commit_candidate(&mut self, mut cand: String) -> Action {
        let now = Instant::now();
        let py = self.last_lookup_pinyin.clone();

        if self.enable_user_dict && !py.is_empty() {
            // 1. 记录单个词的频率
            self.record_usage(&py, &cand);

            // 2. 尝试与历史记录合并组词
            // 如果距离上次上屏超过 3 秒，清空历史
            if now.duration_since(self.last_commit_time) > Duration::from_secs(3) {
                self.commit_history.clear();
            }

            // 将当前词加入历史
            self.commit_history.push((py.clone(), cand.clone()));

            // 尝试组合（取最近 2 到 4 个部分进行组合）
            if self.commit_history.len() >= 2 {
                let start = if self.commit_history.len() > 4 { self.commit_history.len() - 4 } else { 0 };
                let mut new_combinations = Vec::new();
                
                {
                    let history_slice = &self.commit_history[start..];
                    for i in 0..(history_slice.len() - 1) {
                        let mut combined_py = String::new();
                        let mut combined_word = String::new();
                        for j in i..history_slice.len() {
                            combined_py.push_str(&history_slice[j].0);
                            combined_word.push_str(&history_slice[j].1);
                        }
                        if combined_word.chars().count() <= 8 {
                            new_combinations.push((combined_py, combined_word));
                        }
                    }
                }

                // 统一写入词库
                for (py, word) in new_combinations {
                    self.record_usage(&py, &word);
                }
            }

            self.last_commit_time = now;
        }

        if self.active_profiles.len() == 1 && self.active_profiles[0] == "english" && !cand.is_empty() && cand.chars().last().unwrap_or(' ').is_alphanumeric() { cand.push(' '); }
        let del = self.phantom_text.chars().count(); self.reset(); Action::DeleteAndEmit { delete: del, insert: cand }
    }

    fn update_phantom_action(&mut self) -> Action {
        if self.phantom_mode == PhantomMode::None { return Action::Consume; }
        let target = if self.preview_selected_candidate && !self.candidates.is_empty() { self.candidates[self.selected.min(self.candidates.len()-1)].clone() } else { self.buffer.clone() };
        if target == self.phantom_text { return Action::Consume; }
        let old_phantom = self.phantom_text.clone(); self.phantom_text = target.clone();
        let old_chars: Vec<char> = old_phantom.chars().collect(); let target_chars: Vec<char> = target.chars().collect();
        if target.starts_with(&old_phantom) { let added: String = target_chars[old_chars.len()..].iter().collect(); return Action::Emit(added); }
        if old_phantom.starts_with(&target) { let count = old_chars.len() - target_chars.len(); return Action::DeleteAndEmit { delete: count, insert: "".into() }; }
        Action::DeleteAndEmit { delete: old_chars.len(), insert: target }
    }

    pub fn lookup(&mut self) -> Option<Action> {
        if self.buffer.is_empty() { self.reset(); return None; }

        if self.filter_mode == FilterMode::Page && !self.page_snapshot.is_empty() {
            let filter_lower = self.aux_filter.to_lowercase();
            let mut filtered_cands = Vec::new();
            let mut filtered_hints = Vec::new();
            for (cand, hint) in &self.page_snapshot {
                let hint_lower = hint.to_lowercase();
                let parts: Vec<&str> = hint_lower.split_whitespace().collect();
                let mut matched = false;
                for part in parts { if part.starts_with(&filter_lower) { matched = true; break; } }
                if matched {
                    filtered_cands.push(cand.clone());
                    filtered_hints.push(hint.clone());
                }
            }
            if !filtered_cands.is_empty() {
                self.candidates = filtered_cands;
                self.candidate_hints = filtered_hints;
                if self.candidates.len() == 1 {
                    let word = self.candidates[0].clone();
                    return Some(self.commit_candidate(word));
                }
            } else {
                self.candidates.clear();
                self.candidate_hints.clear();
            }
            self.selected = 0;
            self.update_state();
            return None;
        }

        let parsed_parts = self.parse_buffer();
        self.last_lookup_pinyin = parsed_parts.iter().map(|p| p.pinyin.clone()).collect::<Vec<_>>().join("");

        let mut greedy_sentence = String::new(); let mut all_raw_segments = Vec::new(); let mut last_matches: Vec<(String, String, String, u32, bool)> = Vec::new();
        let has_trailing_space = self.buffer.ends_with(' ');

        for (i, part) in parsed_parts.iter().enumerate() {
            all_raw_segments.push(part.raw.clone());
            let mut combined_matches = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for profile in &self.active_profiles {
                if let Some(d) = self.tries.get(profile) {
                    if let Some(matches) = d.get_all_exact(&part.pinyin) {
                        for (w, t, e, weight) in matches {
                            if seen.insert(w.clone()) { combined_matches.push((w, t, e, weight, true)); }
                        }
                    }
                    if self.enable_prefix_matching && !part.pinyin.is_empty() {
                        let limit = if part.aux_code.is_some() { 50 } else { 20 };
                        let prefix_matches = d.search_bfs(&part.pinyin, limit);
                        for (w, t, e, weight) in prefix_matches {
                            if seen.insert(w.clone()) { combined_matches.push((w, t, e, weight, false)); }
                        }
                    }
                }
            }
            combined_matches.sort_by(|a, b| {
                b.4.cmp(&a.4).then_with(|| a.0.chars().count().cmp(&b.0.chars().count())).then_with(|| b.3.cmp(&a.3))
            });
            let idx = part.specified_idx.unwrap_or(1).saturating_sub(1);
            if let Some((w, _, _, _, _)) = combined_matches.get(idx) { greedy_sentence.push_str(w); } else { greedy_sentence.push_str(&part.raw); }
            if i == parsed_parts.len() - 1 { last_matches = combined_matches; }
        }

        if has_trailing_space && !greedy_sentence.is_empty() { greedy_sentence.push(' '); }
        self.joined_sentence = greedy_sentence;
        self.best_segmentation = all_raw_segments;
        self.candidates.clear(); self.candidate_hints.clear(); self.has_dict_match = !last_matches.is_empty();

        for (cand, tone, en, _, _) in last_matches {
            self.candidates.push(cand);
            let mut h = String::new();
            if self.show_tone_hint && !tone.is_empty() { h.push_str(&tone); }
            if self.show_en_hint && !en.is_empty() { if !h.is_empty() { h.push(' '); } h.push_str(&en); }
            self.candidate_hints.push(h);
        }

        // 根据用户词库重排
        if self.enable_user_dict && !self.last_lookup_pinyin.is_empty() {
            if let Some(user_entries) = self.user_dict.get(&self.last_lookup_pinyin) {
                let insert_pos = if self.enable_fixed_first_candidate && !self.candidates.is_empty() { 1 } else { 0 };
                
                for (word, _count) in user_entries.iter().rev() {
                    if let Some(pos) = self.candidates.iter().position(|c| c == word) {
                        // 如果固定首词开启且当前词就是第一名，跳过重排（保持在第一）
                        if insert_pos == 1 && pos == 0 { continue; }
                        
                        let c = self.candidates.remove(pos);
                        let h = self.candidate_hints.remove(pos);
                        self.candidates.insert(insert_pos, c);
                        self.candidate_hints.insert(insert_pos, h);
                    } else {
                        // 如果系统词库没有，但是用户词库有（新词），也加入候选
                        self.candidates.insert(insert_pos, word.clone());
                        self.candidate_hints.insert(insert_pos, "✨ 用户词".to_string());
                    }
                }
            }
        }

        if self.filter_mode == FilterMode::Global && !self.aux_filter.is_empty() {
            let filter_lower = self.aux_filter.to_lowercase();
            let mut filtered_candidates = Vec::new();
            let mut filtered_hints = Vec::new();
            for (i, hint) in self.candidate_hints.iter().enumerate() {
                let hint_lower = hint.to_lowercase();
                let parts: Vec<&str> = hint_lower.split_whitespace().collect();
                let mut matched = false;
                for part in parts { if part.starts_with(&filter_lower) { matched = true; break; } }
                if matched {
                    filtered_candidates.push(self.candidates[i].clone());
                    filtered_hints.push(hint.clone());
                }
            }
            if !filtered_candidates.is_empty() {
                self.candidates = filtered_candidates;
                self.candidate_hints = filtered_hints;
                if self.candidates.len() == 1 {
                    let word = self.candidates[0].clone();
                    return Some(self.commit_candidate(word));
                }
            }
        }

        if self.candidates.is_empty() { self.candidates.push(self.buffer.clone()); self.candidate_hints.push(String::new()); }
        self.selected = 0; self.page = 0; self.update_state();
        None
    }

    pub fn get_current_profile_display(&self) -> String {
        if self.active_profiles.is_empty() { return "None".to_string(); }
        if self.active_profiles.len() == 1 { return self.active_profiles[0].clone(); }
        "Mixed".to_string()
    }

    fn update_state(&mut self) {
        if self.buffer.is_empty() { self.state = if self.candidates.is_empty() { ImeState::Direct } else { ImeState::Multi }; }
        else { self.state = match self.candidates.len() { 0 => ImeState::NoMatch, 1 => ImeState::Single, _ => ImeState::Multi }; }
    }

    pub fn next_profile(&mut self) -> String {
        let mut all: Vec<String> = self.tries.keys().cloned().collect();
        if all.is_empty() { return String::new(); }
        all.sort();
        if self.active_profiles.len() > 1 {
            let next = all[0].clone();
            self.active_profiles = vec![next.clone()];
            self.reset();
            return next;
        }
        let current = self.active_profiles.get(0).cloned().unwrap_or_default();
        let idx = all.iter().position(|p| p == &current).unwrap_or(0);
        if idx + 1 < all.len() {
            let next = all[idx + 1].clone();
            self.active_profiles = vec![next.clone()];
            self.reset();
            next
        } else {
            self.active_profiles = all.clone();
            self.reset();
            "Mixed (All)".to_string()
        }
    }

    fn check_auto_commit(&mut self) -> Option<Action> {
        if !self.auto_commit_unique_full_match || self.candidates.len() != 1 || !self.has_dict_match || self.state == ImeState::NoMatch { return None; }
        let raw_input = &self.buffer;
        let mut total_longer = 0;
        for p in &self.active_profiles {
            if let Some(d) = self.tries.get(p) { if d.has_longer_match(raw_input) { total_longer += 1; break; } }
        }
        if total_longer == 0 { return Some(self.commit_candidate(self.candidates[0].clone())); }
        None
    }

    pub fn start_global_filter(&mut self) {
        if self.state == ImeState::Direct { return; }
        self.filter_mode = FilterMode::Global;
        self.aux_filter.clear();
    }

    pub fn start_page_filter(&mut self) {
        if self.state == ImeState::Direct || self.candidates.is_empty() { return; }
        let start = self.page;
        let end = (start + self.page_size).min(self.candidates.len());
        self.page_snapshot.clear();
        for i in start..end { self.page_snapshot.push((self.candidates[i].clone(), self.candidate_hints[i].clone())); }
        self.filter_mode = FilterMode::Page;
        self.aux_filter.clear();
    }

    fn load_user_dict(&mut self) {
        let path = std::path::Path::new("data/user_dict.json");
        if path.exists() {
            if let Ok(file) = std::fs::File::open(path) {
                if let Ok(dict) = serde_json::from_reader(std::io::BufReader::new(file)) {
                    self.user_dict = dict;
                }
            }
        }
    }

    fn save_user_dict(&self) {
        let path = std::path::Path::new("data/user_dict.json");
        if let Ok(file) = std::fs::File::create(path) {
            let _ = serde_json::to_writer_pretty(std::io::BufWriter::new(file), &self.user_dict);
        }
    }

    fn record_usage(&mut self, pinyin: &str, word: &str) {
        if !self.enable_user_dict || pinyin.is_empty() || word.is_empty() { return; }
        let entries = self.user_dict.entry(pinyin.to_string()).or_insert(Vec::new());
        if let Some(pos) = entries.iter().position(|(w, _)| w == word) {
            entries[pos].1 += 1;
        } else {
            entries.push((word.to_string(), 1));
        }
        // 简单的排序：按频率降序
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        self.save_user_dict();
    }
}

pub fn is_letter(key: VirtualKey) -> bool {
    matches!(key,
        VirtualKey::Q | VirtualKey::W | VirtualKey::E | VirtualKey::R | VirtualKey::T | VirtualKey::Y | VirtualKey::U | VirtualKey::I | VirtualKey::O | VirtualKey::P |
        VirtualKey::A | VirtualKey::S | VirtualKey::D | VirtualKey::F | VirtualKey::G | VirtualKey::H | VirtualKey::J | VirtualKey::K | VirtualKey::L |
        VirtualKey::Z | VirtualKey::X | VirtualKey::C | VirtualKey::V | VirtualKey::B | VirtualKey::N | VirtualKey::M |
        VirtualKey::Apostrophe
    )
}
pub fn is_digit(key: VirtualKey) -> bool { matches!(key, VirtualKey::Digit1 | VirtualKey::Digit2 | VirtualKey::Digit3 | VirtualKey::Digit4 | VirtualKey::Digit5 | VirtualKey::Digit6 | VirtualKey::Digit7 | VirtualKey::Digit8 | VirtualKey::Digit9 | VirtualKey::Digit0) }
pub fn key_to_digit(key: VirtualKey) -> Option<usize> { match key { VirtualKey::Digit1 => Some(1), VirtualKey::Digit2 => Some(2), VirtualKey::Digit3 => Some(3), VirtualKey::Digit4 => Some(4), VirtualKey::Digit5 => Some(5), VirtualKey::Digit6 => Some(6), VirtualKey::Digit7 => Some(7), VirtualKey::Digit8 => Some(8), VirtualKey::Digit9 => Some(9), VirtualKey::Digit0 => Some(0), _ => None } }
pub fn key_to_char(key: VirtualKey, shift: bool) -> Option<char> {
    let c = match key {
        VirtualKey::Q => Some('q'), VirtualKey::W => Some('w'), VirtualKey::E => Some('e'), VirtualKey::R => Some('r'), VirtualKey::T => Some('t'), VirtualKey::Y => Some('y'), VirtualKey::U => Some('u'), VirtualKey::I => Some('i'), VirtualKey::O => Some('o'), VirtualKey::P => Some('p'), VirtualKey::A => Some('a'), VirtualKey::S => Some('s'), VirtualKey::D => Some('d'), VirtualKey::F => Some('f'), VirtualKey::G => Some('g'), VirtualKey::H => Some('h'), VirtualKey::J => Some('j'), VirtualKey::K => Some('k'), VirtualKey::L => Some('l'), VirtualKey::Z => Some('z'), VirtualKey::X => Some('x'), VirtualKey::C => Some('c'), VirtualKey::V => Some('v'), VirtualKey::B => Some('b'), VirtualKey::N => Some('n'), VirtualKey::M => Some('m'), VirtualKey::Apostrophe => Some('\''), VirtualKey::Slash => Some('/'),
        VirtualKey::Minus => Some('-'), VirtualKey::Equal => Some('='), VirtualKey::Comma => Some(','), VirtualKey::Dot => Some('.'),
        _ => None
    };
    if shift { c.map(|ch| ch.to_ascii_uppercase()) } else { c }
}
fn get_punctuation_key(key: VirtualKey, shift: bool) -> Option<&'static str> {
    match (key, shift) { (VirtualKey::Grave, false) => Some("`"), (VirtualKey::Grave, true) => Some("~"), (VirtualKey::Minus, false) => Some("-"), (VirtualKey::Minus, true) => Some("_"), (VirtualKey::Equal, false) => Some("="), (VirtualKey::Equal, true) => Some("+"), (VirtualKey::LeftBrace, false) => Some("["), (VirtualKey::LeftBrace, true) => Some("{"), (VirtualKey::RightBrace, false) => Some("]"), (VirtualKey::RightBrace, true) => Some("}"), (VirtualKey::Backslash, false) => Some("\\"), (VirtualKey::Backslash, true) => Some("|"), (VirtualKey::Semicolon, false) => Some(";"), (VirtualKey::Semicolon, true) => Some(":"), (VirtualKey::Apostrophe, false) => Some("'"), (VirtualKey::Apostrophe, true) => Some("\""), (VirtualKey::Comma, false) => Some(","), (VirtualKey::Comma, true) => Some("<"), (VirtualKey::Dot, false) => Some("."), (VirtualKey::Dot, true) => Some(">"), (VirtualKey::Slash, false) => Some("/"), (VirtualKey::Slash, true) => Some("?"), (VirtualKey::Digit1, true) => Some("!"), (VirtualKey::Digit2, true) => Some("@"), (VirtualKey::Digit3, true) => Some("#"), (VirtualKey::Digit4, true) => Some("$"), (VirtualKey::Digit5, true) => Some("%"), (VirtualKey::Digit6, true) => Some("^"), (VirtualKey::Digit7, true) => Some("&"), (VirtualKey::Digit8, true) => Some("*"), (VirtualKey::Digit9, true) => Some("("), (VirtualKey::Digit0, true) => Some(")"), _ => None }
}
pub fn strip_tones(s: &str) -> String {
    let mut res = String::new();
    for c in s.chars() { match c { 'ā'|'á'|'ǎ'|'à' => res.push('a'), 'ē'|'é'|'ě'|'è' => res.push('e'), 'ī'|'í'|'ǐ'|'ì' => res.push('i'), 'ō'|'ó'|'ǒ'|'ò' => res.push('o'), 'ū'|'ú'|'ǔ'|'ù' => res.push('u'), 'ǖ'|'ǘ'|'ǚ'|'ǜ' => res.push('v'), 'Ā'|'Á'|'Ǎ'|'À' => res.push('A'), 'Ē'|'É'|'Ě'|'È' => res.push('E'), 'Ī'|'Í'|'Ǐ'|'Ì' => res.push('I'), 'Ō'|'Ó'|'Ǒ'|'Ò' => res.push('O'), 'Ū'|'Ú'|'Ǔ'|'Ù' => res.push('U'), 'Ǖ'|'Ǘ'|'Ǚ'|'Ǜ' => res.push('V'), _ => res.push(c) } } 
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup_mock_processor() -> Processor {
        let mut tries = HashMap::new();
        Processor {
            state: ImeState::Direct, buffer: String::new(), tries, active_profiles: vec!["chinese".to_string()], punctuation: HashMap::new(),
            candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, chinese_enabled: true, best_segmentation: vec![], joined_sentence: String::new(),
            show_candidates: true, show_modern_candidates: false, show_notifications: true, show_keystrokes: true, phantom_mode: PhantomMode::Pinyin, phantom_text: String::new(),
            preview_selected_candidate: false, enable_anti_typo: true, commit_mode: "double".to_string(), switch_mode: false, cursor_pos: 0,
            aux_filter: String::new(), filter_mode: FilterMode::None, page_snapshot: Vec::new(),
            enable_english_filter: true, enable_caps_selection: true, enable_number_selection: true,
            enable_double_tap: true, double_tap_timeout: Duration::from_millis(250), double_taps: HashMap::new(), last_tap_key: None, last_tap_time: None,
                        enable_long_press: true, long_press_timeout: Duration::from_millis(400), long_press_mappings: HashMap::new(), key_press_info: None, long_press_triggered: false,
                        nav_mode: false, enable_user_dict: true, enable_fixed_first_candidate: false, user_dict: HashMap::new(), last_lookup_pinyin: String::new(),
                        commit_history: Vec::new(), last_commit_time: Instant::now(),
            
                        profile_keys: Vec::new(),
             auto_commit_unique_en_fuzhuma: false, auto_commit_unique_full_match: false, enable_prefix_matching: true, prefix_matching_limit: 20, enable_abbreviation_matching: true, filter_proper_nouns_by_case: true, enable_error_sound: true, has_dict_match: false, page_size: 5, show_tone_hint: false, show_en_hint: true, page_flipping_styles: vec!["arrow".to_string()], swap_arrow_keys: false,
        }
    }
    #[test] fn test_dummy() { let _p = setup_mock_processor(); }
}
