use fst::MapBuilder;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use serde_json::Value;
use walkdir::WalkDir;
use std::time::SystemTime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("data")?;

    // 0. Ensure Jianpin Dictionary is up-to-date
    let chars_path = if Path::new("dicts/chinese/chars/chars.json").exists() { "dicts/chinese/chars/chars.json" } else { "dicts/chinese/chars.json" };
    let words_path = if Path::new("dicts/chinese/words/words.json").exists() { "dicts/chinese/words/words.json" } else { "dicts/chinese/words.json" };
    let jianpin_path = if Path::new("dicts/chinese/words/words_jianpin.json").exists() { "dicts/chinese/words/words_jianpin.json" } else { "dicts/chinese/words_jianpin.json" };

    let freq_map = load_frequency_map();

    if Path::new(chars_path).exists() && Path::new(words_path).exists() {
        ensure_jianpin_up_to_date(chars_path, words_path, jianpin_path, &freq_map)?;
    }

    // 1. 自动提取并加载音节表
    if Path::new(chars_path).exists() {
        extract_syllables_to_file(chars_path, "dicts/chinese/syllables.txt")?;
    }
    
    let mut syllables = HashSet::new();
    if let Ok(content) = fs::read_to_string("dicts/chinese/syllables.txt") {
        for line in content.lines() {
            let s = line.trim();
            if !s.is_empty() { syllables.insert(s.to_string()); }
        }
    }

    // 2. 动态扫描 dicts 目录下的所有子目录并编译
    if let Ok(entries) = fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let src_path = format!("dicts/{}", dir_name);
                let out_dir = format!("data/{}", dir_name);
                fs::create_dir_all(&out_dir)?;
                
                let trie_idx = format!("{}/trie.index", out_dir);
                
                // 3. 检查是否需要编译 Trie
                if should_compile(Path::new(&src_path), Path::new(&trie_idx)) {
                    let is_english = dir_name.contains("english");
                    compile_dict_for_path(&src_path, &format!("{}/trie", out_dir), is_english, &syllables, &freq_map)?;
                } else {
                    println!("[Compiler] Skipping Trie for: {} (No changes detected)", dir_name);
                }
            }
        }
    }
    
    Ok(())
}

fn load_frequency_map() -> HashMap<String, u64> {
    let mut map = HashMap::new();
    let rime_dir = Path::new("dicts/rime-ice");
    if !rime_dir.exists() { return map; }
    
    println!("[Compiler] Loading frequency data from {}...", rime_dir.display());
    for entry in WalkDir::new(rime_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "yaml") {
             if let Ok(file) = File::open(path) {
                 use std::io::{BufRead, BufReader};
                 let reader = BufReader::new(file);
                 let mut in_data = false;
                 for line in reader.lines() {
                     if let Ok(l) = line {
                         if !in_data {
                             if l.starts_with("...") { in_data = true; }
                             continue;
                         }
                         if l.starts_with('#') || l.trim().is_empty() { continue; }
                         let parts: Vec<&str> = l.split('\t').collect();
                         if parts.len() >= 3 {
                             let word = parts[0];
                             if let Ok(weight) = parts[2].parse::<u64>() {
                                 let entry = map.entry(word.to_string()).or_insert(0);
                                 if weight > *entry { *entry = weight; }
                             }
                         }
                     }
                 }
             }
        }
    }
    map
}

fn ensure_jianpin_up_to_date(chars_path: &str, words_path: &str, jianpin_path: &str, freq_map: &HashMap<String, u64>) -> Result<(), Box<dyn std::error::Error>> {
    let words = Path::new(words_path);
    let jianpin = Path::new(jianpin_path);
    
    let force_regen = std::env::var("FORCE_REGEN_JIANPIN").is_ok();

    if !words.exists() { return Ok(()); }
    
    let words_mtime = words.metadata()?.modified()?;
    let jianpin_mtime = if jianpin.exists() {
        jianpin.metadata()?.modified().unwrap_or(SystemTime::UNIX_EPOCH)
    } else {
        SystemTime::UNIX_EPOCH
    };
    
    if force_regen || words_mtime > jianpin_mtime {
        println!("[Compiler] Regenerating {} from {} with sorting...", jianpin_path, words_path);
        
        let mut syllables = HashSet::new();
        if let Ok(file) = File::open(chars_path) {
             let json: Value = serde_json::from_reader(file)?;
             if let Some(obj) = json.as_object() {
                 for k in obj.keys() { syllables.insert(k.clone()); }
             }
        }
        
        let file = File::open(words_path)?;
        let words_data: HashMap<String, Value> = serde_json::from_reader(file)?;
        let mut jianpin_map: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        
        for (pinyin, val) in words_data {
             let syls = split_syllables(&pinyin, &syllables);
             let abbr: String = syls.iter().filter_map(|s| s.chars().next()).collect();
             
             if abbr.len() > 1 && abbr != pinyin {
                 let entry = jianpin_map.entry(abbr).or_default();
                 if let Some(arr) = val.as_array() {
                     for v in arr { 
                         let v_str = v.to_string();
                         if !entry.iter().any(|e| e.to_string() == v_str) {
                             entry.push(v.clone());
                         }
                     }
                 } else {
                     let v_str = val.to_string();
                     if !entry.iter().any(|e| e.to_string() == v_str) {
                         entry.push(val.clone());
                     }
                 }
             }
        }

        // Sort candidates
        for (_, candidates) in jianpin_map.iter_mut() {
            candidates.sort_by(|a, b| {
                let get_weight = |v: &Value| -> u64 {
                    if let Some(s) = v.as_str() { return *freq_map.get(s).unwrap_or(&0); }
                    if let Some(o) = v.as_object() {
                        if let Some(c) = o.get("char").and_then(|s| s.as_str()) {
                            return *freq_map.get(c).unwrap_or(&0);
                        }
                    }
                    0
                };
                let w_a = get_weight(a);
                let w_b = get_weight(b);
                w_b.cmp(&w_a) // Descending
            });
        }
        
        let out_file = File::create(jianpin_path)?;
        serde_json::to_writer_pretty(out_file, &jianpin_map)?;
        println!("[Compiler] Generated {}.", jianpin_path);
    }
    
    Ok(())
}

fn split_syllables(pinyin: &str, syllables: &HashSet<String>) -> Vec<String> {
    let mut res = Vec::new();
    let mut i = 0;
    while i < pinyin.len() {
        let mut found = false;
        // Try longest match first (max syllable length is usually 6 like 'zhuang')
        for len in (1..=7).rev() {
            if i + len <= pinyin.len() && pinyin.is_char_boundary(i + len) {
                let sub = &pinyin[i..i+len];
                // 注意：这里比较时使用小写，因为 syllables.txt 通常是小写，但我们要保留原始 pinyin 大小写
                if syllables.contains(&sub.to_lowercase()) {
                    res.push(sub.to_string());
                    i += len;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            if let Some(c) = pinyin[i..].chars().next() {
                res.push(c.to_string());
                i += c.len_utf8();
            } else {
                break;
            }
        }
    }
    res
}

fn should_compile(src_dir: &Path, target_file: &Path) -> bool {
    if !target_file.exists() { return true; } 
    
    let target_mtime = target_file.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    
    // 1. 检查文件夹本身的修改时间 (新增/删除文件会触发)
    if let Ok(dir_mtime) = src_dir.metadata().and_then(|m| m.modified()) {
        if dir_mtime > target_mtime { return true; }
    }

    // 2. 递归检查源目录下所有文件的最大修改时间
    let mut max_src_mtime = SystemTime::UNIX_EPOCH;
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            if let Ok(mtime) = entry.path().metadata().and_then(|m| m.modified()) {
                if mtime > max_src_mtime { max_src_mtime = mtime; }
            }
        }
    }
    
    max_src_mtime > target_mtime
}

fn extract_syllables_to_file(src_json: &str, out_txt: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Compiler] Extracting syllables from {}...", src_json);
    let file = File::open(src_json)?;
    let json: Value = serde_json::from_reader(file)?;
    if let Some(obj) = json.as_object() {
        let mut syllables: Vec<_> = obj.keys().cloned().collect();
        syllables.sort();
        let mut f = File::create(out_txt)?;
        for s in syllables {
            writeln!(f, "{}", s)?;
        }
    }
    Ok(())
}

fn compile_dict_for_path(src_dir: &str, out_stem: &str, is_english: bool, syllables: &HashSet<String>, freq_map: &HashMap<String, u64>) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut abbrev_entries: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    println!("[Compiler] Compiling dictionary from {} -> {}...", src_dir, out_stem);
    
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n == "punctuation.json") {
                continue;
            }
            process_json_file(path, &mut entries, &mut abbrev_entries, is_english, syllables)?;
        } else if path.extension().map_or(false, |ext| ext == "yaml") {
            process_yaml_file(path, &mut entries, &mut abbrev_entries)?;
        } else if path.extension().map_or(false, |ext| ext == "txt") {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if filename == "syllables.txt" || filename == "basic_tokens.txt" { continue; }
            process_txt_file(path, &mut entries, &mut abbrev_entries)?;
        }
    }

    // Sort entries by frequency
    for (_, candidates) in entries.iter_mut() {
        candidates.sort_by(|(word_a, _), (word_b, _)| {
            let w_a = freq_map.get(word_a).unwrap_or(&0);
            let w_b = freq_map.get(word_b).unwrap_or(&0);
            w_b.cmp(w_a)
        });
    }
    
    let idx_path = format!("{}.index", out_stem);
    let dat_path = format!("{}.data", out_stem);
    let out_dir = Path::new(out_stem).parent().unwrap();
    let abbrev_idx_path = out_dir.join("abbrev.index").to_str().unwrap().to_string();

    write_binary_dict(&idx_path, &abbrev_idx_path, &dat_path, entries, abbrev_entries)?;
    println!("[Compiler] Finished: {}", out_stem);
    Ok(())
}

fn process_txt_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String)>>, abbrev_entries: &mut BTreeMap<String, Vec<(String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader};
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if line.starts_with('#') || line.trim().is_empty() { continue; }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let word = parts[0].trim().to_string();
            let pinyin_raw = parts[1].trim();
            // 索引键：保留原始大小写，去掉空格
            let pinyin = pinyin_raw.replace(' ', "");
            
            let hint = if parts.len() >= 3 { 
                parts[2].trim().to_string() 
            } else { 
                pinyin_raw.to_string()
            };

            entries.entry(pinyin).or_default().push((word.clone(), hint.clone()));

            // 提取简拼：保留原始大小写
            let abbrev: String = pinyin_raw.split_whitespace().filter_map(|s| s.chars().next()).collect();
            if abbrev.len() > 1 && abbrev != pinyin_raw.replace(' ', "") {
                abbrev_entries.entry(abbrev).or_default().push((word, hint));
            }
        }
    }
    Ok(())
}

fn process_yaml_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String)>>, abbrev_entries: &mut BTreeMap<String, Vec<(String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader};
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut in_data = false;

    for line in reader.lines() {
        let line = line?;
        if !in_data {
            if line.starts_with("...") { in_data = true; }
            continue;
        }
        if line.starts_with('#') || line.trim().is_empty() { continue; }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let word = parts[0].to_string();
            let pinyin_raw = parts[1].trim();
            let pinyin = pinyin_raw.replace(' ', "");
            let weight = if parts.len() >= 3 { parts[2] } else { "" };
            entries.entry(pinyin).or_default().push((word.clone(), weight.to_string()));

            let abbrev: String = pinyin_raw.split_whitespace().filter_map(|s| s.chars().next()).collect();
            if abbrev.len() > 1 && abbrev != pinyin_raw.replace(' ', "") {
                abbrev_entries.entry(abbrev).or_default().push((word, weight.to_string()));
            }
        }
    }
    Ok(())
}

fn process_json_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String)>>, abbrev_entries: &mut BTreeMap<String, Vec<(String, String)>>, is_english: bool, syllables: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let json: Value = serde_json::from_reader(file)?;
    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            let key_raw = key.as_str();
            
            let abbrev = if !is_english {
                let syls = split_syllables(key_raw, syllables);
                let a: String = syls.iter().filter_map(|s| s.chars().next()).collect();
                if a.len() > 1 && a != key_raw { Some(a) } else { None }
            } else { None };

            if let Some(arr) = val.as_array() {
                if is_english {
                    let hint = arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ");
                    entries.entry(key.clone()).or_default().push((key.clone(), hint));
                } else {
                    for v in arr {
                        if let Some(s) = v.as_str() { 
                            entries.entry(key.clone()).or_default().push((s.to_string(), String::new())); 
                            if let Some(ref a) = abbrev { abbrev_entries.entry(a.clone()).or_default().push((s.to_string(), String::new())); }
                        }
                        else if let Some(o) = v.as_object() {
                            if let Some(c) = o.get("char").and_then(|c| c.as_str()) {
                                let en_hint = o.get("en").and_then(|e| e.as_str()).unwrap_or("");
                                let tone_hint = o.get("tone").and_then(|t| t.as_str()).unwrap_or("");
                                
                                let mut combined_hint = tone_hint.to_string();
                                if !en_hint.is_empty() {
                                    if !combined_hint.is_empty() { combined_hint.push(' '); }
                                    combined_hint.push_str(en_hint);
                                }
                                
                                entries.entry(key.clone()).or_default().push((c.to_string(), combined_hint.clone()));
                                if let Some(ref a) = abbrev { abbrev_entries.entry(a.clone()).or_default().push((c.to_string(), combined_hint)); }
                            }
                        }
                    }
                }
            } else if let Some(s) = val.as_str() {
                if is_english {
                    entries.entry(key.clone()).or_default().push((key.clone(), s.to_string()));
                } else {
                    entries.entry(key.clone()).or_default().push((s.to_string(), String::new()));
                    if let Some(ref a) = abbrev { abbrev_entries.entry(a.clone()).or_default().push((s.to_string(), String::new())); }
                }
            }
        }
    }
    Ok(())
}

fn write_binary_dict(idx_path: &str, abbrev_idx_path: &str, dat_path: &str, entries: BTreeMap<String, Vec<(String, String)>>, abbrev_entries: BTreeMap<String, Vec<(String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    let data_file = File::create(dat_path)?;
    let mut data_writer = BufWriter::new(data_file);
    let mut index_builder = MapBuilder::new(File::create(idx_path)?)?;
    let mut abbrev_index_builder = MapBuilder::new(File::create(abbrev_idx_path)?)?;

    let mut current_offset = 0u64;
    
    // Write normal entries
    for (pinyin, mut pairs) in entries {
        let mut seen = std::collections::HashSet::new();
        pairs.retain(|(c, _)| seen.insert(c.clone()));

        index_builder.insert(&pinyin, current_offset)?;
        let block = encode_block(&pairs);
        data_writer.write_all(&block)?;
        current_offset += block.len() as u64;
    }

    // Write abbrev entries
    for (abbr, mut pairs) in abbrev_entries {
        let mut seen = std::collections::HashSet::new();
        pairs.retain(|(c, _)| seen.insert(c.clone()));

        abbrev_index_builder.insert(&abbr, current_offset)?;
        let block = encode_block(&pairs);
        data_writer.write_all(&block)?;
        current_offset += block.len() as u64;
    }

    index_builder.finish()?;
    abbrev_index_builder.finish()?;
    data_writer.flush()?;
    Ok(())
}

fn encode_block(pairs: &[(String, String)]) -> Vec<u8> {
    let mut block = Vec::new();
    block.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
    for (word, hint) in pairs {
        let w_bytes = word.as_bytes();
        let h_bytes = hint.as_bytes();
        block.extend_from_slice(&(w_bytes.len() as u16).to_le_bytes());
        block.extend_from_slice(w_bytes);
        block.extend_from_slice(&(h_bytes.len() as u16).to_le_bytes());
        block.extend_from_slice(h_bytes);
    }
    block
}