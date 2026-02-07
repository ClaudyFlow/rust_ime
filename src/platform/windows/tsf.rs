use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::engine::Processor;
use crate::ui::gui::GuiEvent;
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

            println!("[TSF Server] 正在启动命名管道: \\\\.\\pipe\\rust_ime_pipe");

            loop {
                let pipe_name = crate::registry::to_pcwstr("\\\\.\\pipe\\rust_ime_pipe");
                unsafe {
                    let h_pipe = CreateNamedPipeW(
                        PCWSTR(pipe_name.as_ptr()),
                        PIPE_ACCESS_DUPLEX,
                        PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                        PIPE_UNLIMITED_INSTANCES,
                        1024,
                        1024,
                        0,
                        None,
                    );
                    
                    if h_pipe.is_invalid() {
                        eprintln!("[TSF Server] CreateNamedPipeW 失败。");
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

                                if bytes_read >= 6 {
                                    let key_code = u32::from_le_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]);
                                    let modifiers = buffer[5];
                                    let shift = (modifiers & 1) != 0;
                                    let ctrl = (modifiers & 2) != 0;
                                    let alt = (modifiers & 4) != 0;
                                    
                                    println!("[TSF Pipe] Key: 0x{:02X}, Shift: {}, Ctrl: {}, Alt: {}", key_code, shift, ctrl, alt);

                                    // 检查语言切换热键 (默认为 Tab 或 Shift)
                                    if (key_code == 0x09 || key_code == 0x10) && !ctrl && !alt && !shift {
                                        let mut p = processor.lock().unwrap();
                                        p.toggle();
                                        let enabled = p.chinese_enabled;
                                        let summary = p.get_current_profile_display();
                                        drop(p);
                                        
                                        let msg = if enabled { "中文模式" } else { "直通模式" };
                                        let _ = notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                                        
                                        update_gui_impl(&gui_tx, &processor);
                                        
                                        let mut response = vec![2u8]; // Consume
                                        let mut bytes_written = 0;
                                        let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                        continue;
                                    }

                                    // 也可以支持 Ctrl+Space
                                    if key_code == 0x20 && ctrl && !alt && !shift {
                                        let mut p = processor.lock().unwrap();
                                        p.toggle();
                                        let enabled = p.chinese_enabled;
                                        let summary = p.get_current_profile_display();
                                        drop(p);
                                        
                                        let msg = if enabled { "中文模式" } else { "直通模式" };
                                        let _ = notify_tx.send(NotifyEvent::Message(summary, msg.to_string()));
                                        
                                        update_gui_impl(&gui_tx, &processor);
                                        
                                        let mut response = vec![2u8]; // Consume
                                        let mut bytes_written = 0;
                                        let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                        continue;
                                    }

                                    let key = match key_code {
                                        0x41..=0x5A => {
                                            // 直接转换回字母，Processor::handle_key 会根据 shift 处理大小写
                                            let base = if shift { 0 } else { 0 }; // 这里其实没区别，transmute 需要确切的 index
                                            // 参考 main.rs 中 Key 的定义: KEY_A = 0
                                            Some(std::mem::transmute::<u32, crate::evdev::Key>(key_code - 0x41))
                                        }
                                        0x30..=0x39 => Some(std::mem::transmute::<u32, crate::evdev::Key>(key_code - 0x30 + 26)),
                                        0x60..=0x69 => Some(std::mem::transmute::<u32, crate::evdev::Key>(key_code - 0x60 + 26)), // Numpad
                                        0x20 => Some(crate::evdev::Key::KEY_SPACE),
                                        0x08 => Some(crate::evdev::Key::KEY_BACKSPACE),
                                        0x0D => Some(crate::evdev::Key::KEY_ENTER),
                                        0x1B => Some(crate::evdev::Key::KEY_ESC),
                                        0x09 => Some(crate::evdev::Key::KEY_TAB),
                                        0x21 => Some(crate::evdev::Key::KEY_PAGEUP),
                                        0x22 => Some(crate::evdev::Key::KEY_PAGEDOWN),
                                        0x25 => Some(crate::evdev::Key::KEY_LEFT),
                                        0x26 => Some(crate::evdev::Key::KEY_UP),
                                        0x27 => Some(crate::evdev::Key::KEY_RIGHT),
                                        0x28 => Some(crate::evdev::Key::KEY_DOWN),
                                        0xBB => Some(crate::evdev::Key::KEY_EQUAL),
                                        0xBD => Some(crate::evdev::Key::KEY_MINUS),
                                        0xBC => Some(crate::evdev::Key::KEY_COMMA),
                                        0xBE => Some(crate::evdev::Key::KEY_DOT),
                                        0xBF => Some(crate::evdev::Key::KEY_SLASH),
                                        0xBA => Some(crate::evdev::Key::KEY_SEMICOLON),
                                        0xDE => Some(crate::evdev::Key::KEY_APOSTROPHE),
                                        0xDB => Some(crate::evdev::Key::KEY_LEFTBRACE),
                                        0xDD => Some(crate::evdev::Key::KEY_RIGHTBRACE),
                                        0xDC => Some(crate::evdev::Key::KEY_BACKSLASH),
                                        0xC0 => Some(crate::evdev::Key::KEY_GRAVE),
                                        _ => None,
                                    };

                                    if let Some(key) = key {
                                        let mut p = processor.lock().unwrap();
                                        let action = p.handle_key(key, 1, shift);
                                        drop(p);
                                        
                                        update_gui_impl(&gui_tx, &processor);

                                        let mut response = Vec::new();
                                        println!("[TSF Action] {:?} -> {:?}", key, action);
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
                                        
                                        let mut bytes_written = 0;
                                        let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                    } else {
                                        let mut response = vec![0u8]; // Action::None
                                        let mut bytes_written = 0;
                                        let _ = WriteFile(handle, Some(&response), Some(&mut bytes_written), None);
                                    }
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

