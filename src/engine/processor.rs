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

pub struct Processor {
    pub state: ImeState,
    pub buffer: String,
    pub tries: HashMap<String, Trie>, 
    pub ngrams: HashMap<String, NgramModel>,
    pub current_profile: String,
    pub punctuation: HashMap<String, String>,
    pub en_to_zh: HashMap<String, Vec<String>>, // English -> Chinese words
    pub candidates: Vec<String>,
    pub candidate_hints: Vec<String>, 
    pub selected: usize,
    pub page: usize,
    pub chinese_enabled: bool,
    pub segmenter: Segmenter,
    pub best_segmentation: Vec<String>,
    
    pub show_candidates: bool,
    pub show_modern_candidates: bool,
    pub show_notifications: bool,
    pub show_keystrokes: bool,
    pub phantom_mode: PhantomMode,
    pub phantom_text: String,
}

impl Processor {
    pub fn new(
        tries: HashMap<String, Trie>, 
        ngrams: HashMap<String, NgramModel>,
        initial_profile: String, 
        punctuation: HashMap<String, String>, 
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

        Self {
            state: ImeState::Direct, buffer: String::new(), tries, ngrams, current_profile: initial_profile,
            punctuation, en_to_zh, candidates: vec![], candidate_hints: vec![], selected: 0, page: 0, 
            chinese_enabled: false, segmenter: Segmenter::new(), best_segmentation: vec![],
            show_candidates: true, show_modern_candidates: false, show_notifications: true, show_keystrokes: true,
            phantom_mode: PhantomMode::Pinyin,
            phantom_text: String::new(),
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
        self.selected = 0;
        self.page = 0;
        self.state = ImeState::Direct;
        self.phantom_text.clear();
    }

    pub fn handle_key(&mut self, key: Key, is_press: bool, shift_pressed: bool) -> Action {
        if !is_press {
            if self.buffer.is_empty() { return Action::PassThrough; }
            if is_letter(key) || is_digit(key) || matches!(key, Key::KEY_BACKSPACE | Key::KEY_SPACE | Key::KEY_ENTER | Key::KEY_TAB | Key::KEY_ESC | Key::KEY_MINUS | Key::KEY_EQUAL) { 
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
            if let Some(zh_punc) = self.punctuation.get(punc_key) { Action::Emit(zh_punc.clone()) } else { Action::PassThrough }
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
                    if shift_pressed { if self.selected > 0 { self.selected -= 1; self.page = (self.selected / 5) * 5; } }
                    else { if self.selected + 1 < self.candidates.len() { self.selected += 1; self.page = (self.selected / 5) * 5; } }
                }
                Action::Consume
            }
            Key::KEY_MINUS => { self.page = self.page.saturating_sub(5); self.selected = self.page; Action::Consume }
            Key::KEY_EQUAL => { if self.page + 5 < self.candidates.len() { self.page += 5; self.selected = self.page; } Action::Consume }
            Key::KEY_SPACE => { 
                // 现在空格作为手动分词符，且 buffer 中存入真实空格
                self.buffer.push(' ');
                self.lookup();
                self.update_phantom_action()
            }
            Key::KEY_ENTER => { 
                // 现在回车作为确认上屏键
                if let Some(word) = self.candidates.get(self.selected) { 
                    self.commit_candidate(word.clone())
                } else if !self.buffer.is_empty() { 
                    let out = self.buffer.clone(); 
                    self.commit_candidate(out)
                } else { Action::Consume }
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
                if digit >= 1 && digit <= 5 { 
                    let idx = self.page + (digit - 1); 
                    if let Some(word) = self.candidates.get(idx) { 
                        return self.commit_candidate(word.clone());
                    } 
                }
                Action::Consume
            }
            _ if is_letter(key) => {
                if let Some(c) = key_to_char(key, shift_pressed) {
                    self.buffer.push(c); self.lookup();
                    let has_filter = self.buffer.char_indices().skip(1).any(|(_, c)| c.is_ascii_uppercase());
                    if has_filter && self.candidates.len() == 1 { 
                        return self.commit_candidate(self.candidates[0].clone());
                    }
                    self.update_phantom_action()
                } else { Action::Consume }
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
        
        let target = self.buffer.clone();
        if target == self.phantom_text { return Action::Consume; }
        
        let old_phantom = self.phantom_text.clone();
        self.phantom_text = target.clone();

        // 1. 如果是追加 (例如从 "w" 变成 "wo")
        if target.starts_with(&old_phantom) {
            let added = &target[old_phantom.len()..];
            return Action::Emit(added.to_string());
        }
        
        // 2. 如果是简单的退格 (例如从 "wo" 变成 "w")
        if old_phantom.starts_with(&target) {
            let count = old_phantom.chars().count() - target.chars().count();
            return Action::DeleteAndEmit { delete: count, insert: "".into() };
        }

        // 3. 复杂变更 (强制原子同步)
        Action::DeleteAndEmit { delete: old_phantom.chars().count(), insert: target }
    }

    pub fn lookup(&mut self) {
        if self.buffer.is_empty() { self.reset(); return; }
        let dict = if let Some(d) = self.tries.get(&self.current_profile.to_lowercase()) { d } else { return; };

        let mut pinyin_search = self.buffer.clone();
        let mut filter_string = String::new();
        if let Some((idx, _)) = self.buffer.char_indices().skip(1).find(|(_, c)| c.is_ascii_uppercase()) {
            pinyin_search = self.buffer.get(..idx).unwrap_or(&self.buffer).to_string();
            filter_string = self.buffer.get(idx..).unwrap_or("").to_lowercase();
        }
        let pinyin_stripped = strip_tones(&pinyin_search).to_lowercase();
        let pinyin_for_dict = pinyin_stripped.replace(' ', "").replace('\'', "").replace('`', "");

        let mut final_candidates: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 1. 语义映射优先 (English Semantic)
        let buf_lower = self.buffer.to_lowercase();
        if let Some(zh_words) = self.en_to_zh.get(&buf_lower) {
            for zh in zh_words {
                if seen.insert(zh.clone()) { final_candidates.push((zh.clone(), format!("[{}]", buf_lower))); }
            }
        }

        // 2. 词典全量匹配 (高优先级)
        if let Some(exact_matches) = dict.get_all_exact(&pinyin_for_dict) {
            let mut matches = exact_matches;
            // 按照权重排序 (如果 hint 是数字)
            matches.sort_by(|a, b| {
                let wa = a.1.parse::<u32>().unwrap_or(0);
                let wb = b.1.parse::<u32>().unwrap_or(0);
                wb.cmp(&wa)
            });
            for (word, hint) in matches {
                if seen.insert(word.clone()) { final_candidates.push((word, hint)); }
            }
        }

        // 3. 分段稳定查找逻辑
        let parts: Vec<&str> = pinyin_stripped.split(' ').filter(|s| !s.is_empty()).collect();
        if parts.len() > 1 {
            // 有空格：执行“锁定前缀”逻辑
            let mut stable_prefix = String::new();
            for i in 0..parts.len() - 1 {
                let part = parts[i].replace('\'', "").replace('`', "");
                // 取该段的最佳候选
                let matches = if part.len() == 1 { dict.search_bfs(&part, 1) } else { dict.get_all_exact(&part).unwrap_or_default() };
                if let Some((word, _)) = matches.first() { stable_prefix.push_str(word); }
                else { stable_prefix.push_str(&part); }
            }

            let last_part = parts.last().unwrap().replace('\'', "").replace('`', "");
            let last_options = if last_part.len() == 1 { dict.search_bfs(&last_part, 10) } else { dict.get_all_exact(&last_part).unwrap_or_default() };
            
            for (word, hint) in last_options {
                let mut full = stable_prefix.clone();
                full.push_str(&word);
                if seen.insert(full.clone()) { final_candidates.push((full, hint)); }
            }
        } else {
            // 无空格：执行 Viterbi 全量寻径
            let all_segmentations = self.segmenter.segment_all(&pinyin_stripped, dict);
            let ngram_model = self.ngrams.get(&self.current_profile.to_lowercase());

            for segments in all_segmentations {
                if segments.len() <= 1 { continue; }
                let mut current_paths: Vec<(String, u32)> = vec![("".to_string(), 0)];
                for seg in segments {
                    let options = if seg.len() == 1 { dict.search_bfs(&seg, 5) } else { dict.get_all_exact(&seg).unwrap_or_default() };
                    let mut next_paths = Vec::new();
                    for (prev_text, prev_score) in &current_paths {
                        for (opt_word, hint) in &options {
                            let mut score = *prev_score;
                            if let Ok(weight) = hint.parse::<u32>() { score += weight / 100; }
                            if let Some(model) = ngram_model {
                                let context: Vec<char> = prev_text.chars().collect();
                                score += model.get_score(&context, opt_word);
                            }
                            let mut new_text = prev_text.clone();
                            new_text.push_str(opt_word);
                            next_paths.push((new_text, score));
                        }
                    }
                    next_paths.sort_by(|a, b| b.1.cmp(&a.1));
                    next_paths.truncate(10);
                    current_paths = next_paths;
                }
                for (w, _) in current_paths {
                    if seen.insert(w.clone()) { final_candidates.push((w, String::new())); }
                }
            }
        }

        // 4. 辅助过滤 (仅在有大写辅码时触发)
        if !filter_string.is_empty() {
            final_candidates.retain(|(_, hint)| hint.to_lowercase().starts_with(&filter_string));
        }

        self.candidates.clear();
        self.candidate_hints.clear();
        for (cand, hint) in final_candidates {
            self.candidates.push(cand);
            if !hint.is_empty() && hint.chars().all(|c| c.is_ascii_digit()) { self.candidate_hints.push(String::new()); }
            else { self.candidate_hints.push(hint); }
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
        Key::KEY_Q => Some('q'), Key::KEY_W => Some('w'), Key::KEY_E => Some('e'), Key::KEY_R => Some('r'), Key::KEY_T => Some('t'), Key::KEY_Y => Some('y'), Key::KEY_U => Some('u'), Key::KEY_I => Some('i'), Key::KEY_O => Some('o'), Key::KEY_P => Some('p'), Key::KEY_A => Some('a'), Key::KEY_S => Some('s'), Key::KEY_D => Some('d'), Key::KEY_F => Some('f'), Key::KEY_G => Some('g'), Key::KEY_H => Some('h'), Key::KEY_J => Some('j'), Key::KEY_K => Some('k'), Key::KEY_L => Some('l'), Key::KEY_Z => Some('z'), Key::KEY_X => Some('x'), Key::KEY_C => Some('c'), Key::KEY_V => Some('v'), Key::KEY_B => Some('b'), Key::KEY_N => Some('n'), Key::KEY_M => Some('m'), Key::KEY_APOSTROPHE => Some('\''), _ => None
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
