use fst::MapBuilder;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufWriter, Write, BufRead, BufReader};
use std::path::Path;
use serde_json::Value;
use walkdir::WalkDir;
use std::time::SystemTime;

pub fn check_and_compile_all() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("data")?;

    if let Ok(entries) = fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let src_path = format!("dicts/{}", dir_name);
                let out_dir = format!("data/{}", dir_name);
                fs::create_dir_all(&out_dir)?;
                
                let trie_idx = format!("{}/trie.index", out_dir);
                if should_compile(Path::new(&src_path), Path::new(&trie_idx)) {
                    println!("[Compiler] 检测到变动，正在重新编译方案: {}", dir_name);
                    compile_dict_for_path(&src_path, &format!("{}/trie", out_dir))?;
                }
            }
        }
    }
    
    if Path::new("dicts/chinese/chars.json").exists() {
        let src = Path::new("dicts/chinese/chars.json");
        let dst = Path::new("dicts/chinese/syllables.txt");
        let should_update = !dst.exists() || {
            let src_mtime = src.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            let dst_mtime = dst.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            src_mtime > dst_mtime
        };
        if should_update {
            println!("[Compiler] 更新音节表 (Syllables)...");
            extract_syllables_to_file("dicts/chinese/chars.json", "dicts/chinese/syllables.txt")?;
        }
    }
    Ok(())
}

fn should_compile(src_dir: &Path, target_file: &Path) -> bool {
    if !target_file.exists() { return true; } 
    let target_mtime = target_file.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut max_src_mtime = SystemTime::UNIX_EPOCH;
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            let ext = entry.path().extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext == "json" || ext == "yaml" {
                if let Ok(mtime) = entry.path().metadata().and_then(|m| m.modified()) {
                    if mtime > max_src_mtime { max_src_mtime = mtime; }
                }
            }
        }
    }
    max_src_mtime > target_mtime
}

fn compile_dict_for_path(src_dir: &str, out_stem: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n == "punctuation.json") { continue; }
            process_json_file(path, &mut entries)?;
        } else if path.extension().map_or(false, |ext| ext == "yaml") {
            process_yaml_file(path, &mut entries)?;
        }
    }
    write_binary_dict(&format!("{}.index", out_stem), &format!("{}.data", out_stem), entries)
}

fn process_json_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let json: Value = serde_json::from_reader(file)?;
    if let Some(obj) = json.as_object() {
        for (pinyin, val) in obj {
            let pinyin_lower = pinyin.to_lowercase();
            if let Some(arr) = val.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() { entries.entry(pinyin_lower.clone()).or_default().push((s.to_string(), String::new())); }
                    else if let Some(o) = v.as_object() {
                        if let Some(c) = o.get("char").and_then(|c| c.as_str()) {
                            let hint = o.get("en").and_then(|e| e.as_str()).unwrap_or("").to_string();
                            entries.entry(pinyin_lower.clone()).or_default().push((c.to_string(), hint));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_yaml_file(path: &Path, entries: &mut BTreeMap<String, Vec<(String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut in_data = false;
    for line in reader.lines().flatten() {
        if !in_data { if line.starts_with("...") { in_data = true; } continue; }
        if line.starts_with('#') || line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let word = parts[0].to_string();
            let pinyin = parts[1].replace(' ', "").to_lowercase();
            let weight = if parts.len() >= 3 { parts[2] } else { "" };
            entries.entry(pinyin).or_default().push((word, weight.to_string()));
        }
    }
    Ok(())
}

fn write_binary_dict(idx_path: &str, dat_path: &str, entries: BTreeMap<String, Vec<(String, String)>>) -> Result<(), Box<dyn std::error::Error>> {
    let mut data_writer = BufWriter::new(File::create(dat_path)?);
    let mut index_builder = MapBuilder::new(File::create(idx_path)?)?;
    let mut current_offset = 0u64;
    for (pinyin, mut pairs) in entries {
        let mut seen = std::collections::HashSet::new();
        pairs.retain(|(c, _)| seen.insert(c.clone()));
        index_builder.insert(&pinyin, current_offset)?;
        let mut block = Vec::new();
        block.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
        for (word, hint) in pairs {
            let w_bytes = word.as_bytes(); let h_bytes = hint.as_bytes();
            block.extend_from_slice(&(w_bytes.len() as u16).to_le_bytes());
            block.extend_from_slice(w_bytes);
            block.extend_from_slice(&(h_bytes.len() as u16).to_le_bytes());
            block.extend_from_slice(h_bytes);
        }
        data_writer.write_all(&block)?;
        current_offset += block.len() as u64;
    }
    index_builder.finish()?;
    Ok(())
}

fn extract_syllables_to_file(src_json: &str, out_txt: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(src_json)?;
    let json: Value = serde_json::from_reader(file)?;
    if let Some(obj) = json.as_object() {
        let mut syllables: Vec<_> = obj.keys().cloned().collect();
        syllables.sort();
        let mut f = File::create(out_txt)?;
        for s in syllables { writeln!(f, "{}", s)? };
    }
    Ok(())
}
