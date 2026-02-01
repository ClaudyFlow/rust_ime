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

pub struct EvdevHost {
    processor: Arc<Mutex<Processor>>,
    vkbd: Mutex<Vkbd>,
    dev: Mutex<Device>,
    gui_tx: Option<Sender<GuiEvent>>,
    notify_tx: Sender<NotifyEvent>,
    should_exit: Arc<AtomicBool>,
    config: Arc<std::sync::RwLock<Config>>,
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
        let vkbd = Vkbd::new(&dev)?;
        Ok(Self {
            processor,
            vkbd: Mutex::new(vkbd),
            dev: Mutex::new(dev),
            gui_tx,
            notify_tx,
            should_exit: Arc::new(AtomicBool::new(false)),
            config,
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
        println!("[EvdevHost] 启动硬件拦截模式 (无感知窗口检测已停用)...");

        while !self.should_exit.load(Ordering::Relaxed) {
            let events: Vec<_> = if let Ok(mut dev) = self.dev.lock() {
                dev.fetch_events()?.collect()
            } else { break; };

            for ev in events {
                if let InputEventKind::Key(key) = ev.kind() {
                    let val = ev.value();
                    if val == 1 { 
                        held_keys.insert(key); 
                    } else if val == 0 { held_keys.remove(&key); }

                    // --- 快捷键检测 ---
                    if val == 1 {
                        let (toggle_main, toggle_alt, switch_prof, cycle_preview, toggle_notify, cycle_paste, toggle_trad, toggle_mod) = {
                            let conf = self.config.read().unwrap();
                            (
                                parse_key(&conf.hotkeys.switch_language.key),
                                parse_key(&conf.hotkeys.switch_language_alt.key),
                                parse_key(&conf.hotkeys.switch_dictionary.key),
                                parse_key(&conf.hotkeys.cycle_preview_mode.key),
                                parse_key(&conf.hotkeys.toggle_notifications.key),
                                parse_key(&conf.hotkeys.cycle_paste_method.key),
                                parse_key(&conf.hotkeys.toggle_traditional_gui.key),
                                parse_key(&conf.hotkeys.toggle_modern_gui.key),
                            )
                        };
                        
                        // 1. 中英切换
                        if is_combo(&held_keys, &toggle_main) || is_combo(&held_keys, &toggle_alt) {
                            let mut p = self.processor.lock().unwrap();
                            let action = p.toggle();
                            let enabled = p.chinese_enabled;
                            let msg = if enabled { "中文模式" } else { "英文模式" };
                            let summary = p.current_profile.clone();
                            
                            // 发送新通知的同时，确保之前的候选列表通知已关闭
                            let _ = self.notify_tx.send(NotifyEvent::Close);
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                            
                            // 执行切换产生的动作（目前 toggle 返回 Consume，此处保留结构以防万一）
                            match action {
                                Action::DeleteAndEmit { delete, insert } => {
                                    if let Ok(mut vkbd) = self.vkbd.lock() {
                                        if delete > 0 { vkbd.backspace(delete); }
                                        if !insert.is_empty() { let _ = vkbd.send_text(&insert); }
                                    }
                                }
                                Action::Emit(s) => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.send_text(&s); } }
                                _ => {}
                            }

                            drop(p); self.update_gui(); continue;
                        }

                        // 2. 方案切换 (Ctrl+Alt+S)
                        if is_combo(&held_keys, &switch_prof) {
                            let mut p = self.processor.lock().unwrap();
                            let profile = p.next_profile();
                            let _ = self.notify_tx.send(NotifyEvent::Message(profile.clone(), "方案已切换".to_string()));
                            if let Ok(mut w) = self.config.write() {
                                w.input.default_profile = profile;
                                let _ = crate::save_config(&w);
                            }
                            drop(p); self.update_gui(); continue;
                        }

                        // 3. 预览模式切换 (Ctrl+Alt+P)
                        if is_combo(&held_keys, &cycle_preview) {
                            let mut p = self.processor.lock().unwrap();
                            p.phantom_mode = match p.phantom_mode {
                                crate::engine::processor::PhantomMode::None => crate::engine::processor::PhantomMode::Pinyin,
                                crate::engine::processor::PhantomMode::Pinyin => crate::engine::processor::PhantomMode::None,
                            };
                            let mode_str = match p.phantom_mode {
                                crate::engine::processor::PhantomMode::Pinyin => "pinyin",
                                _ => "none",
                            };
                            let msg = if mode_str == "pinyin" { "预览: 开启" } else { "预览: 关闭" };
                            let summary = p.current_profile.clone();
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                            if let Ok(mut w) = self.config.write() {
                                w.appearance.preview_mode = mode_str.to_string();
                                let _ = crate::save_config(&w);
                            }
                            drop(p); self.update_gui(); continue;
                        }

                        // 4. 通知开关 (Ctrl+Alt+N)
                        if is_combo(&held_keys, &toggle_notify) {
                            let mut p = self.processor.lock().unwrap();
                            p.show_notifications = !p.show_notifications;
                            let enabled = p.show_notifications;
                            let msg = if enabled { "通知: 开启" } else { "通知: 关闭" };
                            let summary = p.current_profile.clone();
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                            if let Ok(mut w) = self.config.write() {
                                w.appearance.show_notifications = enabled;
                                let _ = crate::save_config(&w);
                            }
                            drop(p); continue;
                        }

                        // 5. 粘贴模式切换 (Ctrl+Alt+V)
                        if is_combo(&held_keys, &cycle_paste) {
                            let msg = if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.cycle_paste_mode() } else { "切换失败".to_string() };
                            let p = self.processor.lock().unwrap();
                            let summary = p.current_profile.clone();
                            let _ = self.notify_tx.send(NotifyEvent::Message(summary, msg));
                            drop(p); continue;
                        }

                        // 6. 切换传统候选窗 (Ctrl+Alt+G)
                        if is_combo(&held_keys, &toggle_trad) {
                            let enabled = {
                                let mut p = self.processor.lock().unwrap();
                                p.show_candidates = !p.show_candidates;
                                p.show_candidates
                            };
                            if let Ok(mut w) = self.config.write() {
                                w.appearance.show_candidates = enabled;
                                let _ = crate::save_config(&w);
                                let _ = self.gui_tx.as_ref().unwrap().send(crate::ui::GuiEvent::ApplyConfig(w.clone()));
                            }
                            let msg = if enabled { "显示传统候选窗" } else { "隐藏传统候选窗" };
                            let _ = self.notify_tx.send(NotifyEvent::Message("UI切换".into(), msg.into()));
                            continue;
                        }

                        // 7. 切换卡片式候选词 (Ctrl+Alt+H)
                        if is_combo(&held_keys, &toggle_mod) {
                            let enabled = {
                                let mut p = self.processor.lock().unwrap();
                                p.show_modern_candidates = !p.show_modern_candidates;
                                p.show_modern_candidates
                            };
                            if let Ok(mut w) = self.config.write() {
                                w.appearance.show_modern_candidates = enabled;
                                let _ = crate::save_config(&w);
                                let _ = self.gui_tx.as_ref().unwrap().send(crate::ui::GuiEvent::ApplyConfig(w.clone()));
                            }
                            let msg = if enabled { "显示卡片候选词" } else { "隐藏卡片候选词" };
                            let _ = self.notify_tx.send(NotifyEvent::Message("UI切换".into(), msg.into()));
                            continue;
                        }
                    }

                    let shift = held_keys.contains(&Key::KEY_LEFTSHIFT) || held_keys.contains(&Key::KEY_RIGHTSHIFT);
                    let ctrl = held_keys.contains(&Key::KEY_LEFTCTRL) || held_keys.contains(&Key::KEY_RIGHTCTRL);
                    let alt = held_keys.contains(&Key::KEY_LEFTALT) || held_keys.contains(&Key::KEY_RIGHTALT);
                    let meta = held_keys.contains(&Key::KEY_LEFTMETA) || held_keys.contains(&Key::KEY_RIGHTMETA);
                    let has_mod = ctrl || alt || meta;

                    let mut p = self.processor.lock().unwrap();
                    if p.chinese_enabled && !has_mod {
                        match p.handle_key(key, val != 0, shift) {
                            Action::Emit(s) => { 
                                if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.send_text(&s); }
                            }
                            Action::DeleteAndEmit { delete, insert } => { 
                                if let Ok(mut vkbd) = self.vkbd.lock() {
                                    if delete > 0 { vkbd.backspace(delete); }
                                    if !insert.is_empty() { let _ = vkbd.send_text(&insert); }
                                }
                            }
                            Action::Consume => {}
                            Action::PassThrough => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); } }
                        }
                        drop(p);
                        if val != 0 {
                            self.update_gui();
                            self.notify_preview();
                        }
                    } else {
                        // 如果有修饰键按下，且当前正在输入拼音，重置输入法并清除屏幕预览
                        if has_mod && p.state != crate::engine::processor::ImeState::Direct {
                            let del = p.phantom_text.chars().count();
                            p.reset();
                            if del > 0 {
                                if let Ok(mut vkbd) = self.vkbd.lock() {
                                    vkbd.backspace(del);
                                }
                            }
                        }
                        drop(p);
                        if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); }
                    }

                    // --- UI 回显移动到逻辑处理之后 ---
                    if val == 1 {
                        let show_ks = self.processor.lock().unwrap().show_keystrokes;
                        if show_ks {
                            if let Some(ref tx) = self.gui_tx {
                                let name = format!("{:?}", key).replace("KEY_", "");
                                let _ = tx.send(GuiEvent::Keystroke(name));
                            }
                        }
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
            
            // 如果不显示候选框，且没有 buffer，清空并返回
            if p.buffer.is_empty() || !p.chinese_enabled {
                let _ = tx.send(GuiEvent::Update { pinyin: "".into(), candidates: vec![], hints: vec![], selected: 0 });
                return;
            }

            let pinyin = if p.best_segmentation.is_empty() { p.buffer.clone() } else { p.best_segmentation.join("'") };
            
            // 打印到终端 (前台模式)
            if !p.candidates.is_empty() {
                print!("\r\x1b[K[Console] {} | ", pinyin);
                let start = p.page;
                let end = (start + 5).min(p.candidates.len());
                for (i, cand) in p.candidates[start..end].iter().enumerate() {
                    let abs_idx = start + i;
                    if abs_idx == p.selected { print!("\x1b[1;32m{}.{}\x1b[0m ", i+1, cand); }
                    else { print!("{}.{} ", i+1, cand); }
                }
                use std::io::Write;
                std::io::stdout().flush().unwrap();
            }

            // 如果开启了候选框显示，发送完整数据
            if p.show_candidates {
                let _ = tx.send(GuiEvent::Update { pinyin, candidates: p.candidates.clone(), hints: p.candidate_hints.clone(), selected: p.selected });
            } else {
                // 否则仅在控制台或通过拼音预览（由 handle_key 处理）工作，GUI 保持清空
                let _ = tx.send(GuiEvent::Update { pinyin: "".into(), candidates: vec![], hints: vec![], selected: 0 });
            }
        }
    }

    fn notify_preview(&self) {
        let p = self.processor.lock().unwrap();
        if !p.show_notifications || p.buffer.is_empty() { 
            let _ = self.notify_tx.send(NotifyEvent::Close);
            return; 
        }
        let pinyin = if p.best_segmentation.is_empty() { p.buffer.clone() } else { p.best_segmentation.join("'") };
        let mut body = String::new();
        let start = p.page;
        let end = (start + 5).min(p.candidates.len());
        for (i, cand) in p.candidates[start..end].iter().enumerate() {
            let abs_idx = start + i;
            let hint = p.candidate_hints.get(abs_idx).cloned().unwrap_or_default();
            if abs_idx == p.selected { body.push_str(&format!("【{}.{}{}】 ", i+1, cand, hint)); }
            else { body.push_str(&format!("{}.{}{} ", i+1, cand, hint)); }
        }
        let summary = format!("[{}] {}", p.current_profile, pinyin);
        let _ = self.notify_tx.send(NotifyEvent::Update(summary, body));
    }
}

fn is_combo(held: &HashSet<Key>, target: &[Key]) -> bool {
    if target.is_empty() { return false; }
    target.iter().all(|k| held.contains(k))
}