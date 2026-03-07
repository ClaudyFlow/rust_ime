use std::collections::HashMap;
use std::sync::Arc;
use std::collections::HashSet;
use crate::engine::trie::Trie;
use crate::engine::keys::VirtualKey;
use crate::engine::scheme::{InputScheme, SchemeContext};
use crate::engine::pipeline::{Pipeline, DefaultSegmentor, TableTranslator};
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
pub enum FilterMode {
    None,
    Global, // Shift + 字母 (全局筛选)
    Page,   // Caps + 字母 (当前页筛选)
}

use crate::config::PunctuationEntry;

pub struct InputContext {
    pub buffer: String,
    pub candidates: Vec<crate::engine::pipeline::Candidate>,
    pub selected: usize,
    pub page: usize,
    pub cursor_pos: usize,
    pub joined_sentence: String,
    pub last_lookup_pinyin: String,
    pub state: ImeState,
    pub nav_mode: bool,
    pub switch_mode: bool,
    pub aux_filter: String,
    pub filter_mode: FilterMode,
    pub page_snapshot: Vec<crate::engine::pipeline::Candidate>,
    pub shift_used_as_modifier: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

pub struct Processor {
    pub ctx: InputContext,
    pub config: crate::engine::ConfigManager,
    pub key_map: HashMap<(VirtualKey, ModifierState), Command>,
    
    pub tries: HashMap<String, Trie>,
    pub active_profiles: Vec<String>,
    pub syllables: std::collections::HashSet<String>,
    
    pub chinese_enabled: bool,
    pub best_segmentation: Vec<String>,
    
    pub phantom_text: String,
    pub preview_selected_candidate: bool,
    pub last_blocked_buffer: String,
    pub has_dict_match: bool,
    
    // 双击相关
    pub last_tap_key: Option<VirtualKey>,
    pub last_tap_time: Option<Instant>,

    // 长按相关
    pub key_press_info: Option<(VirtualKey, Instant)>,
    pub long_press_triggered: bool,

    // 连续选词记忆
    pub commit_history: Vec<(String, String)>, // 最近上屏的 (拼音, 词组)
    pub last_commit_time: Instant,

    // 标点状态相关
    pub quote_open: bool,
    pub single_quote_open: bool,

    // 方案与流水线
    pub schemes: HashMap<String, Box<dyn InputScheme>>,
    pub pipelines: HashMap<String, Pipeline>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    NextPage,
    PrevPage,
    NextCandidate,
    PrevCandidate,
    Select(usize),
    Commit,
    Clear,
}

impl Processor {
    pub fn execute_command(&mut self, cmd: Command) -> Action {
        match cmd {
            Command::NextPage => {
                if !self.ctx.candidates.is_empty() {
                    if self.ctx.page + self.config.page_size < self.ctx.candidates.len() {
                        self.ctx.page += self.config.page_size;
                        self.ctx.selected = self.ctx.page;
                    } else {
                        self.trigger_incremental_search();
                        if self.ctx.page + self.config.page_size < self.ctx.candidates.len() {
                            self.ctx.page += self.config.page_size;
                            self.ctx.selected = self.ctx.page;
                        }
                    }
                }
                Action::Consume
            }
            Command::PrevPage => {
                self.ctx.page = self.ctx.page.saturating_sub(self.config.page_size);
                self.ctx.selected = self.ctx.page;
                Action::Consume
            }
            Command::NextCandidate => {
                if !self.ctx.candidates.is_empty() {
                    self.preview_selected_candidate = true;
                    if self.ctx.selected + 1 < self.ctx.candidates.len() {
                        self.ctx.selected += 1;
                    } else {
                        // 尝试增量搜索
                        self.trigger_incremental_search();
                        if self.ctx.selected + 1 < self.ctx.candidates.len() {
                            self.ctx.selected += 1;
                        }
                    }
                    self.ctx.page = (self.ctx.selected / self.config.page_size) * self.config.page_size;
                    return self.update_phantom_action();
                }
                Action::PassThrough
            }
            Command::PrevCandidate => {
                if !self.ctx.candidates.is_empty() {
                    self.preview_selected_candidate = true;
                    if self.ctx.selected > 0 {
                        self.ctx.selected -= 1;
                    }
                    self.ctx.page = (self.ctx.selected / self.config.page_size) * self.config.page_size;
                    return self.update_phantom_action();
                }
                Action::PassThrough
            }
            Command::Select(idx) => {
                let abs_idx = self.ctx.page + idx;
                if let Some(cand) = self.ctx.candidates.get(abs_idx) {
                    let word = cand.text.clone();
                    return self.commit_candidate(word, abs_idx);
                }
                Action::Consume
            }
            Command::Commit => {
                if self.ctx.buffer.is_empty() { return Action::PassThrough; }
                
                // 优先尝试提交当前选中的候选词
                if !self.ctx.candidates.is_empty() {
                    let idx = self.ctx.selected;
                    if let Some(cand) = self.ctx.candidates.get(idx) {
                        let word = cand.text.clone();
                        return self.commit_candidate(word, idx);
                    }
                }

                // 如果完全没有候选词，才提交原始 buffer (例如未知输入)
                let out = self.ctx.buffer.clone();
                self.commit_candidate(out, 99)
            }
            Command::Clear => {
                self.commit_history.clear();
                let del = self.phantom_text.chars().count();
                self.reset();
                if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
            }
        }
    }
    pub fn new(
        tries: HashMap<String, Trie>, 
        initial_profile: String, 
        _punctuations: HashMap<String, HashMap<String, Vec<PunctuationEntry>>>, 
        syllables: HashSet<String>,
    ) -> Self {
        let config = crate::engine::ConfigManager::new();
        let syllables_arc = Arc::new(syllables.clone());
        
        let mut pipelines = HashMap::new();
        for (name, trie) in &tries {
            let mut pipeline = Pipeline::new(Box::new(DefaultSegmentor));
            pipeline.add_translator(Box::new(crate::engine::pipeline::UserDictTranslator { 
                user_dict: config.user_dict.clone(), 
                profile: name.clone() 
            }));
            pipeline.add_translator(Box::new(TableTranslator { 
                trie: Arc::new(trie.clone()),
                syllables: syllables_arc.clone(),
            }));
            pipeline.add_filter(Box::new(crate::engine::pipeline::AdaptiveFilter {
                user_dict: config.user_dict.clone(),
                profile: name.clone()
            }));
            pipeline.add_filter(Box::new(crate::engine::pipeline::SortFilter));
            pipeline.add_filter(Box::new(crate::engine::pipeline::TraditionalFilter));
            pipelines.insert(name.clone(), pipeline);
        }

        Self {
            ctx: InputContext {
                state: ImeState::Direct,
                buffer: String::new(),
                candidates: vec![],
                selected: 0,
                page: 0,
                cursor_pos: 0,
                joined_sentence: String::new(),
                last_lookup_pinyin: String::new(),
                nav_mode: false,
                switch_mode: false,
                aux_filter: String::new(),
                filter_mode: FilterMode::None,
                page_snapshot: Vec::new(),
                shift_used_as_modifier: false,
            },
            config,
            key_map: HashMap::new(),
            tries, 
            active_profiles: vec![initial_profile],
            syllables,
            chinese_enabled: true,
            best_segmentation: vec![],

            phantom_text: String::new(),
            preview_selected_candidate: false,
            last_blocked_buffer: String::new(),
            has_dict_match: false,
            
            last_tap_key: None,
            last_tap_time: None,

            key_press_info: None,
            long_press_triggered: false,
            commit_history: Vec::new(),
            last_commit_time: Instant::now(),
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
            pipelines,
        }
    }

    pub fn apply_config(&mut self, conf: &crate::config::Config) {
        self.config.apply_config(conf);

        if !conf.input.active_profiles.is_empty() {
            self.active_profiles = conf.input.active_profiles.iter().map(|p: &String| p.to_lowercase()).collect();
        } else {
            let new_profile = conf.input.default_profile.to_lowercase();
            if !new_profile.is_empty() && self.tries.contains_key(&new_profile) {
                self.active_profiles = vec![new_profile];
            }
        }

        if self.ctx.buffer.is_empty() {
            self.reset();
        } else {
            let _ = self.lookup();
        }
        self.setup_default_keymap();
    }

    fn setup_default_keymap(&mut self) {
        self.key_map.clear();
        let none = ModifierState { shift: false, ctrl: false, alt: false, meta: false };
        // let shift = ModifierState { shift: true, ctrl: false, alt: false, meta: false };

        // 基础导航
        self.key_map.insert((VirtualKey::Left, none), Command::PrevCandidate);
        self.key_map.insert((VirtualKey::Right, none), Command::NextCandidate);
        self.key_map.insert((VirtualKey::Up, none), Command::PrevPage);
        self.key_map.insert((VirtualKey::Down, none), Command::NextPage);
        self.key_map.insert((VirtualKey::PageUp, none), Command::PrevPage);
        self.key_map.insert((VirtualKey::PageDown, none), Command::NextPage);
        
        self.key_map.insert((VirtualKey::Space, none), Command::Commit);
        self.key_map.insert((VirtualKey::Enter, none), Command::Commit);
        self.key_map.insert((VirtualKey::Esc, none), Command::Clear);
        self.key_map.insert((VirtualKey::Delete, none), Command::Clear);

        // HJKL 映射 (虽然目前是在 handle_composing 里根据 nav_mode 判断，但也可以预设)
        // 这里的 key_map 目前还不支持“模式感知的映射”，暂时保持简单的静态映射
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
        self.ctx.buffer.push_str(text);
        if self.ctx.state == ImeState::Direct { self.ctx.state = ImeState::Composing; }
        self.preview_selected_candidate = false;
        if let Some(act) = self.lookup() { return act; }
        if let Some(act) = self.check_auto_commit() { return act; }
        self.update_phantom_action()
    }

    pub fn clear_composing(&mut self) {
        self.ctx.buffer.clear();
        self.ctx.candidates.clear();
        self.best_segmentation.clear();
        self.ctx.joined_sentence.clear();
        self.ctx.selected = 0;
        self.ctx.page = 0;
        self.ctx.state = ImeState::Direct;
        self.phantom_text.clear();
        self.preview_selected_candidate = false;
        self.ctx.cursor_pos = 0;
        self.ctx.aux_filter.clear();
        self.ctx.filter_mode = FilterMode::None;
        self.ctx.page_snapshot.clear();
        self.ctx.nav_mode = false;
    }

    pub fn reset(&mut self) {
        self.clear_composing();
        self.ctx.switch_mode = false;
        self.quote_open = false;
        self.single_quote_open = false;
    }

    pub fn handle_key(&mut self, key: VirtualKey, val: i32, shift_pressed: bool, ctrl_pressed: bool, alt_pressed: bool) -> Action {
        self.handle_key_ext(key, val, shift_pressed, ctrl_pressed, alt_pressed, true)
    }

    pub fn handle_key_ext(&mut self, key: VirtualKey, val: i32, shift_pressed: bool, ctrl_pressed: bool, alt_pressed: bool, perform_lookup: bool) -> Action {
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
                let mut commit_text = if !self.ctx.joined_sentence.is_empty() { 
                    self.ctx.joined_sentence.trim_end().to_string() 
                } else if !self.ctx.candidates.is_empty() { 
                    self.ctx.candidates[0].text.trim_end().to_string() 
                } else { 
                    self.ctx.buffer.trim_end().to_string() 
                };
                commit_text.push_str(p_key); 
                let del_len = self.phantom_text.chars().count();
                self.clear_composing();
                self.commit_history.clear(); 
                return Action::DeleteAndEmit { delete: del_len, insert: commit_text };
            }
        }

        // 处理长按逻辑
        if (self.config.enable_long_press && is_letter(key)) || (self.config.enable_punctuation_long_press && get_punctuation_key(key, shift_pressed).is_some()) {
            if !shift_pressed {
                if val == 1 {
                    self.key_press_info = Some((key, now));
                    self.long_press_triggered = false;
                } else if is_repeat {
                    if !self.long_press_triggered {
                        if let Some((press_key, press_time)) = self.key_press_info {
                            if press_key == key && now.duration_since(press_time) >= self.config.long_press_timeout {
                                if is_letter(key) {
                                    if let Some(c) = key_to_char(key, false) {
                                        if let Some(replacement) = self.config.long_press_mappings.get(&c.to_string()).cloned() {
                                            self.long_press_triggered = true;
                                            if !self.ctx.buffer.is_empty() {
                                                if let Some(last_char) = self.ctx.buffer.chars().last() {
                                                    if last_char.to_string() == c.to_string() {
                                                        self.ctx.buffer.pop();
                                                    }
                                                }
                                            }
                                            return self.inject_text(&replacement);
                                        }
                                    }
                                } else {
                                    if let Some(p_key) = get_punctuation_key(key, false) {
                                        if let Some(replacement) = self.config.punctuation_long_press_mappings.get(p_key).cloned() {
                                            self.long_press_triggered = true;
                                            let mut commit_text = if !self.ctx.joined_sentence.is_empty() { 
                                                self.ctx.joined_sentence.trim_end().to_string() 
                                            } else if !self.ctx.candidates.is_empty() { 
                                                self.ctx.candidates[0].text.trim_end().to_string() 
                                            } else { 
                                                self.ctx.buffer.trim_end().to_string() 
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

        if is_press && key == VirtualKey::Shift {
            self.ctx.shift_used_as_modifier = false;
        }

        if is_release {
            if key == VirtualKey::CapsLock { return Action::Consume; }
            if key == VirtualKey::Shift && !self.ctx.buffer.is_empty() {
                if !self.ctx.shift_used_as_modifier {
                    self.start_global_filter();
                }
                self.ctx.shift_used_as_modifier = false;
                return Action::Consume;
            }
            if self.ctx.buffer.is_empty() { return Action::PassThrough; }
            return Action::Consume;
        }

        if key == VirtualKey::CapsLock {
            if is_press {
                if self.ctx.buffer.is_empty() {
                    self.ctx.switch_mode = !self.ctx.switch_mode;
                    return if self.ctx.switch_mode { 
                        Action::Notify("快捷切换".into(), "已进入方案切换模式".into()) 
                    } else { 
                        Action::Notify("快捷切换".into(), "已退出".into()) 
                    };
                } else {
                    self.ctx.nav_mode = !self.ctx.nav_mode;
                    if self.ctx.nav_mode {
                        if self.ctx.page + self.config.page_size < self.ctx.candidates.len() {
                            self.ctx.page += self.config.page_size;
                            self.ctx.selected = self.ctx.page;
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

        if self.ctx.switch_mode && is_press {
            match key {
                VirtualKey::Esc | VirtualKey::Space | VirtualKey::Enter => { self.ctx.switch_mode = false; return Action::Notify("快捷切换".into(), "已退出".into()); }
                VirtualKey::E => {
                    self.ctx.switch_mode = false;
                    if let Some((pinyin, word)) = self.commit_history.pop() {
                        let del_count = word.chars().count();
                        self.ctx.buffer = pinyin;
                        self.ctx.state = ImeState::Composing;
                        let _ = self.lookup();
                        return Action::DeleteAndEmit { delete: del_count, insert: "".into() };
                    }
                    return Action::Consume;
                }
                VirtualKey::Z => {
                    self.ctx.switch_mode = false;
                    if let Some(_d) = self.tries.get("english") {
                        self.active_profiles = vec!["english".to_string()];
                        self.reset();
                        return Action::Notify("英".into(), "已切换至英语方案".into());
                    }
                    return Action::Consume;
                }
                _ if is_letter(key) => {
                    let k = key_to_char(key, false).unwrap_or(' ').to_string();
                    let mut target_profile = None;
                    for (trigger_key, profile_name) in &self.config.profile_keys {
                        if trigger_key == &k { target_profile = Some(profile_name.clone()); break; }
                    }

                    if let Some(p_str) = target_profile {
                        let profiles: Vec<String> = p_str.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty() && self.tries.contains_key(s)).collect();
                        if !profiles.is_empty() {
                            self.active_profiles = profiles;
                            let display = self.get_current_profile_display();
                            let short_display = self.get_short_display();
                            let _ = self.lookup();
                            self.ctx.switch_mode = false;
                            return Action::Notify(short_display, format!("方案: {}", display));
                        } else {
                            self.ctx.switch_mode = false;
                            return Action::Notify("❌".into(), format!("错误: 方案 [{}] 的词库未加载", p_str));
                        }
                    }
                }
                _ => {} 
            }
            return Action::Consume;
        }

        if self.ctx.switch_mode && is_release {
            return Action::Consume;
        }

        if !self.ctx.buffer.is_empty() { return self.handle_composing(key, shift_pressed, perform_lookup); }
        match self.ctx.state {
            ImeState::Direct => self.handle_direct(key, shift_pressed, perform_lookup),
            _ => self.handle_composing(key, shift_pressed, perform_lookup)
        }
    }

    fn handle_direct(&mut self, key: VirtualKey, shift_pressed: bool, perform_lookup: bool) -> Action {
        if key == VirtualKey::Enter || key == VirtualKey::Space {
            return Action::PassThrough;
        }
        if is_letter(key) {
            if let Some(c) = key_to_char(key, shift_pressed) {
                let lang = self.active_profiles.get(0).cloned().unwrap_or_default().to_lowercase();
                if let Some(layout) = self.config.keyboard_layouts.get(&lang) {
                    if let Some(mapped) = layout.get(&c.to_string()) {
                        return Action::Emit(mapped.clone());
                    }
                }

                self.ctx.buffer.push(c);
                self.ctx.state = ImeState::Composing;
                if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                if self.should_block_invalid_input(&self.ctx.buffer.clone()) { return Action::Alert; }
                return self.update_phantom_action();
            }
        }

        if get_punctuation_key(key, shift_pressed).is_some() {
            return self.handle_punctuation(key, shift_pressed);
        }

        Action::PassThrough
    }

    fn handle_composing(&mut self, key: VirtualKey, shift_pressed: bool, perform_lookup: bool) -> Action {
        let mods = ModifierState { shift: shift_pressed, ctrl: false, alt: false, meta: false };
        
        // 1. 优先尝试从 KeyMap 中获取统一指令
        if let Some(cmd) = self.key_map.get(&(key, mods)).cloned() {
            // 处理方向键交换逻辑 (如果是方向键且启用了交换)
            let final_cmd = if self.config.swap_arrow_keys {
                match (key, cmd.clone()) {
                    (VirtualKey::Up, Command::PrevPage) => Command::PrevCandidate,
                    (VirtualKey::Down, Command::NextPage) => Command::NextCandidate,
                    (VirtualKey::Left, Command::PrevCandidate) => Command::PrevPage,
                    (VirtualKey::Right, Command::NextCandidate) => Command::NextPage,
                    _ => cmd
                }
            } else { cmd };
            
            // 特殊处理：Space 在 Shift 状态下有不同的 Commit 逻辑，
            // 这里的静态 Map 可能覆盖不了，暂且在 execute_command 内部或这里二次处理。
            if key == VirtualKey::Space && shift_pressed {
                if let Some(cand) = self.ctx.candidates.get(self.ctx.selected) {
                    if !cand.hint.is_empty() {
                        return self.commit_candidate(cand.hint.clone(), 99);
                    }
                }
            }
            return self.execute_command(final_cmd);
        }

        // 2. 如果处于导航模式，映射 HJKL
        if self.ctx.nav_mode {
            match key {
                VirtualKey::H => return self.execute_command(Command::PrevCandidate),
                VirtualKey::L => return self.execute_command(Command::NextCandidate),
                VirtualKey::K => return self.execute_command(Command::PrevPage),
                VirtualKey::J => return self.execute_command(Command::NextPage),
                _ => { /* 继续处理其他按键，或退出模式 */ }
            }
        }

        let has_cand = !self.ctx.candidates.is_empty();
        let now = Instant::now();

        // --- Shift + Letter 辅助码过滤 / 精确选词 ---
        if is_letter(key) && shift_pressed && !self.ctx.buffer.is_empty() {
             if let Some(c) = key_to_char(key, false) {
                 self.ctx.shift_used_as_modifier = true;
                 // 第一次按下 Shift+字母，进入页面过滤模式 (保存快照)
                 if self.ctx.filter_mode == FilterMode::None {
                     self.ctx.filter_mode = FilterMode::Page;
                     self.ctx.page_snapshot = self.ctx.candidates.clone();
                     self.ctx.aux_filter = c.to_string();
                 } else {
                     self.ctx.aux_filter.push(c);
                 }

                 if let Some(act) = self.lookup() { return act; }
                 return self.update_phantom_action();
             }
        }

        let current_profile = self.active_profiles.get(0).cloned().unwrap_or_default();
        if let Some(scheme) = self.schemes.get(&current_profile) {
            let context = SchemeContext {
                config: &self.config.master_config,
                tries: &self.tries,
                syllables: &self.syllables,
                _user_dict: &self.config.user_dict,
                active_profiles: &self.active_profiles,
                candidate_count: self.ctx.candidates.len(),
                _filter_mode: self.ctx.filter_mode.clone(),
                _aux_filter: &self.ctx.aux_filter,
            };
            if let Some(act) = scheme.handle_special_key(key, &mut self.ctx.buffer, &context) {
                if act == Action::Consume {
                    if perform_lookup { if let Some(lookup_act) = self.lookup() { return lookup_act; } }
                    return self.update_phantom_action();
                }
                return act;
            }
        }

        if is_letter(key) {
            if self.ctx.filter_mode != FilterMode::None {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    self.ctx.aux_filter.push(c);
                    self.ctx.selected = 0;
                    if self.ctx.filter_mode == FilterMode::Global { self.ctx.page = 0; }
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    return self.update_phantom_action();
                }
            }
            
            if !shift_pressed && self.config.enable_double_tap {
                if let Some(last_k) = self.last_tap_key {
                    if last_k == key {
                        if let Some(last_t) = self.last_tap_time {
                            if now.duration_since(last_t) <= self.config.double_tap_timeout {
                                if let Some(c) = key_to_char(key, false) {
                                    if let Some(replacement) = self.config.double_taps.get(&c.to_string()) {
                                        if self.ctx.buffer.ends_with(c) {
                                            self.ctx.buffer.pop();
                                            self.ctx.buffer.push_str(replacement);
                                            self.last_tap_key = None;
                                            self.last_tap_time = None;
                                            if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                                            return self.update_phantom_action();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
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

        let styles = &self.config.page_flipping_styles;
        let flip_me = styles.contains(&"minus_equal".to_string());
        let flip_cd = styles.contains(&"comma_dot".to_string());

        if key == VirtualKey::Semicolon && !shift_pressed {
            self.ctx.buffer.push(';');
            if perform_lookup { if let Some(act) = self.lookup() { return act; } }
            return self.update_phantom_action();
        }

        match key {
            VirtualKey::Backspace => {
                if self.ctx.filter_mode != FilterMode::None {
                    self.ctx.aux_filter.pop();
                    if self.ctx.aux_filter.is_empty() {
                        self.ctx.filter_mode = FilterMode::None;
                        self.ctx.page_snapshot.clear();
                        self.ctx.page = 0; 
                    } else {
                        self.ctx.selected = 0;
                        if self.ctx.filter_mode == FilterMode::Global { self.ctx.page = 0; }
                    }
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    return self.update_phantom_action();
                }

                if self.ctx.buffer.is_empty() {
                    self.commit_history.clear();
                    return Action::PassThrough;
                }

                self.ctx.buffer.pop();

                if self.ctx.buffer.is_empty() {
                    let del = self.phantom_text.chars().count(); self.reset();
                    if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
                } else { 
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    self.update_phantom_action() 
                }
            }
            VirtualKey::Minus if flip_me && has_cand => self.execute_command(Command::PrevPage),
            VirtualKey::Equal if flip_me && has_cand => self.execute_command(Command::NextPage),
            VirtualKey::Comma if flip_cd && has_cand => self.execute_command(Command::PrevPage),
            VirtualKey::Dot if flip_cd && has_cand => self.execute_command(Command::NextPage),

            VirtualKey::Home => { if shift_pressed { self.ctx.selected = 0; self.ctx.page = 0; } else { self.ctx.selected = self.ctx.page; } Action::Consume }
            VirtualKey::End => { if has_cand { if shift_pressed { self.ctx.selected = self.ctx.candidates.len() - 1; self.ctx.page = (self.ctx.selected / self.config.page_size) * self.config.page_size; } else { self.ctx.selected = (self.ctx.page + self.config.page_size - 1).min(self.ctx.candidates.len() - 1); } } Action::Consume }

            VirtualKey::Apostrophe if !shift_pressed => {
                self.ctx.buffer.push('\'');
                self.preview_selected_candidate = false;
                if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                self.update_phantom_action()
            }

            VirtualKey::Slash if !self.ctx.buffer.is_empty() => {
                let mut new_buffer = self.ctx.buffer.clone();
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
                    self.ctx.buffer = new_buffer;
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    return self.update_phantom_action();
                }
                Action::PassThrough
            }

            _ if is_digit(key) => {
                let digit = key_to_digit(key).unwrap_or(0);
                if self.config.enable_number_selection && self.config.commit_mode == "single" && digit >= 1 && digit <= self.config.page_size {
                    return self.execute_command(Command::Select(digit as usize - 1));
                }
                let old_buffer = self.ctx.buffer.clone(); self.ctx.buffer.push_str(&digit.to_string()); 
                if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                if self.should_block_invalid_input(&old_buffer) { return Action::Alert; }
                if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
            }
            _ => {
                if get_punctuation_key(key, shift_pressed).is_some() {
                    self.handle_punctuation(key, shift_pressed)
                } else if let Some(c) = key_to_char(key, shift_pressed) {
                    let old_buffer = self.ctx.buffer.clone();
                    self.ctx.buffer.push(c);
                    self.preview_selected_candidate = false; 
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    if self.should_block_invalid_input(&old_buffer) { return Action::Alert; }
                    if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
                } else { Action::PassThrough }
            }
        }
    }

    fn handle_punctuation(&mut self, key: VirtualKey, shift_pressed: bool) -> Action {
        let punc_key_owned = get_punctuation_key(key, shift_pressed)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{:?}", key));
        let punc_key = punc_key_owned.as_str();
        let lang = self.active_profiles.get(0).cloned().unwrap_or_else(|| "chinese".to_string());
        
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
            let zh_puncs = self.config.punctuations.get(&lang).and_then(|m| m.get(punc_key))
                .or_else(|| self.config.punctuations.get("chinese").and_then(|m| m.get(punc_key)));
            
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

        let mut commit_text = if !self.ctx.joined_sentence.is_empty() { 
            self.ctx.joined_sentence.trim_end().to_string() 
        } else if !self.ctx.candidates.is_empty() { 
            self.ctx.candidates[0].text.trim_end().to_string() 
        } else { 
            self.ctx.buffer.trim_end().to_string() 
        };
        commit_text.push_str(&zh_punc);
        let del_len = self.phantom_text.chars().count();
        self.clear_composing();
        self.commit_history.clear(); 
        Action::DeleteAndEmit { delete: del_len, insert: commit_text }
    }

    fn commit_candidate(&mut self, mut cand: String, _index: usize) -> Action {
        let now = Instant::now();
        let py = self.ctx.last_lookup_pinyin.clone();

        if self.config.enable_user_dict && !py.is_empty() {
            self.record_usage(&py, &cand);
            if now.duration_since(self.last_commit_time) > Duration::from_secs(3) {
                self.commit_history.clear();
            }
            self.commit_history.push((py.clone(), cand.clone()));
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
                for (py, word) in new_combinations {
                    self.record_usage(&py, &word);
                }
            }
            self.last_commit_time = now;
        }

        if self.active_profiles.len() == 1 && self.active_profiles[0] == "english" && !cand.is_empty() && cand.chars().last().unwrap_or(' ').is_alphanumeric() { cand.push(' '); }
        let del = self.phantom_text.chars().count(); self.clear_composing(); Action::DeleteAndEmit { delete: del, insert: cand }
    }

    pub fn update_phantom_action(&mut self) -> Action {
        if self.config.phantom_type == crate::config::PhantomType::None { return Action::Consume; }
        
        let target = crate::engine::compositor::Compositor::get_phantom_text(self);

        if target == self.phantom_text { return Action::Consume; }
        let old_phantom = self.phantom_text.clone();
        let old_chars: Vec<char> = old_phantom.chars().collect();
        let target_chars: Vec<char> = target.chars().collect();
        let mut common_prefix_len = 0;
        for (c1, c2) in old_chars.iter().zip(target_chars.iter()) {
            if c1 == c2 { common_prefix_len += 1; }
            else { break; }
        }
        let delete_count = old_chars.len() - common_prefix_len;
        let insert_text: String = target_chars[common_prefix_len..].iter().collect();
        self.phantom_text = target;
        
        if delete_count == 0 && insert_text.is_empty() {
            Action::Consume
        } else if delete_count == 0 {
            Action::Emit(insert_text)
        } else {
            Action::DeleteAndEmit { delete: delete_count, insert: insert_text }
        }
    }
    pub fn lookup(&mut self) -> Option<Action> {
        self.lookup_with_limit(20)
    }

    pub fn trigger_incremental_search(&mut self) {
        let current_len = self.ctx.candidates.len();
        if current_len >= 200 { return; } // 避免无限搜索
        self.lookup_with_limit(current_len + 50);
    }

    pub fn lookup_with_limit(&mut self, limit: usize) -> Option<Action> {
        if self.ctx.buffer.is_empty() { self.reset(); return None; }

        // 辅助函数：判断候选词是否匹配当前的辅助码过滤器
        let matches_filter = |cand: &crate::engine::pipeline::Candidate, filter: &str| -> bool {
            if filter.is_empty() { return true; }
            let filter_lower = filter.to_lowercase();
            let hint_lower = cand.hint.to_lowercase();
            let hint_clean = strip_tones(&hint_lower);
            
            // 将 Hint 切分为多个部分（空格、斜杠、括号分隔）
            let parts: Vec<&str> = hint_clean.split(|c| c == ' ' || c == '/' || c == '(' || c == ')' || c == ',').collect();
            
            // 逻辑：输入的辅助码必须匹配 Hint 中某一部分的开头
            // 比如 Hint 是 "Code Dx", 输入 "c", "co", "cod", "code" 都能匹配
            parts.iter().any(|p| p.starts_with(&filter_lower)) || hint_clean.starts_with(&filter_lower)
        };

        // 1. 优先处理分页过滤模式 (针对当前已有候选词的快照进行过滤)
        if self.ctx.filter_mode == FilterMode::Page && !self.ctx.page_snapshot.is_empty() {
            let mut filtered = Vec::new();
            for c in &self.ctx.page_snapshot {
                if matches_filter(c, &self.ctx.aux_filter) {
                    filtered.push(c.clone());
                }
            }
            
            if !filtered.is_empty() {
                self.ctx.candidates = filtered;
                if self.ctx.candidates.len() == 1 { 
                    let word = self.ctx.candidates[0].text.clone(); 
                    return Some(self.commit_candidate(word, 0)); 
                }
            } else {
                // 如果完全没匹配到，不清除候选词，保持上一次的结果，或者只显示缓冲区
                // 这样用户输错辅助码时，不会看到一片空白
                self.ctx.candidates.clear();
            }
            self.update_state();
            return None;
        }

        let current_profile = self.active_profiles.get(0).cloned().unwrap_or_default();
        let config = &self.config.master_config; 

        if let Some(pipeline) = self.pipelines.get(&current_profile) {
            self.best_segmentation = pipeline.segmentor.segment(&self.ctx.buffer, &self.syllables);
            let results = pipeline.run(&self.ctx.buffer, &self.syllables, config, limit);
            
            self.ctx.candidates = results;
            self.has_dict_match = !self.ctx.candidates.is_empty();
            self.ctx.last_lookup_pinyin = self.ctx.buffer.clone();

            // --- 全局过滤逻辑 (针对检索结果进行实时过滤) ---
            if self.ctx.filter_mode == FilterMode::Global && !self.ctx.aux_filter.is_empty() {
                let mut fc = Vec::new();
                for c in &self.ctx.candidates {
                    if matches_filter(c, &self.ctx.aux_filter) {
                        fc.push(c.clone());
                    }
                }
                self.ctx.candidates = fc;
                if self.ctx.candidates.len() == 1 {
                    let word = self.ctx.candidates[0].text.clone();
                    return Some(self.commit_candidate(word, 0));
                }
            }

            self.update_state();
            return None;
        }

        if let Some(scheme) = self.schemes.get(&current_profile) {
            let context = SchemeContext {
                config: config,
                tries: &self.tries,
                syllables: &self.syllables,
                _user_dict: &self.config.user_dict,
                active_profiles: &self.active_profiles,
                candidate_count: self.ctx.candidates.len(),
                _filter_mode: self.ctx.filter_mode.clone(),
                _aux_filter: &self.ctx.aux_filter,
            };
            let query = scheme.pre_process(&self.ctx.buffer, &context);
            let mut candidates = scheme.lookup(&query, &context);
            scheme.post_process(&query, &mut candidates, &context);
            
            self.ctx.candidates.clear();
            self.has_dict_match = !candidates.is_empty();
            self.ctx.last_lookup_pinyin = query.clone();
            
            for c in candidates {
                let mut hint = String::new();
                let is_chinese_pure = self.active_profiles.len() == 1 && self.active_profiles[0] == "chinese";
                let is_stroke = current_profile == "stroke";
                if self.config.show_tone_hint && !c.tone.is_empty() && !is_chinese_pure { hint.push_str(&c.tone); }
                if !is_stroke && !c.english.is_empty() {
                    if !hint.is_empty() { hint.push(' '); }
                    hint.push_str(&c.english);
                }
                if self.config.show_stroke_aux && !c.stroke_aux.is_empty() {
                    if !hint.is_empty() { hint.push(' '); }
                    hint.push_str(&c.stroke_aux);
                }
                
                self.ctx.candidates.push(crate::engine::pipeline::Candidate {
                    text: if self.config.enable_traditional { c.traditional.clone() } else { c.simplified.clone() },
                    simplified: c.simplified,
                    traditional: c.traditional,
                    hint,
                    source: "Scheme".into(),
                    weight: c.weight as f64,
                });
            }
            // --- 过滤逻辑 ---
            if self.ctx.filter_mode == FilterMode::Global && !self.ctx.aux_filter.is_empty() {
                let filter_lower = self.ctx.aux_filter.to_lowercase();
                let mut fc = Vec::new();
                for c in &self.ctx.candidates {
                    let hint_lower = c.hint.to_lowercase();
                    let hint_clean = strip_tones(&hint_lower);
                    let parts: Vec<&str> = hint_clean.split(|ch| ch == ' ' || ch == '/' || ch == '(' || ch == ')').collect();
                    let is_match = parts.iter().any(|p| p.starts_with(&filter_lower)) || hint_clean.starts_with(&filter_lower) || hint_lower.starts_with(&filter_lower);
                    if is_match {
                        fc.push(c.clone());
                    }
                }
                if !fc.is_empty() { 
                    self.ctx.candidates = fc; 
                    if self.ctx.candidates.len() == 1 {
                        let word = self.ctx.candidates[0].text.clone();
                        return Some(self.commit_candidate(word, 0));
                    }
                }
            }
            if self.ctx.candidates.is_empty() {
                self.ctx.candidates.push(crate::engine::pipeline::Candidate {
                    text: self.ctx.buffer.clone(),
                    simplified: self.ctx.buffer.clone(),
                    traditional: self.ctx.buffer.clone(),
                    hint: "".into(),
                    source: "Raw".into(),
                    weight: 0.0,
                });
            }
            self.update_state();
            return None;
        }

        if self.ctx.candidates.is_empty() {
            self.ctx.candidates.push(crate::engine::pipeline::Candidate {
                text: self.ctx.buffer.clone(),
                simplified: self.ctx.buffer.clone(),
                traditional: self.ctx.buffer.clone(),
                hint: "".into(),
                source: "Raw".into(),
                weight: 0.0,
            });
        }
        self.update_state();
        None
    }

    pub fn get_current_profile_display(&self) -> String {
        if self.active_profiles.is_empty() { return "None".to_string(); }
        if self.active_profiles.len() == 1 { return self.active_profiles[0].clone(); }
        "Mixed".to_string()
    }

    fn update_state(&mut self) {
        if self.ctx.buffer.is_empty() { self.ctx.state = if self.ctx.candidates.is_empty() { ImeState::Direct } else { ImeState::Multi }; }
        else { self.ctx.state = match self.ctx.candidates.len() { 0 => ImeState::NoMatch, 1 => ImeState::Single, _ => ImeState::Multi }; }
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
        if !self.config.auto_commit_unique_full_match || self.ctx.candidates.len() != 1 || !self.has_dict_match || self.ctx.state == ImeState::NoMatch { return None; }
        let raw_input = &self.ctx.buffer;
        let mut total_longer = 0;
        for p in &self.active_profiles {
            if let Some(d) = self.tries.get(p) { if d.has_longer_match(raw_input) { total_longer += 1; break; } }
        }
        if total_longer == 0 { return Some(self.commit_candidate(self.ctx.candidates[0].text.clone(), 0)); }
        None
    }

    fn should_block_invalid_input(&mut self, old_buffer: &str) -> bool {
        if self.has_dict_match { self.last_blocked_buffer.clear(); return false; }
        match self.config.anti_typo_mode {
            crate::config::AntiTypoMode::None => false,
            crate::config::AntiTypoMode::Strict => { self.ctx.buffer = old_buffer.to_string(); let _ = self.lookup(); true }
            crate::config::AntiTypoMode::Smart => {
                if !self.last_blocked_buffer.is_empty() && self.ctx.buffer == self.last_blocked_buffer { self.last_blocked_buffer.clear(); false }
                else { self.last_blocked_buffer = self.ctx.buffer.clone(); self.ctx.buffer = old_buffer.to_string(); let _ = self.lookup(); true }
            }
        }
    }

    pub fn start_global_filter(&mut self) {
        if self.ctx.state == ImeState::Direct { return; }
        self.ctx.filter_mode = FilterMode::Global;
        self.ctx.aux_filter.clear();
    }

    pub fn save_user_dict(&self) {
        self.config.save_user_dict();
    }

    fn record_usage(&mut self, pinyin: &str, word: &str) {
        if !self.config.enable_user_dict || pinyin.is_empty() || word.is_empty() { return; }
        let profile = self.active_profiles.get(0).cloned().unwrap_or_else(|| "chinese".to_string());
        let mut dict = self.config.user_dict.lock().unwrap();
        let profile_dict = dict.entry(profile).or_insert_with(HashMap::new);
        let entries = profile_dict.entry(pinyin.to_string()).or_insert_with(Vec::new);
        if let Some(pos) = entries.iter().position(|(w, _)| w == word) { entries[pos].1 += 1; }
        else { entries.push((word.to_string(), 1)); }
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        drop(dict);
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
    for c in s.chars() { match c { 'ā'|'á'|'ǎ'|'à' => res.push('a'), 'ē'|'é'|'ě'|'è' => res.push('e'), 'ī'|'í'|'ǐ'|'ì' => res.push('i'), 'ō'|'ó'|'ǒ'|'ò' => res.push('o'), 'ū'|'ú'|'ǔ'|'ù' => res.push('u'), 'ǖ'|'ǘ'|'ǚ'|'ǜ' => res.push('v'), 'Ā'|'Á'|'Ǎ'|'À' => res.push('a'), 'Ē'|'É'|'Ě'|'È' => res.push('e'), 'Ī'|'Í'|'Ǐ'|'Ì' => res.push('i'), 'Ō'|'Ó'|'Ǒ'|'Ò' => res.push('o'), 'Ū'|'Ú'|'Ǔ'|'Ù' => res.push('u'), 'Ǖ'|'Ǘ'|'Ǚ'|'Ǜ' => res.push('v'), _ => res.push(c) } } 
    res
}
