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
use std::io::BufReader;
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

pub fn load_punctuation_dict(p: &str) -> HashMap<String, Vec<config::PunctuationEntry>> {
    let mut m = HashMap::new();
    if let Ok(f) = File::open(p) { 
        if let Ok(v) = serde_json::from_reader::<_, Value>(BufReader::new(f)) {
            if let Some(obj) = v.as_object() { 
                for (k, val) in obj { 
                    if let Some(arr) = val.as_array() {
                        let entries = arr.iter().filter_map(|item| {
                            let c = item.get("char")?.as_str()?;
                            let d = item.get("desc").and_then(|d| d.as_str()).unwrap_or("");
                            Some(config::PunctuationEntry { char: c.to_string(), desc: d.to_string() })
                        }).collect();
                        m.insert(k.clone(), entries);
                    }
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
    // 移除软渲染限制，让系统自动决定 GPU/CPU
    // #[cfg(target_os = "windows")]
    // std::env::set_var("SLINT_BACKEND", "software");

    #[cfg(target_os = "windows")]
    let _mutex_handle = unsafe {
        use windows::Win32::System::Threading::*;
        use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
        use windows::core::PCWSTR;

        let name = PCWSTR("Global\\RustImeUniqueMutex\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
        let handle = CreateMutexW(None, true, name)?;
        if let Ok(_) = windows::Win32::Foundation::GetLastError() {
            // 获取错误码，如果是已存在则退出
        }
        let last_err = windows::Win32::Foundation::GetLastError();
        if last_err == ERROR_ALREADY_EXISTS {
             let _ = notify_rust::Notification::new()
                .summary("Rust IME")
                .body("程序已经在运行中。")
                .appname("Rust IME")
                .timeout(notify_rust::Timeout::Milliseconds(3000))
                .show();
            return Ok(());
        }
        handle
    };

    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::HiDpi::*;
        let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
    }

    // 1. 确定并锁定工作目录
    let root = find_project_root();
    env::set_current_dir(&root)?;
    println!("[Main] 锁定工作目录: {:?}", root);

    let args: Vec<String> = env::args().collect();
    let mut should_daemonize = true;

    // 处理命令行参数
    if args.len() > 1 {
        match args[1].as_str() {
            "--compile-only" => {
                println!("[Main] 正在强制编译词库...");
                let _ = engine::compiler::check_and_compile_all();
                return Ok(());
            }
            "--register" => {
                #[cfg(target_os = "windows")]
                {
                    unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)?; }
                    let mut dll_path = std::env::current_exe()?;
                    dll_path.set_file_name("rust_ime_tsf_v3.dll");
                    let path_str = dll_path.to_str().ok_or("Path error")?;
                    unsafe { registry::register_server(windows::Win32::Foundation::HINSTANCE(0), &IME_ID, "Rust IME", Some(path_str))?; }
                    println!("✅ TSF 注册成功。");
                }
                return Ok(());
            }
            "--unregister" => {
                #[cfg(target_os = "windows")]
                {
                    unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)?; }
                    unsafe { registry::unregister_server(&IME_ID)?; }
                    println!("✅ TSF 注销成功。");
                }
                return Ok(());
            }
            "--daemon" => { should_daemonize = true; }
            "--foreground" => { should_daemonize = false; }
            _ => {}
        }
    }

    if should_daemonize {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Console::GetConsoleWindow;
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            let window = unsafe { GetConsoleWindow() };
            if window.0 != 0 { unsafe { ShowWindow(window, SW_HIDE); } }
        }
    }

    // 2. 核心检测：如果 data 目录为空，必须先编译
    if !root.join("data/chinese/trie.index").exists() {
        println!("[Main] 词库索引缺失，正在进行静默编译...");
        let _ = engine::compiler::check_and_compile_all();
    }

    // 3. 加载配置和标点 (优化后的加载逻辑)
    let mut current_config = Config::load();
    {
        let mut punctuations = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(root.join("dicts")) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let lang = entry.file_name().to_string_lossy().to_string();
                    let punc_file = entry.path().join("punctuation.json");
                    if punc_file.exists() {
                        punctuations.insert(lang, load_punctuation_dict(&punc_file.to_string_lossy()));
                    }
                }
            }
        }
        current_config.input.punctuations = punctuations;
    }

    let config = Arc::new(RwLock::new(current_config));
    let (gui_tx, gui_rx) = std::sync::mpsc::channel();
    let (tray_tx, tray_rx) = std::sync::mpsc::channel();
    
    // 4. 启动 GUI
    let gui_config = config.read().unwrap().clone();
    let tray_tx_gui = tray_tx.clone();
    std::thread::spawn(move || { ui::gui::start_gui(gui_rx, gui_config, tray_tx_gui); });

    // 5. 加载词库地图
    let mut tries_map = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(root.join("data")) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string().to_lowercase();
                let trie_idx = entry.path().join("trie.index");
                let trie_dat = entry.path().join("trie.data");
                if trie_idx.exists() && trie_dat.exists() {
                    if let Ok(trie) = Trie::load(&trie_idx, &trie_dat) {
                        tries_map.insert(dir_name, trie);
                    }
                }
            }
        }
    }

    let tries_count = tries_map.len();
    println!("[Main] 成功加载 {} 个词库方案。", tries_count);

    // 6. 初始化 Processor
    let conf_guard = config.read().unwrap();
    let default_p = conf_guard.input.default_profile.clone();
    let mut processor_obj = Processor::new(tries_map, default_p, conf_guard.input.punctuations.clone());
    processor_obj.apply_config(&conf_guard);
    processor_obj.set_syllables(load_syllables(&root));    
    let processor = Arc::new(Mutex::new(processor_obj));
    drop(conf_guard);

    // 7. 启动托盘
    let conf = config.read().unwrap();
    let tray_handle = ui::tray::start_tray(false, conf.input.default_profile.clone(), conf.appearance.show_candidates, conf.input.anti_typo_mode, conf.input.enable_double_pinyin, conf.input.commit_mode.clone(), conf.appearance.preview_mode.clone(), conf.appearance.candidate_layout.clone(), tray_tx.clone());
    drop(conf);

    // 8. 消息循环处理
    let processor_clone = processor.clone();
    let gui_tx_tray = gui_tx.clone();
    let config_msg = config.clone();
    
    std::thread::spawn(move || {
        while let Ok(event) = tray_rx.recv() {
            match event {
                ui::tray::TrayEvent::ToggleIme => {
                    let mut p = processor_clone.lock().unwrap();
                    p.toggle();
                    let enabled = p.chinese_enabled;
                    let short = p.get_short_display();
                    tray_handle.update(|t| t.chinese_enabled = enabled);
                    let _ = gui_tx_tray.send(GuiEvent::ShowStatus(short, enabled));
                }
                ui::tray::TrayEvent::NextProfile => {
                    let mut p = processor_clone.lock().unwrap();
                    let profile = p.next_profile();
                    let enabled = p.chinese_enabled;
                    let short = p.get_short_display();
                    tray_handle.update(|t| t.active_profile = profile);
                    let _ = gui_tx_tray.send(GuiEvent::ShowStatus(short, enabled));
                }
                ui::tray::TrayEvent::ReloadConfig => {
                    let new_conf = Config::load();
                    if let Ok(mut p) = processor_clone.lock() {
                        p.apply_config(&new_conf);
                    }
                    let _ = gui_tx_tray.send(GuiEvent::ApplyConfig(new_conf));
                }
                ui::tray::TrayEvent::Exit => std::process::exit(0),
                _ => {}
            }
        }
    });

    // 9. 启动 TSF 主机
    #[cfg(target_os = "windows")]
    {
        let mut host = platform::windows::tsf::TsfHost::new(processor, Some(gui_tx), config.clone(), tray_tx);
        host.run()?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn setup_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let exe_path = exe.to_str().ok_or("Invalid path")?;
    let _ = std::process::Command::new("reg").arg("add").arg("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run").arg("/v").arg("RustIME").arg("/t").arg("REG_SZ").arg("/d").arg(exe_path).arg("/f").status();
    Ok(())
}
