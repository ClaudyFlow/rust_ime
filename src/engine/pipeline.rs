use crate::Config;
use std::collections::{HashSet, HashMap};
use std::sync::{Arc, Mutex};
use crate::engine::Trie;
// use crate::engine::keys::VirtualKey;

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

/*
/// 1. 预处理器：按键到字符串映射的转换
pub trait Preprocessor: Send {
    fn process(&self, key: VirtualKey, shift: bool, buffer: &mut String) -> bool;
}
*/

/// 2. 切分器：字符串到音节序列的转换
pub trait Segmentor: Send {
    fn segment(&self, input: &str, syllables: &HashSet<String>) -> Vec<String>;
}

/// 3. 翻译器：音节到候选词的转换
pub trait Translator: Send {
    fn translate(&self, input: &str, segments: &[String], config: &Config, limit: usize) -> Vec<Candidate>;
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
            // 尝试最长匹配音节 (Max Match)
            for len in (1..=6).rev() {
                if len <= remaining.len() {
                    let part = &remaining[..len];
                    if syllables.contains(part) {
                        // 额外校验：如果这是一个音节，但输入后面还有内容，
                        // 我们需要确保这不是一个简拼的情况（如 'nh'，'n' 是音节但 'nh' 不是）
                        // 在基础实现中，我们先保留最长音节匹配
                        segments.push(part.to_string());
                        remaining = remaining[len..].to_string();
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                // 如果没有任何音节匹配，按单字母切分（简拼或未知输入）
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
    fn translate(&self, _input: &str, segments: &[String], config: &Config, limit: usize) -> Vec<Candidate> {
        if segments.is_empty() { return vec![]; }
        let query = segments.join("");
        let mut candidates = Vec::new();

        // 1. 尝试全拼精确匹配
        if let Some(exact_results) = self.trie.get_all_exact(&query) {
            for (text, trad, _tone, en, stroke_aux, weight) in exact_results {
                let mut hint = String::new();
                if config.appearance.show_english_aux && !en.is_empty() { hint.push_str(&en); }
                if config.appearance.show_stroke_aux && !stroke_aux.is_empty() {
                    if !hint.is_empty() { hint.push(' '); }
                    hint.push_str(&stroke_aux);
                }
                candidates.push(Candidate {
                    simplified: text.clone(),
                    traditional: if trad.is_empty() { text.clone() } else { trad },
                    text, hint, source: "Table (Exact)".into(),
                    weight: weight as f64 + config.input.ranking.exact_match_bonus, 
                });
            }
        }
        
        // 2. 尝试前缀匹配
        let results = self.trie.search_bfs(&query, limit);
        for (text, trad, _tone, en, stroke_aux, weight) in results {
            if candidates.iter().any(|c| c.simplified == text) { continue; }
            let mut hint = String::new();
            if config.appearance.show_english_aux && !en.is_empty() { hint.push_str(&en); }
            if config.appearance.show_stroke_aux && !stroke_aux.is_empty() {
                if !hint.is_empty() { hint.push(' '); }
                hint.push_str(&stroke_aux);
            }
            candidates.push(Candidate {
                simplified: text.clone(),
                traditional: if trad.is_empty() { text.clone() } else { trad },
                text, hint, source: "Table".into(),
                weight: weight as f64,
            });
            if candidates.len() >= limit { break; }
        }
        
        // 3. 简拼匹配 (如果结果较少)
        if candidates.len() < 10 && config.input.enable_abbreviation_matching {
            let abbr_results = self.trie.search_abbreviation(segments, &self.syllables, limit);
            for ar in abbr_results {
                if !candidates.iter().any(|r| r.simplified == ar.0) {
                    let mut hint = String::new();
                    if config.appearance.show_english_aux && !ar.3.is_empty() { hint.push_str(&ar.3); }
                    if config.appearance.show_stroke_aux && !ar.4.is_empty() {
                        if !hint.is_empty() { hint.push(' '); }
                        hint.push_str(&ar.4);
                    }
                    candidates.push(Candidate {
                        simplified: ar.0.clone(),
                        traditional: if ar.1.is_empty() { ar.0.clone() } else { ar.1 },
                        text: ar.0, hint, source: "Table (Abbr)".into(),
                        weight: (ar.5 as f64) - 5000.0, 
                    });
                }
                if candidates.len() >= limit + 10 { break; }
            }
        }
        candidates
    }
}

/// 用户词库翻译器 (仅处理用户自造词)
pub struct UserDictTranslator {
    pub user_dict: Arc<Mutex<HashMap<String, HashMap<String, Vec<(String, u32)>>>>>,
    pub profile: String,
}
impl Translator for UserDictTranslator {
    fn translate(&self, _input: &str, segments: &[String], _config: &Config, _limit: usize) -> Vec<Candidate> {
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
                        weight: (*freq as f64) + 10000.0, // 基础分
                    });
                }
            }
        }
        results
    }
}

/// 调频过滤器：根据用户历史频率对已有候选词进行动态评分加成
pub struct AdaptiveFilter {
    pub user_dict: Arc<Mutex<HashMap<String, HashMap<String, Vec<(String, u32)>>>>>,
    pub profile: String,
}
impl Filter for AdaptiveFilter {
    fn filter(&self, candidates: &mut Vec<Candidate>, _config: &Config) {
        let dict = self.user_dict.lock().unwrap();
        if let Some(profile_dict) = dict.get(&self.profile) {
            for c in candidates.iter_mut() {
                // 在用户历史中查找该词的出现频率 (调频)
                for words in profile_dict.values() {
                    if let Some(pos) = words.iter().position(|(w, _)| w == &c.simplified) {
                        c.weight += words[pos].1 as f64 * 1000.0; // 显著加成
                    }
                }
            }
        }
    }
}

/// 排序与去重过滤器
pub struct SortFilter;
impl Filter for SortFilter {
    fn filter(&self, candidates: &mut Vec<Candidate>, config: &Config) {
        let ranking = &config.input.ranking;
        // 核心排序：
        // 1. 评分基础 = 权重
        // 2. 来源加成：User 来源最优先
        // 3. 长度惩罚：字数越多的词，惩罚越大
        candidates.sort_by(|a, b| {
            let mut score_a = a.weight;
            let mut score_b = b.weight;

            if a.source == "User" { score_a += ranking.user_dict_bonus; }
            if b.source == "User" { score_b += ranking.user_dict_bonus; }

            // 针对拼音输入优化：
            // 单字通常有更高的优先级
            if a.text.chars().count() == 1 { score_a += ranking.single_char_bonus; }
            if b.text.chars().count() == 1 { score_b += ranking.single_char_bonus; }

            // 惩罚过长的词
            score_a -= (a.text.chars().count() as f64) * ranking.length_penalty;
            score_b -= (b.text.chars().count() as f64) * ranking.length_penalty;

            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // 去重
        let mut seen = HashSet::new();
        candidates.retain(|c| seen.insert(c.simplified.clone()));
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
    // pub preprocessors: Vec<Box<dyn Preprocessor>>,
    pub segmentor: Box<dyn Segmentor>,
    pub translators: Vec<Box<dyn Translator>>,
    pub filters: Vec<Box<dyn Filter>>,
}

impl Pipeline {
    pub fn new(segmentor: Box<dyn Segmentor>) -> Self {
        Self {
            // preprocessors: vec![],
            segmentor,
            translators: vec![],
            filters: vec![],
        }
    }

    /*
    pub fn add_preprocessor(&mut self, p: Box<dyn Preprocessor>) { self.preprocessors.push(p); }
    */
    pub fn add_translator(&mut self, t: Box<dyn Translator>) { self.translators.push(t); }
    pub fn add_filter(&mut self, f: Box<dyn Filter>) { self.filters.push(f); }

    /*
    pub fn run_preprocessors(&self, key: VirtualKey, shift: bool, buffer: &mut String) -> bool {
        for p in &self.preprocessors {
            if p.process(key, shift, buffer) { return true; }
        }
        false
    }
    */

    pub fn run(&self, input: &str, syllables: &HashSet<String>, config: &Config, limit: usize) -> Vec<Candidate> {
        let segments = self.segmentor.segment(input, syllables);
        let mut candidates = Vec::new();
        for t in &self.translators {
            candidates.extend(t.translate(input, &segments, config, limit));
        }
        for f in &self.filters {
            f.filter(&mut candidates, config);
        }
        candidates
    }
}
