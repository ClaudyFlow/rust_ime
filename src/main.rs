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
use std::path::PathBuf;
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

#[derive(Debug)]
pub enum NotifyEvent {
    Update(String, String),
    Message(String, String), // Summary, Body
    Close,
}

static WEB_SERVER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn find_project_root() -> PathBuf {
    let mut curr = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..3 {
        if curr.join("dicts").exists() { return curr; }
        if !curr.pop() { break; }
    }
    curr
}

pub fn save_config(c: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut p = find_project_root(); p.push("config.json");
    let f = File::create(p)?; serde_json::to_writer_pretty(f, c)?;
    Ok(())
}

fn load_config() -> Config {
    let mut p = find_project_root(); p.push("config.json");
    if let Ok(f) = File::open(&p) { 
        if let Ok(c) = serde_json::from_reader(BufReader::new(f)) { return c; } 
    }
    Config::default_config()
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

pub fn load_syllables() -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let mut path = find_project_root();
    path.push("dicts/chinese/syllables.txt");
    if let Ok(f) = File::open(path) {
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
                
                // 1. First, try the DLL in the same directory as the current EXE (Standard for Release Package)
                let mut dll_path = std::env::current_exe()?;
                dll_path.set_file_name("rust_ime_tsf_v3.dll");
                
                if !dll_path.exists() {
                    // 2. Fallback: If not found, look in target/release or target/debug relative to current working directory (For Development)
                    let mut fallback = std::env::current_dir()?;
                    fallback.push("target");
                    fallback.push("release");
                    fallback.push("rust_ime_tsf_v3.dll");
                    
                    if !fallback.exists() {
                        fallback.pop();
                        fallback.pop();
                        fallback.push("debug");
                        fallback.push("rust_ime_tsf_v3.dll");
                    }
                    
                    if fallback.exists() {
                        dll_path = fallback;
                    }
                }

                if !dll_path.exists() {
                    eprintln!("❌ Error: Could not find 'rust_ime_tsf_v3.dll' in the current folder or target directories.");
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
            "--foreground" => {
                should_daemonize = false;
            }
            "--daemon" => {
                should_daemonize = true;
            }
            "--stop" => {
                #[cfg(target_os = "linux")]
                let _ = Command::new("pkill").arg("-f").arg("rust-ime").status();
                #[cfg(target_os = "windows")]
                let _ = Command::new("taskkill").arg("/F").arg("/IM").arg("rust-ime.exe").arg("/T").status();
                println!("✅ 已停止后台进程。");
                return Ok(());
            }
            _ if !args[1].starts_with('-') => {
                // 原有的命令行即时转换模式
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
                let mut p = Processor::new(tries_map, "chinese".into(), HashMap::new());
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
            
            // Redirect stdout and stderr to a file for debugging
            let mut log_path = root.clone();
            log_path.push("rust-ime.log");
            if let Ok(log_file) = File::create(log_path) {
                let _ = std::os::windows::io::AsRawHandle::as_raw_handle(&log_file);
                // Note: Rust doesn't have a simple standard way to redirect stdout/stderr of the current process 
                // globally without external crates like 'libc' or 'winapi' directly, 
                // but we can at least print a message before hiding the console.
            }

            let window = unsafe { GetConsoleWindow() };
            if window.0 != 0 {
                unsafe { ShowWindow(window, SW_HIDE); }
            }
            println!("✅ 已转入后台运行 (窗口已隐藏)。");
        }
    }

    // 忽略 SIGHUP，防止终端关闭时程序退出
    #[cfg(target_os = "linux")]
    {
        let mut signals = Signals::new(&[SIGHUP])?;
        std::thread::spawn(move || {
            for _ in signals.forever() {
                // 忽略 SIGHUP
            }
        });
    }

    // 0. 自动检查并增量编译词库 (已根据用户要求移除自动调用，改为手动触发)
    /*
    if let Err(e) = engine::compiler::check_and_compile_all() {
        eprintln!("[Main] 词库自动编译失败: {}", e);
    }
    */

    let mut current_config = load_config();
    let mut profiles_changed = false;
    if let Ok(entries) = std::fs::read_dir("dicts") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !current_config.files.profiles.iter().any(|p| p.name == name) {
                    println!("[Main] 发现新词库方案: {}", name);
                    current_config.files.profiles.push(config::Profile {
                        name: name.clone(),
                        path: format!("data/{}/trie", name),
                    });
                    profiles_changed = true;
                }
            }
        }
    }
    if profiles_changed { let _ = save_config(&current_config); }

    let config = Arc::new(RwLock::new(current_config));
    let (gui_tx, gui_rx) = std::sync::mpsc::channel();
    let (tray_tx, tray_rx) = std::sync::mpsc::channel();
    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    
    // 1. 启动 GUI 线程
    let gui_config = config.read().expect("Failed to acquire config read lock for GUI").clone();
    let gui_tx_main = gui_tx.clone();
    std::thread::spawn(move || {
        ui::gui::start_gui(gui_rx, gui_config);
    });

    // 2. 加载词库
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
                        println!("[Main] 加载方案: {}", dir_name);
                        tries_map.insert(dir_name.clone(), trie);
                    }
                }
            }
        }
    }
    let tries_arc = Arc::new(RwLock::new(tries_map.clone()));

    let conf_guard = config.read().unwrap();
    let punctuation = load_punctuation_dict(&conf_guard.files.punctuation_file);
    let mut default_profile = conf_guard.input.default_profile.to_lowercase();
    if default_profile.is_empty() || !tries_map.contains_key(&default_profile) {
        if tries_map.contains_key("chinese") {
            default_profile = "chinese".to_string();
        } else if let Some(k) = tries_map.keys().next() {
            default_profile = k.clone();
        }
    }
    
    let mut processor_obj = Processor::new(tries_map, default_profile, punctuation);
    processor_obj.apply_config(&conf_guard);
    processor_obj.set_syllables(load_syllables());

    let processor = Arc::new(Mutex::new(processor_obj));
    drop(conf_guard);

    // 3. 通知线程
    #[cfg(target_os = "linux")]
    std::thread::spawn(move || {
        let mut handle: Option<notify_rust::NotificationHandle> = None;
        while let Ok(event) = notify_rx.recv() {
            match event {
                NotifyEvent::Message(summary, body) => { let _ = notify_rust::Notification::new().summary(&summary).body(&body).timeout(1500).show(); },
                NotifyEvent::Update(s, b) => { if let Ok(h) = notify_rust::Notification::new().summary(&s).body(&b).id(9999).timeout(0).show() { handle = Some(h); } },
                NotifyEvent::Close => { if let Some(h) = handle.take() { h.close(); } }
            }
        }
    });

    #[cfg(target_os = "windows")]
    std::thread::spawn(move || {
        while let Ok(event) = notify_rx.recv() {
            match event {
                NotifyEvent::Message(summary, body) => { 
                    let _ = notify_rust::Notification::new()
                        .summary(&summary)
                        .body(&body)
                        .appname("Rust IME")
                        .timeout(notify_rust::Timeout::Milliseconds(3000))
                        .show(); 
                },
                NotifyEvent::Update(s, b) => { 
                    let _ = notify_rust::Notification::new()
                        .summary(&s)
                        .body(&b)
                        .appname("Rust IME")
                        .timeout(notify_rust::Timeout::Milliseconds(1000))
                        .show(); 
                },
                NotifyEvent::Close => {}
            }
        }
    });

    // 5. 准备 Web Server 端口
    let actual_web_port = Arc::new(std::sync::atomic::AtomicU16::new(18765));

    // 6. 托盘处理器
    let conf = config.read().unwrap();
    let tray_handle = ui::tray::start_tray(false, conf.input.default_profile.clone(), conf.appearance.show_candidates, conf.input.anti_typo_mode, conf.input.enable_double_pinyin, conf.input.commit_mode.clone(), conf.appearance.preview_mode.clone(), conf.appearance.candidate_layout.clone(), tray_tx.clone());
    drop(conf);

    let processor_clone = processor.clone();
    let gui_tx_tray = gui_tx.clone();
    let config_tray = config.clone();
    let notify_tx_tray = notify_tx.clone();
    let actual_web_port_tray = actual_web_port.clone();
    let tries_tray = tries_arc.clone();
    let tray_tx_for_web = tray_tx.clone();
    
    std::thread::spawn(move || {
        while let Ok(event) = tray_rx.recv() {
            match event {
                ui::tray::TrayEvent::ToggleIme => {
                    let (profile, enabled) = {
                        let mut p = processor_clone.lock().unwrap();
                        let _action = p.toggle(); // 获取但不处理（托盘点击较少发生）
                        (p.get_current_profile_display(), p.chinese_enabled)
                    };
                    let msg = if enabled { "输入法模式" } else { "直通键盘模式" };
                    let status_text = if enabled { "中" } else { "英" };
                    let _ = notify_tx_tray.send(NotifyEvent::Message(profile, msg.to_string()));
                    tray_handle.update(|t| t.chinese_enabled = enabled);
                    
                    let _ = gui_tx_tray.send(GuiEvent::ShowStatus(status_text.into()));
                    let _ = gui_tx_tray.send(GuiEvent::Update { 
                        pinyin: "".into(), 
                        candidates: vec![], 
                        hints: vec![], 
                        selected: 0, 
                        sentence: "".into(),
                        cursor_pos: 0,
                        commit_mode: "single".into(),
                    });
                }
                ui::tray::TrayEvent::NextProfile => {
                    let profile = {
                        let mut p = processor_clone.lock().unwrap();
                        p.next_profile()
                    };
                    let _ = notify_tx_tray.send(NotifyEvent::Message(profile.clone(), "已切换方案".to_string()));
                    tray_handle.update(|t| t.active_profile = profile);
                }
                ui::tray::TrayEvent::ToggleGui => {
                    let enabled = {
                        let mut p = processor_clone.lock().unwrap();
                        p.show_candidates = !p.show_candidates;
                        p.show_candidates
                    };
                    tray_handle.update(|t| t.show_candidates = enabled);
                    if let Ok(mut w) = config_tray.write() { 
                        w.appearance.show_candidates = enabled; 
                        let _ = save_config(&w); 
                        let _ = gui_tx_tray.send(GuiEvent::ApplyConfig(w.clone()));
                    }
                }
                ui::tray::TrayEvent::ToggleAntiTypo => {
                    let mut w = config_tray.write().unwrap();
                    let next_mode = match w.input.anti_typo_mode {
                        config::AntiTypoMode::None => config::AntiTypoMode::Strict,
                        config::AntiTypoMode::Strict => config::AntiTypoMode::Smart,
                        config::AntiTypoMode::Smart => config::AntiTypoMode::None,
                    };
                    w.input.anti_typo_mode = next_mode;
                    tray_handle.update(|t| t.anti_typo_mode = next_mode);
                    let _ = save_config(&w);
                    processor_clone.lock().unwrap().anti_typo_mode = next_mode;
                }
                ui::tray::TrayEvent::ToggleDoublePinyin => {
                    let (enabled, profile) = {
                        let mut w = config_tray.write().unwrap();
                        w.input.enable_double_pinyin = !w.input.enable_double_pinyin;
                        let e = w.input.enable_double_pinyin;
                        let _ = save_config(&w);
                        let mut p = processor_clone.lock().unwrap();
                        p.enable_double_pinyin = e;
                        (e, p.get_current_profile_display())
                    };
                    tray_handle.update(|t| t.double_pinyin = enabled);
                    let msg = if enabled { "开启" } else { "关闭" };
                    let _ = notify_tx_tray.send(NotifyEvent::Message(profile, format!("小鹤双拼模式: {}", msg)));
                }
                ui::tray::TrayEvent::SwitchCommitMode => {
                    let mut w = config_tray.write().unwrap();
                    w.input.commit_mode = if w.input.commit_mode == "single" { "double".into() } else { "single".into() };
                    let mode = w.input.commit_mode.clone();
                    tray_handle.update(|t| t.commit_mode = mode.clone());
                    let _ = save_config(&w);
                    processor_clone.lock().unwrap().commit_mode = mode.clone();
                    let msg = if mode == "single" { "词模式(单空格)" } else { "长句模式(双空格)" };
                    let _ = notify_tx_tray.send(NotifyEvent::Message("上屏模式切换".into(), msg.into()));
                }
                ui::tray::TrayEvent::CyclePreview => {
                    let mode_str = {
                        let mut p = processor_clone.lock().unwrap();
                        p.phantom_mode = match p.phantom_mode {
                            engine::processor::PhantomMode::None => engine::processor::PhantomMode::Pinyin,
                            engine::processor::PhantomMode::Pinyin => engine::processor::PhantomMode::None,
                        };
                        match p.phantom_mode { engine::processor::PhantomMode::Pinyin => "pinyin", _ => "none" }
                    }.to_string();
                    tray_handle.update(|t| t.preview_mode = mode_str.to_string());
                    if let Ok(mut w) = config_tray.write() { w.appearance.preview_mode = mode_str; let _ = save_config(&w); }
                }
                ui::tray::TrayEvent::ReloadConfig => {
                    let new_conf = load_config();
                    processor_clone.lock().unwrap().apply_config(&new_conf);
                    let _ = gui_tx_tray.send(GuiEvent::ApplyConfig(new_conf.clone()));
                    
                    // 同步更新托盘菜单状态
                    tray_handle.update(|t| {
                        t.show_candidates = new_conf.appearance.show_candidates;
                        t.preview_mode = new_conf.appearance.preview_mode.clone();
                        t.candidate_layout = new_conf.appearance.candidate_layout.clone();
                        t.anti_typo_mode = new_conf.input.anti_typo_mode;
                        t.double_pinyin = new_conf.input.enable_double_pinyin;
                        t.commit_mode = new_conf.input.commit_mode.clone();
                    });

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
                        // 首次启动给一点预热时间
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }

                    let port = actual_web_port_tray.load(std::sync::atomic::Ordering::SeqCst);
                    let url = format!("http://localhost:{}", port);
                    #[cfg(target_os = "linux")]
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("cmd").arg("/c").arg("start").arg(&url).spawn();
                }
                ui::tray::TrayEvent::SyncStatus { chinese_enabled, active_profile } => {
                    tray_handle.update(|t| {
                        t.chinese_enabled = chinese_enabled;
                        t.active_profile = active_profile;
                    });
                }
                ui::tray::TrayEvent::Restart => {
                    let args: Vec<String> = std::env::args().collect();
                    let _ = std::process::Command::new(&args[0]).args(&args[1..]).spawn();
                    std::process::exit(0);
                }
                ui::tray::TrayEvent::Exit => std::process::exit(0),
            }
        }
    });

    // 7. 运行 Host
    #[cfg(target_os = "linux")]
    {
        println!("[Main] 启动 Evdev 兼容模式 (原生 Wayland 协议暂避)...");
        let device_path = find_keyboard_device()?;
        let mut host = platform::linux::evdev_host::EvdevHost::new(processor, &device_path, Some(gui_tx_main), config.clone(), notify_tx.clone(), tray_tx.clone())?;
        host.run()?;
    }

    #[cfg(target_os = "windows")]
    {
        println!("[Main] 启动 Windows TSF 模式 (实验中)...");
        let mut host = platform::windows::tsf::TsfHost::new(processor, Some(gui_tx_main), config.clone(), notify_tx.clone(), tray_tx.clone());
        host.run()?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn find_keyboard_device() -> Result<String, Box<dyn std::error::Error>> {
    let ps = std::fs::read_dir("/dev/input")?;
    for e in ps {
        let e = e?;
        if let Ok(d) = evdev::Device::open(e.path()) {
            if d.supported_keys().map_or(false, |k| k.contains(evdev::Key::KEY_A) && k.contains(evdev::Key::KEY_ENTER)) {
                return Ok(e.path().to_string_lossy().to_string());
            }
        }
    }
    Err("未检测到合适的键盘设备。".into())
}

#[cfg(target_os = "linux")]

pub fn setup_autostart() -> Result<(), Box<dyn std::error::Error>> {

    let home = env::var("HOME")?;

    let autostart_dir = format!("{}/.config/autostart", home);

    std::fs::create_dir_all(&autostart_dir)?;

    

    let mut desktop_path = PathBuf::from(autostart_dir);

    desktop_path.push("rust-ime.desktop");

    

    let current_exe = env::current_exe()?;
    let exe_path = current_exe.to_string_lossy();

    

    let content = format!(r#"[Desktop Entry]

Type=Application

Name=Rust-IME

Exec={}

Icon=input-keyboard

Comment=Rust Input Method Engine

Terminal=false

X-GNOME-Autostart-enabled=true

"#, exe_path);



    let mut file = File::create(desktop_path)?;

    file.write_all(content.as_bytes())?;

    Ok(())

}



#[cfg(target_os = "windows")]



pub fn setup_autostart() -> Result<(), Box<dyn std::error::Error>> {



    let exe = std::env::current_exe()?;



    let exe_path = exe.to_str().ok_or("Invalid path encoding")?;



    let status = std::process::Command::new("reg")



        .arg("add")



        .arg("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run")



        .arg("/v")



        .arg("RustIME")



        .arg("/t")



        .arg("REG_SZ")



        .arg("/d")



        .arg(exe_path)



        .arg("/f")



        .status()?;



    if status.success() { Ok(()) } else { Err("Failed to add registry key".into()) }



}



#[cfg(target_os = "linux")]

pub fn remove_autostart() -> Result<(), Box<dyn std::error::Error>> {

    let home = env::var("HOME")?;

    let autostart_file = format!("{}/.config/autostart/rust-ime.desktop", home);

    let path = std::path::Path::new(&autostart_file);

    if path.exists() {

        std::fs::remove_file(path)?;

    }

    Ok(())

}



#[cfg(target_os = "windows")]



pub fn remove_autostart() -> Result<(), Box<dyn std::error::Error>> {



    let status = std::process::Command::new("reg")



        .arg("delete")



        .arg("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run")



        .arg("/v")



        .arg("RustIME")



        .arg("/f")



        .status()?;



    if status.success() { Ok(()) } else { Err("Failed to remove registry key".into()) }



}
