#[cfg(target_os = "windows")]
pub mod evdev {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(non_camel_case_types)]
    #[repr(u32)]
    pub enum Key {
        KEY_A = 0, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J,
        KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P, KEY_Q, KEY_R, KEY_S, KEY_T,
        KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
        KEY_0, KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9,
        KEY_SPACE, KEY_ENTER, KEY_TAB, KEY_BACKSPACE, KEY_ESC, KEY_CAPSLOCK,
        KEY_LEFTCTRL, KEY_RIGHTCTRL, KEY_LEFTSHIFT, KEY_RIGHTSHIFT,
        KEY_LEFTALT, KEY_RIGHTALT, KEY_LEFTMETA, KEY_RIGHTMETA,
        KEY_GRAVE, KEY_MINUS, KEY_EQUAL, KEY_LEFTBRACE, KEY_RIGHTBRACE,
        KEY_BACKSLASH, KEY_SEMICOLON, KEY_APOSTROPHE, KEY_COMMA, KEY_DOT, KEY_SLASH,
        KEY_LEFT, KEY_RIGHT, KEY_UP, KEY_DOWN,
        KEY_PAGEUP, KEY_PAGEDOWN, KEY_HOME, KEY_END, KEY_DELETE,
    }
}

#[cfg(target_os = "windows")]
pub mod registry;

#[cfg(target_os = "windows")]
pub const IME_ID: windows::core::GUID = windows::core::GUID::from_u128(0xc03c9525_2c5e_4959_9988_51787281d523);
#[cfg(target_os = "windows")]
pub const LANG_PROFILE_ID: windows::core::GUID = windows::core::GUID::from_u128(0xc03c9525_2c5e_4959_9988_51787281d524);

mod engine;
mod platform;
mod ui;
mod config;

use std::fs::File;
use std::sync::{Arc, RwLock, Mutex};
use std::path::{Path, PathBuf};
use std::env;
use std::collections::HashMap;
use std::io::{BufReader, Write};
#[cfg(target_os = "linux")]
use signal_hook::consts::signal::*;
#[cfg(target_os = "linux")]
use signal_hook::iterator::Signals;
#[cfg(target_os = "linux")]
use daemonize::Daemonize;
use std::process::Command;

use engine::{Processor, Trie};
use platform::traits::InputMethodHost;
pub use config::Config;
use ui::GuiEvent;
use serde_json::Value;

static WEB_SERVER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn find_project_root() -> PathBuf {
    if let Ok(mut exe_path) = env::current_exe() {
        exe_path.pop();
        if exe_path.join("dicts").exists() { return exe_path; }
    }
    
    let mut curr = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..3 {
        if curr.join("dicts").exists() { return curr; }
        if !curr.pop() { break; }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn load_punctuation_dict(p: &str) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    if let Ok(f) = File::open(p) { 
        if let Ok(v) = serde_json::from_reader::<_, Value>(BufReader::new(f)) {
            if let Some(obj) = v.as_object() { 
                for (k, val) in obj { 
                    m.insert(k.clone(), val.clone());
                } 
            }
        } 
    } 
    m
}

pub fn load_syllables(root: &Path) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let path = root.join("dicts/chinese/syllables.txt");
    if let Ok(f) = File::open(&path) {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(f);
        for line in reader.lines().flatten() {
            let s = line.trim().to_lowercase();
            if !s.is_empty() {
                set.insert(s);
            }
        }
    }
    set
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    std::env::set_var("SLINT_BACKEND", "software");

    #[cfg(target_os = "windows")]
    let _mutex_handle = unsafe {
        use windows::Win32::System::Threading::*;
        use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        use windows::core::PCWSTR;

        let name = PCWSTR("Global\\RustImeUniqueMutex\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
        let handle = CreateMutexW(None, true, name)?;
        if let Err(e) = windows::Win32::Foundation::GetLastError() {
            if e.code() == ERROR_ALREADY_EXISTS.to_hresult() {
                let _ = notify_rust::Notification::new()
                    .summary("Rust IME")
                    .body("程序已经在运行中。")
                    .appname("Rust IME")
                    .timeout(notify_rust::Timeout::Milliseconds(3000))
                    .show();
                return Ok(());
            }
        }
        handle
    };

    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::HiDpi::*;
        let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
    }

    let root = if let Ok(mut exe_path) = std::env::current_exe() {
        exe_path.pop();
        if exe_path.join("dicts").exists() {
            exe_path
        } else {
            find_project_root()
        }
    } else {
        find_project_root()
    };
    env::set_current_dir(&root)?;

    let args: Vec<String> = env::args().collect();
    let mut should_daemonize = true;

    if args.len() > 1 {
        match args[1].as_str() {
            "--compile-only" => {
                println!("[Main] 正在强制编译词库...");
                match engine::compiler::check_and_compile_all() {
                    Ok(_) => println!("✅ 词库编译成功。"),
                    Err(e) => eprintln!("❌ 编译失败: {}", e),
                }
                return Ok(());
            }
            "--install" => {
                setup_autostart()?;
                println!("✅ 已设置开机自启。");
                return Ok(());
            }
            #[cfg(target_os = "windows")]
            "--register" => {
                unsafe {
                    windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)?;
                }
                
                let mut dll_path = std::env::current_exe()?;
                dll_path.set_file_name("rust_ime_tsf_v3.dll");
                
                if !dll_path.exists() {
                    let mut fallback = std::env::current_dir()?;
                    fallback.push("target");
                    fallback.push("release");
                    fallback.push("rust_ime_tsf_v3.dll");
                    if !fallback.exists() {
                        fallback.pop(); fallback.pop();
                        fallback.push("debug");
                        fallback.push("rust_ime_tsf_v3.dll");
                    }
                    if fallback.exists() { dll_path = fallback; }
                }

                if !dll_path.exists() {
                    eprintln!("❌ Error: Could not find 'rust_ime_tsf_v3.dll'.");
                    return Err("DLL not found".into());
                }
                
                println!("Registering TSF from: {:?}", dll_path);
                let path_str = dll_path.to_str().ok_or("Path contains invalid UTF-8")?;
                unsafe {
                    registry::register_server(windows::Win32::Foundation::HINSTANCE(0), &IME_ID, "Rust IME", Some(path_str))?;
                }
                println!("✅ TSF Input Method registered successfully.");
                return Ok(());
            }
            #[cfg(target_os = "windows")]
            "--unregister" => {
                unsafe {
                    windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)?;
                    registry::unregister_server(&IME_ID)?;
                }
                println!("✅ 已注销 TSF 输入法。");
                return Ok(());
            }
            "--foreground" => { should_daemonize = false; }
            "--daemon" => { should_daemonize = true; }
            "--stop" => {
                #[cfg(target_os = "linux")]
                let _ = Command::new("pkill").arg("-f").arg("rust-ime").status();
                #[cfg(target_os = "windows")]
                let _ = Command::new("taskkill").arg("/F").arg("/IM").arg("rust-ime.exe").arg("/T").status();
                println!("✅ 已停止后台进程。");
                return Ok(());
            }
            _ if !args[1].starts_with('-') => {
                let mut tries_map = HashMap::new();
                if let Ok(entries) = std::fs::read_dir("data") {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            let dir_name = entry.file_name().to_string_lossy().to_string().to_lowercase();
                            if dir_name == "ngram" || dir_name.contains("user_adapter") { continue; }
                            let trie_idx = entry.path().join("trie.index");
                            let trie_dat = entry.path().join("trie.data");
                            if trie_idx.exists() && trie_dat.exists() {
                                if let Ok(trie) = Trie::load(&trie_idx, &trie_dat) {
                                    tries_map.insert(dir_name.clone(), trie);
                                }
                            }
                        }
                    }
                }
                let input = args[1..].join(" ");
                let mut p = Processor::new(tries_map, "chinese".into(), HashMap::new(), HashMap::new());
                p.buffer = input;
                p.lookup();
                for (i, cand) in p.candidates.iter().take(10).enumerate() {
                    println!("  {}. {}", i + 1, cand);
                }
                return Ok(());
            }
            _ => {}
        }
    }

    if should_daemonize {
        #[cfg(target_os = "linux")]
        {
            let stdout = File::create("/tmp/rust-ime.out")?;
            let stderr = File::create("/tmp/rust-ime.err")?;
            let daemonize = Daemonize::new()
                .working_directory(&root)
                .stdout(stdout)
                .stderr(stderr);
            match daemonize.start() {
                Ok(_) => println!("✅ 已转入后台运行。"),
                Err(e) => {
                    eprintln!("❌ 无法启动后台模式: {}", e);
                    return Err(e.into());
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Console::GetConsoleWindow;
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            let window = unsafe { GetConsoleWindow() };
            if window.0 != 0 { unsafe { ShowWindow(window, SW_HIDE); } }
            println!("✅ 已转入后台运行 (窗口已隐藏)。");
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut signals = Signals::new(&[SIGHUP])?;
        std::thread::spawn(move || { for _ in signals.forever() {} });
    }

    match engine::compiler::check_and_compile_all() {
        Ok(_) => println!("[Main] 词库同步完成。"),
        Err(e) => eprintln!("[Main] 词库自动编译失败: {}", e),
    }

    let mut current_config = Config::load();
    if current_config.input.autostart { let _ = setup_autostart(); }

    // 自动扫描并加载词库方案到 files.json
    let mut profiles_changed = false;
    if let Ok(entries) = std::fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !current_config.files.profiles.iter().any(|p| p.name == name) {
                    current_config.files.profiles.push(config::Profile {
                        name: name.clone(),
                        path: format!("data/{}/trie", name),
                    });
                    profiles_changed = true;
                }
            }
        }
    }
    if profiles_changed { let _ = current_config.save(); }

    // 加载标点符号
    {
        let mut punctuations = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(root.join("dicts")) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let lang = entry.file_name().to_string_lossy().to_string();
                    let punc_file = entry.path().join("punctuation.json");
                    if punc_file.exists() {
                        let raw = load_punctuation_dict(&punc_file.to_string_lossy());
                        let mut lang_punc = HashMap::new();
                        for (k, v) in raw {
                            if let Some(arr) = v.as_array() {
                                let entries: Vec<config::PunctuationEntry> = arr.iter().filter_map(|item| {
                                    let c = item.get("char").and_then(|c| c.as_str())?;
                                    let d = item.get("desc").and_then(|d| d.as_str()).unwrap_or("");
                                    Some(config::PunctuationEntry { char: c.to_string(), desc: d.to_string() })
                                }).collect();
                                lang_punc.insert(k, entries);
                            }
                        }
                        punctuations.insert(lang, lang_punc);
                    }
                }
            }
        }
        current_config.input.punctuations = punctuations;
    }

    let config = Arc::new(RwLock::new(current_config));
    let (gui_tx, gui_rx) = std::sync::mpsc::channel();
    let (tray_tx, tray_rx) = std::sync::mpsc::channel();
    
    let gui_config = config.read().unwrap().clone();
    let gui_tx_main = gui_tx.clone();
    let tray_tx_gui = tray_tx.clone();
    std::thread::spawn(move || { ui::gui::start_gui(gui_rx, gui_config, tray_tx_gui); });

    let mut tries_map = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("data") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string().to_lowercase();
                if dir_name == "ngram" || dir_name.contains("user_adapter") { continue; }
                let trie_idx = entry.path().join("trie.index");
                let trie_dat = entry.path().join("trie.data");
                if trie_idx.exists() && trie_dat.exists() {
                    if let Ok(trie) = Trie::load(&trie_idx, &trie_dat) {
                        tries_map.insert(dir_name.clone(), trie);
                    }
                }
            }
        }
    }
    let tries_arc = Arc::new(RwLock::new(tries_map.clone()));

    let conf_guard = config.read().unwrap();
    let mut default_profile = conf_guard.input.default_profile.to_lowercase();
    if default_profile.is_empty() || !tries_map.contains_key(&default_profile) {
        if tries_map.contains_key("chinese") { default_profile = "chinese".to_string(); }
        else if let Some(k) = tries_map.keys().next() { default_profile = k.clone(); }
    }
    
    let mut processor_obj = Processor::new(tries_map, default_profile, conf_guard.input.punctuations.clone(), conf_guard.input.keyboard_layouts.clone());
    processor_obj.apply_config(&conf_guard);
    processor_obj.set_syllables(load_syllables(&root));    
    let processor = Arc::new(Mutex::new(processor_obj));
    drop(conf_guard);

    let actual_web_port = Arc::new(std::sync::atomic::AtomicU16::new(18765));
    let conf = config.read().unwrap();
    let tray_handle = ui::tray::start_tray(false, conf.input.default_profile.clone(), conf.appearance.show_candidates, conf.input.anti_typo_mode, conf.input.enable_double_pinyin, conf.input.commit_mode.clone(), conf.appearance.preview_mode.clone(), conf.appearance.candidate_layout.clone(), tray_tx.clone());
    drop(conf);

    let processor_clone = processor.clone();
    let gui_tx_tray = gui_tx.clone();
    let config_tray = config.clone();
    let actual_web_port_tray = actual_web_port.clone();
    let tries_tray = tries_arc.clone();
    let tray_tx_for_web = tray_tx.clone();
    
    std::thread::spawn(move || {
        while let Ok(event) = tray_rx.recv() {
            match event {
                ui::tray::TrayEvent::ToggleIme => {
                    let (short, enabled) = {
                        let mut p = processor_clone.lock().unwrap();
                        p.toggle(); (p.get_short_display(), p.chinese_enabled)
                    };
                    tray_handle.update(|t| t.chinese_enabled = enabled);
                    let _ = gui_tx_tray.send(GuiEvent::ShowStatus(short, enabled));
                    let _ = gui_tx_tray.send(GuiEvent::SetVisible(true));
                    let _ = gui_tx_tray.send(GuiEvent::Update { pinyin: "".into(), candidates: vec![], hints: vec![], selected: 0, sentence: "".into(), cursor_pos: 0, commit_mode: "single".into() });
                }
                ui::tray::TrayEvent::NextProfile => {
                    let (profile, short, enabled) = {
                        let mut p = processor_clone.lock().unwrap();
                        let profile = p.next_profile();
                        (profile, p.get_short_display(), p.chinese_enabled)
                    };
                    tray_handle.update(|t| t.active_profile = profile);
                    let _ = gui_tx_tray.send(GuiEvent::ShowStatus(short, enabled));
                }
                ui::tray::TrayEvent::ReloadConfig => {
                    let new_conf = Config::load();
                    let enabled = {
                        let mut p = processor_clone.lock().unwrap();
                        p.apply_config(&new_conf);
                        if !p.buffer.is_empty() { p.lookup(); }
                        let _ = gui_tx_tray.send(GuiEvent::Update { pinyin: p.buffer.clone(), candidates: p.candidates.clone(), hints: p.candidate_hints.clone(), selected: p.selected, cursor_pos: p.cursor_pos, sentence: p.joined_sentence.clone(), commit_mode: p.commit_mode.clone() });
                        p.chinese_enabled
                    };
                    let display = { processor_clone.lock().unwrap().get_current_profile_display() };
                    let short_display = match display.to_lowercase().as_str() {
                        "chinese" => "中", "english" => "英", "japanese" => "日", "mixed" => "混",
                        _ => if display.len() > 1 { &display[..1] } else { &display }
                    };
                    let _ = gui_tx_tray.send(GuiEvent::ShowStatus(short_display.into(), enabled));
                    let _ = gui_tx_tray.send(GuiEvent::ApplyConfig(new_conf.clone()));
                    tray_handle.update(|t| { t.chinese_enabled = new_conf.input.default_profile == "chinese"; t.active_profile = new_conf.input.default_profile.clone(); });
                    if let Ok(mut w) = config_tray.write() { *w = new_conf; }
                }
                ui::tray::TrayEvent::OpenConfig => {
                    if !WEB_SERVER_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
                        WEB_SERVER_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
                        let config_web = config_tray.clone();
                        let tries_web = tries_tray.clone();
                        let tray_tx_web = tray_tx_for_web.clone();
                        let actual_web_port_web = actual_web_port_tray.clone();
                        std::thread::spawn(move || {
                            if let Ok(rt) = tokio::runtime::Runtime::new() {
                                rt.block_on(async {
                                    let server = ui::web::WebServer::new(18765, actual_web_port_web, config_web, tries_web, tray_tx_web);
                                    server.start().await;
                                });
                            }
                        });
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    let port = actual_web_port_tray.load(std::sync::atomic::Ordering::SeqCst);
                    let url = format!("http://localhost:{}", port);
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("cmd").arg("/c").arg("start").arg(&url).spawn();
                }
                ui::tray::TrayEvent::SyncStatus { chinese_enabled, active_profile } => { tray_handle.update(|t| { t.chinese_enabled = chinese_enabled; t.active_profile = active_profile; }); }
                ui::tray::TrayEvent::ClearUserDict => { let mut p = processor_clone.lock().unwrap(); p.user_dict.clear(); }
                ui::tray::TrayEvent::RequestMenu { x, y } => {
                    let (enabled, profile) = { let p = processor_clone.lock().unwrap(); (p.chinese_enabled, p.get_current_profile_display()) };
                    let _ = gui_tx_tray.send(GuiEvent::OpenTrayMenu { x, y, chinese_enabled: enabled, active_profile: profile });
                }
                ui::tray::TrayEvent::Exit => std::process::exit(0),
            }
        }
    });

    #[cfg(target_os = "windows")]
    {
        let mut host = platform::windows::tsf::TsfHost::new(processor, Some(gui_tx_main), config.clone(), tray_tx.clone());
        host.run()?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn setup_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let exe_path = exe.to_str().ok_or("Invalid path encoding")?;
    let status = std::process::Command::new("reg").arg("add").arg("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run").arg("/v").arg("RustIME").arg("/t").arg("REG_SZ").arg("/d").arg(exe_path).arg("/f").status()?;
    if status.success() { Ok(()) } else { Err("Failed to add registry key".into()) }
}

#[cfg(target_os = "windows")]
pub fn remove_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("reg").arg("delete").arg("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run").arg("/v").arg("RustIME").arg("/f").status()?;
    if status.success() { Ok(()) } else { Err("Failed to remove registry key".into()) }
}
