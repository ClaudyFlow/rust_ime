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
        // 内部扩大搜索范围，避免字典序靠后的词被埋没
        let internal_scan_limit = limit.max(500);
        let mut results = Vec::with_capacity(limit);
        
        let first_seg = &segments[0];
        let matcher = fst::automaton::Str::new(first_seg).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((key_bytes, offset)) = stream.next() {
            if key_bytes.len() < segments.len() { continue; }

            let key = String::from_utf8_lossy(key_bytes);
            if self.matches_segments(&key, segments, syllables) {
                let pairs = self.read_block(offset as usize);
                for pair in pairs {
                    if !results.iter().any(|tr: &TrieResult| tr.word == pair.word) {
                        results.push(pair);
                        // 达到内部扫描上限才停止
                        if results.len() >= internal_scan_limit { break; }
                    }
                }
            }
            if results.len() >= internal_scan_limit { break; }
        }
        
        // 外部只需要 limit 个，但我们可以返回全部供上层排序，上层会取前 limit
        results
    }

    fn matches_segments(&self, key: &str, segments: &[String], syllables: &std::collections::HashSet<String>) -> bool {
        if segments.is_empty() { return false; }
        self.recursive_match(key, segments, syllables)
    }

    fn recursive_match(&self, key: &str, segments: &[String], syllables: &std::collections::HashSet<String>) -> bool {
        if segments.is_empty() {
            // 所有片段都已匹配，如果此时 key 刚好耗尽，或者是多音节词的部分匹配，返回 true
            return true;
        }

        if key.is_empty() {
            return false;
        }

        let first_seg = &segments[0];
        if first_seg.is_empty() { return self.recursive_match(key, &segments[1..], syllables); }

        // 简拼的核心：当前第一个 segment 必须匹配 key 中一个音节的开头
        // 尝试从当前 key 的起始位置切分出一个合法音节
        let mut found_match = false;
        for len in (1..=6).rev() {
            if len <= key.len() {
                let syl = &key[..len];
                // 如果这是一个已知音节
                if syllables.contains(syl) {
                    // 如果这个音节以当前第一个 segment 开头
                    if syl.starts_with(first_seg) {
                        // 递归尝试匹配剩余部分
                        if self.recursive_match(&key[len..], &segments[1..], syllables) {
                            found_match = true;
                            break;
                        }
                    }
                }
            }
        }
        
        if found_match { return true; }

        // 兜底逻辑：如果由于音节切分不规范导致匹配不到，尝试单字母前缀匹配
        // 如果 key[0..1] 与 first_seg[0..1] 匹配，且 key 后续部分能被递归处理
        // (这应对了一些不规范的全拼或分隔符场景)
        if key.starts_with(&first_seg[..1]) {
            // 跳过 key 中当前的这一个字母，继续寻找下一个音节的匹配
            // 注意：这里需要谨慎处理，以防退化为子串匹配
            // 在输入法语境下，我们通常寻找音节切分点
            // 这里我们尝试找到下一个元音后的辅音作为可能的切分点
            let next_start = key.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            if self.recursive_match(&key[next_start..], &segments[1..], syllables) {
                return true;
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
