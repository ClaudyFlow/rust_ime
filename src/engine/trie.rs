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
    abbrev_index: Option<Map<MmapData>>,
    data: MmapData,
}

impl Trie {
    pub fn load<P: AsRef<Path>>(index_path: P, data_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let index_file = File::open(&index_path)?;
        let data_file = File::open(&data_path)?;
        let index_data = MmapData(Arc::new(unsafe { Mmap::map(&index_file)? }));
        let data_data = MmapData(Arc::new(unsafe { Mmap::map(&data_file)? }));
        let index = Map::new(index_data)?;

        // Try to load abbreviation index if it exists in the same directory
        let abbrev_index = if let Some(parent) = index_path.as_ref().parent() {
            let abbrev_path = parent.join("abbrev.index");
            if abbrev_path.exists() {
                let abbrev_file = File::open(abbrev_path)?;
                let abbrev_data = MmapData(Arc::new(unsafe { Mmap::map(&abbrev_file)? }));
                Map::new(abbrev_data).ok()
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self { index, abbrev_index, data: data_data })
    }

    pub fn get_all_exact(&self, pinyin: &str) -> Option<Vec<(String, String)>> {
        let offset = self.index.get(pinyin)? as usize;
        Some(self.read_block(offset))
    }

    pub fn get_all_abbrev(&self, abbr: &str) -> Option<Vec<(String, String)>> {
        let abbrev_index = self.abbrev_index.as_ref()?;
        let offset = abbrev_index.get(abbr)? as usize;
        Some(self.read_block(offset))
    }

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

    pub fn search_abbreviation(&self, abbr: &str, limit: usize) -> Vec<(String, String)> {
        let mut results = Vec::new();
        if abbr.is_empty() { return results; }
        
        // 性能优化：只搜索以 abbr 第一个字母开头的路径
        let first_char = abbr.chars().next().unwrap().to_string();
        let matcher = fst::automaton::Str::new(&first_char).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((key_bytes, offset)) = stream.next() {
            let key = String::from_utf8_lossy(key_bytes);
            if self.is_abbreviation_match(abbr, &key) {
                let pairs = self.read_block(offset as usize);
                for pair in pairs {
                    if !results.iter().any(|(w, _)| w == &pair.0) {
                        results.push(pair);
                        if results.len() >= limit { return results; }
                    }
                }
            }
        }
        results
    }

    fn is_abbreviation_match(&self, abbr: &str, pinyin: &str) -> bool {
        let abbr_chars: Vec<char> = abbr.chars().collect();
        let py_chars: Vec<char> = pinyin.chars().collect();
        
        // 规则 1：首字母必须绝对匹配
        if abbr_chars[0] != py_chars[0] { return false; }
        
        let mut py_idx = 1;
        for &ac in &abbr_chars[1..] {
            let mut found = false;
            while py_idx < py_chars.len() {
                if py_chars[py_idx] == ac {
                    // 启发式：判断当前 py_chars[py_idx] 是否可能是音节开头
                    // 1. 如果是 'h'，且前一个字母是 'z/c/s'，则它属于 zh/ch/sh，不是新音节开头
                    let is_h_of_zh_ch_sh = ac == 'h' && py_idx > 0 && 
                        (py_chars[py_idx-1] == 'z' || py_chars[py_idx-1] == 'c' || py_chars[py_idx-1] == 's');
                    
                    if !is_h_of_zh_ch_sh {
                        found = true;
                        py_idx += 1; // 匹配成功，继续寻找下一个
                        break;
                    }
                }
                py_idx += 1;
            }
            if !found { return false; }
        }
        true
    }

    pub fn search_bfs(&self, prefix: &str, limit: usize) -> Vec<(String, String)> {
        let mut results = Vec::new();
        let matcher = fst::automaton::Str::new(prefix).starts_with();
        let mut stream = self.index.search(matcher).into_stream();

        while let Some((_, offset)) = stream.next() {
            let pairs = self.read_block(offset as usize);
            for pair in pairs {
                if !results.iter().any(|(w, _)| w == &pair.0) {
                    results.push(pair);
                    if results.len() >= limit { return results; }
                }
            }
        }
        results
    }

    #[allow(dead_code)]
    pub fn get_random_entry(&self) -> Option<(String, String)> {
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

    fn read_block(&self, offset: usize) -> Vec<(String, String)> {
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
            let h_len = u16::from_le_bytes(data[cursor..cursor+2].try_into().unwrap_or([0; 2])) as usize;
            cursor += 2;
            
            if cursor + h_len > data.len() { break; }
            let hint = String::from_utf8_lossy(&data[cursor..cursor+h_len]).to_string();
            cursor += h_len;
            
            results.push((word, hint));
        }
        results
    }
}