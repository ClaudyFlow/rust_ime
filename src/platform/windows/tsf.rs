use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::engine::Processor;
use crate::ui::GuiEvent;
use crate::{Config, NotifyEvent};

pub struct TsfHost {
    processor: Arc<Mutex<Processor>>,
    gui_tx: Option<Sender<GuiEvent>>,
    config: Arc<RwLock<Config>>,
    notify_tx: Sender<NotifyEvent>,
}

impl TsfHost {
    pub fn new(
        processor: Arc<Mutex<Processor>>,
        gui_tx: Option<Sender<GuiEvent>>,
        config: Arc<RwLock<Config>>,
        notify_tx: Sender<NotifyEvent>,
    ) -> Self {
        Self {
            processor,
            gui_tx,
            config,
            notify_tx,
        }
    }

    fn update_gui(&self) {
        update_gui_impl(&self.gui_tx, &self.processor);
    }
}

fn update_gui_impl(gui_tx: &Option<Sender<GuiEvent>>, processor: &Arc<Mutex<Processor>>) {
    if let Some(ref tx) = gui_tx {
        let p = processor.lock().unwrap();
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
        
        let mut pinyin = if p.best_segmentation.is_empty() { p.buffer.clone() } else { p.best_segmentation.join(" ") };
        if p.nav_mode { pinyin.push_str(" [NAV]"); }
        if !p.aux_filter.is_empty() {
            let mut display_aux = String::new();
            for (i, c) in p.aux_filter.chars().enumerate() {
                if i == 0 { display_aux.push(c.to_ascii_uppercase()); }
                else { display_aux.push(c.to_ascii_lowercase()); }
            }
            pinyin.push_str(&display_aux);
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

#[cfg(target_os = "windows")]
fn get_system_cursor_pos() -> Option<(i32, i32)> {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Graphics::Gdi::ClientToScreen;
        use windows::Win32::Foundation::*;
        let mut info = GUITHREADINFO::default();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut info).is_ok() {
            let mut pt = POINT {
                x: info.rcCaret.left,
                y: info.rcCaret.bottom,
            };
            let _ = ClientToScreen(info.hwndCaret, &mut pt);
            if pt.x != 0 || pt.y != 0 {
                return Some((pt.x, pt.y));
            }
        }
    }
    None
}

impl InputMethodHost for TsfHost {
    fn set_preedit(&self, _text: &str, _cursor_pos: usize) {}
    fn commit_text(&self, _text: &str) {}
    fn get_cursor_rect(&self) -> Option<Rect> { None }
    
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Pipes::*;
            use windows::Win32::Storage::FileSystem::*;
            use windows::Win32::Foundation::*;
            use windows::core::PCWSTR;

            let pipe_name_w = crate::registry::to_pcwstr("\\\\.\\pipe\\rust_ime_pipe");
            let pipe_pcwstr = PCWSTR(pipe_name_w.as_ptr());

            println!("[TSF Server] 正在启动命名管道: \\\\.\\pipe\\rust_ime_pipe");

            loop {
                unsafe {
                    let h_pipe = CreateNamedPipeW(
                        pipe_pcwstr,
                        PIPE_ACCESS_DUPLEX,
                        PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                        PIPE_UNLIMITED_INSTANCES,
                        1024,
                        1024,
                        0,
                        None,
                    );
                    
                    if h_pipe.is_invalid() {
                        continue;
                    }

                    if ConnectNamedPipe(h_pipe, None).is_ok() {
                        let processor = self.processor.clone();
                        let gui_tx = self.gui_tx.clone();
                        let notify_tx = self.notify_tx.clone();
                        
                        std::thread::spawn(move || {
                            let handle = h_pipe;
                            let mut buffer = [0u8; 1024];
                            loop {
                                let mut bytes_read = 0;
                                if ReadFile(handle, Some(&mut buffer), Some(&mut bytes_read), None).is_err() || bytes_read == 0 {
                                    break;
                                }
                                
                                if bytes_read < 6 { continue; }
                                
                                let msg_type = buffer[0]; // 1=Actual, 2=Test
                                let key_code = u32::from_le_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]);
                                let modifiers = buffer[5];
                                let shift = (modifiers & 1) != 0;
                                let ctrl = (modifiers & 2) != 0;
                                let alt = (modifiers & 4) != 0;
                                
                                if msg_type == 1 {
                                    println!("[TSF Pipe] KeyDown: 0x{:02X}, Shift: {}, Ctrl: {}, Alt: {}", key_code, shift, ctrl, alt);
                                }

                                // 1. 优先检查切换热键 (必须排在第一位)
                                let is_toggle = (key_code == 0x09 && !ctrl && !alt) || (key_code == 0x20 && ctrl);
                                if is_toggle {
                                    if msg_type == 1 {
                                        let mut p = processor.lock().unwrap();
                                        p.toggle();
                                        let enabled = p.chinese_enabled;
                                        let summary = p.get_current_profile_display();
                                        drop(p);
                                        println!("[TSF Toggle] Key: 0x{:02X}, Enabled: {}", key_code, enabled);
                                        let msg = if enabled { "中文模式" } else { "直通模式" };
                                        let _ = notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                                        update_gui_impl(&gui_tx, &processor);
                                    }
                                    let mut response = vec![2u8]; // Consume
                                    let mut bytes_written = 0;
                                    let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                    continue;
                                }

                                if msg_type == 1 {
                                    let mut x = i32::from_le_bytes([buffer[6], buffer[7], buffer[8], buffer[9]]);
                                    let mut y = i32::from_le_bytes([buffer[10], buffer[11], buffer[12], buffer[13]]);
                                    
                                    if x == 0 && y == 0 {
                                        if let Some((sx, sy)) = get_system_cursor_pos() {
                                            x = sx;
                                            y = sy;
                                        }
                                    }

                                    if x != 0 || y != 0 {
                                        if let Some(ref tx) = gui_tx {
                                            let _ = tx.send(GuiEvent::MoveTo { x, y });
                                        }
                                    }
                                }

                                // 灵魂功能：CapsLock / Shift 触发筛选
                                if (key_code == 0x14 || key_code == 0x10) && !ctrl && !alt {
                                    let mut p = processor.lock().unwrap();
                                    if p.chinese_enabled && !p.buffer.is_empty() {
                                        if msg_type == 1 {
                                            if key_code == 0x14 {
                                                p.start_page_filter();
                                            } else {
                                                p.start_global_filter();
                                            }
                                            drop(p);
                                            update_gui_impl(&gui_tx, &processor);
                                        }
                                        let mut response = vec![2u8]; // Consume
                                        let mut bytes_written = 0;
                                        let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                        continue;
                                    }
                                }

                                // 直通逻辑：如果 buffer 为空且不是切换键，允许常用控制键直通
                                if (key_code == 0x0D || key_code == 0x08 || (key_code >= 0x30 && key_code <= 0x39)) && !ctrl && !alt && !shift {
                                    let p = processor.lock().unwrap();
                                    if p.buffer.is_empty() {
                                        let mut response = vec![0u8]; // PassThrough
                                        let mut bytes_written = 0;
                                        let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                        continue;
                                    }
                                }

                                // 如果是 Shift 键且不是组合键，直接放过 (PassThrough)
                                if key_code == 0x10 && !ctrl && !alt {
                                    let mut response = vec![0u8]; 
                                    let mut bytes_written = 0;
                                    let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                    continue;
                                }

                                let key = match key_code {
                                    0x41..=0x5A => Some(std::mem::transmute::<u8, crate::engine::keys::VirtualKey>((key_code - 0x41) as u8)),
                                    0x30..=0x39 => Some(std::mem::transmute::<u8, crate::engine::keys::VirtualKey>((key_code - 0x30 + 26) as u8)),
                                    0x20 => Some(crate::engine::keys::VirtualKey::Space),
                                    0x08 => Some(crate::engine::keys::VirtualKey::Backspace),
                                    0x0D => Some(crate::engine::keys::VirtualKey::Enter),
                                    0x1B => Some(crate::engine::keys::VirtualKey::Esc),
                                    0x09 => Some(crate::engine::keys::VirtualKey::Tab),
                                    0x25 => Some(crate::engine::keys::VirtualKey::Left),
                                    0x26 => Some(crate::engine::keys::VirtualKey::Up),
                                    0x27 => Some(crate::engine::keys::VirtualKey::Right),
                                    0x28 => Some(crate::engine::keys::VirtualKey::Down),
                                    0xBB => Some(crate::engine::keys::VirtualKey::Equal),
                                    0xBD => Some(crate::engine::keys::VirtualKey::Minus),
                                    0xBC => Some(crate::engine::keys::VirtualKey::Comma),
                                    0xBE => Some(crate::engine::keys::VirtualKey::Dot),
                                    0xBF => Some(crate::engine::keys::VirtualKey::Slash),
                                    0xBA => Some(crate::engine::keys::VirtualKey::Semicolon),
                                    0xDE => Some(crate::engine::keys::VirtualKey::Apostrophe),
                                    0xDB => Some(crate::engine::keys::VirtualKey::LeftBrace),
                                    0xDD => Some(crate::engine::keys::VirtualKey::RightBrace),
                                    0xDC => Some(crate::engine::keys::VirtualKey::Backslash),
                                    0xC0 => Some(crate::engine::keys::VirtualKey::Grave),
                                    _ => None,
                                };

                                if let Some(key) = key {
                                    let mut response = Vec::new();
                                    if msg_type == 1 {
                                        let mut p = processor.lock().unwrap();
                                        let action = p.handle_key(key, 1, shift);
                                        drop(p);
                                        update_gui_impl(&gui_tx, &processor);

                                        match action {
                                            crate::engine::processor::Action::Emit(txt) => {
                                                response.push(1); 
                                                response.extend_from_slice(txt.as_bytes());
                                            }
                                            crate::engine::processor::Action::DeleteAndEmit { delete: _, insert } => {
                                                response.push(1);
                                                response.extend_from_slice(insert.as_bytes());
                                            }
                                            crate::engine::processor::Action::Consume => {
                                                response.push(2);
                                            }
                                            crate::engine::processor::Action::Notify(s, b) => {
                                                let _ = notify_tx.send(NotifyEvent::Message(s, b));
                                                response.push(2);
                                            }
                                            _ => {
                                                response.push(0);
                                            }
                                        }
                                    } else {
                                        let p = processor.lock().unwrap();
                                        let is_letter = key_code >= 0x41 && key_code <= 0x5A;
                                        let would_handle = p.chinese_enabled && (!p.buffer.is_empty() || is_letter);
                                        if would_handle {
                                            response.push(2);
                                        } else {
                                            response.push(0);
                                        }
                                    }
                                    let mut bytes_written = 0;
                                    let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                } else {
                                    let mut response = vec![0u8]; 
                                    let mut bytes_written = 0;
                                    let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                }
                            }
                            let _ = CloseHandle(handle);
                        });
                    } else {
                        let _ = CloseHandle(h_pipe);
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        { Err("TsfHost 仅支持 Windows。".into()) }
    }
}