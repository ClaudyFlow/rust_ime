use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::engine::Processor;
use crate::ui::gui::GuiEvent;
use crate::{Config, NotifyEvent};

pub struct TsfHost {
    processor: Arc<Mutex<Processor>>,
    gui_tx: Option<Sender<GuiEvent>>,
    _config: Arc<RwLock<Config>>,
    _notify_tx: Sender<NotifyEvent>,
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
            _config: config,
            _notify_tx: notify_tx,
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
                                    let shift = buffer[5] != 0;
                                    
                                    let key = match key_code {
                                        0x41..=0x5A => std::mem::transmute::<u32, crate::evdev::Key>(key_code - 0x41),
                                        0x20 => crate::evdev::Key::KEY_SPACE,
                                        0x08 => crate::evdev::Key::KEY_BACKSPACE,
                                        0x0D => crate::evdev::Key::KEY_ENTER,
                                        _ => crate::evdev::Key::KEY_ESC,
                                    };

                                    let mut p = processor.lock().unwrap();
                                    let action = p.handle_key(key, 1, shift);
                                    
                                    if let Some(ref tx) = gui_tx {
                                        let _ = tx.send(GuiEvent::Update {
                                            pinyin: p.buffer.clone(),
                                            candidates: p.candidates.clone(),
                                            hints: p.candidate_hints.clone(),
                                            selected: p.selected,
                                            sentence: p.joined_sentence.clone(),
                                            cursor_pos: p.cursor_pos,
                                            commit_mode: p.commit_mode.clone(),
                                        });
                                    }

                                    let mut response = Vec::new();
                                    match action {
                                        crate::engine::processor::Action::Emit(txt) => {
                                            response.push(1); 
                                            response.extend_from_slice(txt.as_bytes());
                                        }
                                        crate::engine::processor::Action::Consume => {
                                            response.push(2);
                                        }
                                        _ => {
                                            response.push(0);
                                        }
                                    }
                                    
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

