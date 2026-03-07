use crate::engine::Processor;
use crate::engine::processor::Action;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::platform::linux::vkbd::Vkbd;
use crate::config::Config;
use crate::ui::GuiEvent;
use crate::engine::keys::VirtualKey;
use evdev::{Device, InputEventKind, Key};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

fn evdev_to_virtual(key: Key) -> Option<VirtualKey> {
    match key {
        Key::KEY_A => Some(VirtualKey::A), Key::KEY_B => Some(VirtualKey::B), Key::KEY_C => Some(VirtualKey::C), Key::KEY_D => Some(VirtualKey::D), Key::KEY_E => Some(VirtualKey::E), Key::KEY_F => Some(VirtualKey::F), Key::KEY_G => Some(VirtualKey::G), Key::KEY_H => Some(VirtualKey::H), Key::KEY_I => Some(VirtualKey::I), Key::KEY_J => Some(VirtualKey::J), Key::KEY_K => Some(VirtualKey::K), Key::KEY_L => Some(VirtualKey::L), Key::KEY_M => Some(VirtualKey::M), Key::KEY_N => Some(VirtualKey::N), Key::KEY_O => Some(VirtualKey::O), Key::KEY_P => Some(VirtualKey::P), Key::KEY_Q => Some(VirtualKey::Q), Key::KEY_R => Some(VirtualKey::R), Key::KEY_S => Some(VirtualKey::S), Key::KEY_T => Some(VirtualKey::T), Key::KEY_U => Some(VirtualKey::U), Key::KEY_V => Some(VirtualKey::V), Key::KEY_W => Some(VirtualKey::W), Key::KEY_X => Some(VirtualKey::X), Key::KEY_Y => Some(VirtualKey::Y), Key::KEY_Z => Some(VirtualKey::Z),
        Key::KEY_0 => Some(VirtualKey::Digit0), Key::KEY_1 => Some(VirtualKey::Digit1), Key::KEY_2 => Some(VirtualKey::Digit2), Key::KEY_3 => Some(VirtualKey::Digit3), Key::KEY_4 => Some(VirtualKey::Digit4), Key::KEY_5 => Some(VirtualKey::Digit5), Key::KEY_6 => Some(VirtualKey::Digit6), Key::KEY_7 => Some(VirtualKey::Digit7), Key::KEY_8 => Some(VirtualKey::Digit8), Key::KEY_9 => Some(VirtualKey::Digit9),
        Key::KEY_SPACE => Some(VirtualKey::Space), Key::KEY_ENTER | Key::KEY_KPENTER => Some(VirtualKey::Enter), Key::KEY_TAB => Some(VirtualKey::Tab), Key::KEY_BACKSPACE => Some(VirtualKey::Backspace), Key::KEY_ESC => Some(VirtualKey::Esc), Key::KEY_CAPSLOCK => Some(VirtualKey::CapsLock),
        Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => Some(VirtualKey::Shift),
        Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => Some(VirtualKey::Control),
        Key::KEY_LEFTALT | Key::KEY_RIGHTALT => Some(VirtualKey::Alt),
        Key::KEY_LEFT => Some(VirtualKey::Left), Key::KEY_RIGHT => Some(VirtualKey::Right), Key::KEY_UP => Some(VirtualKey::Up), Key::KEY_DOWN => Some(VirtualKey::Down),
        Key::KEY_PAGEUP => Some(VirtualKey::PageUp), Key::KEY_PAGEDOWN => Some(VirtualKey::PageDown), Key::KEY_HOME => Some(VirtualKey::Home), Key::KEY_END => Some(VirtualKey::End), Key::KEY_DELETE => Some(VirtualKey::Delete),
        Key::KEY_GRAVE => Some(VirtualKey::Grave), Key::KEY_MINUS => Some(VirtualKey::Minus), Key::KEY_EQUAL => Some(VirtualKey::Equal), Key::KEY_LEFTBRACE => Some(VirtualKey::LeftBrace), Key::KEY_RIGHTBRACE => Some(VirtualKey::RightBrace), Key::KEY_BACKSLASH => Some(VirtualKey::Backslash), Key::KEY_SEMICOLON => Some(VirtualKey::Semicolon), Key::KEY_APOSTROPHE => Some(VirtualKey::Apostrophe), Key::KEY_COMMA => Some(VirtualKey::Comma), Key::KEY_DOT => Some(VirtualKey::Dot), Key::KEY_SLASH => Some(VirtualKey::Slash),
        _ => None,
    }
}

fn name_to_evdev_key(name: &str) -> Key {
    match name.to_lowercase().as_str() {
        "ctrl" | "control" => Key::KEY_LEFTCTRL,
        "alt" => Key::KEY_LEFTALT,
        "shift" => Key::KEY_LEFTSHIFT,
        "meta" | "super" | "win" => Key::KEY_LEFTMETA,
        "tab" => Key::KEY_TAB,
        "space" => Key::KEY_SPACE,
        "caps" | "capslock" => Key::KEY_CAPSLOCK,
        "esc" | "escape" => Key::KEY_ESC,
        "enter" => Key::KEY_ENTER,
        "backspace" => Key::KEY_BACKSPACE,
        "left" => Key::KEY_LEFT,
        "right" => Key::KEY_RIGHT,
        "up" => Key::KEY_UP,
        "down" => Key::KEY_DOWN,
        "a" => Key::KEY_A, "b" => Key::KEY_B, "c" => Key::KEY_C, "d" => Key::KEY_D, "e" => Key::KEY_E, "f" => Key::KEY_F, "g" => Key::KEY_G, "h" => Key::KEY_H, "i" => Key::KEY_I, "j" => Key::KEY_J, "k" => Key::KEY_K, "l" => Key::KEY_L, "m" => Key::KEY_M, "n" => Key::KEY_N, "o" => Key::KEY_O, "p" => Key::KEY_P, "q" => Key::KEY_Q, "r" => Key::KEY_R, "s" => Key::KEY_S, "t" => Key::KEY_T, "u" => Key::KEY_U, "v" => Key::KEY_V, "w" => Key::KEY_W, "x" => Key::KEY_X, "y" => Key::KEY_Y, "z" => Key::KEY_Z,
        "0" => Key::KEY_0, "1" => Key::KEY_1, "2" => Key::KEY_2, "3" => Key::KEY_3, "4" => Key::KEY_4, "5" => Key::KEY_5, "6" => Key::KEY_6, "7" => Key::KEY_7, "8" => Key::KEY_8, "9" => Key::KEY_9,
        _ => Key::KEY_RESERVED,
    }
}

fn parse_key(s: &str) -> Vec<Vec<Vec<Key>>> {
    s.split(',')
        .map(|combo| {
            combo.trim().split('+')
                .map(|part| {
                    part.trim().split('|')
                        .map(name_to_evdev_key)
                        .collect()
                })
                .collect()
        })
        .collect()
}

pub struct EvdevHost {
    processor: Arc<Mutex<Processor>>,
    vkbd: Arc<Mutex<Vkbd>>,
    dev: Arc<Mutex<Device>>, // 修改为 Arc 以便在 Guard 中共享
    gui_tx: Option<Sender<GuiEvent>>,
    tray_tx: Sender<crate::ui::tray::TrayEvent>,
    should_exit: Arc<AtomicBool>,
    config: Arc<std::sync::RwLock<Config>>,
    tab_held_and_not_used: bool,
    lookup_tx: std::sync::mpsc::Sender<()>,
    lookup_pending: Arc<AtomicBool>,
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
        let vkbd_raw = Vkbd::new(&dev)?;
        let vkbd = Arc::new(Mutex::new(vkbd_raw));
        {
            let conf = config.read().unwrap();
            vkbd.lock().unwrap().apply_config(&conf);
        }

        let (lookup_tx, lookup_rx) = std::sync::mpsc::channel::<()>();
        let lookup_pending = Arc::new(AtomicBool::new(false));

        // 启动后台检索线程
        let p_bg = processor.clone();
        let v_bg = vkbd.clone();
        let g_bg = gui_tx.clone();
        let pending_bg = lookup_pending.clone();

        std::thread::spawn(move || {
            while let Ok(_) = lookup_rx.recv() {
                // 消耗掉积压的所有检索请求，只做最后一次
                while let Ok(_) = lookup_rx.try_recv() {}

                let mut p = p_bg.lock().unwrap();
                if let Some(commit_action) = p.lookup() {
                    if let Ok(mut vkbd) = v_bg.lock() {
                        execute_action(&mut *vkbd, commit_action, None);
                    }
                }
                
                // 【核心修复】检索完成后，重新计算并执行 Phantom Text 更新
                // 这样能确保编辑器里的预览汉字和候选词窗口同步
                let phantom_action = p.update_phantom_action();
                if let Ok(mut vkbd) = v_bg.lock() {
                    execute_action(&mut *vkbd, phantom_action, None);
                }

                update_gui_internal(&*p, &g_bg);
                pending_bg.store(false, Ordering::SeqCst);
            }
        });

        Ok(Self {
            processor, vkbd, dev: Arc::new(Mutex::new(dev)), gui_tx, tray_tx,
            should_exit: Arc::new(AtomicBool::new(false)), config, tab_held_and_not_used: false,
            lookup_tx, lookup_pending,
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

                    // 1. 基础状态维护
                    if val == 1 { 
                        held_keys.insert(key); 
                        if key != Key::KEY_TAB { self.tab_held_and_not_used = false; }
                    } else if val == 0 { 
                        held_keys.remove(&key); 
                    }

                    let ctrl = held_keys.contains(&Key::KEY_LEFTCTRL) || held_keys.contains(&Key::KEY_RIGHTCTRL);
                    let alt = held_keys.contains(&Key::KEY_LEFTALT) || held_keys.contains(&Key::KEY_RIGHTALT);
                    let meta = held_keys.contains(&Key::KEY_LEFTMETA) || held_keys.contains(&Key::KEY_RIGHTMETA);
                    let has_mod = ctrl || alt || meta;

                    // 2. 【核心修复】直接透传判断 (英文模式 或 Tab 键除外)
                    {
                        let mut p = self.processor.lock().unwrap();
                        let is_direct = !p.chinese_enabled;
                        let is_empty = p.session.buffer.is_empty();
                        
                        // 如果处于直通(英文)模式，除 Tab 键外全部直接物理透传
                        if is_direct && key != Key::KEY_TAB {
                            drop(p);
                            if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); }
                            continue;
                        }

                        // 如果是 Enter 键且缓冲区为空，也直接透传
                        if (key == Key::KEY_ENTER || key == Key::KEY_KPENTER) && is_empty {
                            if !is_empty { p.reset(); }
                            drop(p);
                            if let Ok(mut vkbd) = self.vkbd.lock() { 
                                let _ = vkbd.emit_raw(key, val); 
                            }
                            continue;
                        }
                        drop(p);
                    }

                    if key == Key::KEY_TAB && !has_mod {
                        if val == 1 { 
                            self.tab_held_and_not_used = true;
                            continue;
                        } 
                        else if val == 0 {
                            if self.tab_held_and_not_used {
                                let mut p = self.processor.lock().unwrap();
                                p.toggle();
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
                            continue;
                        }
                    }

                    // 拦截 Shift 触发全局筛选
                    if (key == Key::KEY_LEFTSHIFT || key == Key::KEY_RIGHTSHIFT) && !has_mod {
                        if val == 1 {
                            let mut p = self.processor.lock().unwrap();
                            if p.chinese_enabled && p.session.state != crate::engine::processor::ImeState::Direct {
                                println!("[Host] Triggering Global Filter");
                                p.start_global_filter();
                                drop(p);
                                self.update_gui();
                                continue; 
                            }
                        }
                    }

                    if val == 1 {
                        let toggle_main = {
                            let conf = self.config.read().unwrap();
                            parse_key(&conf.hotkeys.switch_language.key)
                        };
                        
                        if is_combo(&held_keys, &toggle_main) {
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
                    }

                    let shift = held_keys.contains(&Key::KEY_LEFTSHIFT) || held_keys.contains(&Key::KEY_RIGHTSHIFT);
                    let ctrl = held_keys.contains(&Key::KEY_LEFTCTRL) || held_keys.contains(&Key::KEY_RIGHTCTRL);
                    let alt = held_keys.contains(&Key::KEY_LEFTALT) || held_keys.contains(&Key::KEY_RIGHTALT);
                    let mut p = self.processor.lock().unwrap();
                    if p.chinese_enabled && !has_mod {
                        if let Some(vk) = evdev_to_virtual(key) {
                            // 所有的确认、分页、选词、导航键都走“慢车道”，确保状态同步
                            let is_sync_key = vk == VirtualKey::Space || vk == VirtualKey::Enter 
                                || (vk.to_u32() >= VirtualKey::Digit0.to_u32() && vk.to_u32() <= VirtualKey::Digit9.to_u32())
                                || matches!(vk, VirtualKey::PageUp | VirtualKey::PageDown | VirtualKey::Up | VirtualKey::Down | VirtualKey::Left | VirtualKey::Right | VirtualKey::Minus | VirtualKey::Equal | VirtualKey::Comma | VirtualKey::Dot);

                            if is_sync_key {
                                // 【慢车道：等待检索完成】
                                drop(p);
                                while self.lookup_pending.load(Ordering::SeqCst) {
                                    std::thread::yield_now();
                                }
                                p = self.processor.lock().unwrap();
                                
                                let action = p.handle_key_ext(vk, val, shift, ctrl, alt, true);
                                if let Ok(mut vkbd) = self.vkbd.lock() {
                                    execute_action(&mut *vkbd, action, Some((key, val)));
                                }
                            } else {
                                // 【快车道：非阻塞字母、退格、Esc】
                                let fast_action = p.handle_key_ext(vk, val, shift, ctrl, alt, false);
                                if let Ok(mut vkbd) = self.vkbd.lock() {
                                    execute_action(&mut *vkbd, fast_action, Some((key, val)));
                                }

                                // 如果是字母/退格且按下状态，发送异步检索请求
                                if val != 0 {
                                    self.lookup_pending.store(true, Ordering::SeqCst);
                                    let _ = self.lookup_tx.send(());
                                }
                            }
                        } else {
                            if let Ok(mut vkbd) = self.vkbd.lock() { let _ = vkbd.emit_raw(key, val); }
                        }
                        drop(p); if val != 0 { self.update_gui(); }
                    } else {
                        if has_mod && p.session.state != crate::engine::processor::ImeState::Direct { let del = p.session.phantom_text.chars().count(); p.reset(); if del > 0 { if let Ok(mut vkbd) = self.vkbd.lock() { vkbd.backspace(del); } } }
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
        let p = self.processor.lock().unwrap();
        update_gui_internal(&*p, &self.gui_tx);
    }
}

fn update_gui_internal(p: &Processor, gui_tx: &Option<Sender<GuiEvent>>) {
    if let Some(ref tx) = gui_tx {
        if p.session.buffer.is_empty() || !p.chinese_enabled { 
            let _ = tx.send(GuiEvent::Update { 
                pinyin: "".into(), 
                candidates: vec![], 
                selected: 0, 
                sentence: "".into(),
                cursor_pos: 0,
                commit_mode: p.config.commit_mode.clone(),
            }); 
            return; 
        }
        
        let pinyin = crate::engine::compositor::Compositor::get_preedit(p);

        if p.config.show_candidates {
            let page_size = p.config.page_size;
            let start = p.session.page.min(p.session.candidates.len());
            let end = (start + page_size).min(p.session.candidates.len());
            
            let mut display_candidates = Vec::new();
            for (i, c) in p.session.candidates[start..end].iter().enumerate() {
                let label = format!("{}.", i + 1);
                let full_display = if c.hint.is_empty() {
                    format!("{}{}", label, c.text)
                } else {
                    format!("{}{}({})", label, c.text, c.hint)
                };
                display_candidates.push(crate::ui::DisplayCandidate {
                    text: c.text.clone(),
                    label,
                    hint: c.hint.clone(),
                    full_display,
                });
            }

            let relative_selected = p.session.selected.saturating_sub(start);

            let _ = tx.send(GuiEvent::Update { 
                pinyin, 
                candidates: display_candidates, 
                selected: relative_selected, 
                sentence: p.session.joined_sentence.clone(),
                cursor_pos: p.session.cursor_pos,
                commit_mode: p.config.commit_mode.clone(),
            });
        } else { 
            let _ = tx.send(GuiEvent::Update { 
                pinyin: "".into(), 
                candidates: vec![], 
                selected: 0, 
                sentence: "".into(),
                cursor_pos: 0,
                commit_mode: p.config.commit_mode.clone(),
            }); 
        }
    }
}

fn execute_action(vkbd: &mut Vkbd, action: Action, raw_key: Option<(Key, i32)>) {
    match action {
        Action::Emit(s) => { vkbd.send_text(&s); }
        Action::DeleteAndEmit { delete, insert } => {
            if delete > 0 { vkbd.backspace(delete); }
            if !insert.is_empty() { vkbd.send_text(&insert); }
        }
        Action::PassThrough => {
            if let Some((k, v)) = raw_key {
                let _ = vkbd.emit_raw(k, v);
            }
        }
        Action::Alert => {
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
        _ => {}
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
