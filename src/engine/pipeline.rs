use crate::Config;
use std::collections::{HashSet, HashMap};
use std::sync::{Arc, Mutex};
use crate::engine::Trie;
use crate::engine::keys::VirtualKey;

/// 候选词元数据
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub text: String,
    pub simplified: String,
    pub traditional: String,
    pub hint: String,
    pub source: String, // 来源：如 "User", "Table", "Script"
    pub weight: f64,
}

/// 1. 预处理器：按键到字符串映射的转换
pub trait Preprocessor: Send {
    fn process(&self, key: VirtualKey, shift: bool, buffer: &mut String) -> bool;
}

/// 2. 切分器：字符串到音节序列的转换
pub trait Segmentor: Send {
    fn segment(&self, input: &str, syllables: &HashSet<String>) -> Vec<String>;
}

/// 3. 翻译器：音节到候选词的转换
pub trait Translator: Send {
    fn translate(&self, input: &str, segments: &[String], config: &Config) -> Vec<Candidate>;
}

/// 4. 过滤器：对候选词列表的后期加工
pub trait Filter: Send {
    fn filter(&self, candidates: &mut Vec<Candidate>, config: &Config);
}

/// 默认切分器实现 (Max Match)
pub struct DefaultSegmentor;
impl Segmentor for DefaultSegmentor {
    fn segment(&self, input: &str, syllables: &HashSet<String>) -> Vec<String> {
        let mut segments = Vec::new();
        let mut remaining = input.to_lowercase();

        while !remaining.is_empty() {
            let mut matched = false;
            for len in (1..=6).rev() {
                if len <= remaining.len() {
                    let part = &remaining[..len];
                    if syllables.contains(part) {
                        segments.push(part.to_string());
                        remaining = remaining[len..].to_string();
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                // 如果无法匹配音节，取第一个字符并继续
                let first_char = remaining.chars().next().unwrap();
                segments.push(first_char.to_string());
                remaining = remaining[first_char.len_utf8()..].to_string();
            }
        }
        segments
    }
}

/// 系统词库翻译器
pub struct TableTranslator {
    pub trie: Arc<Trie>,
    pub syllables: Arc<HashSet<String>>,
}
impl Translator for TableTranslator {
    fn translate(&self, _input: &str, segments: &[String], config: &Config) -> Vec<Candidate> {
        if segments.is_empty() { return vec![]; }
        let query = segments.join("");
        
        // 1. 尝试全拼/前缀匹配
        let mut results = self.trie.search_bfs(&query, 100);
        
        // 2. 如果结果不足，尝试简拼匹配 (只有当开启简拼且 query 较短时)
        if results.len() < 5 && config.input.enable_abbreviation_matching {
            let abbr_results = self.trie.search_abbreviation(segments, &self.syllables, 100);
            for ar in abbr_results {
                if !results.iter().any(|r| r.0 == ar.0) {
                    results.push(ar);
                }
            }
        }

        results.into_iter().map(|(text, en_aux, stroke_aux, _meaning, trad, weight)| {
            let mut hint = String::new();
            if config.appearance.show_english_aux && !en_aux.is_empty() {
                hint.push_str(&en_aux);
            }
            if config.appearance.show_stroke_aux && !stroke_aux.is_empty() {
                if !hint.is_empty() { hint.push(' '); }
                hint.push_str(&stroke_aux);
            }

            Candidate {
                simplified: text.clone(),
                traditional: if trad.is_empty() { text.clone() } else { trad },
                text,
                hint,
                source: "Table".into(),
                weight: weight as f64,
            }
        }).collect()
    }
}

/// 用户词库翻译器
pub struct UserDictTranslator {
    pub user_dict: Arc<Mutex<HashMap<String, HashMap<String, Vec<(String, u32)>>>>>,
    pub profile: String,
}
impl Translator for UserDictTranslator {
    fn translate(&self, _input: &str, segments: &[String], _config: &Config) -> Vec<Candidate> {
        let query = segments.join("");
        let dict = self.user_dict.lock().unwrap();
        let mut results = Vec::new();
        
        if let Some(profile_dict) = dict.get(&self.profile) {
            if let Some(words) = profile_dict.get(&query) {
                for (text, freq) in words {
                    results.push(Candidate {
                        simplified: text.clone(),
                        traditional: text.clone(),
                        text: text.clone(),
                        hint: "★".into(),
                        source: "User".into(),
                        weight: (*freq as f64) + 1000000.0,
                    });
                }
            }
        }
        results
    }
}

/// 排序与去重过滤器
pub struct SortFilter;
impl Filter for SortFilter {
    fn filter(&self, candidates: &mut Vec<Candidate>, _config: &Config) {
        // 先按权重降序排列
        candidates.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
        
        // 去重
        let mut seen = HashSet::new();
        candidates.retain(|c| seen.insert(c.text.clone()));
    }
}

/// 繁简转换过滤器
pub struct TraditionalFilter;
impl Filter for TraditionalFilter {
    fn filter(&self, candidates: &mut Vec<Candidate>, config: &Config) {
        let use_trad = config.input.enable_traditional;
        for c in candidates.iter_mut() {
            c.text = if use_trad { c.traditional.clone() } else { c.simplified.clone() };
        }
    }
}

/// 核心流水线：管理并执行整个输入处理流程
pub struct Pipeline {
    pub preprocessors: Vec<Box<dyn Preprocessor>>,
    pub segmentor: Box<dyn Segmentor>,
    pub translators: Vec<Box<dyn Translator>>,
    pub filters: Vec<Box<dyn Filter>>,
}

impl Pipeline {
    pub fn new(segmentor: Box<dyn Segmentor>) -> Self {
        Self {
            preprocessors: vec![],
            segmentor,
            translators: vec![],
            filters: vec![],
        }
    }

    pub fn add_preprocessor(&mut self, p: Box<dyn Preprocessor>) { self.preprocessors.push(p); }
    pub fn add_translator(&mut self, t: Box<dyn Translator>) { self.translators.push(t); }
    pub fn add_filter(&mut self, f: Box<dyn Filter>) { self.filters.push(f); }

    pub fn run_preprocessors(&self, key: VirtualKey, shift: bool, buffer: &mut String) -> bool {
        for p in &self.preprocessors {
            if p.process(key, shift, buffer) { return true; }
        }
        false
    }

    pub fn run(&self, input: &str, syllables: &HashSet<String>, config: &Config) -> Vec<Candidate> {
        // 1. 切分
        let segments = self.segmentor.segment(input, syllables);
        
        // 2. 翻译 (汇总所有翻译器的结果)
        let mut candidates = Vec::new();
        for t in &self.translators {
            candidates.extend(t.translate(input, &segments, config));
        }

        // 3. 过滤
        for f in &self.filters {
            f.filter(&mut candidates, config);
        }

        candidates
    }
}

