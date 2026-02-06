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
use crate::NotifyEvent;

fn map_key_to_display_name(key: Key) -> String {
    match key {
        Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => "Ctrl".to_string(),
        Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => "Shift".to_string(),
        Key::KEY_LEFTALT | Key::KEY_RIGHTALT => "Alt".to_string(),
        Key::KEY_LEFTMETA | Key::KEY_RIGHTMETA => "Win".to_string(),
        Key::KEY_CAPSLOCK => "Caps".to_string(),
        Key::KEY_ESC => "Esc".to_string(),
        Key::KEY_TAB => "Tab".to_string(),
        Key::KEY_ENTER => "Enter".to_string(),
        Key::KEY_SPACE => "Space".to_string(),
        Key::KEY_BACKSPACE => "Backspace".to_string(),
        Key::KEY_DELETE => "Delete".to_string(),
        Key::KEY_INSERT => "Insert".to_string(),
        Key::KEY_HOME => "Home".to_string(),
        Key::KEY_END => "End".to_string(),
        Key::KEY_PAGEUP => "PgUp".to_string(),
        Key::KEY_PAGEDOWN => "PgDn".to_string(),
        Key::KEY_UP => "↑".to_string(),
        Key::KEY_DOWN => "↓".to_string(),
        Key::KEY_LEFT => "←".to_string(),
        Key::KEY_RIGHT => "→".to_string(),
        _ => format!("{:?}", key).replace("KEY_", "")
    }
}

pub struct EvdevHost {
    processor: Arc<Mutex<Processor>>,
    vkbd: Mutex<Vkbd>,
    dev: Mutex<Device>,
    gui_tx: Option<Sender<GuiEvent>>,
    notify_tx: Sender<NotifyEvent>,
    should_exit: Arc<AtomicBool>,
    config: Arc<std::sync::RwLock<Config>>,
    tab_held_and_not_used: bool,
}

impl EvdevHost {
    pub fn new(
        processor: Arc<Mutex<Processor>>, 
        device_path: &str, 
        gui_tx: Option<Sender<GuiEvent>>,
        config: Arc<std::sync::RwLock<Config>>,
        notify_tx: Sender<NotifyEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let dev = Device::open(device_path)?;
        let mut vkbd = Vkbd::new(&dev)?;
        {
            let conf = config.read().unwrap();
            vkbd.apply_config(&conf);
        }
        Ok(Self {
            processor, vkbd: Mutex::new(vkbd), dev: Mutex::new(dev), gui_tx, notify_tx,
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
        if let Ok(mut dev) = self.dev.lock() { let _ = dev.grab(); }
        let mut held_keys = HashSet::new();
        println!("[EvdevHost] 启动硬件拦截模式...");

        while !self.should_exit.load(Ordering::Relaxed) {
            let events: Vec<_> = if let Ok(mut dev) = self.dev.lock() { dev.fetch_events()?.collect() } else { break; };

            for ev in events {
                if let InputEventKind::Key(key) = ev.kind() {
                    let val = ev.value();
                    if val == 1 { held_keys.insert(key); } else if val == 0 { held_keys.remove(&key); }

                    let ctrl = held_keys.contains(&Key::KEY_LEFTCTRL) || held_keys.contains(&Key::KEY_RIGHTCTRL);
                    let alt = held_keys.contains(&Key::KEY_LEFTALT) || held_keys.contains(&Key::KEY_RIGHTALT);
                    let meta = held_keys.contains(&Key::KEY_LEFTMETA) || held_keys.contains(&Key::KEY_RIGHTMETA);
                    let has_mod = ctrl || alt || meta;

                    if key == Key::KEY_TAB && !has_mod {
                        if val == 1 { self.tab_held_and_not_used = true; } 
                        else if val == 0 {
                            if self.tab_held_and_not_used {
                                let mut p = self.processor.lock().unwrap();
                                p.toggle();
                                let msg = if p.chinese_enabled { "中文模式" } else { "直通模式" };
                                let summary = p.get_current_profile_display();
                                let _ = self.notify_tx.send(NotifyEvent::Close);
                                let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                                drop(p); self.update_gui();
                            }
                            self.tab_held_and_not_used = false;
                        }
                        continue;
                    }

                    // 拦截 CapsLock 和 Shift 触发筛选
                    if (key == Key::KEY_CAPSLOCK || key == Key::KEY_LEFTSHIFT || key == Key::KEY_RIGHTSHIFT) && !has_mod {
                        if val == 1 {
                            let mut p = self.processor.lock().unwrap();
                            if p.chinese_enabled && p.state != crate::engine::processor::ImeState::Direct {
                                if key == Key::KEY_CAPSLOCK {
                                    println!("[Host] Triggering Page Filter");
                                    p.start_page_filter();
                                } else {
                                    println!("[Host] Triggering Global Filter");
                                    p.start_global_filter();
                                }
                                drop(p);
                                self.update_gui();
                                continue; 
                            }
                        }
                    }

                    if key == Key::KEY_CAPSLOCK && !has_mod {
                        if val == 1 {
                            if held_keys.contains(&Key::KEY_TAB) {
                                self.tab_held_and_not_used = false;
                                if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.tap(Key::KEY_CAPSLOCK); }
                            }
                        }
                        continue;
                    }

                    if val == 1 {
                        let (toggle_main, toggle_alt, switch_prof, cycle_preview, toggle_notify, cycle_paste, toggle_trad, toggle_mod, toggle_ks, toggle_commit) = {
                            let conf = self.config.read().unwrap();
                            (parse_key(&conf.hotkeys.switch_language.key), parse_key(&conf.hotkeys.switch_language_alt.key), parse_key(&conf.hotkeys.switch_dictionary.key), parse_key(&conf.hotkeys.cycle_preview_mode.key), parse_key(&conf.hotkeys.toggle_notifications.key), parse_key(&conf.hotkeys.cycle_paste_method.key), parse_key(&conf.hotkeys.toggle_traditional_gui.key), parse_key(&conf.hotkeys.toggle_modern_gui.key), parse_key(&conf.hotkeys.toggle_keystrokes.key), parse_key(&conf.hotkeys.switch_commit_mode.key))
                        };
                        
                        if is_combo(&held_keys, &toggle_main) || is_combo(&held_keys, &toggle_alt) {
                            let mut p = self.processor.lock().unwrap(); p.toggle();
                            let summary = p.get_current_profile_display();
                            let msg = if p.chinese_enabled { "中文模式" } else { "直通模式" };
                            let _ = self.notify_tx.send(NotifyEvent::Close);
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                            drop(p); self.update_gui(); continue;
                        }

                        if is_combo(&held_keys, &switch_prof) {
                            let mut p = self.processor.lock().unwrap(); let profile = p.next_profile();
                            let _ = self.notify_tx.send(NotifyEvent::Message(profile.clone(), "方案已切换".to_string()));
                            if let Ok(mut w) = self.config.write() { w.input.active_profiles = vec![profile]; let _ = crate::save_config(&w); }
                            drop(p); self.update_gui(); continue;
                        }

                        if is_combo(&held_keys, &cycle_preview) {
                            let mut p = self.processor.lock().unwrap();
                            p.phantom_mode = match p.phantom_mode { crate::engine::processor::PhantomMode::None => crate::engine::processor::PhantomMode::Pinyin, _ => crate::engine::processor::PhantomMode::None };
                            let msg = if p.phantom_mode == crate::engine::processor::PhantomMode::Pinyin { "预览: 开启" } else { "预览: 关闭" };
                            let summary = p.get_current_profile_display();
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                            drop(p); continue;
                        }

                        if is_combo(&held_keys, &toggle_notify) {
                            let mut p = self.processor.lock().unwrap(); p.show_notifications = !p.show_notifications;
                            let msg = if p.show_notifications { "通知: 开启" } else { "通知: 关闭" };
                            let summary = p.get_current_profile_display();
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                            drop(p); continue;
                        }

                        if is_combo(&held_keys, &cycle_paste) {
                            let msg = if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.cycle_paste_mode() } else { "失败".into() };
                            let summary = self.processor.lock().unwrap().get_current_profile_display();
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg));
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_trad) {
                            let mut p = self.processor.lock().unwrap(); p.show_candidates = !p.show_candidates;
                            let _ = self.notify_tx.send(NotifyEvent::Message("UI".into(), if p.show_candidates { "显示传统窗" } else { "隐藏传统窗" }.into()));
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_mod) {
                            let mut p = self.processor.lock().unwrap(); p.show_modern_candidates = !p.show_modern_candidates;
                            let _ = self.notify_tx.send(NotifyEvent::Message("UI".into(), if p.show_modern_candidates { "显示卡片窗" } else { "隐藏卡片窗" }.into()));
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_ks) {
                            let mut p = self.processor.lock().unwrap(); p.show_keystrokes = !p.show_keystrokes;
                            if !p.show_keystrokes { if let Some(ref tx) = self.gui_tx { let _ = tx.send(GuiEvent::ClearKeystrokes); } }
                            continue;
                        }

                        if is_combo(&held_keys, &toggle_commit) {
                            let (mode, summary) = {
                                let mut p = self.processor.lock().unwrap();
                                p.commit_mode = if p.commit_mode == "single" { "double".into() } else { "single".into() };
                                (p.commit_mode.clone(), p.get_current_profile_display())
                            };
                            if let Ok(mut w) = self.config.write() { w.input.commit_mode = mode.clone(); let _ = crate::save_config(&w); }
                            let msg = format!("上屏模式: {}", mode);
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg));
                            continue;
                        }
                    }

                    let shift = held_keys.contains(&Key::KEY_LEFTSHIFT) || held_keys.contains(&Key::KEY_RIGHTSHIFT);
                    let mut p = self.processor.lock().unwrap();
                    if p.chinese_enabled && !has_mod {
                        match p.handle_key(key, val, shift) {
                            Action::Emit(s) => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.send_text(&s); } }
                            Action::DeleteAndEmit { delete, insert } => { if let Ok(mut vkbd) = self.vkbd.lock() { if delete > 0 { vkbd.backspace(delete); } if !insert.is_empty() { let _ = vkbd.send_text(&insert); } } }
                            Action::Notify(s, b) => { let _ = self.notify_tx.send(NotifyEvent::Message(s, b)); }
                            Action::Alert => { if self.config.read().unwrap().input.enable_error_sound { let _ = std::process::Command::new("canberra-gtk-play").arg("--id=dialog-error").spawn(); } }
                            Action::PassThrough => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); } }
                            _ => {}
                        }
                        drop(p); if val != 0 { self.update_gui(); self.notify_preview(); }
                    } else {
                        if has_mod && p.state != crate::engine::processor::ImeState::Direct { let del = p.phantom_text.chars().count(); p.reset(); if del > 0 { if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.backspace(del); } } }
                        drop(p); if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); }
                    }

                    if val == 1 {
                        let p = self.processor.lock().unwrap();
                        if p.show_keystrokes { if let Some(ref tx) = self.gui_tx { let name = map_key_to_display_name(key); if !name.is_empty() { let _ = tx.send(GuiEvent::Keystroke(name)); } } }
                    }
                }
            }
        }
        if let Ok(mut dev) = self.dev.lock() { let _ = dev.ungrab(); }
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
            if !p.aux_filter.is_empty() {
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

    fn notify_preview(&self) {
        let p = self.processor.lock().unwrap();
        if !p.show_notifications || (p.buffer.is_empty() && !p.switch_mode) { let _ = self.notify_tx.send(NotifyEvent::Close); return; }
        let summary = if p.switch_mode { format!("[快捷切换] {}: {}", p.get_current_profile_display(), p.joined_sentence) } else { format!("{}: {}", p.get_current_profile_display(), p.joined_sentence) };
        let mut body = String::new();
        if p.switch_mode && p.buffer.is_empty() { body.push_str("请按键切换方案: C(中) E(英) R(雾) J(日)"); }
        let start = p.page; let end = (start + p.page_size).min(p.candidates.len());
        for (i, cand) in p.candidates[start..end].iter().enumerate() {
            let abs_idx = start + i; let hint = p.candidate_hints.get(abs_idx).cloned().unwrap_or_default();
            if abs_idx == p.selected { body.push_str(&format!("【{}.{} {}】", (i % p.page_size)+1, cand, hint)); }
            else { body.push_str(&format!("{}.{}{} ", (i % p.page_size)+1, cand, hint)); }
        }
        let _ = self.notify_tx.send(NotifyEvent::Update(summary, body));
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
