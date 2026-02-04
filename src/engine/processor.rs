use std::collections::HashMap;
use evdev::Key;
use crate::engine::trie::Trie;
use serde_json::Value;

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

        Self {
            state: ImeState::Direct, buffer: String::new(), tries, 
            active_profiles: vec![initial_profile],
            punctuation, candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, 
            chinese_enabled: false, best_segmentation: vec![],
            joined_sentence: String::new(),
            show_candidates: true, show_modern_candidates: false, show_notifications: true, show_keystrokes: true,
            phantom_mode: PhantomMode::Pinyin,
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
        }
    }

    pub fn apply_config(&mut self, conf: &crate::config::Config) {
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
        
        if !conf.input.active_profiles.is_empty() {
            self.active_profiles = conf.input.active_profiles.iter().map(|p| p.to_lowercase()).collect();
        } else {
            let new_profile = conf.input.default_profile.to_lowercase();
            if !new_profile.is_empty() && self.tries.contains_key(&new_profile) {
                self.active_profiles = vec![new_profile];
            }
        }

        self.phantom_mode = match conf.appearance.preview_mode.as_str() {
            "pinyin" => PhantomMode::Pinyin,
            _ => PhantomMode::None,
        };
    }

    pub fn toggle(&mut self) -> Action {
        self.chinese_enabled = !self.chinese_enabled;
        self.reset();
        Action::Consume
    }

    pub fn inject_text(&mut self, text: &str) -> Action {
        self.buffer.push_str(text);
        if self.state == ImeState::Direct { self.state = ImeState::Composing; }
        self.preview_selected_candidate = false;
        self.lookup();
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
    }

    pub fn handle_key(&mut self, key: Key, is_press: bool, shift_pressed: bool) -> Action {
        if !is_press {
            if self.buffer.is_empty() { return Action::PassThrough; }
            if key == Key::KEY_GRAVE { return Action::Consume; }
            return Action::Consume;
        }

        if key == Key::KEY_GRAVE {
            self.switch_mode = !self.switch_mode;
            return if self.switch_mode { Action::Notify("快捷切换".into(), "已进入方案切换模式".into()) } else { Action::Notify("快捷切换".into(), "已退出".into()) };
        }

        if self.switch_mode {
            match key {
                Key::KEY_ESC | Key::KEY_SPACE | Key::KEY_ENTER => { self.switch_mode = false; return Action::Notify("快捷切换".into(), "已退出".into()); }
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
                            self.lookup();
                            self.switch_mode = false;
                            return Action::Notify("输入方案".into(), format!("已切换至: {}", display));
                        }
                    }
                }
                _ => {} // Do nothing for other keys in switch mode
            }
            return Action::Consume;
        }

        if !self.buffer.is_empty() { return self.handle_composing(key, shift_pressed); }
        match self.state {
            ImeState::Direct => self.handle_direct(key, shift_pressed),
            _ => self.handle_composing(key, shift_pressed)
        }
    }

    fn handle_direct(&mut self, key: Key, shift_pressed: bool) -> Action {
        if let Some(c) = key_to_char(key, shift_pressed) {
            let old_buffer = self.buffer.clone();
            self.buffer.push(c); 
            self.state = ImeState::Composing; 
            self.lookup();
            if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; self.lookup(); return Action::Alert; }
            self.update_phantom_action()
        } else if let Some(punc_key) = get_punctuation_key(key, shift_pressed) {
            if let Some(zh_puncs) = self.punctuation.get(punc_key) { if let Some(first) = zh_puncs.first() { return Action::Emit(first.clone()); } } // Use first punctuation if available
            Action::PassThrough
        } else { Action::PassThrough } 
    }

    fn handle_composing(&mut self, key: Key, shift_pressed: bool) -> Action {
        let has_cand = !self.candidates.is_empty();
        let styles = &self.page_flipping_styles;
        let flip_me = styles.contains(&"minus_equal".to_string());
        let flip_cd = styles.contains(&"comma_dot".to_string());
        let flip_arrow = styles.contains(&"arrow".to_string());

        match key {
            Key::KEY_BACKSPACE => {
                self.buffer.pop();
                if self.buffer.is_empty() { 
                    let del = self.phantom_text.chars().count(); self.reset(); 
                    if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
                } else { self.lookup(); self.update_phantom_action() }
            }
            // Page flipping logic
            Key::KEY_MINUS if flip_me && has_cand => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            Key::KEY_EQUAL if flip_me && has_cand => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }
            Key::KEY_COMMA if flip_cd && has_cand => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            Key::KEY_DOT if flip_cd && has_cand => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }
            
            // Arrow key logic (supports role swapping)
            Key::KEY_LEFT | Key::KEY_RIGHT | Key::KEY_UP | Key::KEY_DOWN => {
                let (move_prev, move_next, page_prev, page_next) = if self.swap_arrow_keys {
                    (Key::KEY_UP, Key::KEY_DOWN, Key::KEY_LEFT, Key::KEY_RIGHT)
                } else {
                    (Key::KEY_LEFT, Key::KEY_RIGHT, Key::KEY_UP, Key::KEY_DOWN)
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
                    // If not page flipping or candidate selection, try to input as character
                    if let Some(c) = key_to_char(key, shift_pressed) {
                        let old_buffer = self.buffer.clone(); self.buffer.push(c); self.preview_selected_candidate = false; self.lookup(); 
                        if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; self.lookup(); return Action::Alert; }
                        if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action() 
                    } else { self.handle_punctuation(key, shift_pressed) }
                }
            }

            Key::KEY_PAGEUP => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            Key::KEY_PAGEDOWN => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }
            Key::KEY_HOME => { if shift_pressed { self.selected = 0; self.page = 0; } else { self.selected = self.page; } Action::Consume }
            Key::KEY_END => { if has_cand { if shift_pressed { self.selected = self.candidates.len() - 1; self.page = (self.selected / self.page_size) * self.page_size; } else { self.selected = (self.page + self.page_size - 1).min(self.candidates.len() - 1); } } Action::Consume }
            
            Key::KEY_SPACE => {
                if self.preview_selected_candidate || self.commit_mode == "single" { if let Some(word) = self.candidates.get(self.selected) { return self.commit_candidate(word.clone()); } }
                if self.buffer.ends_with(' ') && !self.joined_sentence.is_empty() { return self.commit_candidate(self.joined_sentence.clone()); }
                self.buffer.push(' '); self.preview_selected_candidate = false; self.lookup(); self.update_phantom_action()
            }
            Key::KEY_ENTER => {
                if self.commit_mode == "single" { let out = self.buffer.clone(); return self.commit_candidate(out); }
                if self.preview_selected_candidate { if let Some(word) = self.candidates.get(self.selected) { return self.commit_candidate(word.clone()); } }
                if !self.joined_sentence.is_empty() { self.commit_candidate(self.joined_sentence.clone()) } else { let out = self.buffer.clone(); self.commit_candidate(out) }
            }
            Key::KEY_ESC | Key::KEY_DELETE => { let del = self.phantom_text.chars().count(); self.reset(); if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume } }
            
            _ if is_digit(key) => {
                let digit = key_to_digit(key).unwrap_or(0);
                if self.commit_mode == "single" && digit >= 1 && digit <= self.page_size { let abs_idx = self.page + digit - 1; if let Some(word) = self.candidates.get(abs_idx) { return self.commit_candidate(word.clone()); } }
                let old_buffer = self.buffer.clone(); self.buffer.push_str(&digit.to_string()); self.lookup();
                if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; self.lookup(); return Action::Alert; }
                if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action()
            }
            // Fallback: all keys recognized as characters
            _ if let Some(c) = key_to_char(key, shift_pressed) => {
                let old_buffer = self.buffer.clone(); self.buffer.push(c); self.preview_selected_candidate = false; self.lookup(); 
                if self.enable_anti_typo && !self.has_dict_match { self.buffer = old_buffer; self.lookup(); return Action::Alert; }
                if let Some(act) = self.check_auto_commit() { return act; } self.update_phantom_action() 
            }
            _ if get_punctuation_key(key, shift_pressed).is_some() => { self.handle_punctuation(key, shift_pressed) }
            _ => Action::PassThrough,
        }
    }

    fn handle_punctuation(&mut self, key: Key, shift_pressed: bool) -> Action {
        let punc_key = get_punctuation_key(key, shift_pressed).unwrap();
        let zh_punc = self.punctuation.get(punc_key).and_then(|v| v.first()).cloned().unwrap_or_else(|| punc_key.to_string());
        let mut commit_text = if !self.joined_sentence.is_empty() { self.joined_sentence.clone() } else if !self.candidates.is_empty() { self.candidates[0].clone() } else { self.buffer.clone() };
        commit_text.push_str(&zh_punc);
        let del_len = self.phantom_text.chars().count();
        self.reset();
        Action::DeleteAndEmit { delete: del_len, insert: commit_text }
    }

    fn commit_candidate(&mut self, mut cand: String) -> Action {
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

    fn lookup_part(&self, dict: &Trie, part: &ParsedPart) -> Vec<(String, String, String)> {
        let mut pool = Vec::new(); let mut seen = std::collections::HashSet::new();
        if let Some(matches) = dict.get_all_exact(&part.pinyin) { for m in matches { if seen.insert(m.0.clone()) { pool.push(m); } } }
        if self.enable_abbreviation_matching && part.pinyin.len() <= 4 { if let Some(abbrs) = dict.get_all_abbrev(&part.pinyin) { for m in abbrs { if seen.insert(m.0.clone()) { pool.push(m); } } } }
        if self.enable_prefix_matching && !part.pinyin.is_empty() { let limit = if part.aux_code.is_some() { 50 } else { 20 }; let prefix_matches = dict.search_bfs(&part.pinyin, limit); for m in prefix_matches { if seen.insert(m.0.clone()) { pool.push(m); } } }
        if let Some(ref code) = part.aux_code {
            let is_single_upper = code.len() == 1 && code.chars().next().unwrap().is_ascii_uppercase();
            let code_lower = code.to_lowercase();
            pool.retain(|(_, _, en)| { if is_single_upper { en.to_lowercase().starts_with(&code_lower) } else { en.to_lowercase().contains(&code_lower) } });
        }
        pool
    }

    pub fn lookup(&mut self) {
        if self.buffer.is_empty() { self.reset(); return; }
        let parsed_parts = self.parse_buffer();
        let mut greedy_sentence = String::new(); let mut all_raw_segments = Vec::new(); let mut last_matches: Vec<(String, String, String)> = Vec::new();
        for (i, part) in parsed_parts.iter().enumerate() {
            all_raw_segments.push(part.raw.clone());
            let mut combined_matches = Vec::new(); let mut seen = std::collections::HashSet::new();
            for profile in &self.active_profiles { if let Some(d) = self.tries.get(profile) { for m in self.lookup_part(d, part) { if seen.insert(m.0.clone()) { combined_matches.push(m); } } } }
            let idx = part.specified_idx.unwrap_or(1).saturating_sub(1);
            if let Some((w, _, _)) = combined_matches.get(idx) { greedy_sentence.push_str(w); } else { greedy_sentence.push_str(&part.raw); }
            if i == parsed_parts.len() - 1 { last_matches = combined_matches; }
        }
        self.joined_sentence = greedy_sentence; self.best_segmentation = all_raw_segments;
        self.candidates.clear(); self.candidate_hints.clear(); self.has_dict_match = !last_matches.is_empty();
        for (cand, tone, en) in last_matches {
            self.candidates.push(cand);
            let mut h = String::new();
            if self.show_tone_hint && !tone.is_empty() { h.push_str(&tone); }
            if self.show_en_hint && !en.is_empty() { if !h.is_empty() { h.push(' '); } h.push_str(&en); }
            self.candidate_hints.push(h);
        }
        if self.candidates.is_empty() { self.candidates.push(self.buffer.clone()); self.candidate_hints.push(String::new()); }
        self.selected = 0; self.page = 0; self.update_state();
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
            if let Some(d) = self.tries.get(p) {
                if d.has_longer_match(raw_input) { total_longer += 1; break; }
            }
        }
        if total_longer == 0 { return Some(self.commit_candidate(self.candidates[0].clone())); }
        None
    }
}

pub fn is_letter(key: Key) -> bool { key_to_char(key, false).is_some() }
pub fn is_digit(key: Key) -> bool { matches!(key, Key::KEY_1 | Key::KEY_2 | Key::KEY_3 | Key::KEY_4 | Key::KEY_5 | Key::KEY_6 | Key::KEY_7 | Key::KEY_8 | Key::KEY_9 | Key::KEY_0) }
pub fn key_to_digit(key: Key) -> Option<usize> { match key { Key::KEY_1 => Some(1), Key::KEY_2 => Some(2), Key::KEY_3 => Some(3), Key::KEY_4 => Some(4), Key::KEY_5 => Some(5), Key::KEY_6 => Some(6), Key::KEY_7 => Some(7), Key::KEY_8 => Some(8), Key::KEY_9 => Some(9), Key::KEY_0 => Some(0), _ => None } }
pub fn key_to_char(key: Key, shift: bool) -> Option<char> {
    let c = match key {
        Key::KEY_Q => Some('q'), Key::KEY_W => Some('w'), Key::KEY_E => Some('e'), Key::KEY_R => Some('r'), Key::KEY_T => Some('t'), Key::KEY_Y => Some('y'), Key::KEY_U => Some('u'), Key::KEY_I => Some('i'), Key::KEY_O => Some('o'), Key::KEY_P => Some('p'), Key::KEY_A => Some('a'), Key::KEY_S => Some('s'), Key::KEY_D => Some('d'), Key::KEY_F => Some('f'), Key::KEY_G => Some('g'), Key::KEY_H => Some('h'), Key::KEY_J => Some('j'), Key::KEY_K => Some('k'), Key::KEY_L => Some('l'), Key::KEY_Z => Some('z'), Key::KEY_X => Some('x'), Key::KEY_C => Some('c'), Key::KEY_V => Some('v'), Key::KEY_B => Some('b'), Key::KEY_N => Some('n'), Key::KEY_M => Some('m'), Key::KEY_APOSTROPHE => Some('\''), Key::KEY_SLASH => Some('/'),
        Key::KEY_MINUS => Some('-'), Key::KEY_EQUAL => Some('='), Key::KEY_COMMA => Some(','), Key::KEY_DOT => Some('.'),
        _ => None
    };
    if shift { c.map(|ch| ch.to_ascii_uppercase()) } else { c }
}
fn get_punctuation_key(key: Key, shift: bool) -> Option<&'static str> {
    match (key, shift) { (Key::KEY_GRAVE, false) => Some("`"), (Key::KEY_GRAVE, true) => Some("~"), (Key::KEY_MINUS, false) => Some("-"), (Key::KEY_MINUS, true) => Some("_"), (Key::KEY_EQUAL, false) => Some("="), (Key::KEY_EQUAL, true) => Some("+"), (Key::KEY_LEFTBRACE, false) => Some("["), (Key::KEY_LEFTBRACE, true) => Some("{"), (Key::KEY_RIGHTBRACE, false) => Some("]"), (Key::KEY_RIGHTBRACE, true) => Some("}"), (Key::KEY_BACKSLASH, false) => Some("\\"), (Key::KEY_BACKSLASH, true) => Some("|"), (Key::KEY_SEMICOLON, false) => Some(";"), (Key::KEY_SEMICOLON, true) => Some(":"), (Key::KEY_APOSTROPHE, false) => Some("'"), (Key::KEY_APOSTROPHE, true) => Some("\""), (Key::KEY_COMMA, false) => Some(","), (Key::KEY_COMMA, true) => Some("<"), (Key::KEY_DOT, false) => Some("."), (Key::KEY_DOT, true) => Some(">"), (Key::KEY_SLASH, false) => Some("/"), (Key::KEY_SLASH, true) => Some("?"), (Key::KEY_1, true) => Some("!"), (Key::KEY_2, true) => Some("@"), (Key::KEY_3, true) => Some("#"), (Key::KEY_4, true) => Some("$"), (Key::KEY_5, true) => Some("%"), (Key::KEY_6, true) => Some("^"), (Key::KEY_7, true) => Some("&"), (Key::KEY_8, true) => Some("*"), (Key::KEY_9, true) => Some("("), (Key::KEY_0, true) => Some(")"), _ => None } }
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
            preview_selected_candidate: false, enable_anti_typo: true, commit_mode: "double".to_string(), switch_mode: false, cursor_pos: 0, profile_keys: Vec::new(),
            auto_commit_unique_en_fuzhuma: false, auto_commit_unique_full_match: false, enable_prefix_matching: true, prefix_matching_limit: 20, enable_abbreviation_matching: true, filter_proper_nouns_by_case: true, enable_error_sound: true, has_dict_match: false, page_size: 5, show_tone_hint: false, show_en_hint: true, page_flipping_styles: vec!["arrow".to_string()], swap_arrow_keys: false,
        }
    }
    #[test] fn test_dummy() { let _p = setup_mock_processor(); }
}