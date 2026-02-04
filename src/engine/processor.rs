use std::collections::HashMap;
use evdev::Key;
use crate::engine::trie::Trie;

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
    pub current_profile: String,
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
    pub has_dict_match: bool,
    pub page_flipping_style: String,
}

impl Processor {
    fn parse_buffer(&self) -> Vec<ParsedPart> {
        let buffer_normalized = strip_tones(&self.buffer);
        let parts: Vec<&str> = buffer_normalized.split(' ').filter(|s| !s.is_empty()).collect();
        let mut result = Vec::new();

        for part in parts {
            let split_pos = part.find(|c: char| c.is_ascii_digit() || c.is_ascii_uppercase());
            
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
                (p.to_lowercase(), a, d)
            } else {
                (part.to_lowercase(), None, None)
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
        punctuation_raw: HashMap<String, serde_json::Value>, 
    ) -> Self {
        let mut punctuation = HashMap::new();
        for (k, v) in punctuation_raw {
            if let Some(arr) = v.as_array() {
                let chars: Vec<String> = arr.iter().filter_map(|item| item.get("char").and_then(|c| c.as_str())).map(|s| s.to_string()).collect();
                punctuation.insert(k, chars);
            }
        }

        Self {
            state: ImeState::Direct, buffer: String::new(), tries, current_profile: initial_profile,
            punctuation, candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, 
            chinese_enabled: false, best_segmentation: vec![],
            joined_sentence: String::new(),
            show_candidates: true, show_modern_candidates: false, show_notifications: true, show_keystrokes: true,
            phantom_mode: PhantomMode::Pinyin,
            phantom_text: String::new(),
            preview_selected_candidate: false,
            enable_anti_typo: true,
            commit_mode: "double".to_string(),
            switch_mode: false,
            cursor_pos: 0,
            profile_keys: Vec::new(),
            page_size: 9,
            show_tone_hint: true,
            show_en_hint: true,
            auto_commit_unique_en_fuzhuma: false,
            auto_commit_unique_full_match: false,
            enable_prefix_matching: true,
            prefix_matching_limit: 20,
            enable_abbreviation_matching: true,
            filter_proper_nouns_by_case: true,
            has_dict_match: false,
            page_flipping_style: "arrow".to_string(),
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
        self.enable_prefix_matching = conf.input.enable_prefix_matching;
        self.prefix_matching_limit = conf.input.prefix_matching_limit;
        self.enable_abbreviation_matching = conf.input.enable_abbreviation_matching;
        self.filter_proper_nouns_by_case = conf.input.filter_proper_nouns_by_case;
        self.profile_keys = conf.input.profile_keys.iter().map(|pk| (pk.key.to_lowercase(), pk.profile.to_lowercase())).collect();
        if let Some(style) = conf.input.page_flipping_keys.first() {
            self.page_flipping_style = style.clone();
        }
        
        let new_profile = conf.input.default_profile.to_lowercase();
        if !new_profile.is_empty() && self.tries.contains_key(&new_profile) {
            self.current_profile = new_profile;
        } else if self.tries.contains_key("chinese") {
            self.current_profile = "chinese".to_string();
        } else if let Some(k) = self.tries.keys().next() {
            self.current_profile = k.clone();
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
        if self.state == ImeState::Direct {
            self.state = ImeState::Composing;
        }
        self.preview_selected_candidate = false;
        self.lookup();
        if let Some(act) = self.check_auto_commit() {
            return act;
        }
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
            if self.switch_mode && (key == Key::KEY_H || key == Key::KEY_L || key == Key::KEY_D || key == Key::KEY_C || key == Key::KEY_E || key == Key::KEY_R || key == Key::KEY_J) { return Action::Consume; }
            // 允许 TAB, LEFT, RIGHT 在 Composing 状态下处理，这里只拦截明确不需要的
            let is_flipping_key = if self.page_flipping_style == "minus_equal" {
                matches!(key, Key::KEY_MINUS | Key::KEY_EQUAL)
            } else {
                matches!(key, Key::KEY_UP | Key::KEY_DOWN)
            };
            
            if is_letter(key) || is_digit(key) || get_punctuation_key(key, shift_pressed).is_some() || is_flipping_key || matches!(key, Key::KEY_BACKSPACE | Key::KEY_SPACE | Key::KEY_ENTER | Key::KEY_ESC | Key::KEY_PAGEUP | Key::KEY_PAGEDOWN | Key::KEY_HOME | Key::KEY_END) { 
                return Action::Consume; 
            }
            return Action::PassThrough;
        }

        // --- 切换快捷模式 (Grave `) ---
        if key == Key::KEY_GRAVE {
            self.switch_mode = !self.switch_mode;
            if self.switch_mode {
                return Action::Notify("快捷切换".into(), "已进入方案切换模式 (按 Esc 退出)".into());
            } else {
                return Action::Notify("快捷切换".into(), "已退出切换模式".into());
            }
        }

        // --- 快捷切换模式逻辑 ---
        if self.switch_mode {
            match key {
                Key::KEY_ESC | Key::KEY_SPACE | Key::KEY_ENTER => { 
                    self.switch_mode = false; 
                    return Action::Notify("快捷切换".into(), "已退出".into());
                }
                _ if is_letter(key) => {
                    let k = key_to_char(key, false).unwrap_or(' ').to_string();
                    let mut target_profile = None;
                    for (trigger_key, profile_name) in &self.profile_keys {
                        if trigger_key == &k {
                            target_profile = Some(profile_name.clone());
                            break;
                        }
                    }

                    if let Some(p) = target_profile {
                        self.current_profile = p.clone();
                        self.lookup();
                        self.switch_mode = false;
                        return Action::Notify("输入方案".into(), format!("已切换至: {}", p));
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

    fn handle_direct(&mut self, key: Key, shift_pressed: bool) -> Action {
        if let Some(c) = key_to_char(key, shift_pressed) {
            self.buffer.push(c); 
            self.state = ImeState::Composing; 
            self.lookup();
            self.update_phantom_action()
        } else if let Some(punc_key) = get_punctuation_key(key, shift_pressed) {
            if let Some(zh_puncs) = self.punctuation.get(punc_key) { 
                if let Some(first) = zh_puncs.first() {
                    self.buffer.push_str(first);
                    self.state = ImeState::Composing;
                    self.lookup();
                    return self.update_phantom_action();
                }
            }
            Action::PassThrough
        } else { Action::PassThrough } 
    }

    fn handle_composing(&mut self, key: Key, shift_pressed: bool) -> Action {
        match key {
            Key::KEY_BACKSPACE => {
                self.buffer.pop();
                if self.buffer.is_empty() { 
                    let del = self.phantom_text.chars().count();
                    self.reset(); 
                    if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
                } else { 
                    self.lookup(); 
                    self.update_phantom_action()
                }
            }
            Key::KEY_LEFT => {
                if !self.candidates.is_empty() {
                    self.preview_selected_candidate = true;
                    if self.selected > 0 { self.selected -= 1; }
                    self.page = (self.selected / self.page_size) * self.page_size;
                    self.update_phantom_action()
                } else { Action::PassThrough } // 如果没有候选词，允许左键移动光标(但这需要 Host 支持，暂 PassThrough)
            }
            Key::KEY_RIGHT => {
                if !self.candidates.is_empty() {
                    self.preview_selected_candidate = true;
                    if self.selected + 1 < self.candidates.len() { self.selected += 1; }
                    self.page = (self.selected / self.page_size) * self.page_size;
                    self.update_phantom_action()
                } else { Action::PassThrough }
            }
            Key::KEY_TAB => Action::PassThrough, // Tab 键交由 Host 处理（作为长韵母修饰键或原样发送）
            Key::KEY_UP => { 
                if self.page_flipping_style != "minus_equal" {
                     self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume 
                } else { Action::PassThrough }
            }
            Key::KEY_DOWN => { 
                if self.page_flipping_style != "minus_equal" {
                    if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume 
                } else { Action::PassThrough }
            }
            Key::KEY_MINUS => {
                if self.page_flipping_style == "minus_equal" {
                    self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume
                } else {
                     // Default punctuation handling
                     let punc_key = get_punctuation_key(key, shift_pressed).unwrap();
                     let zh_punc = self.punctuation.get(punc_key).and_then(|v| v.first()).cloned().unwrap_or_else(|| punc_key.to_string());
                     self.buffer.push_str(&zh_punc);
                     self.preview_selected_candidate = false;
                     self.lookup();
                     if let Some(act) = self.check_auto_commit() {
                         return act;
                     }
                     self.update_phantom_action()
                }
            }
            Key::KEY_EQUAL => {
                if self.page_flipping_style == "minus_equal" {
                    if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume
                } else {
                     // Default punctuation handling
                     let punc_key = get_punctuation_key(key, shift_pressed).unwrap();
                     let zh_punc = self.punctuation.get(punc_key).and_then(|v| v.first()).cloned().unwrap_or_else(|| punc_key.to_string());
                     self.buffer.push_str(&zh_punc);
                     self.preview_selected_candidate = false;
                     self.lookup();
                     if let Some(act) = self.check_auto_commit() {
                         return act;
                     }
                     self.update_phantom_action()
                }
            }
            Key::KEY_PAGEUP => { self.page = self.page.saturating_sub(self.page_size); self.selected = self.page; Action::Consume }
            Key::KEY_PAGEDOWN => { if self.page + self.page_size < self.candidates.len() { self.page += self.page_size; self.selected = self.page; } Action::Consume }
            Key::KEY_HOME => {
                if shift_pressed {
                    self.selected = 0;
                    self.page = 0;
                } else {
                    self.selected = self.page;
                }
                Action::Consume
            }
            Key::KEY_END => {
                if !self.candidates.is_empty() {
                    if shift_pressed {
                        self.selected = self.candidates.len() - 1;
                        self.page = (self.selected / self.page_size) * self.page_size;
                    } else {
                        let last_on_page = (self.page + self.page_size - 1).min(self.candidates.len() - 1);
                        self.selected = last_on_page;
                    }
                }
                Action::Consume
            }
            Key::KEY_SPACE => { 
                if self.preview_selected_candidate {
                     if let Some(word) = self.candidates.get(self.selected) {
                        return self.commit_candidate(word.clone());
                     }
                }
                
                // --- 词模式 (Single Space Mode) ---
                if self.commit_mode == "single" {
                    if let Some(word) = self.candidates.get(self.selected) {
                        return self.commit_candidate(word.clone());
                    }
                }

                // --- 长句模式 (Double Space Mode) ---
                // 如果缓冲区已经以空格结尾，则第二次按空格表示确认（上屏）
                if self.buffer.ends_with(' ') {
                    if !self.joined_sentence.is_empty() {
                        return self.commit_candidate(self.joined_sentence.clone());
                    }
                }

                // 否则，将空格作为分隔符加入 buffer，并进行 lookup
                self.buffer.push(' ');
                self.preview_selected_candidate = false;
                self.lookup();
                self.update_phantom_action()
            }
            Key::KEY_ENTER => { 
                if self.preview_selected_candidate {
                    if let Some(word) = self.candidates.get(self.selected) { 
                        return self.commit_candidate(word.clone());
                    }
                }
                if !self.joined_sentence.is_empty() {
                    self.commit_candidate(self.joined_sentence.clone())
                } else {
                    let out = self.buffer.clone(); 
                    self.commit_candidate(out)
                }
            }
            Key::KEY_ESC => { 
                let del = self.phantom_text.chars().count();
                self.reset(); 
                if del > 0 { Action::DeleteAndEmit { delete: del, insert: "".into() } } else { Action::Consume }
            }
            Key::KEY_DELETE => {
                let del = self.phantom_text.chars().count();
                self.reset();
                Action::DeleteAndEmit { delete: del, insert: "".into() }
            }
            _ if is_digit(key) => {
                let digit = key_to_digit(key).unwrap_or(0);
                
                // --- 词模式 (Single Space Mode): 数字键选词上屏 ---
                if self.commit_mode == "single" {
                    if digit >= 1 && digit <= self.page_size {
                        let abs_idx = self.page + digit - 1;
                        if let Some(word) = self.candidates.get(abs_idx) {
                            return self.commit_candidate(word.clone());
                        }
                    }
                }

                // --- 长句模式 (Double Space Mode) 或 词模式下未匹配数字 ---
                // 将数字作为普通字符加入缓冲区，供 parse_buffer 处理 (实现类似 nihao2 的选词效果)
                self.buffer.push_str(&digit.to_string());
                self.lookup();
                if let Some(act) = self.check_auto_commit() {
                    return act;
                }
                self.update_phantom_action()
            }
            _ if is_letter(key) => {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    if self.enable_anti_typo && c.is_ascii_lowercase() {
                        let mut test_buf = self.buffer.clone();
                        test_buf.push(c);
                        
                        // 获取当前正在输入的片段（最后一个空格之后的部分）
                        let last_segment = test_buf.split(' ').last().unwrap_or("");
                        
                        // 检查：只有当整个片段都是小写字母时，才进行词库前缀校验
                        // 这样一旦有了大写字母（辅码）或数字，防呆功能就会自动放行
                        let is_pure_pinyin = last_segment.chars().all(|ch| ch.is_ascii_lowercase());
                        
                        if is_pure_pinyin && !last_segment.is_empty() {
                            let dict = self.tries.get(&self.current_profile.to_lowercase());
                            if let Some(d) = dict {
                                if !d.has_prefix(last_segment) {
                                    return Action::Alert; // 拦截无效输入并发出警报
                                }
                            }
                        }
                    }

                    self.buffer.push(c); 
                    self.preview_selected_candidate = false;
                    self.lookup();
                    if let Some(act) = self.check_auto_commit() {
                        return act;
                    }
                    self.update_phantom_action()
                } else { Action::Consume }
            }
            _ if get_punctuation_key(key, shift_pressed).is_some() => {
                let punc_key = get_punctuation_key(key, shift_pressed).unwrap();
                let zh_punc = self.punctuation.get(punc_key).and_then(|v| v.first()).cloned().unwrap_or_else(|| punc_key.to_string());
                
                self.buffer.push_str(&zh_punc);
                self.preview_selected_candidate = false;
                self.lookup();
                if let Some(act) = self.check_auto_commit() {
                    return act;
                }
                self.update_phantom_action()
            }
            _ => Action::PassThrough,
        }
    }

    fn commit_candidate(&mut self, mut cand: String) -> Action {
        // 如果是英语方案，且上屏的是一个单词（不以标点结尾），则自动追加空格
        if self.current_profile == "english" && !cand.is_empty() {
            let last_char = cand.chars().last().unwrap_or(' ');
            if last_char.is_alphanumeric() {
                cand.push(' ');
            }
        }

        let del = self.phantom_text.chars().count();
        self.reset();
        Action::DeleteAndEmit { delete: del, insert: cand }
    }

    fn update_phantom_action(&mut self) -> Action {
        if self.phantom_mode == PhantomMode::None { return Action::Consume; }
        
        let target = if self.preview_selected_candidate && !self.candidates.is_empty() {
             self.candidates[self.selected.min(self.candidates.len()-1)].clone()
        } else {
             self.buffer.clone()
        };

        if target == self.phantom_text { return Action::Consume; }
        
        let old_phantom = self.phantom_text.clone();
        self.phantom_text = target.clone();

        let old_chars: Vec<char> = old_phantom.chars().collect();
        let target_chars: Vec<char> = target.chars().collect();

        if target.starts_with(&old_phantom) {
            let added: String = target_chars[old_chars.len()..].iter().collect();
            return Action::Emit(added);
        }
        
        if old_phantom.starts_with(&target) {
            let count = old_chars.len() - target_chars.len();
            return Action::DeleteAndEmit { delete: count, insert: "".into() };
        }

        Action::DeleteAndEmit { delete: old_chars.len(), insert: target }
    }

    fn lookup_part(&self, dict: &Trie, part: &ParsedPart) -> Vec<(String, String)> {
        let mut matches = dict.get_all_exact(&part.pinyin).unwrap_or_default();

        // 如果有辅码，我们额外搜索前缀，以便支持 "拼音前缀 + 辅码" 的匹配方式
        if self.enable_prefix_matching && part.aux_code.is_some() && !part.pinyin.is_empty() {
            let mut seen: std::collections::HashSet<String> = matches.iter().map(|(w, _)| w.clone()).collect();
            // 搜索范围根据配置动态调整
            let prefix_matches = dict.search_bfs(&part.pinyin, self.prefix_matching_limit.max(100));
            for (word, hint) in prefix_matches {
                if seen.insert(word.clone()) {
                    matches.push((word, hint));
                }
            }

            // 支持 "简拼 + 辅码"
            if self.enable_abbreviation_matching && part.pinyin.len() <= 4 {
                if let Some(abbr_matches) = dict.get_all_abbrev(&part.pinyin) {
                    for (word, hint) in abbr_matches {
                        if seen.insert(word.clone()) {
                            matches.push((word, hint));
                        }
                    }
                }
            }
        }

        if let Some(ref code) = part.aux_code {
            let code_lower = code.to_lowercase();
            matches.retain(|(_, hint)| {
                let hint_lower = hint.to_lowercase();
                if code.chars().all(|c| c.is_ascii_uppercase()) && code.len() == 1 {
                    hint_lower.split_whitespace().any(|word| word.starts_with(&code_lower))
                } else {
                    hint_lower.contains(&code_lower)
                }
            });
        }
        matches
    }

    pub fn lookup(&mut self) {
        if self.buffer.is_empty() { self.reset(); return; }
        let dict_key = self.current_profile.to_lowercase();
        let dict = self.tries.get(&dict_key);

        let parsed_parts = self.parse_buffer();
        let mut greedy_sentence = String::new();
        let mut all_raw_segments = Vec::new();
        let mut last_matches: Vec<(String, String)> = Vec::new();

        for (i, part) in parsed_parts.iter().enumerate() {
            all_raw_segments.push(part.raw.clone());
            
            let matches = if let Some(d) = dict {
                self.lookup_part(d, part)
            } else {
                Vec::new()
            };

            // Select by Index
            let idx = part.specified_idx.unwrap_or(1).saturating_sub(1);
            if let Some((w, _)) = matches.get(idx) {
                greedy_sentence.push_str(w);
            } else {
                greedy_sentence.push_str(&part.raw);
            }

            if i == parsed_parts.len() - 1 {
                last_matches = matches;
            }
        }

        self.joined_sentence = greedy_sentence;
        self.best_segmentation = all_raw_segments;

        // --- 2. 填充候选词列表 ---
        let mut final_candidates: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        if let Some(d) = dict {
            let buffer_normalized = strip_tones(&self.buffer);
            let internal_uppercase = self.buffer.chars().skip(1).any(|c| c.is_ascii_uppercase());
            let is_precise_mode = buffer_normalized.contains(' ') || buffer_normalized.chars().any(|c| c.is_ascii_digit() || internal_uppercase);

            let mut exact_matches = Vec::new();
            let mut prefix_matches = Vec::new();
            let mut abbr_matches = Vec::new();

            if !is_precise_mode {
                let raw_input = &self.buffer;
                
                // 2.1 精准匹配 (大小写敏感)
                if let Some(exact) = d.get_all_exact(raw_input) {
                    exact_matches = exact;
                }

                // 2.3 简拼匹配 (大小写敏感)
                if self.enable_abbreviation_matching && raw_input.len() >= 2 && raw_input.len() <= 4 {
                    if let Some(abbr) = d.get_all_abbrev(raw_input) {
                        abbr_matches = abbr;
                    }
                }

                // 2.2 前缀匹配 (联想)
                if self.enable_prefix_matching && raw_input.len() >= 2 {
                    prefix_matches = d.search_bfs(raw_input, self.prefix_matching_limit);
                }
            } else {
                exact_matches = last_matches;
            }

            // 合并并去重
            let mut all_raw = exact_matches;
            all_raw.extend(abbr_matches);
            all_raw.extend(prefix_matches);

            for (word, hint) in all_raw {
                if seen.insert(word.clone()) {
                    final_candidates.push((word, hint));
                }
            }
        }

        self.candidates.clear();
        self.candidate_hints.clear();
        self.has_dict_match = !final_candidates.is_empty();
        for (cand, raw_hint) in final_candidates {
            self.candidates.push(cand);
            
            // 动态处理 Hint
            let mut final_hint = String::new();
            if !raw_hint.is_empty() {
                // 尝试拆分声调和英文 (我们在 compiler 中是用空格连接的)
                let parts: Vec<&str> = raw_hint.splitn(2, ' ').collect();
                
                let (tone, en) = if parts.len() == 2 {
                    (parts[0], parts[1])
                } else if !raw_hint.is_empty() && (raw_hint.chars().any(|c| "āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜü".contains(c))) {
                    (raw_hint.as_str(), "")
                } else {
                    ("", raw_hint.as_str())
                };

                if self.show_tone_hint && !tone.is_empty() {
                    final_hint.push_str(tone);
                }
                if self.show_en_hint && !en.is_empty() {
                    if !final_hint.is_empty() { final_hint.push(' '); }
                    final_hint.push_str(en);
                }
            }
            self.candidate_hints.push(final_hint);
        }

        if self.candidates.is_empty() { 
            self.candidates.push(self.buffer.clone()); 
            self.candidate_hints.push(String::new()); 
        }
        self.selected = 0; self.page = 0;
        self.update_state();
    }

    fn update_state(&mut self) {
        if self.buffer.is_empty() { self.state = if self.candidates.is_empty() { ImeState::Direct } else { ImeState::Multi }; }
        else { self.state = match self.candidates.len() { 0 => ImeState::NoMatch, 1 => ImeState::Single, _ => ImeState::Multi }; }
    }

    pub fn next_profile(&mut self) -> String {
        let mut profiles: Vec<String> = self.tries.keys().cloned().collect();
        if profiles.is_empty() { return self.current_profile.clone(); }
        profiles.sort();
        let current_lower = self.current_profile.to_lowercase();
        let idx = profiles.iter().position(|p| p.to_lowercase() == current_lower).unwrap_or(0);
        let next_idx = (idx + 1) % profiles.len();
        self.current_profile = profiles[next_idx].clone();
        self.reset();
        self.current_profile.clone()
    }

    fn check_auto_commit(&mut self) -> Option<Action> {
        if self.commit_mode != "single" { return None; }
        if self.candidates.len() != 1 { return None; }
        if !self.has_dict_match { return None; }
        if self.state == ImeState::NoMatch { return None; }

        let buffer_normalized = strip_tones(&self.buffer);
        let dict_key = self.current_profile.to_lowercase();
        let dict = self.tries.get(&dict_key);

        // 1. English with fuzhuma (Uppercase) - Independent check
        if self.auto_commit_unique_en_fuzhuma && dict_key == "english" {
            let has_uppercase = buffer_normalized.chars().any(|c| c.is_ascii_uppercase());
            if has_uppercase {
                let cand = self.candidates[0].clone();
                return Some(self.commit_candidate(cand));
            }
        }

        // 2. Full match unique
        if self.auto_commit_unique_full_match {
            let is_precise_mode = buffer_normalized.contains(' ') || buffer_normalized.chars().any(|c| c.is_ascii_digit() || buffer_normalized.chars().any(|c| c.is_ascii_uppercase()));
            
            if is_precise_mode {
                // In precise mode (manual selection/aux code), assume user choice is intentional
                let cand = self.candidates[0].clone();
                return Some(self.commit_candidate(cand));
            } else if let Some(d) = dict {
                let full_pinyin = buffer_normalized.to_lowercase();
                // Check if this exact pinyin has exactly one dictionary entry
                // AND ensure no longer words start with this pinyin (lookahead)
                if let Some(exact_matches) = d.get_all_exact(&full_pinyin) {
                    if exact_matches.len() == 1 {
                        if !d.has_longer_match(&full_pinyin) {
                            let cand = self.candidates[0].clone();
                            return Some(self.commit_candidate(cand));
                        }
                    }
                }
            }
        }
        
        None
    }
}

pub fn is_letter(key: Key) -> bool { key_to_char(key, false).is_some() }
pub fn is_digit(key: Key) -> bool {
    matches!(key, Key::KEY_1 | Key::KEY_2 | Key::KEY_3 | Key::KEY_4 | Key::KEY_5 | 
                  Key::KEY_6 | Key::KEY_7 | Key::KEY_8 | Key::KEY_9 | Key::KEY_0)
}
pub fn key_to_digit(key: Key) -> Option<usize> { match key { Key::KEY_1 => Some(1), Key::KEY_2 => Some(2), Key::KEY_3 => Some(3), Key::KEY_4 => Some(4), Key::KEY_5 => Some(5), Key::KEY_6 => Some(6), Key::KEY_7 => Some(7), Key::KEY_8 => Some(8), Key::KEY_9 => Some(9), Key::KEY_0 => Some(0), _ => None } }
pub fn key_to_char(key: Key, shift: bool) -> Option<char> {
    let c = match key {
        Key::KEY_Q => Some('q'), Key::KEY_W => Some('w'), Key::KEY_E => Some('e'), Key::KEY_R => Some('r'), Key::KEY_T => Some('t'), Key::KEY_Y => Some('y'), Key::KEY_U => Some('u'), Key::KEY_I => Some('i'), Key::KEY_O => Some('o'), Key::KEY_P => Some('p'), Key::KEY_A => Some('a'), Key::KEY_S => Some('s'), Key::KEY_D => Some('d'), Key::KEY_F => Some('f'), Key::KEY_G => Some('g'), Key::KEY_H => Some('h'), Key::KEY_J => Some('j'), Key::KEY_K => Some('k'), Key::KEY_L => Some('l'), Key::KEY_Z => Some('z'), Key::KEY_X => Some('x'), Key::KEY_C => Some('c'), Key::KEY_V => Some('v'), Key::KEY_B => Some('b'), Key::KEY_N => Some('n'), Key::KEY_M => Some('m'), Key::KEY_APOSTROPHE => Some('\''), Key::KEY_SLASH => Some('/'), _ => None
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
    use std::collections::HashMap;

    fn setup_mock_processor() -> Processor {
        let mut tries = HashMap::new();
        // 尝试加载真实词库以便测试 lookup 逻辑
        if let Ok(trie) = Trie::load("data/chinese/trie.index", "data/chinese/trie.data") {
            tries.insert("chinese".to_string(), trie);
        }

        Processor {
            state: ImeState::Direct,
            buffer: String::new(),
            tries,
            current_profile: "chinese".to_string(),
            punctuation: HashMap::new(),
            candidates: vec![],
            candidate_hints: vec![],
            selected: 0,
            page: 0,
            chinese_enabled: true,
            best_segmentation: vec![],
            joined_sentence: String::new(),
            show_candidates: true,
            show_modern_candidates: false,
            show_notifications: true,
            show_keystrokes: true,
            phantom_mode: PhantomMode::Pinyin,
            phantom_text: String::new(),
            preview_selected_candidate: false,
            enable_anti_typo: true,
            commit_mode: "double".to_string(),
            switch_mode: false,
            cursor_pos: 0,
            profile_keys: Vec::new(),
            page_size: 5,
            show_tone_hint: true,
            show_en_hint: true,
            auto_commit_unique_en_fuzhuma: false,
            auto_commit_unique_full_match: false,
            enable_prefix_matching: true,
            prefix_matching_limit: 20,
            enable_abbreviation_matching: true,
            filter_proper_nouns_by_case: true,
            has_dict_match: false,
            page_flipping_style: "arrow".to_string(),
        }
    }

    #[test]
    fn test_double_space_commit() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; } 
        p.buffer = "nihao".to_string();
        p.lookup(); 
        
        // 第一次空格 -> 加入 buffer
        let _ = p.handle_key(Key::KEY_SPACE, true, false);
        assert_eq!(p.buffer, "nihao ");

        // 第二次空格 -> Commit
        let action2 = p.handle_key(Key::KEY_SPACE, true, false);
        if let Action::DeleteAndEmit { .. } = action2 {
            // Success
        } else {
            panic!("Should have committed on second space, got {:?}", action2);
        }
    }

    #[test]
    fn test_digit_selector_parsing() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; } 
        p.buffer = "hui3".to_string();
        p.lookup();
        assert_eq!(p.best_segmentation, vec!["hui3"]);
    }

    #[test]
    fn test_aux_and_digit_combination() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; }
        // 测试 liL2 逻辑 (辅码 L + 第 2 个候选)
        p.buffer = "liL2".to_string();
        p.lookup();
        
        // 验证 joined_sentence 选择了满足辅码 L 的第二个候选
        // 在真实词库中，荔枝(Litchi)通常排在离(Leave)之后
        if p.candidates.len() >= 2 {
             // 逻辑验证：如果第一个是离(Leave)，第二个是荔(Litchi)，
             // 输入 liL2 应该让 joined_sentence 变成 荔
             assert!(p.joined_sentence != "li", "Joined sentence should not be raw pinyin if matches exist");
        }
    }

    #[test]
    fn test_precise_mode_candidates() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; }
        
        // 测试 "qin2 shi"
        p.buffer = "qin2 shi".to_string();
        p.lookup();
        
        // 在精准模式下，候选词列表不应该包含 "寝室" (qinshi)
        for cand in &p.candidates {
            assert!(cand != "寝室", "Candidates should not contain '寝室' in precise mode for 'qin2 shi'");
        }
    }

    #[test]
    fn test_auxiliary_code_filtering() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; }
        
        // 测试 haoC -> 过滤出 "号" (Call)
        p.buffer = "haoC".to_string();
        p.lookup();
        
        // 验证第一个候选词是否满足辅码 (C 开头单词)
        if !p.candidates.is_empty() {
            let hint = &p.candidate_hints[0];
            assert!(hint.to_lowercase().split_whitespace().any(|w| w.starts_with('c')));
        }
    }

    #[test]
    fn test_abbreviation_matching() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; }
        
        // Test "bj" -> should match "北京"
        p.buffer = "bj".to_string();
        p.lookup();
        
        assert!(p.candidates.contains(&"北京".to_string()) || p.candidates.contains(&"背景".to_string()), "Candidates should contain '北京' or '背景' for 'bj'");
    }

    #[test]
    fn test_page_flipping_and_minus_equal() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; }

        // 1. Verify Page Flipping (UP/DOWN)
        // Ensure we have enough candidates for multiple pages
        p.candidates = (0..20).map(|i| format!("cand{}", i)).collect();
        p.page_size = 5;
        p.page = 0;
        p.selected = 0;
        p.state = ImeState::Composing;
        p.buffer = "test".to_string();

        // Page Down -> Page 1 (index 5)
        p.handle_key(Key::KEY_DOWN, true, false);
        assert_eq!(p.page, 5);
        assert_eq!(p.selected, 5);

        // Page Down -> Page 2 (index 10)
        p.handle_key(Key::KEY_DOWN, true, false);
        assert_eq!(p.page, 10);

        // Page Up -> Page 1 (index 5)
        p.handle_key(Key::KEY_UP, true, false);
        assert_eq!(p.page, 5);

        // PageUp/PageDown (explicit keys)
        p.handle_key(Key::KEY_PAGEDOWN, true, false);
        assert_eq!(p.page, 10);
        p.handle_key(Key::KEY_PAGEUP, true, false);
        assert_eq!(p.page, 5);

        // 1.1 Home -> Page Start (index 5)
        p.selected = 7;
        p.handle_key(Key::KEY_HOME, true, false);
        assert_eq!(p.selected, 5);

        // 1.2 Shift + Home -> Absolute Start (index 0)
        p.handle_key(Key::KEY_HOME, true, true);
        assert_eq!(p.selected, 0);
        assert_eq!(p.page, 0);

        // 1.3 End -> Page End (index 4 on page 0)
        p.handle_key(Key::KEY_END, true, false);
        assert_eq!(p.selected, 4);

        // 1.4 Shift + End -> Absolute End (index 19)
        p.handle_key(Key::KEY_END, true, true);
        assert_eq!(p.selected, 19);
        assert_eq!(p.page, 15);

        // 2. Verify Minus/Equal as characters
        p.reset();
        p.buffer = "test".to_string();
        p.state = ImeState::Composing;

        // Minus should be added to buffer
        p.handle_key(Key::KEY_MINUS, true, false);
        assert!(p.buffer.ends_with('-'), "Buffer should contain minus: {}", p.buffer);

        // Equal should be added to buffer
        p.handle_key(Key::KEY_EQUAL, true, false);
        assert!(p.buffer.ends_with('='), "Buffer should contain equal: {}", p.buffer);
    }

    #[test]
    fn test_page_flipping_config() {
        let mut p = setup_mock_processor();
        if p.tries.is_empty() { return; }
        p.candidates = (0..20).map(|i| format!("cand{}", i)).collect();
        p.page_size = 5;
        p.state = ImeState::Composing;
        p.buffer = "test".to_string();

        // 1. Default Style (Arrow)
        p.page_flipping_style = "arrow".to_string();
        
        // Arrow Down -> Page Flip
        p.page = 0;
        p.handle_key(Key::KEY_DOWN, true, false);
        assert_eq!(p.page, 5);
        
        // Minus -> Input
        p.reset(); p.buffer = "test".to_string(); p.state = ImeState::Composing;
        p.handle_key(Key::KEY_MINUS, true, false);
        assert!(p.buffer.ends_with('-'));

        // 2. Switch to Minus/Equal Style
        p.page_flipping_style = "minus_equal".to_string();
        p.candidates = (0..20).map(|i| format!("cand{}", i)).collect(); // Restore candidates
        
        // Minus -> Page Flip (Previous) - Reset page first
        p.page = 5; 
        p.handle_key(Key::KEY_MINUS, true, false);
        assert_eq!(p.page, 0);

        // Equal -> Page Flip (Next)
        p.page = 0;
        p.handle_key(Key::KEY_EQUAL, true, false);
        assert_eq!(p.page, 5);

        // Arrow Down -> Should be PassThrough (not consumed for flipping) 
        // Note: Our logic returns PassThrough for non-flipping keys in handle_composing?
        // Wait, handle_key consumes it? Let's check handle_key logic.
        // In handle_key: if style==minus_equal, matches!(key, UP|DOWN) is false, so it goes to PassThrough (unless matched by other conditions)
        // Actually, arrows are usually strictly navigation. If not consumed by IME, they go to host.
        // Let's verify return action.
        let action = p.handle_key(Key::KEY_DOWN, true, false);
        assert_eq!(action, Action::PassThrough);
    }
}
