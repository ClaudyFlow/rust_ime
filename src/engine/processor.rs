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
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhantomMode {
    None,
    Pinyin,
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
}

impl Processor {
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
        }
    }

    pub fn apply_config(&mut self, conf: &crate::config::Config) {
        self.show_candidates = conf.appearance.show_candidates;
        self.show_modern_candidates = conf.appearance.show_modern_candidates;
        self.show_notifications = conf.appearance.show_notifications;
        self.show_keystrokes = conf.appearance.show_keystrokes;
        self.current_profile = conf.input.default_profile.to_lowercase();
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
    }

    pub fn handle_key(&mut self, key: Key, is_press: bool, shift_pressed: bool) -> Action {
        if !is_press {
            if self.buffer.is_empty() { return Action::PassThrough; }
            if is_letter(key) || is_digit(key) || get_punctuation_key(key, shift_pressed).is_some() || matches!(key, Key::KEY_BACKSPACE | Key::KEY_SPACE | Key::KEY_ENTER | Key::KEY_TAB | Key::KEY_ESC | Key::KEY_MINUS | Key::KEY_EQUAL) { 
                return Action::Consume; 
            }
            return Action::PassThrough;
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
            Key::KEY_TAB => {
                if !self.candidates.is_empty() {
                    self.preview_selected_candidate = true;
                    if shift_pressed { if self.selected > 0 { self.selected -= 1; } } else { if self.selected + 1 < self.candidates.len() { self.selected += 1; } }
                    self.page = (self.selected / 10) * 10;
                    self.update_phantom_action()
                } else {
                    Action::Consume
                }
            }
            Key::KEY_MINUS => { self.page = self.page.saturating_sub(10); self.selected = self.page; Action::Consume }
            Key::KEY_EQUAL => { if self.page + 10 < self.candidates.len() { self.page += 10; self.selected = self.page; } Action::Consume }
            Key::KEY_SPACE => { 
                if self.preview_selected_candidate {
                     if let Some(word) = self.candidates.get(self.selected) {
                        return self.commit_candidate(word.clone());
                     }
                }
                
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
                self.buffer.push_str(&digit.to_string());
                self.lookup();
                self.update_phantom_action()
            }
            _ if is_letter(key) => {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    self.buffer.push(c); 
                    self.preview_selected_candidate = false;
                    self.lookup();
                    self.update_phantom_action()
                } else { Action::Consume }
            }
            _ if get_punctuation_key(key, shift_pressed).is_some() => {
                let punc_key = get_punctuation_key(key, shift_pressed).unwrap();
                let zh_punc = self.punctuation.get(punc_key).and_then(|v| v.first()).cloned().unwrap_or_else(|| punc_key.to_string());
                
                self.buffer.push_str(&zh_punc);
                self.preview_selected_candidate = false;
                self.lookup();
                self.update_phantom_action()
            }
            _ => Action::PassThrough,
        }
    }

    fn commit_candidate(&mut self, cand: String) -> Action {
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

    pub fn lookup(&mut self) {
        if self.buffer.is_empty() { self.reset(); return; }
        let dict = if let Some(d) = self.tries.get(&self.current_profile.to_lowercase()) { d } else { return; };

        // 不要在这里直接 to_lowercase，否则无法识别大写辅码
        let buffer_normalized = strip_tones(&self.buffer);
        
        let mut final_candidates: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // --- 1. 计算精准组合结果 (原子分词) ---
        let parts: Vec<&str> = buffer_normalized.split(' ').filter(|s| !s.is_empty()).collect();
        let mut greedy_word = String::new();
        let mut all_segments = Vec::new();
        
        for part in &parts {
            let part = *part;
            // 提取数字索引 (hui3) 和 英文辅码 (liH, liIn)
            // 逻辑：找到第一个数字或第一个大写字母的位置
            let split_pos = part.find(|c: char| c.is_ascii_digit() || c.is_ascii_uppercase());
            
            let (pinyin_part, specified_idx, aux_code) = if let Some(pos) = split_pos {
                let (p, suffix) = part.split_at(pos);
                
                let mut d = None;
                let mut a = None;
                
                // 如果后缀以数字开头
                if suffix.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    let end_of_digits = suffix.find(|c: char| !c.is_ascii_digit()).unwrap_or(suffix.len());
                    d = Some(suffix[..end_of_digits].parse::<usize>().unwrap_or(1));
                    if end_of_digits < suffix.len() {
                        a = Some(&suffix[end_of_digits..]);
                    }
                } else {
                    // 否则整个后缀都是辅码
                    a = Some(suffix);
                }
                
                (p, d, a)
            } else {
                (part, None, None)
            };

            let part_clean = pinyin_part.to_lowercase().replace('\'', "").replace('`', "");
            all_segments.push(part.to_string());

            let mut matches = if part_clean.len() == 1 {
                dict.search_bfs(&part_clean, 15)
            } else {
                dict.get_all_exact(&part_clean).unwrap_or_default()
            };

            // 应用英文辅码过滤
            if let Some(code) = aux_code {
                let code_lower = code.to_lowercase();
                matches.retain(|(_, hint)| {
                    let hint_lower = hint.to_lowercase();
                    if code.chars().all(|c| c.is_ascii_uppercase()) && code.len() == 1 {
                        // 单个大写字母：匹配单词开头 (liH -> match "house")
                        hint_lower.split_whitespace().any(|word| word.starts_with(&code_lower))
                    } else {
                        // 多个字母或包含小写：模糊包含匹配 (liIn -> match "find")
                        hint_lower.contains(&code_lower)
                    }
                });
            }

            let idx = specified_idx.unwrap_or(1).saturating_sub(1);
            if let Some((w, _)) = matches.get(idx) {
                greedy_word.push_str(w);
            } else {
                greedy_word.push_str(&part_clean);
            }
        }
        
        self.best_segmentation = all_segments;
        self.joined_sentence = greedy_word;

        // --- 2. 填充候选词列表 ---
        let full_pinyin = buffer_normalized.chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>().to_lowercase();

        // 精准模式判定：包含空格或数字或大写字母
        let is_precise_mode = buffer_normalized.contains(' ') || buffer_normalized.chars().any(|c| c.is_ascii_digit() || c.is_ascii_uppercase());

        if !is_precise_mode {
            // 单词模式：显示整词匹配
            if let Some(exact_matches) = dict.get_all_exact(&full_pinyin) {
                for (word, hint) in exact_matches {
                    if seen.insert(word.clone()) { final_candidates.push((word, hint)); }
                }
            }
        } else {
            // 精准模式：列表只显示“当前输入部分”的候选
            if let Some(last_part) = parts.last() {
                // 同样解析最后一部分的 aux_code
                let split_pos = last_part.find(|c: char| c.is_ascii_digit() || c.is_ascii_uppercase());
                let (last_pinyin, aux_code) = if let Some(pos) = split_pos {
                    let (p, suffix) = last_part.split_at(pos);
                    // 过滤掉后缀中的数字部分
                    let start_of_aux = suffix.find(|c: char| !c.is_ascii_digit()).unwrap_or(suffix.len());
                    let a = if start_of_aux < suffix.len() { Some(&suffix[start_of_aux..]) } else { None };
                    (p, a)
                } else {
                    (*last_part, None)
                };

                let last_clean = last_pinyin.to_lowercase().replace('\'', "").replace('`', "").chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>();
                if !last_clean.is_empty() {
                    let mut matches = if last_clean.len() == 1 {
                        dict.search_bfs(&last_clean, 15)
                    } else {
                        dict.get_all_exact(&last_clean).unwrap_or_default()
                    };

                    if let Some(code) = aux_code {
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

                    for (word, hint) in matches {
                        if seen.insert(word.clone()) { final_candidates.push((word, hint)); }
                    }
                }
            }
        }

        self.candidates.clear();
        self.candidate_hints.clear();
        for (cand, hint) in final_candidates {
            self.candidates.push(cand);
            self.candidate_hints.push(hint);
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
        if p.tries.is_empty() { return; } // 如果没加载到词库则跳过
        p.buffer = "hui3".to_string();
        p.lookup();
        assert_eq!(p.best_segmentation, vec!["hui3"]);
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
}
