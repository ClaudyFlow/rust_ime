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
    let chars_path = "dicts/chinese/chars/chars.json";
    let words_path = "dicts/chinese/words/words.json";
    let jianpin_path = "dicts/chinese/words/words_jianpin.json";

    let freq_map = load_frequency_map();

    if Path::new(chars_path).exists() && Path::new(words_path).exists() {
        ensure_jianpin_up_to_date(chars_path, words_path, jianpin_path, &freq_map)?;
    }

    // 1. 提取并加载音节表
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

    // 2. 扫描词库
    if let Ok(entries) = fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let src_path = format!("dicts/{}", dir_name);
                let out_dir = format!("data/{}", dir_name);
                fs::create_dir_all(&out_dir)?;
                
                let trie_idx = format!("{}/trie.index", out_dir);
                if should_compile(Path::new(&src_path), Path::new(&trie_idx)) {
                    let is_english = dir_name.contains("english");
                    compile_dict_for_path(&src_path, &format!("{}/trie", out_dir), is_english, &syllables, &freq_map)?;
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
    
    for entry in WalkDir::new(rime_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "yaml") {
             if let Ok(file) = File::open(path) {
                 use std::io::{BufRead, BufReader};
                 let reader = BufReader::new(file);
                 let mut in_data = false;
                 for line in reader.lines() {
                     if let Ok(l) = line {
                         if !in_data { if l.starts_with("...") { in_data = true; } continue; }
                         if l.starts_with('#') || l.trim().is_empty() { continue; }
                         let parts: Vec<&str> = l.split('\t').collect();
                         if parts.len() >= 3 {
                             if let Ok(weight) = parts[2].parse::<u64>() {
                                 let entry = map.entry(parts[0].to_string()).or_insert(0);
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
    if !words.exists() { return Ok(()); }
    let words_mtime = words.metadata()?.modified()?;
    let jianpin_mtime = if jianpin.exists() { jianpin.metadata()?.modified().unwrap_or(SystemTime::UNIX_EPOCH) } else { SystemTime::UNIX_EPOCH };
    
    if words_mtime > jianpin_mtime || std::env::var("FORCE_REGEN_JIANPIN").is_ok() {
        println!("[Compiler] Regenerating Jianpin JSON...");
        let mut syllables = HashSet::new();
        if let Ok(file) = File::open(chars_path) {
             let json: Value = serde_json::from_reader(file)?;
             if let Some(obj) = json.as_object() { for k in obj.keys() { syllables.insert(k.clone()); } }
        }
        let file = File::open(words_path)?;
        let words_data: HashMap<String, Value> = serde_json::from_reader(file)?;
        let mut jianpin_map: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (pinyin, val) in words_data {
             let syls = split_syllables(&pinyin, &syllables);
             let abbr: String = syls.iter().filter_map(|s| s.chars().next()).collect();
             if abbr.len() > 1 && abbr != pinyin {
                 let entry = jianpin_map.entry(abbr).or_default();
                 if let Some(arr) = val.as_array() { for v in arr { if !entry.contains(v) { entry.push(v.clone()); } } } 
                 else { if !entry.contains(&val) { entry.push(val.clone()); } }
             }
        }
        for (_, candidates) in jianpin_map.iter_mut() {
            candidates.sort_by(|a, b| {
                let get_w = |v: &Value| {
                    v.as_str().and_then(|s| freq_map.get(s)).or_else(|| v.get("char").and_then(|c| c.as_str()).and_then(|c| freq_map.get(c))).copied().unwrap_or(0)
                };
                get_w(b).cmp(&get_w(a))
            });
        }
        serde_json::to_writer_pretty(File::create(jianpin_path)?, &jianpin_map)?;
    }
    Ok(())
}

fn split_syllables(pinyin: &str, syllables: &HashSet<String>) -> Vec<String> {
    let mut res = Vec::new();
    let mut i = 0;
    while i < pinyin.len() {
        let mut found = false;
        for len in (1..=7).rev() {
            if i + len <= pinyin.len() && pinyin.is_char_boundary(i + len) {
                let sub = &pinyin[i..i+len];
                if syllables.contains(&sub.to_lowercase()) {
                    res.push(sub.to_string()); i += len; found = true; break;
                }
            }
        }
        if !found {
            if let Some(c) = pinyin[i..].chars().next() { res.push(c.to_string()); i += c.len_utf8(); } else { break; }
        }
    }
    res
}

fn should_compile(src_dir: &Path, target_file: &Path) -> bool {
    if !target_file.exists() { return true; }
    let target_mtime = target_file.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut max_src_mtime = SystemTime::UNIX_EPOCH;
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            if let Ok(mtime) = entry.path().metadata().and_then(|m| m.modified()) { if mtime > max_src_mtime { max_src_mtime = mtime; } }
        }
    }
    max_src_mtime > target_mtime
}

fn extract_syllables_to_file(src_json: &str, out_txt: &str) -> Result<(), Box<dyn std::error::Error>> {
    let json: Value = serde_json::from_reader(File::open(src_json)?)?;
    if let Some(obj) = json.as_object() {
        let mut syllables: Vec<_> = obj.keys().cloned().collect();
        syllables.sort();
        let mut f = File::create(out_txt)?;
        for s in syllables { writeln!(f, "{}", s)?;
        }
    }
    Ok(())
}

fn compile_dict_for_path(src_dir: &str, out_stem: &str, is_english: bool, syllables: &HashSet<String>, freq_map: &HashMap<String, u64>) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    let mut abbrev_entries: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n == "punctuation.json") { continue; }
            process_json_file(path, &mut entries, &mut abbrev_entries, is_english, syllables)?;
        } else if path.extension().map_or(false, |ext| ext == "yaml") {
            process_yaml_file(path, &mut entries, &mut abbrev_entries)?;
        }
    }
    for (_, candidates) in entries.iter_mut() {
        candidates.sort_by(|(wa, _, _), (wb, _, _)| freq_map.get(wb).unwrap_or(&0).cmp(freq_map.get(wa).unwrap_or(&0)));
    }
    let out_dir = Path::new(out_stem).parent().unwrap();
    write_binary_dict(&format!("{}.index", out_stem), &out_dir.join("abbrev.index").to_str().unwrap(), &format!("{}.data", out_stem), entries, abbrev_entries)?;
    Ok(())
}

fn process_yaml_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String, String)>>, abbrev_entries: &mut BTreeMap<String, Vec<(String, String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(File::open(path)?);
    let mut in_data = false;
    for line in reader.lines() {
        let line = line?;
        if !in_data { if line.starts_with("...") { in_data = true; } continue; }
        if line.starts_with('#') || line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let pinyin = parts[1].replace(' ', "");
            // 修复：不将权重存入二进制数据的英文释义字段，仅将其设为空
            entries.entry(pinyin.clone()).or_default().push((parts[0].to_string(), String::new(), String::new()));
            
            let abbrev: String = parts[1].split_whitespace().filter_map(|s| s.chars().next()).collect();
            if abbrev.len() > 1 && abbrev != pinyin {
                abbrev_entries.entry(abbrev).or_default().push((parts[0].to_string(), String::new(), String::new()));
            }
        }
    }
    Ok(())
}

fn process_json_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String, String)>>, abbrev_entries: &mut BTreeMap<String, Vec<(String, String, String)>>, is_english: bool, syllables: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
    let json: Value = serde_json::from_reader(File::open(path)?)?;
    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            let abbrev = if !is_english {
                let syls = split_syllables(key, syllables);
                let a: String = syls.iter().filter_map(|s| s.chars().next()).collect();
                if a.len() > 1 && a != *key { Some(a) } else { None }
            } else { None };
            if let Some(arr) = val.as_array() {
                if is_english {
                    let hint = arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ");
                    entries.entry(key.clone()).or_default().push((key.clone(), String::new(), hint));
                } else {
                    for v in arr {
                        if let Some(s) = v.as_str() { 
                            entries.entry(key.clone()).or_default().push((s.to_string(), String::new(), String::new()));
                            if let Some(ref a) = abbrev { abbrev_entries.entry(a.clone()).or_default().push((s.to_string(), String::new(), String::new())); }
                        } else if let Some(o) = v.as_object() {
                            if let Some(c) = o.get("char").and_then(|c| c.as_str()) {
                                let en = o.get("en").and_then(|e| e.as_str()).unwrap_or("").to_string();
                                let tone = o.get("tone").and_then(|t| t.as_str()).unwrap_or("").to_string();
                                entries.entry(key.clone()).or_default().push((c.to_string(), tone.clone(), en.clone()));
                                if let Some(ref a) = abbrev { abbrev_entries.entry(a.clone()).or_default().push((c.to_string(), tone, en)); }
                            }
                        }
                    }
                }
            } else if let Some(s) = val.as_str() {
                if is_english { entries.entry(key.clone()).or_default().push((key.clone(), String::new(), s.to_string())); } 
                else {
                    entries.entry(key.clone()).or_default().push((s.to_string(), String::new(), String::new()));
                    if let Some(ref a) = abbrev { abbrev_entries.entry(a.clone()).or_default().push((s.to_string(), String::new(), String::new())); }
                }
            }
        }
    }
    Ok(())
}

fn write_binary_dict(idx_path: &str, abbrev_idx_path: &str, dat_path: &str, entries: BTreeMap<String, Vec<(String, String, String)>>, abbrev_entries: BTreeMap<String, Vec<(String, String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    let mut data_writer = BufWriter::new(File::create(dat_path)?);
    let mut idx_builder = MapBuilder::new(File::create(idx_path)?)?;
    let mut ab_idx_builder = MapBuilder::new(File::create(abbrev_idx_path)?)?;
    let mut offset = 0u64;
    for (k, mut pairs) in entries {
        let mut seen = HashSet::new(); pairs.retain(|(c, _, _)| seen.insert(c.clone()));
        idx_builder.insert(&k, offset)?;
        let block = encode_block(&pairs); data_writer.write_all(&block)?;
        offset += block.len() as u64;
    }
    for (k, mut pairs) in abbrev_entries {
        let mut seen = HashSet::new(); pairs.retain(|(c, _, _)| seen.insert(c.clone()));
        ab_idx_builder.insert(&k, offset)?;
        let block = encode_block(&pairs); data_writer.write_all(&block)?;
        offset += block.len() as u64;
    }
    idx_builder.finish()?; ab_idx_builder.finish()?; data_writer.flush()?;
    Ok(())
}

fn encode_block(pairs: &[(String, String, String)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
    for (w, t, e) in pairs {
        let wb = w.as_bytes(); let tb = t.as_bytes(); let eb = e.as_bytes();
        b.extend_from_slice(&(wb.len() as u16).to_le_bytes()); b.extend_from_slice(wb);
        b.extend_from_slice(&(tb.len() as u16).to_le_bytes()); b.extend_from_slice(tb);
        b.extend_from_slice(&(eb.len() as u16).to_le_bytes()); b.extend_from_slice(eb);
    }
    b
}