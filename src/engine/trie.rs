use fst::{Map, IntoStreamer, Streamer, Automaton};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct TrieResult<'a> {
    pub word: &'a str,
    pub trad: &'a str,
    pub tone: &'a str,
    pub en: &'a str,
    pub stroke_aux: &'a str,
    pub weight: u32,
}

#[derive(Clone)]
pub struct MmapData(Arc<Mmap>);
impl AsRef<[u8]> for MmapData {
    fn as_ref(&self) -> &[u8] { self.0.as_ref() }
}

#[derive(Clone)]
pub struct Trie {
    index: Map<MmapData>,
    data: MmapData,
}

impl Trie {
    pub fn load<P: AsRef<Path>>(index_path: P, data_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        println!("[Trie] Loading index: {:?}, data: {:?}", index_path.as_ref(), data_path.as_ref());
        let index_file = File::open(&index_path)?;
        let data_file = File::open(&data_path)?;
        let index_data = MmapData(Arc::new(unsafe { Mmap::map(&index_file)? }));
        let data_data = MmapData(Arc::new(unsafe { Mmap::map(&data_file)? }));
        let index = Map::new(index_data)?;

        Ok(Self { index, data: data_data })
    }

    pub fn get_all_exact(&self, pinyin: &str) -> Option<Vec<TrieResult<'_>>> {
        let _span = tracing::debug_span!("trie_exact", %pinyin).entered();
        let offset = self.index.get(pinyin)? as usize;
        Some(self.read_block(offset))
    }

    #[allow(dead_code)]
    pub fn has_prefix(&self, prefix: &str) -> bool {
        let matcher = fst::automaton::Str::new(prefix).starts_with();
        self.index.search(matcher).into_stream().next().is_some()
    }

    pub fn has_longer_match(&self, prefix: &str) -> bool {
        let matcher = fst::automaton::Str::new(prefix).starts_with();
        let mut stream = self.index.search(matcher).into_stream();
        while let Some((key, _)) = stream.next() {
            if key.len() > prefix.len() {
                return true;
            }
        }
        false
    }

    pub fn search_bfs(&self, prefix: &str, limit: usize) -> Vec<TrieResult<'_>> {
        let _span = tracing::debug_span!("trie_bfs", %prefix, limit).entered();
        let mut results = Vec::new();
        
        // 支持通配符 z：将其转换为正则搜索
        if prefix.contains('z') {
            return self.search_wildcard(prefix, limit);
        }

        let matcher = fst::automaton::Str::new(prefix).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((_, offset)) = stream.next() {
            let pairs = self.read_block(offset as usize);
            for pair in pairs {
                if !results.iter().any(|tr: &TrieResult| tr.word == pair.word) {
                    results.push(pair);
                    if results.len() >= limit { return results; }
                }
            }
        }
        results
    }

    /// 通配符搜索实现：z 匹配任意单个 a-y 字母
    pub fn search_wildcard(&self, pattern: &str, limit: usize) -> Vec<TrieResult<'_>> {
        let mut results = Vec::new();
        
        // 简单的 DFS 实现通配符匹配
        let mut stream = self.index.stream();
        while let Some((key_bytes, offset)) = stream.next() {
            let key = String::from_utf8_lossy(key_bytes);
            if self.wildcard_match(pattern, &key) {
                let pairs = self.read_block(offset as usize);
                for pair in pairs {
                    if !results.iter().any(|tr: &TrieResult| tr.word == pair.word) {
                        results.push(pair);
                        if results.len() >= limit { return results; }
                    }
                }
            }
        }
        results
    }

    fn wildcard_match(&self, pattern: &str, key: &str) -> bool {
        let p_chars: Vec<char> = pattern.chars().collect();
        let k_chars: Vec<char> = key.chars().collect();
        
        // 如果 pattern 不包含通配符且不是 key 的前缀，快速失败
        if !pattern.contains('z') {
            return key.starts_with(pattern);
        }

        // 简易正则逻辑：z 匹配任意 1 个字符
        if p_chars.len() > k_chars.len() { return false; }
        
        for i in 0..p_chars.len() {
            if p_chars[i] != 'z' && p_chars[i] != k_chars[i] {
                return false;
            }
        }
        true
    }

    pub fn search_abbreviation(&self, segments: &[String], syllables: &std::collections::HashSet<String>, limit: usize) -> Vec<TrieResult<'_>> {
        if segments.is_empty() { return Vec::new(); }
        let mut results = Vec::with_capacity(limit);
        
        let first_seg = &segments[0];
        // 性能优化：如果首个片段很长，可以利用它作为更精确的前缀
        let matcher = fst::automaton::Str::new(first_seg).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((key_bytes, offset)) = stream.next() {
            // 快速失败：如果 key 的长度连 segment 的数量都凑不齐，直接跳过
            if key_bytes.len() < segments.len() { continue; }

            let key = String::from_utf8_lossy(key_bytes);
            if self.matches_segments(&key, segments, syllables) {
                let pairs = self.read_block(offset as usize);
                for pair in pairs {
                    if !results.iter().any(|tr: &TrieResult| tr.word == pair.word) {
                        results.push(pair);
                        if results.len() >= limit { return results; }
                    }
                }
            }
        }
        results
    }

    fn matches_segments(&self, key: &str, segments: &[String], syllables: &std::collections::HashSet<String>) -> bool {
        if segments.is_empty() { return false; }
        self.recursive_match(key, segments, syllables)
    }

    fn recursive_match(&self, key: &str, segments: &[String], syllables: &std::collections::HashSet<String>) -> bool {
        if segments.is_empty() {
            return key.is_empty(); // 必须刚好消耗完 key，或者是最后一个音节匹配
        }

        if key.is_empty() {
            return false;
        }

        // 简拼的核心：每个 segment 必须匹配 key 中一个完整音节的开头
        // 尝试从当前 key 的起始位置切分出一个合法音节
        for len in (1..=6).rev() {
            if len <= key.len() {
                let syl = &key[..len];
                if syllables.contains(syl) {
                    // 如果这个音节以当前第一个 segment 开头
                    if syl.starts_with(&segments[0]) {
                        // 递归尝试匹配剩余部分
                        if self.recursive_match(&key[len..], &segments[1..], syllables) {
                            return true;
                        }
                    }
                }
            }
        }
        
        // 特殊处理最后一个 segment：它可能只匹配了最后一个音节的前缀
        if segments.len() == 1 {
            // 找到 key 剩余部分能切分出的第一个音节
            for len in (1..=6).rev() {
                if len <= key.len() {
                    let syl = &key[..len];
                    if syllables.contains(syl) && syl.starts_with(&segments[0]) && key.len() == len {
                        return true;
                    }
                }
            }
        }

        false
    }

    #[allow(dead_code)]
    pub fn get_random_entry(&self) -> Option<TrieResult<'_>> {
        let len = self.index.len();
        if len == 0 { return None; }
        
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let target_idx = rng.gen_range(0..len);
        
        let mut stream = self.index.stream();
        let mut current = 0;
        while let Some((_, offset)) = stream.next() {
            if current == target_idx {
                let pairs = self.read_block(offset as usize);
                return pairs.first().copied();
            }
            current += 1;
        }
        None
    }

    fn read_block(&self, offset: usize) -> Vec<TrieResult<'_>> {
        let data = self.data.as_ref();
        if offset + 4 > data.len() { return Vec::new(); }
        
        let count = u32::from_le_bytes(data[offset..offset+4].try_into().unwrap_or([0; 4]));
        let mut cursor = offset + 4;
        
        let mut results = Vec::with_capacity(count as usize);
        for _ in 0..count {
            if cursor + 2 > data.len() { break; }
            let w_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + w_len > data.len() { break; }
            let word = std::str::from_utf8(&data[cursor..cursor+w_len]).unwrap_or("");
            cursor += w_len;

            if cursor + 2 > data.len() { break; }
            let tr_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + tr_len > data.len() { break; }
            let trad = std::str::from_utf8(&data[cursor..cursor+tr_len]).unwrap_or("");
            cursor += tr_len;
            
            if cursor + 2 > data.len() { break; }
            let t_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + t_len > data.len() { break; }
            let tone = std::str::from_utf8(&data[cursor..cursor+t_len]).unwrap_or("");
            cursor += t_len;

            if cursor + 2 > data.len() { break; }
            let e_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + e_len > data.len() { break; }
            let en = std::str::from_utf8(&data[cursor..cursor+e_len]).unwrap_or("");
            cursor += e_len;

            if cursor + 2 > data.len() { break; }
            let s_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + s_len > data.len() { break; }
            let stroke_aux = std::str::from_utf8(&data[cursor..cursor+s_len]).unwrap_or("");
            cursor += s_len;

            if cursor + 4 > data.len() { break; }
            let weight = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap_or([0; 4]));
            cursor += 4;
            
            results.push(TrieResult { word, trad, tone, en, stroke_aux, weight });
        }
        results
    }
}
