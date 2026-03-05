use fst::MapBuilder;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write, BufRead, BufReader};
use std::path::Path;
use serde_json::Value;
use walkdir::WalkDir;
use std::time::SystemTime;

pub fn check_and_compile_all() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("data")?;
    println!("[Compiler] 正在扫描 dicts 目录...");

    if let Ok(entries) = fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                let src_path = format!("dicts/{}", dir_name);
                let out_dir = format!("data/{}", dir_name);
                
                println!("[Compiler] 检查方案: {}", dir_name);
                fs::create_dir_all(&out_dir)?;
                
                let trie_idx = format!("{}/trie.index", out_dir);
                if should_compile(Path::new(&src_path), Path::new(&trie_idx)) {
                    println!("[Compiler] 方案 [{}] 需要编译，正在执行...", dir_name);
                    let is_english = dir_name.contains("english");
                    let start = std::time::Instant::now();
                    compile_dict_for_path(&src_path, &format!("{}/trie", out_dir), is_english)?;
                    println!("[Compiler] 方案 [{}] 编译完成，耗时 {:?}", dir_name, start.elapsed());
                } else {
                    println!("[Compiler] 方案 [{}] 已是最新，跳过。", dir_name);
                }
            }
        }
    } else {
        println!("[Compiler] 错误：无法读取 dicts 目录！");
    }
    
    if Path::new("dicts/chinese/chars.json").exists() {
        let src = Path::new("dicts/chinese/chars.json");
        let dst = Path::new("dicts/chinese/syllables.txt");
        if should_update_syllables(src, dst) {
            println!("[Compiler] 更新拼音音节表...");
            extract_syllables_to_file("dicts/chinese/chars.json", "dicts/chinese/syllables.txt")?;
        }
    }
    
    if Path::new("dicts/stroke/words/stroke_char.json").exists() {
        let src = Path::new("dicts/stroke/words/stroke_char.json");
        let dst = Path::new("dicts/stroke/syllables.txt");
        if should_update_syllables(src, dst) {
            println!("[Compiler] 更新笔画编码表...");
            extract_syllables_to_file("dicts/stroke/words/stroke_char.json", "dicts/stroke/syllables.txt")?;
        }
    }
    Ok(())
}

fn should_update_syllables(src: &Path, dst: &Path) -> bool {
    !dst.exists() || {
        let src_mtime = src.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let dst_mtime = dst.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        src_mtime > dst_mtime
    }
}

fn should_compile(src_dir: &Path, target_file: &Path) -> bool {
    if !target_file.exists() { return true; } 
    let target_mtime = target_file.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    
    // 检查目录本身的修改时间 (只有当目录比目标文件新时才考虑进一步检查)
    if let Ok(m) = src_dir.metadata().and_then(|m| m.modified()) {
        if m > target_mtime {
            // 目录变了不一定代表内容变了，继续深挖
        } else {
            // 目录都没变，肯定没加减文件
            return false;
        }
    }

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
    
    // 只有源文件明确比编译产物新时（允许 1 秒以内的误差，解决某些打包工具的时间戳舍入问题）
    if let Ok(duration) = max_src_mtime.duration_since(target_mtime) {
        duration.as_secs() >= 1
    } else {
        false // 源文件比产物旧或时间一致
    }
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
    trad: String,
    tone: String,
    en: String,
    stroke_aux: String,
    weight: u32,
}

fn process_json_file(path: &Path, entries: &mut BTreeMap<String, Vec<DictEntry>>, is_english: bool) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let json: Value = serde_json::from_reader(file)?;
    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            // 不再转为小写，保留原始 Key
            if let Some(arr) = val.as_array() {
                if is_english {
                    let en_hint = arr.iter().filter_map(|v| v.as_str()).next().unwrap_or("").to_string();
                    entries.entry(key.clone()).or_default().push(DictEntry {
                        word: key.clone(),
                        trad: key.clone(),
                        tone: String::new(),
                        en: en_hint,
                        stroke_aux: String::new(),
                        weight: 0,
                    });
                } else {
                    for v in arr {
                        if let Some(s) = v.as_str() { 
                            entries.entry(key.clone()).or_default().push(DictEntry {
                                word: s.to_string(),
                                trad: s.to_string(),
                                tone: String::new(),
                                en: String::new(),
                                stroke_aux: String::new(),
                                weight: 0,
                            });
                        }
                        else if let Some(o) = v.as_object() {
                            if let Some(c) = o.get("char").and_then(|c| c.as_str()) {
                                let trad = o.get("trad").and_then(|t| t.as_str()).unwrap_or(c);
                                let en_hint = o.get("en").and_then(|e| e.as_str()).unwrap_or("");
                                let tone_hint = o.get("tone").and_then(|t| t.as_str()).unwrap_or("");
                                let stroke_aux = o.get("stroke_aux").and_then(|s| s.as_str()).unwrap_or("");
                                let weight = o.get("weight").and_then(|w| w.as_u64()).unwrap_or(0) as u32;
                                
                                entries.entry(key.clone()).or_default().push(DictEntry {
                                    word: c.to_string(),
                                    trad: trad.to_string(),
                                    tone: tone_hint.to_string(),
                                    en: en_hint.to_string(),
                                    stroke_aux: stroke_aux.to_string(),
                                    weight,
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
            // 不再转为小写，保留原始拼音大小写
            let pinyin = parts[1].replace(' ', "");
            let weight = if parts.len() >= 3 { parts[2].parse::<u32>().unwrap_or(0) } else { 0 };
            entries.entry(pinyin).or_default().push(DictEntry {
                word: word.clone(),
                trad: word,
                tone: String::new(),
                en: String::new(),
                stroke_aux: String::new(),
                weight,
            });
        }
    }
    Ok(())
}

fn write_binary_dict(idx_path: &str, dat_path: &str, entries: BTreeMap<String, Vec<DictEntry>>) -> Result<(), Box<dyn std::error::Error>> {
    let tmp_idx = format!("{}.tmp", idx_path);
    let tmp_dat = format!("{}.tmp", dat_path);
    
    {
        let mut data_writer = BufWriter::new(File::create(&tmp_dat)?);
        let mut index_builder = MapBuilder::new(File::create(&tmp_idx)?)?;
        let mut current_offset = 0u64;
        for (pinyin, mut pairs) in entries {
            let mut seen = std::collections::HashSet::new();
            pairs.retain(|e| seen.insert(e.word.clone()));
            
            index_builder.insert(&pinyin, current_offset)?;
            let mut block = Vec::new();
            block.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
            for entry in pairs {
                let w_bytes = entry.word.as_bytes(); 
                let tr_bytes = entry.trad.as_bytes();
                let t_bytes = entry.tone.as_bytes();
                let e_bytes = entry.en.as_bytes();
                let s_bytes = entry.stroke_aux.as_bytes();
                
                block.extend_from_slice(&(w_bytes.len() as u16).to_le_bytes());
                block.extend_from_slice(w_bytes);
                block.extend_from_slice(&(tr_bytes.len() as u16).to_le_bytes());
                block.extend_from_slice(tr_bytes);
                block.extend_from_slice(&(t_bytes.len() as u16).to_le_bytes());
                block.extend_from_slice(t_bytes);
                block.extend_from_slice(&(e_bytes.len() as u16).to_le_bytes());
                block.extend_from_slice(e_bytes);
                block.extend_from_slice(&(s_bytes.len() as u16).to_le_bytes());
                block.extend_from_slice(s_bytes);
                block.extend_from_slice(&entry.weight.to_le_bytes());
            }
            data_writer.write_all(&block)?;
            current_offset += block.len() as u64;
        }
        index_builder.finish()?;
    }
    
    // Windows 兼容处理：如果文件正在被 Mmap 映射，rename 会失败
    #[cfg(target_os = "windows")]
    {
        // 尝试先删除旧文件（通常也会失败，但能触发明确的错误）
        let _ = fs::remove_file(idx_path);
        let _ = fs::remove_file(dat_path);
    }

    if let Err(e) = fs::rename(&tmp_idx, idx_path) {
        eprintln!("[Compiler] 无法重命名索引文件 (可能正在被使用): {}", e);
        // 如果 rename 失败，尝试直接拷贝（虽然通常也会失败，但作为最后尝试）
        let _ = fs::copy(&tmp_idx, idx_path);
    }
    if let Err(e) = fs::rename(&tmp_dat, dat_path) {
        eprintln!("[Compiler] 无法重命名数据文件 (可能正在被使用): {}", e);
        let _ = fs::copy(&tmp_dat, dat_path);
    }
    
    let _ = fs::remove_file(tmp_idx);
    let _ = fs::remove_file(tmp_dat);
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
