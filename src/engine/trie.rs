use fst::{Map, IntoStreamer, Streamer, Automaton};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

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

    pub fn get_all_exact(&self, pinyin: &str) -> Option<Vec<(String, String, String, String, u32)>> {
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
            if key.len() > prefix.as_bytes().len() {
                return true;
            }
        }
        false
    }

    pub fn search_bfs(&self, prefix: &str, limit: usize) -> Vec<(String, String, String, String, u32)> {
        let mut results = Vec::new();
        let matcher = fst::automaton::Str::new(prefix).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((_, offset)) = stream.next() {
            let pairs = self.read_block(offset as usize);
            for pair in pairs {
                if !results.iter().any(|(w, _, _, _, _)| w == &pair.0) {
                    results.push(pair);
                    if results.len() >= limit { return results; }
                }
            }
        }
        results
    }

    pub fn search_abbreviation(&self, segments: &[String], syllables: &std::collections::HashSet<String>, limit: usize) -> Vec<(String, String, String, String, u32)> {
        if segments.is_empty() { return Vec::new(); }
        let mut results = Vec::new();
        
        // 使用第一个片段作为 FST 检索的前缀，减少搜索范围
        let first_seg = &segments[0];
        let matcher = fst::automaton::Str::new(first_seg).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((key_bytes, offset)) = stream.next() {
            let key = String::from_utf8_lossy(key_bytes);
            if self.matches_segments(&key, segments, syllables) {
                let pairs = self.read_block(offset as usize);
                for pair in pairs {
                    if !results.iter().any(|(w, _, _, _, _)| w == &pair.0) {
                        results.push(pair);
                        if results.len() >= limit { return results; }
                    }
                }
            }
        }
        results
    }

    fn matches_segments(&self, key: &str, segments: &[String], syllables: &std::collections::HashSet<String>) -> bool {
        let mut current_key = key;
        for (i, seg) in segments.iter().enumerate() {
            if current_key.is_empty() { return false; }
            
            if i == segments.len() - 1 {
                return current_key.starts_with(seg);
            }

            // 尝试将当前片段匹配为当前音节的前缀
            let mut found_match = false;
            for len in (1..=6).rev() {
                if len <= current_key.len() {
                    let syl = &current_key[..len];
                    if syllables.contains(syl) {
                        if syl.starts_with(seg) {
                            current_key = &current_key[len..];
                            found_match = true;
                            break;
                        }
                    }
                }
            }
            if !found_match {
                // 特殊处理：如果片段本身不是任何已知完整音节的前缀，
                // 也要允许它尝试作为当前 key 开头的匹配（针对某些不规范切分）
                if current_key.starts_with(seg) {
                   // 尝试跳过该片段对应的逻辑音节长度（简化处理：跳过 seg 长度并寻找下一个音节起始）
                   // 但为了稳妥，这里我们坚持音节对齐
                   return false;
                }
                return false;
            }
        }
        true
    }

    #[allow(dead_code)]
    pub fn get_random_entry(&self) -> Option<(String, String, String, String, u32)> {
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
                return pairs.first().cloned();
            }
            current += 1;
        }
        None
    }

    fn read_block(&self, offset: usize) -> Vec<(String, String, String, String, u32)> {
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
            let word = String::from_utf8_lossy(&data[cursor..cursor+w_len]).to_string();
            cursor += w_len;
            
            if cursor + 2 > data.len() { break; }
            let t_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + t_len > data.len() { break; }
            let tone = String::from_utf8_lossy(&data[cursor..cursor+t_len]).to_string();
            cursor += t_len;

            if cursor + 2 > data.len() { break; }
            let e_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + e_len > data.len() { break; }
            let en = String::from_utf8_lossy(&data[cursor..cursor+e_len]).to_string();
            cursor += e_len;

            if cursor + 2 > data.len() { break; }
            let s_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            if cursor + s_len > data.len() { break; }
            let stroke_aux = String::from_utf8_lossy(&data[cursor..cursor+s_len]).to_string();
            cursor += s_len;

            if cursor + 4 > data.len() { break; }
            let weight = u32::from_le_bytes(data[cursor..cursor+4].try_into().unwrap_or([0; 4]));
            cursor += 4;
            
            results.push((word, tone, en, stroke_aux, weight));
        }
        results
    }
}
