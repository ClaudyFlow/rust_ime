pub mod utils;
pub mod punctuation;
pub mod intents;
pub mod commands;
pub mod handlers;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Instant, Duration};

use crate::engine::keys::VirtualKey;
use crate::engine::scheme::InputScheme;
use crate::engine::{Command, ModifierState, InputEvent};
use crate::config::Config;

pub use utils::*;

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

pub struct Processor {
    pub session: crate::engine::InputSession,
    pub config: crate::engine::ConfigManager,
    pub dispatcher: crate::engine::KeyDispatcher,
    pub engine: crate::engine::pipeline::SearchEngine,
    
    pub active_profiles: Vec<String>,
    pub syllables: HashSet<String>,
    pub chinese_enabled: bool,
    
    // 连续选词记忆
    pub commit_history: Vec<(String, String)>, // 最近上屏的 (拼音, 词组)
    pub last_commit_time: Instant,
}

impl Processor {
    pub fn new(
        trie_paths: HashMap<String, (std::path::PathBuf, std::path::PathBuf)>, 
        syllables: HashSet<String>,
    ) -> Self {
        let config = crate::engine::ConfigManager::new();
        let syllables_arc = Arc::new(syllables.clone());
        
        let engine = crate::engine::pipeline::SearchEngine::new(
            trie_paths,
            syllables_arc,
            config.learned_words.clone(),
            config.usage_history.clone(),
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

    pub fn execute_command(&mut self, cmd: Command) -> Action {
        commands::execute_command(self, cmd)
    }

    pub fn apply_config(&mut self, conf: &Config) {
        self.config.apply_config(conf);
        self.engine.clear_cache();

        if !conf.input.active_profiles.is_empty() {
            self.active_profiles = conf.input.active_profiles.iter().map(|p| p.to_lowercase()).collect();
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

    pub fn handle_event(&mut self, event: InputEvent) -> Action {
        let span = tracing::info_span!("handle_event", ?event);
        let _enter = span.enter();
        match event {
            InputEvent::Key { key, val, shift, ctrl, alt } => {
                self.handle_key_ext(key, val, shift, ctrl, alt, true)
            }
            InputEvent::Voice(text) => {
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

        if let Some(action) = intents::process_modifiers(self, key, is_press, is_release) {
            return action;
        }

        if let Some(action) = intents::process_intent(self, key, val, shift_pressed, now) {
            return action;
        }

        if key == VirtualKey::Grave {
            return Action::PassThrough;
        }

        if let Some(action) = intents::process_switch_mode(self, key, is_press, is_release) {
            return action;
        }

        if !self.session.buffer.is_empty() { return self.handle_composing(key, shift_pressed, perform_lookup); }
        match self.session.state {
            ImeState::Direct => self.handle_direct(key, shift_pressed, perform_lookup),
            _ => self.handle_composing(key, shift_pressed, perform_lookup)
        }
    }

    pub fn handle_direct(&mut self, key: VirtualKey, shift_pressed: bool, perform_lookup: bool) -> Action {
        handlers::handle_direct(self, key, shift_pressed, perform_lookup)
    }

    pub fn handle_composing(&mut self, key: VirtualKey, shift_pressed: bool, perform_lookup: bool) -> Action {
        handlers::handle_composing(self, key, shift_pressed, perform_lookup)
    }

    pub fn handle_punctuation(&mut self, key: VirtualKey, shift_pressed: bool) -> Action {
        punctuation::handle_punctuation(self, key, shift_pressed)
    }

    pub fn commit_candidate(&mut self, mut cand: String, index: usize) -> Action {
        let now = Instant::now();
        let py = self.session.last_lookup_pinyin.clone();

        if !py.is_empty() && index != 99 {
            if now.duration_since(self.last_commit_time) > Duration::from_secs(3) {
                self.commit_history.clear();
            }
            self.commit_history.push((py.clone(), cand.clone()));
            self.record_usage(&py, &cand);

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
        if delete_count == 0 && insert_text.is_empty() { Action::Consume }
        else if delete_count == 0 { Action::Emit(insert_text) }
        else { Action::DeleteAndEmit { delete: delete_count, insert: insert_text } }
    }

    pub fn lookup(&mut self) -> Option<Action> { self.lookup_with_limit(20) }

    pub fn trigger_incremental_search(&mut self) {
        let current_len = self.session.candidates.len();
        if current_len >= 200 { return; }
        self.lookup_with_limit(current_len + 50);
    }

    pub fn lookup_with_limit(&mut self, limit: usize) -> Option<Action> {
        let span = tracing::debug_span!("lookup", buffer = %self.session.buffer, limit);
        let _enter = span.enter();
        if self.session.buffer.is_empty() { self.reset(); return None; }

        if self.session.filter_mode == FilterMode::Page && !self.session.page_snapshot.is_empty() {
            let mut filtered = Vec::new();
            for c in &self.session.page_snapshot {
                if self.engine.matches_filter(c, &self.session.aux_filter) { filtered.push(c.clone()); }
            }
            if !filtered.is_empty() {
                self.session.candidates = filtered;
                if self.session.candidates.len() == 1 { 
                    let word = self.session.candidates[0].text.clone(); 
                    return Some(self.commit_candidate(word, 0)); 
                }
            } else { self.session.candidates.clear(); }
            self.session.update_state();
            return None;
        }

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
        self.session.update_state();
        None
    }

    pub fn reset(&mut self) {
        self.session.reset();
        self.dispatcher.reset_states();
    }

    pub fn clear_composing(&mut self) { self.session.clear_composing(); }
    pub fn start_global_filter(&mut self) {
        if self.session.state == ImeState::Direct { return; }
        if self.session.filter_mode != FilterMode::Global {
            self.session.filter_mode = FilterMode::Global;
            self.session.aux_filter.clear();
        }
    }

    pub fn inject_text(&mut self, text: &str) -> Action {
        self.session.buffer.push_str(text);
        if self.session.state == ImeState::Direct { self.session.state = ImeState::Composing; }
        self.session.preview_selected_candidate = false;
        if let Some(act) = self.lookup() { return act; }
        if let Some(act) = self.check_auto_commit() { return act; }
        self.update_phantom_action()
    }

    pub fn get_short_display(&self) -> String {
        let display = self.get_current_profile_display();
        match display.to_lowercase().as_str() {
            "chinese" => "中".to_string(), "english" => "英".to_string(), "japanese" => "日".to_string(), "stroke" => "笔".to_string(), "mixed" => "混".to_string(),
            _ => { let mut chars = display.chars(); chars.next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string()) }
        }
    }

    pub fn get_current_profile_display(&self) -> String {
        if self.active_profiles.is_empty() { return "None".to_string(); }
        if self.active_profiles.len() == 1 { return self.active_profiles[0].clone(); }
        "Mixed".to_string()
    }

    pub fn check_auto_commit(&mut self) -> Option<Action> {
        if !self.config.auto_commit_unique_full_match || self.session.candidates.len() != 1 || !self.session.has_dict_match || self.session.state == ImeState::NoMatch { return None; }
        let raw_input = &self.session.buffer;
        let mut total_longer = 0;
        for p in &self.active_profiles { if self.engine.has_longer_match(p, raw_input) { total_longer += 1; break; } }
        if total_longer == 0 { return Some(self.commit_candidate(self.session.candidates[0].text.clone(), 0)); }
        None
    }

    pub fn should_block_invalid_input(&mut self, old_buffer: &str) -> bool {
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

    pub fn record_usage(&mut self, pinyin: &str, word: &str) {
        if pinyin.is_empty() || word.is_empty() { return; }
        if std::env::args().any(|a| a == "--test") { return; }
        
        let profile = self.active_profiles.first().cloned().unwrap_or_else(|| "chinese".to_string());
        let word_len = word.chars().count();

        // 1. 记录调频记录 (Usage History)
        if self.config.enable_auto_reorder {
            let mut hist_clone = (**self.config.usage_history.load()).clone();
            let profile_dict = hist_clone.entry(profile.clone()).or_default();
            let entries = profile_dict.entry(pinyin.to_string()).or_default();
            
            if let Some(pos) = entries.iter().position(|(w, _)| w == word) {
                entries[pos].1 += 1;
            } else {
                entries.push((word.to_string(), 1));
            }
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            
            self.config.usage_history.store(Arc::new(hist_clone));
            self.config.save_usage_history();
        }

        // 2. 记录造词记录 (Word Discovery / Learned Words)
        // 规则：字数 > 1 且 词库中搜不到该全拼匹配
        if self.config.enable_word_discovery && word_len > 1 {
            let is_new_word = !self.engine.has_exact_match(&profile, pinyin, word);
            
            if is_new_word {
                let mut learned_clone = (**self.config.learned_words.load()).clone();
                let profile_dict = learned_clone.entry(profile).or_default();
                let entries = profile_dict.entry(pinyin.to_string()).or_default();
                
                if let Some(pos) = entries.iter().position(|(w, _)| w == word) {
                    entries[pos].1 += 1;
                } else {
                    entries.push((word.to_string(), 1));
                }
                entries.sort_by(|a, b| b.1.cmp(&a.1));
                
                self.config.learned_words.store(Arc::new(learned_clone));
                self.config.save_learned_words();
            }
        }
    }
}
