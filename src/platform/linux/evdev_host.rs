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
        // 修饰键
        Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => "Ctrl".to_string(),
        Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => "Shift".to_string(),
        Key::KEY_LEFTALT | Key::KEY_RIGHTALT => "Alt".to_string(),
        Key::KEY_LEFTMETA | Key::KEY_RIGHTMETA => "Win".to_string(),
        Key::KEY_CAPSLOCK => "Caps".to_string(),
        
        // 特殊键
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
        
        // 功能键
        Key::KEY_F1 => "F1".to_string(),
        Key::KEY_F2 => "F2".to_string(),
        Key::KEY_F3 => "F3".to_string(),
        Key::KEY_F4 => "F4".to_string(),
        Key::KEY_F5 => "F5".to_string(),
        Key::KEY_F6 => "F6".to_string(),
        Key::KEY_F7 => "F7".to_string(),
        Key::KEY_F8 => "F8".to_string(),
        Key::KEY_F9 => "F9".to_string(),
        Key::KEY_F10 => "F10".to_string(),
        Key::KEY_F11 => "F11".to_string(),
        Key::KEY_F12 => "F12".to_string(),
        
        // 字母键
        Key::KEY_A => "A".to_string(),
        Key::KEY_B => "B".to_string(),
        Key::KEY_C => "C".to_string(),
        Key::KEY_D => "D".to_string(),
        Key::KEY_E => "E".to_string(),
        Key::KEY_F => "F".to_string(),
        Key::KEY_G => "G".to_string(),
        Key::KEY_H => "H".to_string(),
        Key::KEY_I => "I".to_string(),
        Key::KEY_J => "J".to_string(),
        Key::KEY_K => "K".to_string(),
        Key::KEY_L => "L".to_string(),
        Key::KEY_M => "M".to_string(),
        Key::KEY_N => "N".to_string(),
        Key::KEY_O => "O".to_string(),
        Key::KEY_P => "P".to_string(),
        Key::KEY_Q => "Q".to_string(),
        Key::KEY_R => "R".to_string(),
        Key::KEY_S => "S".to_string(),
        Key::KEY_T => "T".to_string(),
        Key::KEY_U => "U".to_string(),
        Key::KEY_V => "V".to_string(),
        Key::KEY_W => "W".to_string(),
        Key::KEY_X => "X".to_string(),
        Key::KEY_Y => "Y".to_string(),
        Key::KEY_Z => "Z".to_string(),
        
        // 数字键
        Key::KEY_0 => "0".to_string(),
        Key::KEY_1 => "1".to_string(),
        Key::KEY_2 => "2".to_string(),
        Key::KEY_3 => "3".to_string(),
        Key::KEY_4 => "4".to_string(),
        Key::KEY_5 => "5".to_string(),
        Key::KEY_6 => "6".to_string(),
        Key::KEY_7 => "7".to_string(),
        Key::KEY_8 => "8".to_string(),
        Key::KEY_9 => "9".to_string(),
        
        // 其他键
        Key::KEY_MINUS => "-".to_string(),
        Key::KEY_EQUAL => "=".to_string(),
        Key::KEY_LEFTBRACE => "[".to_string(),
        Key::KEY_RIGHTBRACE => "]".to_string(),
        Key::KEY_SEMICOLON => ";".to_string(),
        Key::KEY_APOSTROPHE => "'".to_string(),
        Key::KEY_GRAVE => "`".to_string(),
        Key::KEY_BACKSLASH => "\\".to_string(),
        Key::KEY_COMMA => ",".to_string(),
        Key::KEY_DOT => ".".to_string(),
        Key::KEY_SLASH => "/".to_string(),
        
        // 其他按键使用原始名称
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
    pending_tab: bool,
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
            pending_tab: false,
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

                    let _shift = held_keys.contains(&Key::KEY_LEFTSHIFT) || held_keys.contains(&Key::KEY_RIGHTSHIFT);
                    let ctrl = held_keys.contains(&Key::KEY_LEFTCTRL) || held_keys.contains(&Key::KEY_RIGHTCTRL);
                    let alt = held_keys.contains(&Key::KEY_LEFTALT) || held_keys.contains(&Key::KEY_RIGHTALT);
                    let meta = held_keys.contains(&Key::KEY_LEFTMETA) || held_keys.contains(&Key::KEY_RIGHTMETA);
                    let has_mod = ctrl || alt || meta;

                    // --- 特殊处理: Tab 键 Dual-Role 逻辑 ---
                    // 仅在无其他修饰键、中文模式且开启长韵母映射时启用
                    let (chinese_mode, quick_rime_enabled) = {
                        let p = self.processor.lock().unwrap();
                        let conf = self.config.read().unwrap();
                        (p.chinese_enabled, conf.input.enable_quick_rime)
                    };

                    if key == Key::KEY_TAB && !has_mod && chinese_mode && quick_rime_enabled {
                        if val == 1 { // Press
                            self.pending_tab = true;
                            continue; // 暂不发送，拦截
                        } else if val == 0 { // Release
                            if self.pending_tab {
                                // Tab 未被消费，原样发送
                                if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.tap(Key::KEY_TAB); }
                                self.pending_tab = false;
                            }
                            continue;
                        }
                    }

                    // --- 快捷键检测 ---
                    if val == 1 {
                        // 0. 优先检测：Tab 组合键 / 快速韵母 (Quick Rime)
                        if self.pending_tab {
                             let quick_rimes = self.config.read().unwrap().input.quick_rimes.clone();
                             let mut handled_quick_rime = false;
                             
                             // 构造虚拟触发键名，例如 "tab+l"
                             // 这里简化匹配：直接查找 trigger 字符串包含 "tab+" 且以当前按键名结尾的配置
                             let key_name = map_key_to_display_name(key).to_lowercase();
                             let trigger_target = format!("tab+{}", key_name);

                             for qr in quick_rimes {
                                 if qr.trigger.to_lowercase() == trigger_target {
                                     let mut p = self.processor.lock().unwrap();
                                     if p.chinese_enabled {
                                         // 消费 pending_tab
                                         self.pending_tab = false;
                                         
                                         // 注入韵母
                                         let action = p.inject_text(&qr.insert);
                                         match action {
                                            Action::Emit(s) => { if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.send_text(&s); } }
                                            Action::DeleteAndEmit { delete, insert } => { 
                                                if let Ok(mut vkbd) = self.vkbd.lock() {
                                                    if delete > 0 { vkbd.backspace(delete); }
                                                    if !insert.is_empty() { let _ = vkbd.send_text(&insert); }
                                                }
                                            }
                                            Action::Consume => {}
                                            Action::Alert => {
                                                // QuickRime 注入导致警报 (虽然理论上注入的是预设正确韵母，不太会发生)
                                                if self.config.read().unwrap().input.enable_error_sound {
                                                    let _ = std::process::Command::new("canberra-gtk-play").arg("--id=dialog-error").spawn();
                                                }
                                            }
                                            Action::PassThrough => {} 
                                         }
                                         handled_quick_rime = true;
                                     }
                                                                      drop(p);
                                                                      if handled_quick_rime {
                                                                          self.update_gui();
                                                                          self.notify_preview();
                                                                          break; 
                                                                      }
                                                                  }
                                     
                             }

                             if handled_quick_rime {
                                 continue; // 按键已被处理，跳过后续逻辑
                             } else {
                                 // Tab 被按下了，但当前键不是组合键的一部分 -> 立即补发 Tab
                                 if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.tap(Key::KEY_TAB); }
                                 self.pending_tab = false;
                                 // 然后继续处理当前按键（fall through）
                             }
                        }

                        let (toggle_main, toggle_alt, switch_prof, cycle_preview, toggle_notify, cycle_paste, toggle_trad, toggle_mod, toggle_ks, toggle_commit) = {
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
                                parse_key(&conf.hotkeys.toggle_keystrokes.key),
                                parse_key(&conf.hotkeys.switch_commit_mode.key),
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

// 8. 切换 按键显示 (Ctrl+Alt+K)
                        if is_combo(&held_keys, &toggle_ks) {
                            let enabled = {
                                let mut p = self.processor.lock().unwrap();
                                p.show_keystrokes = !p.show_keystrokes;
                                p.show_keystrokes
                            };
                            if let Ok(mut w) = self.config.write() {
                                w.appearance.show_keystrokes = enabled;
                                let _ = crate::save_config(&w);
                            }
                            let msg = if enabled { "开启 按键显示" } else { "关闭 按键显示" };
                            let _ = self.notify_tx.send(NotifyEvent::Message("功能切换".into(), msg.into()));
                            
                            // 如果关闭按键显示，清除当前的按键显示
                            if !enabled {
                                if let Some(ref tx) = self.gui_tx {
                                    let _ = tx.send(GuiEvent::ClearKeystrokes);
                                }
                            }
                            continue;
                        }

                        // 9. 切换上屏模式 (Ctrl+Alt+M)
                        if is_combo(&held_keys, &toggle_commit) {
                            let (mode, profile) = {
                                let mut p = self.processor.lock().unwrap();
                                p.commit_mode = if p.commit_mode == "single" { "double".into() } else { "single".into() };
                                (p.commit_mode.clone(), p.current_profile.clone())
                            };
                            if let Ok(mut w) = self.config.write() {
                                w.input.commit_mode = mode.clone();
                                let _ = crate::save_config(&w);
                            }
                            let msg = if mode == "single" { "上屏: 词模式(单空格)" } else { "上屏: 长句模式(双空格)" };
                            let _ = self.notify_tx.send(NotifyEvent::Message(profile, msg.into()));
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
                        // 执行动作前同步延迟设置
                        if let Ok(mut vkbd) = self.vkbd.lock() {
                             vkbd.clipboard_delay_ms = self.config.read().unwrap().input.clipboard_delay_ms;
                        }

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
                            Action::Alert => {
                                // 播放错误音
                                let enabled = self.config.read().unwrap().input.enable_error_sound;
                                if enabled {
                                    let _ = std::process::Command::new("canberra-gtk-play")
                                        .arg("--id=dialog-error")
                                        .spawn();
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
                        // 逻辑：要么是英文模式，要么是带修饰键的中文模式
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
                        // 核心：所有非输入法处理的按键必须在这里 Emit 出去！
                        if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); }
                    }

                    // --- UI 回显 (按键显示) 移动到逻辑处理之后 ---
                    if val == 1 {
                        let show_ks = self.processor.lock().unwrap().show_keystrokes;
                        if show_ks {
                            if let Some(ref tx) = self.gui_tx {
                                let name = map_key_to_display_name(key);
                                if !name.is_empty() {
                                    let _ = tx.send(GuiEvent::Keystroke(name));
                                }
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
                let _ = tx.send(GuiEvent::Update { pinyin: "".into(), candidates: vec![], hints: vec![], selected: 0, sentence: "".into() });
                return;
            }

                let pinyin = if p.best_segmentation.is_empty() { p.buffer.clone() } else { p.best_segmentation.join(" ") };
            
            // 打印到终端 (前台模式)
            if !p.candidates.is_empty() || !p.joined_sentence.is_empty() {
                print!("\r\x1b[K[Console] {} | {} | ", pinyin, p.joined_sentence);
                let start = p.page;
                let end = (start + 10).min(p.candidates.len());
                for (i, cand) in p.candidates[start..end].iter().enumerate() {
                    let abs_idx = start + i;
                    let hint = p.candidate_hints.get(abs_idx).cloned().unwrap_or_default();
                    if abs_idx == p.selected { print!("\x1b[1;32m{}.{}{} \x1b[0m ", (i % 10)+1, cand, hint); }
                    else { print!("{}.{}{} ", (i % 10)+1, cand, hint); }
                }
                use std::io::Write;
                std::io::stdout().flush().unwrap();
            }

            // 如果开启了候选框显示，发送完整数据
            if p.show_candidates {
                let _ = tx.send(GuiEvent::Update { 
                    pinyin, 
                    candidates: p.candidates.clone(), 
                    hints: p.candidate_hints.clone(), 
                    selected: p.selected,
                    sentence: p.joined_sentence.clone(),
                });
            } else {
                // 否则仅在控制台或通过拼音预览（由 handle_key 处理）工作，GUI 保持清空
                let _ = tx.send(GuiEvent::Update { pinyin: "".into(), candidates: vec![], hints: vec![], selected: 0, sentence: "".into() });
            }
        }
    }

    fn notify_preview(&self) {
        let p = self.processor.lock().unwrap();
        if !p.show_notifications || p.buffer.is_empty() { 
            let _ = self.notify_tx.send(NotifyEvent::Close);
            return; 
        }

        let summary = if p.vim_mode {
            format!("[Vim] {}: {}", p.current_profile, p.joined_sentence)
        } else {
            format!("{}: {}", p.current_profile, p.joined_sentence)
        };

        let mut body = String::new();
        
        // 如果在 Vim 模式，回显带光标的拼音
        if p.vim_mode {
            let buf_chars: Vec<char> = p.buffer.chars().collect();
            let mut buf_with_cursor = String::new();
            for (i, &c) in buf_chars.iter().enumerate() {
                if i == p.cursor_pos { buf_with_cursor.push('|'); }
                buf_with_cursor.push(c);
            }
            if p.cursor_pos == buf_chars.len() { buf_with_cursor.push('|'); }
            body.push_str(&format!("PinYin: {}\n", buf_with_cursor));
        }

        let start = p.page;
        let end = (start + 10).min(p.candidates.len());
        for (i, cand) in p.candidates[start..end].iter().enumerate() {
            let abs_idx = start + i;
            let hint = p.candidate_hints.get(abs_idx).cloned().unwrap_or_default();
            if abs_idx == p.selected { body.push_str(&format!("【{}.{} {}】", (i % 10)+1, cand, hint)); }
            else { body.push_str(&format!("{}.{}{} ", (i % 10)+1, cand, hint)); }
        }
        
        let _ = self.notify_tx.send(NotifyEvent::Update(summary, body));
    }
}

fn is_combo(held: &HashSet<Key>, target: &[Vec<Key>]) -> bool {

    if target.is_empty() { return false; }

    // 每一组修饰键/按键（如 [LCTRL, RCTRL]）中至少有一个在 held 中

    target.iter().all(|keys| keys.iter().any(|k| held.contains(k)))

}
