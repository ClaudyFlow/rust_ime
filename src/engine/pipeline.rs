use crate::Config;
use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use crate::engine::Trie;
use crate::engine::config_manager::UserDictData;
use lru::LruCache;
use std::num::NonZeroUsize;

#[derive(Hash, PartialEq, Eq, Clone)]
struct SearchCacheKey {
    profile: String,
    buffer: String,
    limit: usize,
    filter_mode: crate::engine::processor::FilterMode,
    aux_filter: String,
}
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
pub trait Segmentor: Send + Sync {
    fn segment(&self, input: &str, syllables: &HashSet<String>) -> Vec<String>;
}

/// 3. 翻译器：音节到候选词的转换
pub trait Translator: Send + Sync {
    fn translate(&self, input: &str, segments: &[String], config: &Config, limit: usize) -> Vec<Candidate>;
}

/// 4. 过滤器：对候选词列表的后期加工
pub trait Filter: Send + Sync {
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
                if let Some(first_char) = remaining.chars().next() {
                    segments.push(first_char.to_string());
                    remaining = remaining[first_char.len_utf8()..].to_string();
                } else {
                    break;
                }
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
            for tr in exact_results {
                let mut hint = String::new();
                if config.appearance.show_english_aux && !tr.en.is_empty() { hint.push_str(tr.en); }
                if config.appearance.show_stroke_aux && !tr.stroke_aux.is_empty() {
                    if !hint.is_empty() { hint.push(' '); }
                    hint.push_str(tr.stroke_aux);
                }
                candidates.push(Candidate {
                    simplified: tr.word.to_string(),
                    traditional: if tr.trad.is_empty() { tr.word.to_string() } else { tr.trad.to_string() },
                    text: tr.word.to_string(), 
                    hint, 
                    source: "Table (Exact)".into(),
                    weight: tr.weight as f64 + config.input.ranking.exact_match_bonus, 
                });
            }
        }
        
        // 2. 尝试前缀匹配
        let results = self.trie.search_bfs(&query, limit);
        for tr in results {
            if candidates.iter().any(|c| c.simplified == tr.word) { continue; }
            let mut hint = String::new();
            if config.appearance.show_english_aux && !tr.en.is_empty() { hint.push_str(tr.en); }
            if config.appearance.show_stroke_aux && !tr.stroke_aux.is_empty() {
                if !hint.is_empty() { hint.push(' '); }
                hint.push_str(tr.stroke_aux);
            }
            candidates.push(Candidate {
                simplified: tr.word.to_string(),
                traditional: if tr.trad.is_empty() { tr.word.to_string() } else { tr.trad.to_string() },
                text: tr.word.to_string(), 
                hint, 
                source: "Table".into(),
                weight: tr.weight as f64,
            });
            if candidates.len() >= limit { break; }
        }
        
        // 3. 简拼匹配 (如果结果较少)
        if candidates.len() < 10 && config.input.enable_abbreviation_matching {
            let abbr_results = self.trie.search_abbreviation(segments, &self.syllables, limit);
            for ar in abbr_results {
                if !candidates.iter().any(|r| r.simplified == ar.word) {
                    let mut hint = String::new();
                    if config.appearance.show_english_aux && !ar.en.is_empty() { hint.push_str(ar.en); }
                    if config.appearance.show_stroke_aux && !ar.stroke_aux.is_empty() {
                        if !hint.is_empty() { hint.push(' '); }
                        hint.push_str(ar.stroke_aux);
                    }
                    candidates.push(Candidate {
                        simplified: ar.word.to_string(),
                        traditional: if ar.trad.is_empty() { ar.word.to_string() } else { ar.trad.to_string() },
                        text: ar.word.to_string(), 
                        hint, 
                        source: "Table (Abbr)".into(),
                        weight: (ar.weight as f64) - 5000.0, 
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
    pub user_dict: Arc<arc_swap::ArcSwap<UserDictData>>,
    pub profile: String,
}
impl Translator for UserDictTranslator {
    fn translate(&self, _input: &str, segments: &[String], _config: &Config, _limit: usize) -> Vec<Candidate> {
        let query = segments.join("");
        let mut results = Vec::new();
        let dict = self.user_dict.load();
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
    pub user_dict: Arc<arc_swap::ArcSwap<UserDictData>>,
    pub profile: String,
}
impl Filter for AdaptiveFilter {
    fn filter(&self, candidates: &mut Vec<Candidate>, _config: &Config) {
        let dict = self.user_dict.load();
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

pub struct SearchEngine {
    pub trie_paths: HashMap<String, (PathBuf, PathBuf)>,
    pub syllables: Arc<HashSet<String>>,
    pub user_dict: Arc<arc_swap::ArcSwap<UserDictData>>,
    pub schemes: HashMap<String, Box<dyn crate::engine::scheme::InputScheme>>,
    pipelines: RwLock<HashMap<String, Arc<Pipeline>>>,
    cache: Mutex<LruCache<SearchCacheKey, (Vec<Candidate>, Vec<String>)>>,
}

pub struct SearchQuery<'a> {
    pub buffer: &'a str,
    pub profile: &'a str,
    pub syllables: &'a HashSet<String>,
    pub config: &'a crate::Config,
    pub limit: usize,
    pub filter_mode: crate::engine::processor::FilterMode,
    pub aux_filter: &'a str,
}

impl SearchEngine {
    pub fn new(
        trie_paths: HashMap<String, (PathBuf, PathBuf)>,
        syllables: Arc<HashSet<String>>,
        user_dict: Arc<arc_swap::ArcSwap<UserDictData>>,
        schemes: HashMap<String, Box<dyn crate::engine::scheme::InputScheme>>,
    ) -> Self {
        Self { 
            trie_paths, 
            syllables,
            user_dict,
            schemes,
            pipelines: RwLock::new(HashMap::new()),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())),
        }
    }

    fn get_or_create_pipeline(&self, profile: &str) -> Option<Arc<Pipeline>> {
        // 1. 尝试读取现有
        {
            let p_map = self.pipelines.read().ok()?;
            if let Some(p) = p_map.get(profile) {
                return Some(p.clone());
            }
        }

        // 2. 如果不存在，尝试创建
        let paths = self.trie_paths.get(profile)?;
        tracing::info!(%profile, "Lazy loading dictionary...");
        let trie = Trie::load(&paths.0, &paths.1).ok()?;
        
        let mut pipeline = Pipeline::new(Box::new(DefaultSegmentor));
        pipeline.add_translator(Box::new(UserDictTranslator { 
            user_dict: self.user_dict.clone(), 
            profile: profile.to_string() 
        }));
        pipeline.add_translator(Box::new(TableTranslator { 
            trie: Arc::new(trie),
            syllables: self.syllables.clone(),
        }));
        pipeline.add_filter(Box::new(AdaptiveFilter {
            user_dict: self.user_dict.clone(),
            profile: profile.to_string()
        }));
        pipeline.add_filter(Box::new(SortFilter));
        pipeline.add_filter(Box::new(TraditionalFilter));

        let arc_p = Arc::new(pipeline);
        let mut p_map = self.pipelines.write().ok()?;
        p_map.insert(profile.to_string(), arc_p.clone());
        Some(arc_p)
    }

    pub fn has_longer_match(&self, profile: &str, buffer: &str) -> bool {
        if let Some(paths) = self.trie_paths.get(profile) {
            if let Ok(trie) = Trie::load(&paths.0, &paths.1) {
                return trie.has_longer_match(buffer);
            }
        }
        false
    }

    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    pub fn search(
        &self,
        query: SearchQuery,
    ) -> (Vec<Candidate>, Vec<String>) {
        let key = SearchCacheKey {
            profile: query.profile.to_string(),
            buffer: query.buffer.to_string(),
            limit: query.limit,
            filter_mode: query.filter_mode.clone(),
            aux_filter: query.aux_filter.to_string(),
        };

        if let Ok(mut cache) = self.cache.lock() {
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }

        let result = self.do_search(query);
        
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(key, result.clone());
        }
        
        result
    }

    fn do_search(
        &self,
        query: SearchQuery,
    ) -> (Vec<Candidate>, Vec<String>) {
        let span = tracing::info_span!("engine_search", profile = %query.profile, buffer = %query.buffer);
        let _enter = span.enter();

        if let Some(pipeline) = self.get_or_create_pipeline(query.profile) {
            let segments = pipeline.segmentor.segment(query.buffer, query.syllables);
            let results = pipeline.run(query.buffer, query.syllables, query.config, query.limit);
            
            let mut final_results = results;
            if query.filter_mode == crate::engine::processor::FilterMode::Global && !query.aux_filter.is_empty() {
                tracing::debug!("Applying global filter: {}", query.aux_filter);
                final_results.retain(|c| self.matches_filter(c, query.aux_filter));
            }
            
            tracing::info!(results_count = final_results.len(), "Search complete");
            return (final_results, segments);
        }

        if let Some(scheme) = self.schemes.get(query.profile) {
            let context = crate::engine::scheme::SchemeContext {
                config: query.config,
                tries: &HashMap::new(),
                syllables: query.syllables,
                _user_dict: &Arc::new(arc_swap::ArcSwap::from_pointee(HashMap::new())),
                active_profiles: &vec![query.profile.to_string()],
                candidate_count: 0,
                _filter_mode: query.filter_mode.clone(),
                _aux_filter: query.aux_filter,
            };
            
            let pre_processed = scheme.pre_process(query.buffer, &context);
            let mut candidates = scheme.lookup(&pre_processed, &context);
            scheme.post_process(&pre_processed, &mut candidates, &context);
            
            let mut results = Vec::new();
            for c in candidates {
                results.push(Candidate {
                    text: if query.config.input.enable_traditional { c.traditional.clone() } else { c.simplified.clone() },
                    simplified: c.simplified,
                    traditional: c.traditional,
                    hint: c.tone,
                    source: "Scheme".into(),
                    weight: c.weight as f64,
                });
            }

            if query.filter_mode == crate::engine::processor::FilterMode::Global && !query.aux_filter.is_empty() {
                results.retain(|c| self.matches_filter(c, query.aux_filter));
            }

            return (results, vec![]);
        }

        (vec![], vec![])
    }

    pub fn matches_filter(&self, cand: &Candidate, filter: &str) -> bool {
        if filter.is_empty() { return true; }
        let filter_lower = filter.to_lowercase();
        let hint_lower = cand.hint.to_lowercase();
        let hint_clean = crate::engine::processor::strip_tones(&hint_lower);
        let parts: Vec<&str> = hint_clean.split([' ', '/', '(', ')', ',']).collect();
        parts.iter().any(|p| p.starts_with(&filter_lower)) || hint_clean.starts_with(&filter_lower)
    }
}
