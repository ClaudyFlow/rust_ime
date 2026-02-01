use std::collections::HashMap;
use evdev::Key;
use crate::engine::trie::Trie;
use crate::engine::ngram::NgramModel;
use crate::engine::segmenter::Segmenter;

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

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Single,
    Long,
}

pub struct Processor {
    pub state: ImeState,
    pub input_mode: InputMode,
    pub buffer: String,
    pub tries: HashMap<String, Trie>, 
    pub ngrams: HashMap<String, NgramModel>,
    pub current_profile: String,
    pub punctuation: HashMap<String, Vec<String>>,
    pub en_to_zh: HashMap<String, Vec<String>>, // English -> Chinese words
    pub candidates: Vec<String>,
    pub candidate_hints: Vec<String>, 
    pub selected: usize,
    pub page: usize,
    pub chinese_enabled: bool,
    pub segmenter: Segmenter,
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
        ngrams: HashMap<String, NgramModel>,
        initial_profile: String, 
        punctuation_raw: HashMap<String, serde_json::Value>, 
    ) -> Self {
        let mut en_to_zh: HashMap<String, Vec<String>> = HashMap::new();
        // 尝试从 chars.json 加载语义映射
        if let Ok(file) = std::fs::File::open("dicts/chinese/chars.json") {
            let reader = std::io::BufReader::new(file);
            if let Ok(json) = serde_json::from_reader::<_, serde_json::Value>(reader) {
                if let Some(obj) = json.as_object() {
                    for (_, val) in obj {
                        if let Some(arr) = val.as_array() {
                            for item in arr {
                                if let (Some(zh), Some(en)) = (item.get("char").and_then(|v| v.as_str()), item.get("en").and_then(|v| v.as_str())) {
                                    if en.len() > 0 {
                                        en_to_zh.entry(en.to_lowercase()).or_default().push(zh.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut punctuation = HashMap::new();
        for (k, v) in punctuation_raw {
            if let Some(arr) = v.as_array() {
                let chars: Vec<String> = arr.iter().filter_map(|item| item.get("char").and_then(|c| c.as_str())).map(|s| s.to_string()).collect();
                punctuation.insert(k, chars);
            }
        }

        Self {
            state: ImeState::Direct, input_mode: InputMode::Single, buffer: String::new(), tries, ngrams, current_profile: initial_profile,
            punctuation, en_to_zh, candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, 
            chinese_enabled: false, segmenter: Segmenter::new(), best_segmentation: vec![],
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
        self.input_mode = InputMode::Single;
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
        
        let mut target = if self.preview_selected_candidate && !self.candidates.is_empty() {
             self.candidates[self.selected.min(self.candidates.len()-1)].clone()
        } else {
             self.buffer.clone()
        };

        if self.buffer.ends_with(' ') && !target.ends_with(' ') {
            target.push(' ');
        }

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

        let pinyin_stripped = strip_tones(&self.buffer).to_lowercase();
        // 含有空格、数字或分号，或者是较长拼音，进入长句/精准模式
        self.input_mode = if pinyin_stripped.contains(' ') || pinyin_stripped.contains('\'') || pinyin_stripped.chars().any(|c| c.is_ascii_digit()) || pinyin_stripped.len() > 7 {
            InputMode::Long
        } else {
            InputMode::Single
        };
        
        let mut final_candidates: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // --- 1. 计算精准 Greedy 结果 ---
        let parts: Vec<&str> = pinyin_stripped.split(' ').filter(|s| !s.is_empty()).collect();
        let mut all_segments = Vec::new();
        let mut greedy_word = String::new();
        
        for part in parts {
            // 解析 part，提取末尾数字索引
            let (pinyin_part, specified_idx) = if let Some(first_digit_idx) = part.find(|c: char| c.is_ascii_digit()) {
                let p = &part[..first_digit_idx];
                let d = part[first_digit_idx..].parse::<usize>().unwrap_or(1);
                (p, Some(d))
            } else {
                (part, None)
            };

            let part_clean = pinyin_part.replace('\'', "").replace('`', "");
            
            // 如果是 Long 模式（意味着有空格或数字），我们不对该 part 进行贪婪分割，除非它完全没匹配
            if self.input_mode == InputMode::Long {
                let matches = if part_clean.len() == 1 {
                    dict.search_bfs(&part_clean, 10)
                } else {
                    dict.get_all_exact(&part_clean).unwrap_or_default()
                };

                if !matches.is_empty() {
                    let idx = specified_idx.unwrap_or(1).saturating_sub(1);
                    let word = matches.get(idx).map(|(w, _)| w.clone()).unwrap_or_else(|| matches[0].0.clone());
                    greedy_word.push_str(&word);
                    all_segments.push(part_clean);
                    continue;
                }
            }

            // 备选：如果不是精准匹配，或者非精准模式，使用贪婪分割
            let segments = self.segmenter.segment_greedy(&part_clean, dict);
            for (i, seg) in segments.iter().enumerate() {
                all_segments.push(seg.clone());
                if seg.starts_with('/') {
                    greedy_word.push_str(&seg[1..]);
                } else {
                    let matches = if seg.chars().count() == 1 { dict.search_bfs(seg, 10) } else { dict.get_all_exact(seg).unwrap_or_default() };
                    let word = if i == segments.len() - 1 && specified_idx.is_some() {
                        let idx = specified_idx.unwrap().saturating_sub(1);
                        matches.get(idx).map(|(w, _)| w.clone()).unwrap_or_else(|| matches.first().map(|(w, _)| w.clone()).unwrap_or_else(|| seg.clone()))
                    } else {
                        matches.first().map(|(w, _)| w.clone()).unwrap_or_else(|| seg.clone())
                    };
                    greedy_word.push_str(&word);
                }
            }
        }
        self.best_segmentation = all_segments;
        self.joined_sentence = greedy_word.clone();

        // --- 2. 填充候选词 (不再将 greedy_word 加入 candidates，它仅在 sentence_label 显示) ---
        let pinyin_for_dict = pinyin_stripped.chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>();
        
        if let Some(exact_matches) = dict.get_all_exact(&pinyin_for_dict) {
            for (word, hint) in exact_matches {
                if seen.insert(word.clone()) { final_candidates.push((word, hint)); }
            }
        }

        // 语义映射
        let buf_lower = self.buffer.to_lowercase().chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>();
        if let Some(zh_words) = self.en_to_zh.get(&buf_lower) {
            for zh in zh_words {
                if seen.insert(zh.clone()) { final_candidates.push((zh.clone(), format!("[{}]", buf_lower))); }
            }
        }

        // 最后分词候选
        if self.best_segmentation.len() > 1 {
            if let Some(last_seg) = self.best_segmentation.last() {
                let last_seg_clean = last_seg.trim_start_matches('/').chars().filter(|c| !c.is_ascii_digit()).collect::<String>();
                if !last_seg_clean.is_empty() {
                    if let Some(last_matches) = dict.get_all_exact(&last_seg_clean) {
                        for (word, hint) in last_matches {
                            if seen.insert(word.clone()) { final_candidates.push((word, hint)); }
                        }
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

        if self.candidates.is_empty() { self.candidates.push(self.buffer.clone()); self.candidate_hints.push(String::new()); }
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
        Key::KEY_Q => Some('q'), Key::KEY_W => Some('w'), Key::KEY_E => Some('e'), Key::KEY_R => Some('r'), Key::KEY_T => Some('t'), Key::KEY_Y => Some('y'), Key::KEY_U => Some('u'), Key::KEY_I => Some('i'), Key::KEY_O => Some('o'), Key::KEY_P => Some('p'), Key::KEY_A => Some('a'), Key::KEY_S => Some('s'), Key::KEY_D => Some('d'), Key::KEY_F => Some('f'), Key::KEY_G => Some('g'), Key::KEY_H => Some('h'), Key::KEY_J => Some('j'), Key::KEY_K => Some('k'), Key::KEY_L => Some('l'), Key::KEY_Z => Some('z'), Key::KEY_X => Some('x'), Key::KEY_C => Some('c'), Key::KEY_V => Some('v'), Key::KEY_B => Some('b'), Key::KEY_N => Some('n'), Key::KEY_M => Some('m'), Key::KEY_APOSTROPHE => Some('"'), Key::KEY_SLASH => Some('/'), _ => None
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
