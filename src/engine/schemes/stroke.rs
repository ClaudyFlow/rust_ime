use crate::engine::scheme::{InputScheme, SchemeContext, SchemeCandidate};
use crate::engine::keys::VirtualKey;
use crate::engine::processor::Action;

pub struct StrokeScheme;

impl StrokeScheme {
    pub fn new() -> Self {
        Self
    }
    
    /// 将 1-5 数字序列转为字母编码 (双笔一键逻辑)
    fn encode_stroke(&self, s: &str) -> String {
        let mut res = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if i + 1 < chars.len() {
                let pair = format!("{}{}", chars[i], chars[i+1]);
                let code = match pair.as_str() {
                    "11" => 'g', "12" => 'f', "13" => 'd', "14" => 's', "15" => 'a',
                    "21" => 'h', "22" => 'j', "23" => 'k', "24" => 'l', "25" => 'm',
                    "31" => 't', "32" => 'r', "33" => 'e', "34" => 'w', "35" => 'q',
                    "41" => 'y', "42" => 'u', "43" => 'i', "44" => 'o', "45" => 'p',
                    "51" => 'n', "52" => 'b', "53" => 'v', "54" => 'c', "55" => 'x',
                    _ => ' ',
                };
                if code != ' ' {
                    res.push(code);
                    i += 2;
                    continue;
                }
            }
            let code = match chars[i] {
                '1' => 'g', '2' => 'h', '3' => 't', '4' => 'y', '5' => 'n',
                c if c.is_ascii_lowercase() => c, // 允许直接输入映射后的字母
                _ => ' ',
            };
            if code != ' ' { res.push(code); }
            i += 1;
        }
        res
    }
}

impl InputScheme for StrokeScheme {
    fn name(&self) -> &str {
        "stroke"
    }

    fn pre_process(&self, buffer: &str, _context: &SchemeContext) -> String {
        // 如果输入包含数字，进行转码；否则保留原样（支持直接输入字母）
        if buffer.chars().any(|c| c.is_ascii_digit()) {
            self.encode_stroke(buffer)
        } else {
            buffer.to_string()
        }
    }

    fn lookup(&self, query: &str, context: &SchemeContext) -> Vec<SchemeCandidate> {
        let mut results = Vec::new();
        if let Some(trie) = context.tries.get("stroke") {
            // 1. 精确匹配
            if let Some(matches) = trie.get_all_exact(query) {
                for (w, tr, t, e, s, weight) in matches {
                    let mut cand = SchemeCandidate::new(w, weight);
                    cand.traditional = tr;
                    cand.tone = t;
                    cand.english = e;
                    cand.stroke_aux = s;
                    cand.match_level = 3;
                    results.push(cand);
                }
            }
            
            // 2. 前缀匹配
            if context.config.input.enable_prefix_matching {
                let matches = trie.search_bfs(query, 50);
                for (w, tr, t, e, s, weight) in matches {
                    let mut cand = SchemeCandidate::new(w, weight);
                    cand.traditional = tr;
                    cand.tone = t;
                    cand.english = e;
                    cand.stroke_aux = s;
                    cand.match_level = 1;
                    results.push(cand);
                }
            }
        }
        results
    }

    fn post_process(&self, _query: &str, candidates: &mut Vec<SchemeCandidate>, _context: &SchemeContext) {
        // 按匹配级别和权重排序
        candidates.sort_by(|a, b| {
            b.match_level.cmp(&a.match_level)
                .then_with(|| b.weight.cmp(&a.weight))
        });
        
        // 去重
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| seen.insert(c.text.clone()));
    }

    fn handle_special_key(&self, key: VirtualKey, buffer: &mut String, context: &SchemeContext) -> Option<Action> {
        // 笔画模式下，1-5 数字优先作为输入，但如果有候选词，则优先选词
        if let Some(digit) = crate::engine::processor::key_to_digit(key) {
            if digit >= 1 && digit <= 5 {
                // 如果当前已经有候选词了，我们返回 None，让 Processor 的通用选词逻辑去处理
                if context.candidate_count > 0 {
                    return None;
                }
                
                // 否则，将其作为笔画输入
                buffer.push_str(&digit.to_string());
                return Some(Action::Consume);
            }
        }
        None
    }

}
