use crate::engine::Processor;
use crate::engine::processor::Action;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::platform::linux::vkbd::Vkbd;
use crate::config::Config;
use crate::ui::GuiEvent;
use evdev::{Device, InputEventKind, Key};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use crate::config::parse_key;

pub struct EvdevHost {
    processor: Arc<Mutex<Processor>>,
    vkbd: Mutex<Vkbd>,
    dev: Arc<Mutex<Device>>, // 修改为 Arc 以便在 Guard 中共享
    gui_tx: Option<Sender<GuiEvent>>,
    tray_tx: Sender<crate::ui::tray::TrayEvent>,
    should_exit: Arc<AtomicBool>,
    config: Arc<std::sync::RwLock<Config>>,
    tab_held_and_not_used: bool,
}

struct GrabGuard {
    device: Arc<Mutex<Device>>,
}

impl GrabGuard {
    fn new(device: Arc<Mutex<Device>>) -> Self {
        if let Ok(mut dev) = device.lock() {
            if let Err(e) = dev.grab() {
                eprintln!("[EvdevHost] 警告: 无法锁定键盘设备: {}", e);
            } else {
                println!("[EvdevHost] 已成功锁定键盘硬件拦截。");
            }
        }
        Self { device }
    }
}

impl Drop for GrabGuard {
    fn drop(&mut self) {
        if let Ok(mut dev) = self.device.lock() {
            let _ = dev.ungrab();
            println!("[EvdevHost] 键盘硬件拦截已安全释放。");
        }
    }
}

impl EvdevHost {
    pub fn new(
        processor: Arc<Mutex<Processor>>, 
        device_path: &str, 
        gui_tx: Option<Sender<GuiEvent>>,
        config: Arc<std::sync::RwLock<Config>>,
        tray_tx: Sender<crate::ui::tray::TrayEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let dev = Device::open(device_path)?;
        let mut vkbd = Vkbd::new(&dev)?;
        {
            let conf = config.read().unwrap();
            vkbd.apply_config(&conf);
        }
        Ok(Self {
            processor, vkbd: Mutex::new(vkbd), dev: Arc::new(Mutex::new(dev)), gui_tx, tray_tx,
            should_exit: Arc::new(AtomicBool::new(false)), config, tab_held_and_not_used: false,
        })
    }
}

impl InputMethodHost for EvdevHost {
    fn set_preedit(&self, _text: &str, _cursor_pos: usize) {}
    fn commit_text(&self, text: &str) {
        if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.send_text(text); }
    }
    fn get_cursor_rect(&self) -> Option<Rect> { None }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 使用 RAII Guard 自动管理 grab 生命周期
        let _guard = GrabGuard::new(self.dev.clone());
        let mut held_keys = HashSet::new();
        println!("[EvdevHost] 正在运行硬件拦截循环...");

        while !self.should_exit.load(Ordering::Relaxed) {
            let events: Vec<_> = if let Ok(mut dev) = self.dev.lock() { 
                match dev.fetch_events() {
                    Ok(evs) => evs.collect(),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            } else { break; };

            for ev in events {
                if let InputEventKind::Key(key) = ev.kind() {
                    let val = ev.value();
                    if val == 1 { 
                        held_keys.insert(key); 
                        // 如果按下了除 Tab 以外的任何键，标记 Tab 已被使用
                        if key != Key::KEY_TAB {
                            self.tab_held_and_not_used = false;
                        }
                    } else if val == 0 { 
                        held_keys.remove(&key); 
                    }

                    let ctrl = held_keys.contains(&Key::KEY_LEFTCTRL) || held_keys.contains(&Key::KEY_RIGHTCTRL);
                    let alt = held_keys.contains(&Key::KEY_LEFTALT) || held_keys.contains(&Key::KEY_RIGHTALT);
                    let meta = held_keys.contains(&Key::KEY_LEFTMETA) || held_keys.contains(&Key::KEY_RIGHTMETA);
                    let has_mod = ctrl || alt || meta;

                    if key == Key::KEY_TAB && !has_mod {
                        if val == 1 { self.tab_held_and_not_used = true; } 
                        else if val == 0 {
                                let enabled = p.chinese_enabled;
                                let profile = p.get_current_profile_display();
                                drop(p);

                                let _ = self.tray_tx.send(crate::ui::tray::TrayEvent::SyncStatus { 
                                    chinese_enabled: enabled, 
                                    active_profile: profile 
                                });

                                self.update_gui();
                            }
                            self.tab_held_and_not_used = false;
                        }
                        continue;
                    }

                    // 拦截 Shift 触发全局筛选
                    if (key == Key::KEY_LEFTSHIFT || key == Key::KEY_RIGHTSHIFT) && !has_mod {
                        if val == 1 {
                            let mut p = self.processor.lock().unwrap();
                            if p.chinese_enabled && p.state != crate::engine::processor::ImeState::Direct {
                                println!("[Host] Triggering Global Filter");
                                p.start_global_filter();
                                drop(p);
                                self.update_gui();
                                continue; 
                            }
                        }
                    }

                    if val == 1 {
                        let (toggle_main, toggle_alt, switch_prof, cycle_preview, cycle_paste, toggle_trad, toggle_commit, toggle_dp) = {
                            let conf = self.config.read().unwrap();
                            (parse_key(&conf.hotkeys.switch_language.key), parse_key(&conf.hotkeys.switch_language_alt.key), parse_key(&conf.hotkeys.switch_dictionary.key), parse_key(&conf.hotkeys.cycle_preview_mode.key), parse_key(&conf.hotkeys.cycle_paste_method.key), parse_key(&conf.hotkeys.toggle_traditional_gui.key), parse_key(&conf.hotkeys.switch_commit_mode.key), parse_key(&conf.hotkeys.toggle_double_pinyin.key))
                        };
                        
                        if is_combo(&held_keys, &toggle_main) || is_combo(&held_keys, &toggle_alt) {
                            let mut p = self.processor.lock().unwrap(); p.toggle();
                            let enabled = p.chinese_enabled;
                            let profile = p.get_current_profile_display();
                            drop(p);

                            let _ = self.tray_tx.send(crate::ui::tray::TrayEvent::SyncStatus { 
                                chinese_enabled: enabled, 
                                active_profile: profile 
                            });

                            self.update_gui(); continue;
                        }

                        if is_combo(&held_keys, &switch_prof) {
                            let mut p = self.processor.lock().unwrap(); let profile = p.next_profile();
                            let enabled = p.chinese_enabled;
                            let profile_copy = profile.clone();
                            if let Ok(mut w) = self.config.write() { w.input.active_profiles = vec![profile]; let _ = crate::save_config(&w); }
                            drop(p);

                            let _ = self.tray_tx.send(crate::ui::tray::TrayEvent::SyncStatus { 
                                chinese_enabled: enabled, 
                                active_profile: profile_copy 
                            });

                            self.update_gui(); continue;
                        }

                        if is_combo(&held_keys, &cycle_preview) {
                            let mut p = self.processor.lock().unwrap();
                            p.phantom_mode = match p.phantom_mode { crate::engine::processor::PhantomMode::None => crate::engine::processor::PhantomMode::Pinyin, _ => crate::engine::processor::PhantomMode::None };
                            drop(p); continue;
                        }

                        if is_combo(&held_keys, &cycle_paste) {
                            let _ = if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.cycle_paste_mode() } else { "失败".into() };
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_trad) {
                            let mut p = self.processor.lock().unwrap(); p.show_candidates = !p.show_candidates;
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_commit) {
                            let (mode, _) = {
                                let mut p = self.processor.lock().unwrap();
                                p.commit_mode = if p.commit_mode == "single" { "double".into() } else { "single".into() };
                                (p.commit_mode.clone(), p.get_current_profile_display())
                            };
                            if let Ok(mut w) = self.config.write() { w.input.commit_mode = mode.clone(); let _ = crate::save_config(&w); }
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_dp) {
                            let (enabled, _) = {
                                let mut p = self.processor.lock().unwrap();
                                p.enable_double_pinyin = !p.enable_double_pinyin;
                                (p.enable_double_pinyin, p.get_current_profile_display())
                            };
                            if let Ok(mut w) = self.config.write() { w.input.enable_double_pinyin = enabled; let _ = crate::save_config(&w); }
                            continue;
                        }
                    }

                    let shift = held_keys.contains(&Key::KEY_LEFTSHIFT) || held_keys.contains(&Key::KEY_RIGHTSHIFT);
                    let mut p = self.processor.lock().unwrap();
                    if p.chinese_enabled && !has_mod {
                        match p.handle_key(key, val, shift) {
                            Action::Emit(s) => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.send_text(&s); } }
                            Action::DeleteAndEmit { delete, insert } => { if let Ok(mut vkbd) = self.vkbd.lock() { if delete > 0 { vkbd.backspace(delete); } if !insert.is_empty() { let _ = vkbd.send_text(&insert); } } }
                            Action::Notify(_, _) => { 
                                // 此处原本负责位置切换提示，现在已无处发送通知，仅保持逻辑通过
                            }
                            Action::Alert => { 
                                if self.config.read().unwrap().input.enable_error_sound { 
                                    let root = crate::find_project_root();
                                    let sound_path = root.join("sounds/beep.wav");
                                    if sound_path.exists() {
                                        let _ = std::process::Command::new("canberra-gtk-play")
                                            .arg("-f")
                                            .arg(sound_path)
                                            .spawn();
                                    } else {
                                        let _ = std::process::Command::new("canberra-gtk-play")
                                            .arg("--id=dialog-error")
                                            .spawn();
                                    }
                                } 
                            }
                            Action::PassThrough => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); } }
                            _ => {}
                        }
                        drop(p); if val != 0 { self.update_gui(); }
                    } else {
                        if has_mod && p.state != crate::engine::processor::ImeState::Direct { let del = p.phantom_text.chars().count(); p.reset(); if del > 0 { if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.backspace(del); } } }
                        drop(p); if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); }
                    }

                    if val == 1 {
                        // 逻辑已移至 Processor::handle_key
                    }
                }
            }
        }
        Ok(())
    }
}

impl EvdevHost {
    fn update_gui(&self) {
        if let Some(ref tx) = self.gui_tx {
            let p = self.processor.lock().unwrap();
            if p.buffer.is_empty() || !p.chinese_enabled { 
                let _ = tx.send(GuiEvent::Update { 
                    pinyin: "".into(), 
                    candidates: vec![], 
                    hints: vec![], 
                    selected: 0, 
                    sentence: "".into(),
                    cursor_pos: 0,
                    commit_mode: p.commit_mode.clone(),
                }); 
                return; 
            }
            
            // 构造显示用的拼音串，包含 aux_filter (首字母大写)
            let mut pinyin = if p.best_segmentation.is_empty() { p.buffer.clone() } else { p.best_segmentation.join(" ") };
                            if p.nav_mode {
                                pinyin.push_str(" [H:左 J:下 K:上 L:右]");
                            }            if !p.aux_filter.is_empty() {
                let mut display_aux = String::new();
                for (i, c) in p.aux_filter.chars().enumerate() {
                    if i == 0 { display_aux.push(c.to_ascii_uppercase()); }
                    else { display_aux.push(c.to_ascii_lowercase()); }
                }
                pinyin.push_str(&display_aux);
            }

            if !p.candidates.is_empty() || !p.joined_sentence.is_empty() {
                let start = p.page; let end = (start + p.page_size).min(p.candidates.len());
                for (abs_idx, cand) in p.candidates.iter().enumerate().skip(start).take(end - start) {
                    let hint = p.candidate_hints.get(abs_idx).cloned().unwrap_or_default();
                    if abs_idx == p.selected { 
                        println!("[Candidate] {}.{} {}", (abs_idx % p.page_size)+1, cand, hint); 
                    }
                }
            }
            if p.show_candidates || p.show_modern_candidates {
                let _ = tx.send(GuiEvent::Update { 
                    pinyin, 
                    candidates: p.candidates.clone(), 
                    hints: p.candidate_hints.clone(), 
                    selected: p.selected, 
                    sentence: p.joined_sentence.clone(),
                    cursor_pos: p.cursor_pos,
                    commit_mode: p.commit_mode.clone(),
                });
            } else { 
                let _ = tx.send(GuiEvent::Update { 
                    pinyin: "".into(), 
                    candidates: vec![], 
                    hints: vec![], 
                    selected: 0, 
                    sentence: "".into(),
                    cursor_pos: 0,
                    commit_mode: p.commit_mode.clone(),
                }); 
            }
        }
    }
}

/// combinations 层: OR (备选方案)
/// requirements 层: AND (必须同时按下的按键组)
/// keys 层: OR (按键组内的可选按键，如 LShift/RShift)
fn is_combo(held: &HashSet<Key>, combinations: &[Vec<Vec<Key>>]) -> bool {
    if combinations.is_empty() { return false; }
    combinations.iter().any(|requirements| {
        requirements.iter().all(|keys| {
            keys.iter().any(|k| held.contains(k))
        })
    })
}
