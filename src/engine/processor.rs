use std::collections::HashMap;
use std::sync::Arc;
use std::collections::HashSet;
use crate::engine::keys::VirtualKey;
use crate::engine::scheme::InputScheme;
use crate::engine::pipeline::Candidate;
use crate::engine::{Command, ModifierState, InputEvent};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FilterMode {
    None,
    Global, // Shift + 字母 (全局筛选)
    Page,   // Caps + 字母 (当前页筛选)
}

use crate::config::PunctuationEntry;

pub struct Processor {
    pub session: crate::engine::InputSession,
    pub config: crate::engine::ConfigManager,
    pub dispatcher: crate::engine::KeyDispatcher,
    pub engine: crate::engine::pipeline::SearchEngine,
    
    pub active_profiles: Vec<String>,
    pub syllables: std::collections::HashSet<String>,
    
    pub chinese_enabled: bool,
    
    // 连续选词记忆
    pub commit_history: Vec<(String, String)>, // 最近上屏的 (拼音, 词组)
    pub last_commit_time: Instant,
}

impl Processor {
    pub fn execute_command(&mut self, cmd: Command) -> Action {
        let page_size = self.config.page_size;
        match cmd {
            Command::NextPage => {
                let old_page = self.session.page;
                self.session.next_page(page_size);
                if self.session.page == old_page && !self.session.candidates.is_empty() {
                    self.trigger_incremental_search();
                    self.session.next_page(page_size);
                }
                Action::Consume
            }
            Command::PrevPage => {
                self.session.prev_page(page_size);
                Action::Consume
            }
            Command::NextCandidate => {
                let old_sel = self.session.selected;
                self.session.next_candidate(page_size);
                if self.session.selected == old_sel && !self.session.candidates.is_empty() {
                    self.trigger_incremental_search();
                    self.session.next_candidate(page_size);
                }
                self.update_phantom_action()
            }
            Command::PrevCandidate => {
                self.session.prev_candidate(page_size);
                self.update_phantom_action()
            }
            Command::Select(idx) => {
                let abs_idx = self.session.page + idx;
                if let Some(cand) = self.session.candidates.get(abs_idx) {
                    let word = cand.text.clone();
                    return self.commit_candidate(word, abs_idx);
                }
                Action::Consume
            }
            Command::Commit => {
                if self.session.buffer.is_empty() { return Action::PassThrough; }
                
                // 优先尝试提交当前选中的候选词
                if !self.session.candidates.is_empty() {
                    let idx = self.session.selected;
                    if let Some(cand) = self.session.candidates.get(idx) {
                        let word = cand.text.clone();
                        return self.commit_candidate(word, idx);
                    }
                }

                // 如果完全没有候选词，才提交原始 buffer (例如未知输入)
                let out = self.session.buffer.clone();
                self.commit_candidate(out, 99)
            }
            Command::CommitRaw => {
                if self.session.buffer.is_empty() { return Action::PassThrough; }
                let out = self.session.buffer.clone();
                self.commit_candidate(out, 99)
            }
            Command::Clear => {
                self.commit_history.clear();
                let del = self.session.phantom_text.chars().count();
                self.reset();
                if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
            }
        }
    }
    pub fn new(
        trie_paths: HashMap<String, (std::path::PathBuf, std::path::PathBuf)>, 
        syllables: HashSet<String>,
    ) -> Self {
        let config = crate::engine::ConfigManager::new();
        let syllables_arc = Arc::new(syllables.clone());
        
        let engine = crate::engine::pipeline::SearchEngine::new(
            trie_paths,
            syllables_arc,
            config.user_dict.clone(),
            {
                let mut m: HashMap<String, Box<dyn InputScheme>> = HashMap::new();
                m.insert("stroke".to_string(), Box::new(crate::engine::schemes::StrokeScheme::new()));
                m.insert("english".to_string(), Box::new(crate::engine::schemes::EnglishScheme::new()));
                m.insert("chinese".to_string(), Box::new(crate::engine::schemes::ChineseScheme::new()));
                m.insert("japanese".to_string(), Box::new(crate::engine::schemes::JapaneseScheme::new()));
                m
            }
        );

        Self {
            session: crate::engine::InputSession::new(),
            config,
            dispatcher: crate::engine::KeyDispatcher::new(),
            engine,
            active_profiles: Vec::new(),
            syllables,
            chinese_enabled: true,
            
            commit_history: Vec::new(),
            last_commit_time: Instant::now(),
        }
    }

    pub fn apply_config(&mut self, conf: &crate::config::Config) {
        self.config.apply_config(conf);
        self.engine.clear_cache();

        if !conf.input.active_profiles.is_empty() {
            self.active_profiles = conf.input.active_profiles.iter().map(|p: &String| p.to_lowercase()).collect();
        } else {
            let new_profile = conf.input.default_profile.to_lowercase();
            if !new_profile.is_empty() && self.engine.trie_paths.contains_key(&new_profile) {
                self.active_profiles = vec![new_profile];
            }
        }

        if self.session.buffer.is_empty() {
            self.reset();
        } else {
            let _ = self.lookup();
        }
        self.setup_default_keymap();
    }

    fn setup_default_keymap(&mut self) {
        self.dispatcher.key_map.clear();
        let none = ModifierState { shift: false, ctrl: false, alt: false, meta: false };

        // 基础导航
        self.dispatcher.key_map.insert((VirtualKey::Left, none), Command::PrevCandidate);
        self.dispatcher.key_map.insert((VirtualKey::Right, none), Command::NextCandidate);
        self.dispatcher.key_map.insert((VirtualKey::Up, none), Command::PrevPage);
        self.dispatcher.key_map.insert((VirtualKey::Down, none), Command::NextPage);
        self.dispatcher.key_map.insert((VirtualKey::PageUp, none), Command::PrevPage);
        self.dispatcher.key_map.insert((VirtualKey::PageDown, none), Command::NextPage);
        
        self.dispatcher.key_map.insert((VirtualKey::Space, none), Command::Commit);
        self.dispatcher.key_map.insert((VirtualKey::Enter, none), Command::CommitRaw);
        self.dispatcher.key_map.insert((VirtualKey::Esc, none), Command::Clear);
        self.dispatcher.key_map.insert((VirtualKey::Delete, none), Command::Clear);
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
        self.session.buffer.push_str(text);
        if self.session.state == ImeState::Direct { self.session.state = ImeState::Composing; }
        self.session.preview_selected_candidate = false;
        if let Some(act) = self.lookup() { return act; }
        if let Some(act) = self.check_auto_commit() { return act; }
        self.update_phantom_action()
    }

    pub fn clear_composing(&mut self) {
        self.session.clear_composing();
    }

    pub fn reset(&mut self) {
        self.session.reset();
        self.dispatcher.reset_states();
    }

    pub fn handle_event(&mut self, event: InputEvent) -> Action {
        let span = tracing::info_span!("handle_event", ?event);
        let _enter = span.enter();
        match event {
            InputEvent::Key { key, val, shift, ctrl, alt } => {
                self.handle_key_ext(key, val, shift, ctrl, alt, true)
            }
            InputEvent::Voice(text) => {
                // 语音输入初步处理
                if !text.is_empty() {
                    self.reset();
                    return Action::Emit(text);
                }
                Action::Consume
            }
            InputEvent::CandidateSelect(idx) => {
                self.execute_command(Command::Select(idx))
            }
        }
    }

    pub fn handle_key(&mut self, key: VirtualKey, val: i32, shift_pressed: bool, ctrl_pressed: bool, alt_pressed: bool) -> Action {
        self.handle_event(InputEvent::Key {
            key,
            val,
            shift: shift_pressed,
            ctrl: ctrl_pressed,
            alt: alt_pressed,
        })
    }

    pub fn handle_key_ext(&mut self, key: VirtualKey, val: i32, shift_pressed: bool, ctrl_pressed: bool, alt_pressed: bool, perform_lookup: bool) -> Action {
        let now = Instant::now();
        let is_press = val == 1;
        let is_release = val == 0;

        if !self.chinese_enabled {
            return Action::PassThrough;
        }

        // 1. 处理 Ctrl + 标点 -> 强制输出原始标点
        if is_press && ctrl_pressed && !alt_pressed {
            if let Some(p_key) = get_punctuation_key(key, shift_pressed) {
                let mut commit_text = if !self.session.joined_sentence.is_empty() { 
                    self.session.joined_sentence.trim_end().to_string() 
                } else if !self.session.candidates.is_empty() { 
                    self.session.candidates[0].text.trim_end().to_string() 
                } else { 
                    self.session.buffer.trim_end().to_string() 
                };
                commit_text.push_str(p_key); 
                let del_len = self.session.phantom_text.chars().count();
                self.clear_composing();
                self.commit_history.clear(); 
                return Action::DeleteAndEmit { delete: del_len, insert: commit_text };
            }
        }

        // 2. 处理修饰键状态 (Shift, CapsLock 等)
        if let Some(action) = self.process_modifiers(key, is_press, is_release) {
            return action;
        }

        // 3. 处理输入意图识别 (长按、双击等)
        if let Some(action) = self.process_intent(key, val, shift_pressed, now) {
            return action;
        }

        // 4. 处理 Grave 键直通
        if key == VirtualKey::Grave {
            return Action::PassThrough;
        }

        // 5. 处理方案切换模式
        if let Some(action) = self.process_switch_mode(key, is_press, is_release) {
            return action;
        }

        // 6. 分发至核心输入状态处理
        if !self.session.buffer.is_empty() { return self.handle_composing(key, shift_pressed, perform_lookup); }
        match self.session.state {
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
                let lang = self.active_profiles.first().cloned().unwrap_or_default().to_lowercase();
                if let Some(layout) = self.config.keyboard_layouts.get(&lang) {
                    if let Some(mapped) = layout.get(&c.to_string()) {
                        return Action::Emit(mapped.clone());
                    }
                }

                self.session.push_char(c);
                if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                if self.should_block_invalid_input(&self.session.buffer.clone()) { return Action::Alert; }
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
        if let Some(cmd) = self.dispatcher.key_map.get(&(key, mods)).cloned() {
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
                if let Some(cand) = self.session.candidates.get(self.session.selected) {
                    if !cand.hint.is_empty() {
                        return self.commit_candidate(cand.hint.clone(), 99);
                    }
                }
            }
            return self.execute_command(final_cmd);
        }

        // 2. 如果处于导航模式，映射 HJKL
        if self.session.nav_mode {
            match key {
                VirtualKey::H => return self.execute_command(Command::PrevCandidate),
                VirtualKey::L => return self.execute_command(Command::NextCandidate),
                VirtualKey::K => return self.execute_command(Command::PrevPage),
                VirtualKey::J => return self.execute_command(Command::NextPage),
                _ => { /* 继续处理其他按键，或退出模式 */ }
            }
        }

        let has_cand = !self.session.candidates.is_empty();
        let now = Instant::now();

        // --- Shift + Letter 辅助码过滤 / 精确选词 ---
        if is_letter(key) && shift_pressed && !self.session.buffer.is_empty() {
             if let Some(c) = key_to_char(key, false) {
                 self.session.shift_used_as_modifier = true;
                 self.session.handle_filter_char(c);

                 if let Some(act) = self.lookup() { return act; }
                 return self.update_phantom_action();
             }
        }

        let current_profile = self.active_profiles.first().cloned().unwrap_or_default();
        if let Some(scheme) = self.engine.schemes.get(&current_profile) {
            let context = crate::engine::scheme::SchemeContext {
                config: &self.config.master_config,
                tries: &HashMap::new(), // 已移至 engine 管理
                syllables: &self.syllables,
                _user_dict: &self.config.user_dict,
                active_profiles: &self.active_profiles,
                candidate_count: self.session.candidates.len(),
                _filter_mode: self.session.filter_mode.clone(),
                _aux_filter: &self.session.aux_filter,
            };
            let act_opt: Option<Action> = scheme.handle_special_key(key, &mut self.session.buffer, &context);
            if let Some(act) = act_opt {
                if act == Action::Consume {
                    if perform_lookup { if let Some(lookup_act) = self.lookup() { return lookup_act; } }
                    return self.update_phantom_action();
                }
                return act;
            }
        }

        if is_letter(key) {
            if self.session.filter_mode != FilterMode::None {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    self.session.handle_filter_char(c);
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    return self.update_phantom_action();
                }
            }
            
            if !shift_pressed && self.config.enable_double_tap {
                if let Some(last_k) = self.dispatcher.last_tap_key {
                    if last_k == key {
                        if let Some(last_t) = self.dispatcher.last_tap_time {
                            if now.duration_since(last_t) <= self.config.double_tap_timeout {
                                if let Some(c) = key_to_char(key, false) {
                                    if let Some(replacement) = self.config.double_taps.get(&c.to_string()) {
                                        if self.session.buffer.ends_with(c) {
                                            self.session.buffer.pop();
                                            self.session.buffer.push_str(replacement);
                                            self.dispatcher.last_tap_key = None;
                                            self.dispatcher.last_tap_time = None;
                                            if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                                            return self.update_phantom_action();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.dispatcher.last_tap_key = Some(key);
                self.dispatcher.last_tap_time = Some(now);
            } else {
                self.dispatcher.last_tap_key = None;
                self.dispatcher.last_tap_time = None;
            }
        } else {
            self.dispatcher.last_tap_key = None;
            self.dispatcher.last_tap_time = None;
        }

        let styles = &self.config.page_flipping_styles;
        let flip_me = styles.contains(&"minus_equal".to_string());
        let flip_cd = styles.contains(&"comma_dot".to_string());

        if key == VirtualKey::Semicolon && !shift_pressed {
            self.session.push_char(';');
            if perform_lookup { if let Some(act) = self.lookup() { return act; } }
            return self.update_phantom_action();
        }

        match key {
            VirtualKey::Backspace => {
                if self.session.filter_mode != FilterMode::None {
                    self.session.pop_filter();
                    if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                    return self.update_phantom_action();
                }

                if self.session.buffer.is_empty() {
                    self.commit_history.clear();
                    return Action::PassThrough;
                }

                self.session.pop_char();

                if self.session.buffer.is_empty() {
                    let del = self.session.phantom_text.chars().count(); 
                    // self.session.pop_char 内部已经处理了 empty 时的 reset，
                    // 但我们需要显式调用这里的 reset 来清理 dispatcher 等
                    self.reset();
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

            VirtualKey::Home => { if shift_pressed { self.session.selected = 0; self.session.page = 0; } else { self.session.selected = self.session.page; } Action::Consume }
            VirtualKey::End => { if has_cand { if shift_pressed { self.session.selected = self.session.candidates.len() - 1; self.session.page = (self.session.selected / self.config.page_size) * self.config.page_size; } else { self.session.selected = (self.session.page + self.config.page_size - 1).min(self.session.candidates.len() - 1); } } Action::Consume }

            VirtualKey::Apostrophe if !shift_pressed => {
                self.session.buffer.push('\'');
                self.session.preview_selected_candidate = false;
                if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                self.update_phantom_action()
            }

            VirtualKey::Slash if !self.session.buffer.is_empty() => {
                let mut new_buffer = self.session.buffer.clone();
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
                    self.session.buffer = new_buffer;
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
                let old_buffer = self.session.buffer.clone(); 
                self.session.push_char(key_to_char(key, false).unwrap_or('0'));
                if perform_lookup { if let Some(act) = self.lookup() { return act; } }
                if self.should_block_invalid_input(&old_buffer) { return Action::Alert; }
                if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
            }
            _ => {
                if get_punctuation_key(key, shift_pressed).is_some() {
                    self.handle_punctuation(key, shift_pressed)
                } else if let Some(c) = key_to_char(key, shift_pressed) {
                    let old_buffer = self.session.buffer.clone();
                    self.session.push_char(c);
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
        let lang = self.active_profiles.first().cloned().unwrap_or_else(|| "chinese".to_string());
        
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
                    let p = if self.session.quote_open { entries.get(1).or(entries.first()) } else { entries.first() };
                    self.session.quote_open = !self.session.quote_open;
                    p.map(|e| e.char.clone()).unwrap_or_else(|| punc_key.to_string())
                } else if punc_key == "'" {
                    let p = if self.session.single_quote_open { entries.get(1).or(entries.first()) } else { entries.first() };
                    self.session.single_quote_open = !self.session.single_quote_open;
                    p.map(|e| e.char.clone()).unwrap_or_else(|| punc_key.to_string())
                } else {
                    entries.first().map(|e| e.char.clone()).unwrap_or_else(|| punc_key.to_string())
                }
            } else {
                punc_key.to_string()
            }
        };

        let mut commit_text = if !self.session.joined_sentence.is_empty() { 
            self.session.joined_sentence.trim_end().to_string() 
        } else if !self.session.candidates.is_empty() { 
            self.session.candidates[0].text.trim_end().to_string() 
        } else { 
            self.session.buffer.trim_end().to_string() 
        };
        commit_text.push_str(&zh_punc);
        let del_len = self.session.phantom_text.chars().count();
        self.clear_composing();
        self.commit_history.clear(); 
        Action::DeleteAndEmit { delete: del_len, insert: commit_text }
    }
    fn commit_candidate(&mut self, mut cand: String, index: usize) -> Action {
        let now = Instant::now();
        let py = self.session.last_lookup_pinyin.clone();

        if self.config.enable_user_dict && !py.is_empty() && index != 99 {
            if now.duration_since(self.last_commit_time) > Duration::from_secs(3) {
                self.commit_history.clear();
            }
            self.commit_history.push((py.clone(), cand.clone()));
            self.record_usage(&py, &cand);

            // 连打组合记忆逻辑
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
            for (py_c, word_c) in new_combinations {
                self.record_usage(&py_c, &word_c);
            }
            self.last_commit_time = now;
        }

        if self.active_profiles.len() == 1 && self.active_profiles[0] == "english" && !cand.is_empty() && cand.chars().last().unwrap_or(' ').is_alphanumeric() { 
            cand.push(' '); 
        }
        
        let del = self.session.phantom_text.chars().count(); 
        self.clear_composing(); 
        Action::DeleteAndEmit { delete: del, insert: cand }
    }

    pub fn update_phantom_action(&mut self) -> Action {
        if self.config.phantom_type == crate::config::PhantomType::None { return Action::Consume; }
        
        let target = crate::engine::compositor::Compositor::get_phantom_text(self);

        if target == self.session.phantom_text { return Action::Consume; }
        let old_phantom = self.session.phantom_text.clone();
        let old_chars: Vec<char> = old_phantom.chars().collect();
        let target_chars: Vec<char> = target.chars().collect();
        let mut common_prefix_len = 0;
        for (c1, c2) in old_chars.iter().zip(target_chars.iter()) {
            if c1 == c2 { common_prefix_len += 1; }
            else { break; }
        }
        let delete_count = old_chars.len() - common_prefix_len;
        let insert_text: String = target_chars[common_prefix_len..].iter().collect();
        self.session.phantom_text = target;
        
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
        let current_len = self.session.candidates.len();
        if current_len >= 200 { return; } // 避免无限搜索
        self.lookup_with_limit(current_len + 50);
    }

    pub fn lookup_with_limit(&mut self, limit: usize) -> Option<Action> {
        let span = tracing::debug_span!("lookup", buffer = %self.session.buffer, limit);
        let _enter = span.enter();
        if self.session.buffer.is_empty() { self.reset(); return None; }

        // 1. 优先处理分页过滤模式 (针对当前已有候选词的快照进行过滤)
        if self.session.filter_mode == FilterMode::Page && !self.session.page_snapshot.is_empty() {
            let mut filtered = Vec::new();
            for c in &self.session.page_snapshot {
                if self.engine.matches_filter(c, &self.session.aux_filter) {
                    filtered.push(c.clone());
                }
            }
            
            if !filtered.is_empty() {
                self.session.candidates = filtered;
                if self.session.candidates.len() == 1 { 
                    let word = self.session.candidates[0].text.clone(); 
                    return Some(self.commit_candidate(word, 0)); 
                }
            } else {
                self.session.candidates.clear();
            }
            self.update_state();
            return None;
        }

        // 2. 委派给 SearchEngine 进行全文/全词库搜索
        let current_profile = self.active_profiles.first().cloned().unwrap_or_default();
        let query = crate::engine::pipeline::SearchQuery {
            buffer: &self.session.buffer,
            profile: &current_profile,
            syllables: &self.syllables,
            config: &self.config.master_config,
            limit,
            filter_mode: self.session.filter_mode.clone(),
            aux_filter: &self.session.aux_filter,
        };
        let (results, segments) = self.engine.search(query);

        self.session.candidates = results;
        self.session.best_segmentation = segments;
        self.session.has_dict_match = !self.session.candidates.is_empty();
        self.session.last_lookup_pinyin = self.session.buffer.clone();

        // 3. 后置处理：单一匹配自动上屏或空结果兜底
        if self.session.candidates.len() == 1 && self.session.filter_mode == FilterMode::Global {
            let word = self.session.candidates[0].text.clone();
            return Some(self.commit_candidate(word, 0));
        }

        if self.session.candidates.is_empty() {
            self.session.candidates.push(crate::engine::pipeline::Candidate {
                text: self.session.buffer.clone(),
                simplified: self.session.buffer.clone(),
                traditional: self.session.buffer.clone(),
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
        self.session.update_state();
    }

    pub fn next_profile(&mut self) -> String {
        let mut all: Vec<String> = self.engine.trie_paths.keys().cloned().collect();
        if all.is_empty() { return String::new(); }
        all.sort();
        if self.active_profiles.len() > 1 {
            let next = all[0].clone();
            self.active_profiles = vec![next.clone()];
            self.reset();
            return next;
        }
        let current = self.active_profiles.first().cloned().unwrap_or_default();
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
        if !self.config.auto_commit_unique_full_match || self.session.candidates.len() != 1 || !self.session.has_dict_match || self.session.state == ImeState::NoMatch { return None; }
        let raw_input = &self.session.buffer;
        let mut total_longer = 0;
        for p in &self.active_profiles {
            if self.engine.has_longer_match(p, raw_input) { total_longer += 1; break; }
        }
        if total_longer == 0 { return Some(self.commit_candidate(self.session.candidates[0].text.clone(), 0)); }
        None
    }

    fn should_block_invalid_input(&mut self, old_buffer: &str) -> bool {
        if self.session.has_dict_match { self.session.last_blocked_buffer.clear(); return false; }
        match self.config.anti_typo_mode {
            crate::config::AntiTypoMode::None => false,
            crate::config::AntiTypoMode::Strict => { self.session.buffer = old_buffer.to_string(); let _ = self.lookup(); true }
            crate::config::AntiTypoMode::Smart => {
                if !self.session.last_blocked_buffer.is_empty() && self.session.buffer == self.session.last_blocked_buffer { self.session.last_blocked_buffer.clear(); false }
                else { self.session.last_blocked_buffer = self.session.buffer.clone(); self.session.buffer = old_buffer.to_string(); let _ = self.lookup(); true }
            }
        }
    }

    pub fn start_global_filter(&mut self) {
        if self.session.state == ImeState::Direct { return; }
        self.session.filter_mode = FilterMode::Global;
        self.session.aux_filter.clear();
    }

    pub fn save_user_dict(&self) {
        self.config.save_user_dict();
    }

    fn record_usage(&mut self, pinyin: &str, word: &str) {
        if !self.config.enable_user_dict || pinyin.is_empty() || word.is_empty() { return; }
        if std::env::args().any(|a| a == "--test") { return; }
        let profile = self.active_profiles.first().cloned().unwrap_or_else(|| "chinese".to_string());
        
        let mut dict_clone = (**self.config.user_dict.load()).clone();
        let profile_dict = dict_clone.entry(profile).or_default();
        let entries = profile_dict.entry(pinyin.to_string()).or_default();
        if let Some(pos) = entries.iter().position(|(w, _)| w == word) { entries[pos].1 += 1; }
        else { entries.push((word.to_string(), 1)); }
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        
        self.config.user_dict.store(Arc::new(dict_clone));
        self.save_user_dict();
    }

    fn process_modifiers(&mut self, key: VirtualKey, is_press: bool, is_release: bool) -> Option<Action> {
        if is_press && key == VirtualKey::Shift {
            self.session.shift_used_as_modifier = false;
        }

        if is_release {
            if key == VirtualKey::CapsLock { return Some(Action::Consume); }
            if key == VirtualKey::Shift && !self.session.buffer.is_empty() {
                if !self.session.shift_used_as_modifier {
                    self.start_global_filter();
                }
                self.session.shift_used_as_modifier = false;
                return Some(Action::Consume);
            }
            if self.session.buffer.is_empty() { return Some(Action::PassThrough); }
            return Some(Action::Consume);
        }

        if key == VirtualKey::CapsLock && is_press {
            if self.session.buffer.is_empty() {
                self.session.switch_mode = !self.session.switch_mode;
                return Some(if self.session.switch_mode { 
                    Action::Notify("快捷切换".into(), "已进入方案切换模式".into()) 
                } else { 
                    Action::Notify("快捷切换".into(), "已退出".into()) 
                });
            } else {
                self.session.toggle_nav_mode(self.config.page_size);
                return Some(Action::Consume);
            }
        }
        
        None
    }

    fn process_intent(&mut self, key: VirtualKey, val: i32, shift_pressed: bool, now: Instant) -> Option<Action> {
        let is_repeat = val == 2;
        let is_release = val == 0;

        if ((self.config.enable_long_press && is_letter(key)) || (self.config.enable_punctuation_long_press && get_punctuation_key(key, shift_pressed).is_some()))
            && !shift_pressed {
                if val == 1 {
                    self.dispatcher.key_press_info = Some((key, now));
                    self.dispatcher.long_press_triggered = false;
                } else if is_repeat {
                    if !self.dispatcher.long_press_triggered {
                        if let Some((press_key, press_time)) = self.dispatcher.key_press_info {
                            if press_key == key && now.duration_since(press_time) >= self.config.long_press_timeout {
                                if is_letter(key) {
                                    if let Some(c) = key_to_char(key, false) {
                                        if let Some(replacement) = self.config.long_press_mappings.get(&c.to_string()).cloned() {
                                            self.dispatcher.long_press_triggered = true;
                                            if !self.session.buffer.is_empty() {
                                                if let Some(last_char) = self.session.buffer.chars().last() {
                                                    if last_char.to_string() == c.to_string() {
                                                        self.session.buffer.pop();
                                                    }
                                                }
                                            }
                                            return Some(self.inject_text(&replacement));
                                        }
                                    }
                                } else if let Some(p_key) = get_punctuation_key(key, false) {
                                    if let Some(replacement) = self.config.punctuation_long_press_mappings.get(p_key).cloned() {
                                        self.dispatcher.long_press_triggered = true;
                                        let mut commit_text = if !self.session.joined_sentence.is_empty() { 
                                            self.session.joined_sentence.trim_end().to_string() 
                                        } else if !self.session.candidates.is_empty() { 
                                            self.session.candidates[0].text.trim_end().to_string() 
                                        } else { 
                                            self.session.buffer.trim_end().to_string() 
                                        };
                                        commit_text.push_str(&replacement);
                                        let del_len = self.session.phantom_text.chars().count();
                                        self.clear_composing();
                                        self.commit_history.clear(); 
                                        return Some(Action::DeleteAndEmit { delete: del_len, insert: commit_text });
                                    }
                                }
                            }
                        }
                    }
                    return Some(Action::Consume); 
                } else if is_release {
                    self.dispatcher.key_press_info = None;
                    if self.dispatcher.long_press_triggered {
                        return Some(Action::Consume); 
                    }
                }
        }
        None
    }

    fn process_switch_mode(&mut self, key: VirtualKey, is_press: bool, is_release: bool) -> Option<Action> {
        if !self.session.switch_mode { return None; }
        
        if is_press {
            match key {
                VirtualKey::Esc | VirtualKey::Space | VirtualKey::Enter => { 
                    self.session.switch_mode = false; 
                    return Some(Action::Notify("快捷切换".into(), "已退出".into())); 
                }
                VirtualKey::E => {
                    self.session.switch_mode = false;
                    if let Some((pinyin, word)) = self.commit_history.pop() {
                        let del_count = word.chars().count();
                        self.session.buffer = pinyin;
                        self.session.state = ImeState::Composing;
                        let _ = self.lookup();
                        return Some(Action::DeleteAndEmit { delete: del_count, insert: "".into() });
                    }
                    return Some(Action::Consume);
                }
                VirtualKey::Z => {
                    self.session.switch_mode = false;
                    if self.engine.trie_paths.contains_key("english") {
                        self.active_profiles = vec!["english".to_string()];
                        self.reset();
                        return Some(Action::Notify("英".into(), "已切换至英语方案".into()));
                    }
                    return Some(Action::Consume);
                }
                _ if is_letter(key) => {
                    let k = key_to_char(key, false).unwrap_or(' ').to_string();
                    let mut target_profile = None;
                    for (trigger_key, profile_name) in &self.config.profile_keys {
                        if trigger_key == &k { target_profile = Some(profile_name.clone()); break; }
                    }

                    if let Some(p_str) = target_profile {
                        let profiles: Vec<String> = p_str.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty() && self.engine.trie_paths.contains_key(s)).collect();
                        if !profiles.is_empty() {
                            self.active_profiles = profiles;
                            let display = self.get_current_profile_display();
                            let short_display = self.get_short_display();
                            let _ = self.lookup();
                            self.session.switch_mode = false;
                            return Some(Action::Notify(short_display, format!("方案: {}", display)));
                        } else {
                            self.session.switch_mode = false;
                            return Some(Action::Notify("❌".into(), format!("错误: 方案 [{}] 的词库未加载", p_str)));
                        }
                    }
                }
                _ => {} 
            }
            return Some(Action::Consume);
        }

        if is_release {
            return Some(Action::Consume);
        }
        
        None
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
