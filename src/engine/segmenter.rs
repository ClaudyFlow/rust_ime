use std::collections::HashSet;
use crate::engine::trie::Trie;

pub struct Segmenter {
    pub syllable_set: HashSet<String>,
}

impl Segmenter {
    pub fn new() -> Self {
        let mut syllable_set = HashSet::new();
        if let Ok(content) = std::fs::read_to_string("dicts/chinese/syllables.txt") {
            for line in content.lines() {
                let s = line.trim();
                if !s.is_empty() { syllable_set.insert(s.to_string()); }
            }
        }
        Self { syllable_set }
    }

    pub fn segment_greedy(&self, pinyin: &str, dict: &Trie) -> Vec<String> {
        let mut segments = Vec::new();
        let chars: Vec<char> = pinyin.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let remaining: String = chars[i..].iter().collect();
            
            // 跳过分隔符
            if remaining.starts_with('`') || remaining.starts_with('\'') || remaining.starts_with(' ') { 
                i += 1; continue; 
            }
            
            // 处理转义英语
            if remaining.starts_with('/') {
                let mut end = 1;
                while i + end < chars.len() && chars[i + end].is_alphanumeric() {
                    end += 1;
                }
                segments.push(chars[i..i+end].iter().collect());
                i += end;
                continue;
            }

            // 贪婪匹配
            let mut found_len = 0;
            let max_match = (chars.len() - i).min(8); 
            for len in (1..=max_match).rev() {
                let sub: String = chars[i..i+len].iter().collect();
                if self.syllable_set.contains(&sub) || dict.contains(&sub) {
                    segments.push(sub);
                    found_len = len;
                    break;
                }
            }
            
            if found_len > 0 {
                i += found_len;
            } else { 
                // 回退：单字符处理（支持标点符号）
                segments.push(chars[i].to_string());
                i += 1;
            }
        }
        segments
    }
}