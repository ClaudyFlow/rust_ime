use std::collections::HashMap;
use crate::engine::trie::Trie;
use crate::engine::keys::VirtualKey;
use crate::engine::scheme::{InputScheme, SchemeCandidate, SchemeContext};
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

use crate::config::{AuxMode, PunctuationEntry};

pub struct Processor {
    pub state: ImeState,
    pub buffer: String,
    pub tries: HashMap<String, Trie>,
    pub active_profiles: Vec<String>,
    pub punctuations: HashMap<String, HashMap<String, Vec<PunctuationEntry>>>, // Language -> Key -> Entries
    pub keyboard_layouts: HashMap<String, HashMap<String, String>>, // Layout Name -> Key -> Char
    pub syllables: std::collections::HashSet<String>,
    pub candidates: Vec<String>,
    pub candidate_hints: Vec<String>, 
    pub selected: usize,
    pub page: usize,
    pub chinese_enabled: bool,
    pub best_segmentation: Vec<String>,
    pub joined_sentence: String,
    
    pub show_candidates: bool,
    pub show_english_translation: bool,
    pub show_stroke_aux: bool,
    pub phantom_mode: PhantomMode,
    pub phantom_text: String,
    pub preview_selected_candidate: bool,
    pub anti_typo_mode: crate::config::AntiTypoMode,
    pub last_blocked_buffer: String,
    pub commit_mode: String,
    pub switch_mode: bool,
    pub cursor_pos: usize,
    pub profile_keys: Vec<(String, String)>,
    pub page_size: usize,
    pub show_tone_hint: bool,
    pub aux_mode: AuxMode,
    pub auto_commit_unique_en_fuzhuma: bool,    pub auto_commit_unique_full_match: bool,
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
    pub enable_punctuation_long_press: bool,
    pub punctuation_long_press_mappings: HashMap<String, String>,
    pub key_press_info: Option<(VirtualKey, Instant)>,
    pub long_press_triggered: bool,

    pub nav_mode: bool,

    // 用户个人词库相关
    pub enable_user_dict: bool,
    pub enable_fixed_first_candidate: bool,
    pub enable_smart_backspace: bool,
    pub enable_double_pinyin: bool,
    pub double_pinyin_scheme: crate::config::DoublePinyinScheme,
    pub enable_fuzzy_pinyin: bool,
    pub fuzzy_config: crate::config::FuzzyPinyinConfig,
    pub enable_traditional: bool,
    pub user_dict: HashMap<String, HashMap<String, Vec<(String, u32)>>>, // 方案 -> 拼音 -> Vec<(词组, 词频)>
    pub last_lookup_pinyin: String, // 记录最近一次检索的拼音串
    
    // 连续选词记忆
    pub commit_history: Vec<(String, String)>, // 最近上屏的 (拼音, 词组)
    pub last_commit_time: Instant,
    pub user_dict_tx: Option<std::sync::mpsc::Sender<HashMap<String, HashMap<String, Vec<(String, u32)>>>>>,

    // 标点状态相关
    pub quote_open: bool,
    pub single_quote_open: bool,

    // 方案注册表
    pub schemes: HashMap<String, Box<dyn InputScheme>>,
}

fn get_stroke_desc(code: &str) -> String {
    code.to_string()
}

impl Processor {
    pub fn new(
        tries: HashMap<String, Trie>, 
        initial_profile: String, 
        punctuations: HashMap<String, HashMap<String, Vec<PunctuationEntry>>>, 
    ) -> Self {
        let phantom_mode = if cfg!(target_os = "windows") { PhantomMode::None } else { PhantomMode::Pinyin };

        Self {
            state: ImeState::Direct, buffer: String::new(), tries, 
            active_profiles: vec![initial_profile],
            punctuations,
            keyboard_layouts: HashMap::new(),
            syllables: std::collections::HashSet::new(),
            candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, 
            chinese_enabled: true, best_segmentation: vec![],
            joined_sentence: String::new(),
            show_candidates: true,
            show_english_translation: true,
            show_stroke_aux: true,

            phantom_mode,
            phantom_text: String::new(),
            preview_selected_candidate: false,
            anti_typo_mode: crate::config::AntiTypoMode::None,
            last_blocked_buffer: String::new(),
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
            aux_mode: AuxMode::English,
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
            enable_punctuation_long_press: true,
            punctuation_long_press_mappings: HashMap::new(),
            key_press_info: None,
            long_press_triggered: false,
            nav_mode: false,
            enable_user_dict: true,
            enable_fixed_first_candidate: false,
            enable_smart_backspace: true,
            enable_double_pinyin: false,
            double_pinyin_scheme: crate::config::DoublePinyinScheme {
                name: "小鹤双拼".into(),
                initials: [("v","zh"), ("u","sh"), ("i","ch")].iter().map(|(k,v)| (k.to_string(), v.to_string())).collect(),
                rimes: [("q","iu"), ("w","ei"), ("r","uan")].iter().map(|(k,v)| (k.to_string(), v.to_string())).collect(), 
            },
            enable_fuzzy_pinyin: false,
            fuzzy_config: crate::config::FuzzyPinyinConfig {
                z_zh: true, c_ch: true, s_sh: true, n_l: false, r_l: false, f_h: false,
                an_ang: false, en_eng: false, in_ing: false, ian_iang: false, uan_uang: false, u_v: false,
                custom_mappings: vec![],
            },
            enable_traditional: false,
            user_dict: HashMap::new(),
            last_lookup_pinyin: String::new(),
            commit_history: Vec::new(),
            last_commit_time: Instant::now(),
            user_dict_tx: None,
            quote_open: false,
            single_quote_open: false,
            schemes: {
                let mut m: HashMap<String, Box<dyn InputScheme>> = HashMap::new();
                m.insert("stroke".to_string(), Box::new(crate::engine::schemes::StrokeScheme::new()));
                m.insert("english".to_string(), Box::new(crate::engine::schemes::EnglishScheme::new()));
                m.insert("chinese".to_string(), Box::new(crate::engine::schemes::ChineseScheme::new()));
                m.insert("japanese".to_string(), Box::new(crate::engine::schemes::JapaneseScheme::new()));
                m
            },
        }
    }

    pub fn apply_config(&mut self, conf: &crate::config::Config) {
        self.enable_user_dict = conf.input.enable_user_dict;
        self.enable_fixed_first_candidate = conf.input.enable_fixed_first_candidate;
        self.enable_smart_backspace = conf.input.enable_smart_backspace;
        self.enable_double_pinyin = conf.input.enable_double_pinyin;
        self.double_pinyin_scheme = conf.input.double_pinyin_scheme.clone();
        self.enable_fuzzy_pinyin = conf.input.enable_fuzzy_pinyin;
        self.fuzzy_config = conf.input.fuzzy_config.clone();
        self.enable_traditional = conf.input.enable_traditional;
        // 如果是初次加载或切换，可以从文件读取
        if self.enable_user_dict && self.user_dict.is_empty() {
            self.load_user_dict();
        }
        self.show_candidates = conf.appearance.show_candidates;
        self.show_english_translation = conf.appearance.show_english_translation;
        self.show_stroke_aux = conf.appearance.show_stroke_aux;
        self.page_size = conf.appearance.page_size;
        self.show_tone_hint = conf.appearance.show_tone_hint;
        self.aux_mode = conf.appearance.aux_mode;
        
        // 确保随机高亮的状态虽然由 GUI 处理，但 Processor 也持有配置副本
        // 这里不需要显式逻辑，因为字段会在下一行同步 (如果 Processor 有对应字段的话)

        self.anti_typo_mode = conf.input.anti_typo_mode;
        self.commit_mode = conf.input.commit_mode.clone();
        self.auto_commit_unique_en_fuzhuma = conf.input.auto_commit_unique_en_fuzhuma;
        self.auto_commit_unique_full_match = conf.input.auto_commit_unique_full_match;
        self.enable_error_sound = conf.input.enable_error_sound;
        self.enable_prefix_matching = conf.input.enable_prefix_matching;
        self.prefix_matching_limit = conf.input.prefix_matching_limit;
        self.enable_abbreviation_matching = conf.input.enable_abbreviation_matching;
        self.filter_proper_nouns_by_case = conf.input.filter_proper_nouns_by_case;
        self.profile_keys = conf.input.profile_keys.iter().map(|pk: &crate::config::ProfileKey| (pk.key.to_lowercase(), pk.profile.to_lowercase())).collect();
        
        self.page_flipping_styles = conf.input.page_flipping_keys.iter().map(|s: &String| s.to_lowercase()).collect();
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

        self.enable_punctuation_long_press = conf.input.enable_punctuation_long_press;
        self.punctuation_long_press_mappings = conf.input.punctuation_long_press_mappings.clone();
        self.punctuations = conf.input.punctuations.clone();
        self.keyboard_layouts = conf.input.keyboard_layouts.clone();

        if !conf.input.active_profiles.is_empty() {
            self.active_profiles = conf.input.active_profiles.iter().map(|p: &String| p.to_lowercase()).collect();
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

    pub fn set_syllables(&mut self, syllables: std::collections::HashSet<String>) {
        println!("[Processor] 加载音节表成功，条目数: {}", syllables.len());
        self.syllables = syllables;
    }

    pub fn get_short_display(&self) -> String {
        let display = self.get_current_profile_display();
        match display.to_lowercase().as_str() {
            "chinese" => "中".to_string(),
            "english" => "英".to_string(),
            "japanese" => "日".to_string(),
            "stroke" => "笔".to_string(),
            "mixed" => "混".to_string(),
            _ => {
                let mut chars = display.chars();
                chars.next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string())
            }
        }
    }

    pub fn toggle(&mut self) -> Action {
        self.chinese_enabled = !self.chinese_enabled;
        let enabled = self.chinese_enabled;
        let short = self.get_short_display();
        self.reset();
        
        if enabled {
            Action::Notify(short, "模式已开启".into())
        } else {
            Action::Notify("英".into(), "英文直通模式".into())
        }
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

    pub fn clear_composing(&mut self) {
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
        self.cursor_pos = 0;
        self.aux_filter.clear();
        self.filter_mode = FilterMode::None;
        self.page_snapshot.clear();
        self.nav_mode = false;
    }

    pub fn reset(&mut self) {
        self.clear_composing();
        self.switch_mode = false;
        self.quote_open = false;
        self.single_quote_open = false;
    }

    pub fn handle_key(&mut self, key: VirtualKey, val: i32, shift_pressed: bool, ctrl_pressed: bool, alt_pressed: bool) -> Action {
        let now = Instant::now();
        let is_press = val == 1;

        if !self.chinese_enabled {
            return Action::PassThrough;
        }
        let is_repeat = val == 2;
        let is_release = val == 0;

        // --- 新增：Ctrl + 标点符号 -> 强制输出英文标点 ---
        if is_press && ctrl_pressed && !alt_pressed {
            if let Some(p_key) = get_punctuation_key(key, shift_pressed) {
                // 如果当前有输入缓冲区，先上屏首位
                let mut commit_text = if !self.joined_sentence.is_empty() { 
                    self.joined_sentence.trim_end().to_string() 
                } else if !self.candidates.is_empty() { 
                    self.candidates[0].trim_end().to_string() 
                } else { 
                    self.buffer.trim_end().to_string() 
                };
                commit_text.push_str(p_key); // 英文标点直接用 p_key
                let del_len = self.phantom_text.chars().count();
                self.clear_composing();
                self.commit_history.clear(); 
                return Action::DeleteAndEmit { delete: del_len, insert: commit_text };
            }
        }

        // 处理长按逻辑
        if (self.enable_long_press && is_letter(key)) || (self.enable_punctuation_long_press && get_punctuation_key(key, shift_pressed).is_some()) {
            if !shift_pressed {
                if val == 1 {
                    self.key_press_info = Some((key, now));
                    self.long_press_triggered = false;
                } else if is_repeat {
                    if !self.long_press_triggered {
                        if let Some((press_key, press_time)) = self.key_press_info {
                            if press_key == key && now.duration_since(press_time) >= self.long_press_timeout {
                                if is_letter(key) {
                                    if let Some(c) = key_to_char(key, false) {
                                        if let Some(replacement) = self.long_press_mappings.get(&c.to_string()).cloned() {
                                            self.long_press_triggered = true;
                                            if !self.buffer.is_empty() && self.buffer.ends_with(c) {
                                                self.buffer.pop();
                                            }
                                            return self.inject_text(&replacement);
                                        }
                                    }
                                } else {
                                    // 标点长按
                                    if let Some(p_key) = get_punctuation_key(key, false) {
                                        if let Some(replacement) = self.punctuation_long_press_mappings.get(p_key).cloned() {
                                            self.long_press_triggered = true;
                                            // 标点通常是直接上屏，或者是接在当前候选词后
                                            // 我们复用 handle_punctuation 的一部分逻辑，但直接用 replacement
                                            let mut commit_text = if !self.joined_sentence.is_empty() { 
                                                self.joined_sentence.trim_end().to_string() 
                                            } else if !self.candidates.is_empty() { 
                                                self.candidates[0].trim_end().to_string() 
                                            } else { 
                                                self.buffer.trim_end().to_string() 
                                            };
                                            commit_text.push_str(&replacement);
                                            let del_len = self.phantom_text.chars().count();
                                            self.clear_composing();
                                            self.commit_history.clear(); 
                                            return Action::DeleteAndEmit { delete: del_len, insert: commit_text };
                                        }
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
        }

        if is_release {
            if key == VirtualKey::CapsLock { return Action::Consume; }
            if key == VirtualKey::Shift && !self.buffer.is_empty() {
                self.start_global_filter();
                return Action::Consume;
            }
            if self.buffer.is_empty() { return Action::PassThrough; }
            return Action::Consume;
        }

        if key == VirtualKey::CapsLock {
            if is_press {
                if self.buffer.is_empty() {
                    self.switch_mode = !self.switch_mode;
                    return if self.switch_mode { 
                        Action::Notify("快捷切换".into(), "已进入方案切换模式".into()) 
                    } else { 
                        Action::Notify("快捷切换".into(), "已退出".into()) 
                    };
                } else {
                    self.nav_mode = !self.nav_mode;
                    if self.nav_mode {
                        // 进入导航模式时，自动跳到下一页
                        if self.page + self.page_size < self.candidates.len() {
                            self.page += self.page_size;
                            self.selected = self.page;
                        }
                    }
                    return Action::Consume;
                }
            }
            return Action::Consume;
        }

        if key == VirtualKey::Grave {
            return Action::PassThrough;
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
                            let short_display = self.get_short_display();
                            let _ = self.lookup();
                            self.switch_mode = false;
                            return Action::Notify(short_display, format!("方案: {}", display));
                        } else {
                            // 切换失败提示
                            self.switch_mode = false;
                            return Action::Notify("❌".into(), format!("错误: 方案 [{}] 的词库未加载", p_str));
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
                // 检查当前方案是否有关联的键盘布局 (如俄语、希腊语)
                let lang = self.active_profiles.get(0).cloned().unwrap_or_default().to_lowercase();
                if let Some(layout) = self.keyboard_layouts.get(&lang) {
                    if let Some(mapped) = layout.get(&c.to_string()) {
                        return Action::Emit(mapped.clone());
                    }
                }

                self.buffer.push(c);
                self.state = ImeState::Composing;
                if let Some(act) = self.lookup() { return act; }
                if self.should_block_invalid_input(&self.buffer.clone()) { return Action::Alert; }
                return self.update_phantom_action();
            }
        }

        if get_punctuation_key(key, shift_pressed).is_some() {
            return self.handle_punctuation(key, shift_pressed);
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

        // 方案级特殊按键拦截（如双拼映射、笔画数字键等）
        let current_profile = self.active_profiles.get(0).cloned().unwrap_or_default();
        if let Some(scheme) = self.schemes.get(&current_profile) {
            let context = SchemeContext {
                config: &crate::Config::load(),
                tries: &self.tries,
                syllables: &self.syllables,
                user_dict: &self.user_dict,
                active_profiles: &self.active_profiles,
                filter_mode: self.filter_mode.clone(),
                aux_filter: &self.aux_filter,
            };
            if let Some(act) = scheme.handle_special_key(key, &mut self.buffer, &context) {
                if act == Action::Consume {
                    if let Some(lookup_act) = self.lookup() { return lookup_act; }
                    return self.update_phantom_action();
                }
                return act;
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

        if key == VirtualKey::Semicolon && !shift_pressed {
            self.buffer.push(';');
            if let Some(act) = self.lookup() { return act; }
            return self.update_phantom_action();
        }

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

                // 统一退格逻辑：逐字符删除。复杂的智能退格后续迁移至 Scheme。
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
                            return self.commit_candidate(hint.clone(), 99);
                        }
                    }
                }
                if self.preview_selected_candidate || self.commit_mode == "single" { if let Some(word) = self.candidates.get(self.selected) { let idx = self.selected; return self.commit_candidate(word.clone(), idx); } }
                if self.buffer.ends_with(' ') && !self.joined_sentence.is_empty() { return self.commit_candidate(self.joined_sentence.clone(), 99); }
                self.buffer.push(' '); self.preview_selected_candidate = false; if let Some(act) = self.lookup() { return act; } self.update_phantom_action()
            }
            VirtualKey::Enter => {
                self.commit_history.clear(); // 强制上屏原始拼音，中断组词历史
                self.last_lookup_pinyin.clear(); // 清空检索记录，确保不触发学习
                if self.commit_mode == "single" { let out = self.buffer.clone(); return self.commit_candidate(out, 99); }
                if self.preview_selected_candidate { if let Some(word) = self.candidates.get(self.selected) { let idx = self.selected; return self.commit_candidate(word.clone(), idx); } }
                if !self.joined_sentence.is_empty() { self.commit_candidate(self.joined_sentence.clone(), 99) } else { let out = self.buffer.clone(); self.commit_candidate(out, 99) }
            }
            VirtualKey::Esc | VirtualKey::Delete => { 
                self.commit_history.clear(); // 取消输入，清空历史
                let del = self.phantom_text.chars().count(); self.reset(); if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume } 
            }

            VirtualKey::Apostrophe if !shift_pressed => {
                self.buffer.push('\'');
                self.preview_selected_candidate = false;
                if let Some(act) = self.lookup() { return act; }
                self.update_phantom_action()
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
                    if let Some(word) = self.candidates.get(abs_idx) { return self.commit_candidate(word.clone(), abs_idx); }
                }
                let old_buffer = self.buffer.clone(); self.buffer.push_str(&digit.to_string()); if let Some(act) = self.lookup() { return act; }
                if self.should_block_invalid_input(&old_buffer) { return Action::Alert; }
                if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
            }
            _ => {
                if get_punctuation_key(key, shift_pressed).is_some() {
                    self.handle_punctuation(key, shift_pressed)
                } else if let Some(c) = key_to_char(key, shift_pressed) {
                    let old_buffer = self.buffer.clone();
                    self.buffer.push(c);
                    self.preview_selected_candidate = false; if let Some(act) = self.lookup() { return act; }
                    if self.should_block_invalid_input(&old_buffer) { return Action::Alert; }
                    if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
                } else { Action::PassThrough }
            }
        }
    }

    fn handle_punctuation(&mut self, key: VirtualKey, shift_pressed: bool) -> Action {
        let punc_key_owned = get_punctuation_key(key, shift_pressed)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                log::warn!("无法获取标点符号键: key={:?}, shift={}", key, shift_pressed);
                format!("{:?}", key)
            });
        let punc_key = punc_key_owned.as_str();
        
        // 查找当前语言方案的标点映射
        let lang = self.active_profiles.get(0).cloned().unwrap_or_else(|| "chinese".to_string());
        
        // 日语特有标点映射逻辑
        let zh_punc = if lang == "japanese" {
            match (punc_key, shift_pressed) {
                (".", false) => "。".to_string(),
                (",", false) => "、".to_string(),
                ("?", _) => "？".to_string(),
                ("!", _) => "！".to_string(),
                ("/", false) => "・".to_string(),
                ("[", false) => "「".to_string(),
                ("]", false) => "」".to_string(),
                ("-", false) => "ー".to_string(),
                ("-", true) => "＝".to_string(),
                _ => punc_key.to_string(),
            }
        } else {
            let zh_puncs = self.punctuations.get(&lang).and_then(|m| m.get(punc_key))
                .or_else(|| self.punctuations.get("chinese").and_then(|m| m.get(punc_key))); // 回退到中文标点
            
            if let Some(entries) = zh_puncs {
                if punc_key == "\"" {
                    let p = if self.quote_open { entries.get(1).or(entries.get(0)) } else { entries.get(0) };
                    self.quote_open = !self.quote_open;
                    p.map(|e| e.char.clone()).unwrap_or_else(|| punc_key.to_string())
                } else if punc_key == "'" {
                    let p = if self.single_quote_open { entries.get(1).or(entries.get(0)) } else { entries.get(0) };
                    self.single_quote_open = !self.single_quote_open;
                    p.map(|e| e.char.clone()).unwrap_or_else(|| punc_key.to_string())
                } else {
                    entries.first().map(|e| e.char.clone()).unwrap_or_else(|| punc_key.to_string())
                }
            } else {
                punc_key.to_string()
            }
        };

        let mut commit_text = if !self.joined_sentence.is_empty() { 
            self.joined_sentence.trim_end().to_string() 
        } else if !self.candidates.is_empty() { 
            self.candidates[0].trim_end().to_string() 
        } else { 
            self.buffer.trim_end().to_string() 
        };
        commit_text.push_str(&zh_punc);
        let del_len = self.phantom_text.chars().count();
        self.clear_composing();
        self.commit_history.clear(); // 标点断句，清空历史
        Action::DeleteAndEmit { delete: del_len, insert: commit_text }
    }

    fn commit_candidate(&mut self, mut cand: String, _index: usize) -> Action {
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
        let del = self.phantom_text.chars().count(); self.clear_composing(); Action::DeleteAndEmit { delete: del, insert: cand }
    }

    fn update_phantom_action(&mut self) -> Action {
        if self.phantom_mode == PhantomMode::None { return Action::Consume; }
        
        // 快捷切换模式下的提示优先
        if self.switch_mode {
            let target = "[方案切换]".to_string();
            if target == self.phantom_text { return Action::Consume; }
            let old_phantom = self.phantom_text.clone();
            self.phantom_text = target.clone();
            return Action::DeleteAndEmit { delete: old_phantom.chars().count(), insert: target };
        }

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

        let current_profile = self.active_profiles.get(0).cloned().unwrap_or_default();
        
        // --- 方案化架构介入 ---
        if let Some(scheme) = self.schemes.get(&current_profile) {
            let context = SchemeContext {
                config: &crate::Config::load(), // 暂时每次创建，后续应优化为引用
                tries: &self.tries,
                syllables: &self.syllables,
                user_dict: &self.user_dict,
                active_profiles: &self.active_profiles,
                filter_mode: self.filter_mode.clone(),
                aux_filter: &self.aux_filter,
            };

            // 1. 预处理
            let query = scheme.pre_process(&self.buffer, &context);
            
            // 2. 检索
            let mut candidates = scheme.lookup(&query, &context);
            
            // 3. 后处理
            scheme.post_process(&query, &mut candidates, &context);

            // 4. 将结果同步到 Processor 状态
            self.candidates.clear();
            self.candidate_hints.clear();
            self.has_dict_match = !candidates.is_empty();
            self.last_lookup_pinyin = query.clone();

            for c in candidates {
                let text = if self.enable_traditional { c.traditional } else { c.simplified };
                self.candidates.push(text);
                
                let mut hint = String::new();
                if self.show_tone_hint && !c.tone.is_empty() { hint.push_str(&c.tone); }
                if !c.english.is_empty() {
                    if !hint.is_empty() { hint.push(' '); }
                    hint.push_str(&c.english);
                }
                if self.show_stroke_aux && !c.stroke_aux.is_empty() {
                    if !hint.is_empty() { hint.push(' '); }
                    hint.push_str(&c.stroke_aux);
                }
                self.candidate_hints.push(hint);
            }

            if self.candidates.is_empty() {
                self.candidates.push(self.buffer.clone());
                self.candidate_hints.push(String::new());
            }

            self.selected = 0; self.page = 0; self.update_state();
            return None;
        }
        // --- 方案化架构结束 ---

        // 2. 优先处理分页过滤模式
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
                    return Some(self.commit_candidate(word, 0));
                }
            } else {
                self.candidates.clear();
                self.candidate_hints.clear();
            }
            self.selected = 0;
            self.update_state();
            return None;
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
        if total_longer == 0 { return Some(self.commit_candidate(self.candidates[0].clone(), 0)); }
        None
    }

    /// 核心防呆逻辑：根据模式决定是否拦截非法拼音。
    /// 返回 true 表示应该拦截并报警。
    fn should_block_invalid_input(&mut self, old_buffer: &str) -> bool {
        if self.has_dict_match {
            self.last_blocked_buffer.clear();
            return false;
        }

        match self.anti_typo_mode {
            crate::config::AntiTypoMode::None => false,
            crate::config::AntiTypoMode::Strict => {
                self.buffer = old_buffer.to_string();
                let _ = self.lookup();
                true
            }
            crate::config::AntiTypoMode::Smart => {
                // 如果当前 buffer 与上次被拦截的一样，说明用户坚持输入，放行
                if !self.last_blocked_buffer.is_empty() && self.buffer == self.last_blocked_buffer {
                    self.last_blocked_buffer.clear();
                    false
                } else {
                    // 第一次拦截，记录状态
                    self.last_blocked_buffer = self.buffer.clone();
                    self.buffer = old_buffer.to_string();
                    let _ = self.lookup();
                    true
                }
            }
        }
    }

    pub fn start_global_filter(&mut self) {
        if self.state == ImeState::Direct { return; }
        self.filter_mode = FilterMode::Global;
        self.aux_filter.clear();
    }

    fn load_user_dict(&mut self) {
        println!("[Processor] Loading profile-aware user dictionary...");
        let path = std::path::Path::new("data/user_dict.json");
        if path.exists() {
            if let Ok(file) = std::fs::File::open(path) {
                if let Ok(dict) = serde_json::from_reader(std::io::BufReader::new(file)) {
                    self.user_dict = dict;
                    println!("[Processor] User dictionary loaded ({} profiles).", self.user_dict.len());
                }
            }
        } else {
            println!("[Processor] No user dictionary found.");
        }

        // 启动后台保存线程
        if self.user_dict_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<HashMap<String, HashMap<String, Vec<(String, u32)>>>>();
            self.user_dict_tx = Some(tx);
            std::thread::spawn(move || {
                let path = std::path::PathBuf::from("data/user_dict.json");
                while let Ok(dict_clone) = rx.recv() {
                    // 简单的去重/节流：如果队列里还有更多，先清空，只存最后一次
                    let mut latest = dict_clone;
                    while let Ok(next) = rx.try_recv() {
                        latest = next;
                    }
                    if let Ok(file) = std::fs::File::create(&path) {
                        let _ = serde_json::to_writer_pretty(std::io::BufWriter::new(file), &latest);
                    }
                }
            });
        }
    }

    fn save_user_dict(&self) {
        if let Some(ref tx) = self.user_dict_tx {
            let _ = tx.send(self.user_dict.clone());
        }
    }

    fn record_usage(&mut self, pinyin: &str, word: &str) {
        if !self.enable_user_dict || pinyin.is_empty() || word.is_empty() { return; }
        
        // 关键：获取当前活跃的第一个方案作为归属方案
        let profile = self.active_profiles.get(0).cloned().unwrap_or_else(|| "chinese".to_string());
        
        let profile_dict = self.user_dict.entry(profile).or_insert_with(HashMap::new);
        let entries = profile_dict.entry(pinyin.to_string()).or_insert_with(Vec::new);
        
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
        VirtualKey::Z | VirtualKey::X | VirtualKey::C | VirtualKey::V | VirtualKey::B | VirtualKey::N | VirtualKey::M
    )
}
pub fn is_digit(key: VirtualKey) -> bool { matches!(key, VirtualKey::Digit1 | VirtualKey::Digit2 | VirtualKey::Digit3 | VirtualKey::Digit4 | VirtualKey::Digit5 | VirtualKey::Digit6 | VirtualKey::Digit7 | VirtualKey::Digit8 | VirtualKey::Digit9 | VirtualKey::Digit0) }
pub fn key_to_digit(key: VirtualKey) -> Option<usize> { match key { VirtualKey::Digit1 => Some(1), VirtualKey::Digit2 => Some(2), VirtualKey::Digit3 => Some(3), VirtualKey::Digit4 => Some(4), VirtualKey::Digit5 => Some(5), VirtualKey::Digit6 => Some(6), VirtualKey::Digit7 => Some(7), VirtualKey::Digit8 => Some(8), VirtualKey::Digit9 => Some(9), VirtualKey::Digit0 => Some(0), _ => None } }
pub fn key_to_char(key: VirtualKey, shift: bool) -> Option<char> {
    let c = match key {
        VirtualKey::Q => Some('q'), VirtualKey::W => Some('w'), VirtualKey::E => Some('e'), VirtualKey::R => Some('r'), VirtualKey::T => Some('t'), VirtualKey::Y => Some('y'), VirtualKey::U => Some('u'), VirtualKey::I => Some('i'), VirtualKey::O => Some('o'), VirtualKey::P => Some('p'), VirtualKey::A => Some('a'), VirtualKey::S => Some('s'), VirtualKey::D => Some('d'), VirtualKey::F => Some('f'), VirtualKey::G => Some('g'), VirtualKey::H => Some('h'), VirtualKey::J => Some('j'), VirtualKey::K => Some('k'), VirtualKey::L => Some('l'), VirtualKey::Z => Some('z'), VirtualKey::X => Some('x'), VirtualKey::C => Some('c'), VirtualKey::V => Some('v'), VirtualKey::B => Some('b'), VirtualKey::N => Some('n'), VirtualKey::M => Some('m'), VirtualKey::Apostrophe => Some('\''), VirtualKey::Slash => Some('/'),
        VirtualKey::Minus => Some('-'), VirtualKey::Equal => Some('='), VirtualKey::Comma => Some(','), VirtualKey::Dot => Some('.'),
        VirtualKey::Digit1 => Some('1'), VirtualKey::Digit2 => Some('2'), VirtualKey::Digit3 => Some('3'), VirtualKey::Digit4 => Some('4'), VirtualKey::Digit5 => Some('5'), VirtualKey::Digit6 => Some('6'), VirtualKey::Digit7 => Some('7'), VirtualKey::Digit8 => Some('8'), VirtualKey::Digit9 => Some('9'), VirtualKey::Digit0 => Some('0'),
        VirtualKey::Grave => Some('`'), VirtualKey::LeftBrace => Some('['), VirtualKey::RightBrace => Some(']'), VirtualKey::Backslash => Some('\\'), VirtualKey::Semicolon => Some(';'),
        _ => None
    };
    if shift {
        match key {
            VirtualKey::Digit1 => Some('!'), VirtualKey::Digit2 => Some('@'), VirtualKey::Digit3 => Some('#'), VirtualKey::Digit4 => Some('$'), VirtualKey::Digit5 => Some('%'), VirtualKey::Digit6 => Some('^'), VirtualKey::Digit7 => Some('&'), VirtualKey::Digit8 => Some('*'), VirtualKey::Digit9 => Some('('), VirtualKey::Digit0 => Some(')'),
            VirtualKey::Minus => Some('_'), VirtualKey::Equal => Some('+'), VirtualKey::Comma => Some('<'), VirtualKey::Dot => Some('>'), VirtualKey::Slash => Some('?'),
            VirtualKey::Grave => Some('~'), VirtualKey::LeftBrace => Some('{'), VirtualKey::RightBrace => Some('}'), VirtualKey::Backslash => Some('|'), VirtualKey::Semicolon => Some(':'),
            VirtualKey::Apostrophe => Some('"'),
            _ => c.map(|ch| ch.to_ascii_uppercase())
        }
    } else { c }
}
fn get_punctuation_key(key: VirtualKey, shift: bool) -> Option<&'static str> {
    match (key, shift) { 
        (VirtualKey::Space, _) => Some(" "),
        (VirtualKey::Grave, false) => Some("`"), 
        (VirtualKey::Grave, true) => Some("~"),  (VirtualKey::Minus, false) => Some("-"), (VirtualKey::Minus, true) => Some("_"), (VirtualKey::Equal, false) => Some("="), (VirtualKey::Equal, true) => Some("+"), (VirtualKey::LeftBrace, false) => Some("["), (VirtualKey::LeftBrace, true) => Some("{"), (VirtualKey::RightBrace, false) => Some("]"), (VirtualKey::RightBrace, true) => Some("}"), (VirtualKey::Backslash, false) => Some("\\"), (VirtualKey::Backslash, true) => Some("|"), (VirtualKey::Semicolon, false) => Some(";"), (VirtualKey::Semicolon, true) => Some(":"), (VirtualKey::Apostrophe, false) => Some("'"), (VirtualKey::Apostrophe, true) => Some("\""), (VirtualKey::Comma, false) => Some(","), (VirtualKey::Comma, true) => Some("<"), (VirtualKey::Dot, false) => Some("."), (VirtualKey::Dot, true) => Some(">"), (VirtualKey::Slash, false) => Some("/"), (VirtualKey::Slash, true) => Some("?"), (VirtualKey::Digit1, true) => Some("!"), (VirtualKey::Digit2, true) => Some("@"), (VirtualKey::Digit3, true) => Some("#"), (VirtualKey::Digit4, true) => Some("$"), (VirtualKey::Digit5, true) => Some("%"), (VirtualKey::Digit6, true) => Some("^"), (VirtualKey::Digit7, true) => Some("&"), (VirtualKey::Digit8, true) => Some("*"), (VirtualKey::Digit9, true) => Some("("), (VirtualKey::Digit0, true) => Some(")"), _ => None }
}
pub fn strip_tones(s: &str) -> String {
    let mut res = String::new();
    for c in s.chars() { match c { 'ā'|'á'|'ǎ'|'à' => res.push('a'), 'ē'|'é'|'ě'|'è' => res.push('e'), 'ī'|'í'|'ǐ'|'ì' => res.push('i'), 'ō'|'ó'|'ǒ'|'ò' => res.push('o'), 'ū'|'ú'|'ǔ'|'ù' => res.push('u'), 'ǖ'|'ǘ'|'ǚ'|'ǜ' => res.push('v'), 'Ā'|'Á'|'Ǎ'|'À' => res.push('A'), 'Ē'|'É'|'Ě'|'È' => res.push('E'), 'Ī'|'Í'|'Ǐ'|'Ì' => res.push('I'), 'Ō'|'Ó'|'Ǒ'|'Ò' => res.push('O'), 'Ū'|'Ú'|'Ǔ'|'Ù' => res.push('U'), 'Ǖ'|'Ǘ'|'Ǚ'|'Ǜ' => res.push('V'), _ => res.push(c) } } 
    res
}


