use fst::MapBuilder;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write, BufRead, BufReader};
use std::path::Path;
use serde_json::Value;
use walkdir::WalkDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("data")?;

    if let Ok(entries) = fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let src_path = format!("dicts/{}", dir_name);
                let out_dir = format!("data/{}", dir_name);
                fs::create_dir_all(&out_dir)?;
                
                let out_stem = format!("{}/trie", out_dir);
                println!("[Standalone Compiler] 正在编译方案: {}", dir_name);
                let is_english = dir_name.contains("english");
                compile_dict_for_path(&src_path, &out_stem, is_english)?;
            }
        }
    }
    
    if Path::new("dicts/chinese/chars.json").exists() {
        println!("[Standalone Compiler] 更新音节表 (Syllables)...");
        extract_syllables_to_file("dicts/chinese/chars.json", "dicts/chinese/syllables.txt")?;
    }
    Ok(())
}

fn compile_dict_for_path(src_dir: &str, out_stem: &str, is_english: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: BTreeMap<String, Vec<DictEntry>> = BTreeMap::new();
    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            if path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n == "punctuation.json") { continue; }
            process_json_file(path, &mut entries, is_english)?;
        } else if path.extension().map_or(false, |ext| ext == "yaml") {
            process_yaml_file(path, &mut entries)?;
        }
    }
    write_binary_dict(&format!("{}.index", out_stem), &format!("{}.data", out_stem), entries)
}

struct DictEntry {
    word: String,
    tone: String,
    en: String,
    weight: u32,
}

fn process_json_file(path: &Path, entries: &mut BTreeMap<String, Vec<DictEntry>>, is_english: bool) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let json: Value = serde_json::from_reader(file)?;
    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            if let Some(arr) = val.as_array() {
                if is_english {
                    let en_hint = arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ");
                    entries.entry(key.clone()).or_default().push(DictEntry {
                        word: key.clone(), tone: String::new(), en: en_hint, weight: 0,
                    });
                } else {
                    for v in arr {
                        if let Some(s) = v.as_str() { 
                            entries.entry(key.clone()).or_default().push(DictEntry {
                                word: s.to_string(), tone: String::new(), en: String::new(), weight: 0,
                            });
                        }
                        else if let Some(o) = v.as_object() {
                            if let Some(c) = o.get("char").and_then(|c| c.as_str()) {
                                let en_hint = o.get("en").and_then(|e| e.as_str()).unwrap_or("");
                                let tone_hint = o.get("tone").and_then(|t| t.as_str()).unwrap_or("");
                                let weight = o.get("weight").and_then(|w| w.as_u64()).unwrap_or(0) as u32;
                                entries.entry(key.clone()).or_default().push(DictEntry {
                                    word: c.to_string(), tone: tone_hint.to_string(), en: en_hint.to_string(), weight,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_yaml_file(path: &Path, entries: &mut BTreeMap<String, Vec<DictEntry>>) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut in_data = false;
    for line in reader.lines().flatten() {
        if !in_data { if line.starts_with("...") { in_data = true; } continue; }
        if line.starts_with('#') || line.trim().is_empty() { continue; }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let word = parts[0].to_string();
            let pinyin = parts[1].replace(' ', "");
            let weight = if parts.len() >= 3 { parts[2].parse::<u32>().unwrap_or(0) } else { 0 };
            entries.entry(pinyin).or_default().push(DictEntry {
                word, tone: String::new(), en: String::new(), weight,
            });
        }
    }
    Ok(())
}

fn write_binary_dict(idx_path: &str, dat_path: &str, entries: BTreeMap<String, Vec<DictEntry>>) -> Result<(), Box<dyn std::error::Error>> {
    let mut data_writer = BufWriter::new(File::create(dat_path)?);
    let mut index_builder = MapBuilder::new(File::create(idx_path)?)?;
    let mut current_offset = 0u64;
    for (pinyin, mut pairs) in entries {
        let mut seen = HashSet::new();
        pairs.retain(|e| seen.insert(e.word.clone()));
        
        index_builder.insert(&pinyin, current_offset)?;
        let mut block = Vec::new();
        block.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
        for entry in pairs {
            let w_bytes = entry.word.as_bytes(); 
            let t_bytes = entry.tone.as_bytes();
            let e_bytes = entry.en.as_bytes();
            block.extend_from_slice(&(w_bytes.len() as u16).to_le_bytes());
            block.extend_from_slice(w_bytes);
            block.extend_from_slice(&(t_bytes.len() as u16).to_le_bytes());
            block.extend_from_slice(t_bytes);
            block.extend_from_slice(&(e_bytes.len() as u16).to_le_bytes());
            block.extend_from_slice(e_bytes);
            block.extend_from_slice(&entry.weight.to_le_bytes());
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
