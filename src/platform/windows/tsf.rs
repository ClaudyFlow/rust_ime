use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::engine::Processor;
use crate::ui::GuiEvent;
use crate::Config;

pub struct TsfHost {
    processor: Arc<Mutex<Processor>>,
    gui_tx: Option<Sender<GuiEvent>>,
    tray_tx: Sender<crate::ui::tray::TrayEvent>,
    config: Arc<RwLock<Config>>,
}

impl TsfHost {
    pub fn new(processor: Arc<Mutex<Processor>>, gui_tx: Option<Sender<GuiEvent>>, config: Arc<RwLock<Config>>, tray_tx: Sender<crate::ui::tray::TrayEvent>) -> Self {
        Self { processor, gui_tx, tray_tx, config }
    }
}

fn update_gui_impl(gui_tx: &Option<Sender<GuiEvent>>, processor: &Arc<Mutex<Processor>>) {
    if let Some(ref tx) = gui_tx {
        let p = processor.lock().unwrap();
        if p.buffer.is_empty() || !p.chinese_enabled { 
            let _ = tx.send(GuiEvent::Update { pinyin: "".into(), candidates: vec![], hints: vec![], selected: 0, sentence: "".into(), cursor_pos: 0, commit_mode: p.commit_mode.clone() }); 
            return; 
        }
        let mut pinyin = if p.best_segmentation.is_empty() { p.buffer.clone() } else { p.best_segmentation.join(" ") };
        if p.nav_mode { pinyin.push_str(" [H:左 J:下 K:上 L:右]"); }
        if !p.aux_filter.is_empty() {
            let mut display_aux = String::new();
            for (i, c) in p.aux_filter.chars().enumerate() { if i == 0 { display_aux.push(c.to_ascii_uppercase()); } else { display_aux.push(c.to_ascii_lowercase()); } }
            pinyin.push_str(&display_aux);
        }
        let _ = tx.send(GuiEvent::Update { pinyin, candidates: p.candidates.clone(), hints: p.candidate_hints.clone(), selected: p.selected, sentence: p.joined_sentence.clone(), cursor_pos: p.cursor_pos, commit_mode: p.commit_mode.clone() });
    }
}

#[cfg(target_os = "windows")]
fn get_system_cursor_pos() -> Option<(i32, i32)> {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GetForegroundWindow, GUITHREADINFO, GetCaretPos};
        use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows::Win32::Graphics::Gdi::ClientToScreen;
        use windows::Win32::Foundation::*;
        let mut info = GUITHREADINFO::default();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut info).is_ok() {
            let mut pt = POINT { x: info.rcCaret.left, y: info.rcCaret.bottom };
            let hwnd = if info.hwndCaret.0 != 0 { info.hwndCaret } else { let focus = GetFocus(); if focus.0 != 0 { focus } else { GetForegroundWindow() } };
            if hwnd.0 != 0 { let _ = ClientToScreen(hwnd, &mut pt); if pt.x != 0 || pt.y != 0 { return Some((pt.x, pt.y)); } }
        }
        let mut pt = POINT::default();
        if GetCaretPos(&mut pt).is_ok() { let hwnd = GetForegroundWindow(); if hwnd.0 != 0 { let _ = ClientToScreen(hwnd, &mut pt); if pt.x != 0 || pt.y != 0 { return Some((pt.x, pt.y + 20)); } } }
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
            use windows::Win32::Security::*;
            let pipe_name_w = crate::registry::to_pcwstr("\\\\.\\pipe\\rust_ime_pipe");
            let mut sd = SECURITY_DESCRIPTOR::default();
            unsafe { let _ = InitializeSecurityDescriptor(PSECURITY_DESCRIPTOR(&mut sd as *mut _ as *mut _), 1); let _ = SetSecurityDescriptorDacl(PSECURITY_DESCRIPTOR(&mut sd as *mut _ as *mut _), true, None, false); }
            let sd_ptr = &sd as *const _ as usize; 
            let processor = self.processor.clone(); let gui_tx = self.gui_tx.clone(); let tray_tx = self.tray_tx.clone(); let config = self.config.clone();
            for _i in 0..3 {
                let pipe_name_u16 = pipe_name_w.clone(); let p = processor.clone(); let g = gui_tx.clone(); let t = tray_tx.clone(); let c = config.clone();
                std::thread::spawn(move || {
                    loop {
                        unsafe {
                            let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd_ptr as *mut _, bInheritHandle: false.into() };
                            let h = CreateNamedPipeW(PCWSTR(pipe_name_u16.as_ptr()), PIPE_ACCESS_DUPLEX, PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT, PIPE_UNLIMITED_INSTANCES, 1024, 1024, 0, Some(&sa));
                            if h.is_invalid() { std::thread::sleep(std::time::Duration::from_millis(100)); continue; }
                            let connect_res = ConnectNamedPipe(h, None);
                            // 如果成功连接，或者客户端已经连上了
                            if connect_res.is_ok() || connect_res.err().map_or(false, |e| e.code() == windows::Win32::Foundation::ERROR_PIPE_CONNECTED.to_hresult()) {
                                let pi = p.clone(); let gi = g.clone(); let ti = t.clone(); let ci = c.clone();
                                std::thread::spawn(move || { handle_client(h, pi, gi, ti, ci); let _ = CloseHandle(h); });
                            } else { let _ = CloseHandle(h); }
                        }
                    }
                });
            }
            loop { std::thread::sleep(std::time::Duration::from_secs(3600)); }
        }
        #[cfg(not(target_os = "windows"))] { Err("TsfHost 仅支持 Windows。".into()) }
    }
}

#[cfg(target_os = "windows")]
fn is_hk_match(config_key: &str, pressed_key_code: u32, ctrl: bool, alt: bool, shift: bool) -> bool {
    let binding = config_key.to_lowercase();
    let parts: Vec<&str> = binding.split('+').map(|s| s.trim()).collect();
    let mut req_ctrl = false; let mut req_alt = false; let mut req_shift = false; let mut target_key = "";
    for part in parts { match part { "ctrl" => req_ctrl = true, "alt" => req_alt = true, "shift" => req_shift = true, _ => target_key = part } }
    if req_ctrl != ctrl || req_alt != alt || req_shift != shift { return false; }
    let target_code = match target_key { "space" => 0x20, "tab" => 0x09, "backspace" => 0x08, "enter" => 0x0D, "esc" => 0x1B, s if s.len() == 1 => s.chars().next().unwrap().to_ascii_uppercase() as u32, _ => 0 };
    pressed_key_code == target_code
}

#[cfg(target_os = "windows")]
unsafe fn handle_client(handle: windows::Win32::Foundation::HANDLE, processor: std::sync::Arc<std::sync::Mutex<crate::engine::Processor>>, gui_tx: Option<std::sync::mpsc::Sender<crate::ui::GuiEvent>>, tray_tx: std::sync::mpsc::Sender<crate::ui::tray::TrayEvent>, config: Arc<RwLock<Config>>) {
    use windows::Win32::Storage::FileSystem::*;
    use crate::engine::processor::Action;
    let mut buffer = [0u8; 1024];
    loop {
        let mut bytes_read = 0;
        if ReadFile(handle, Some(&mut buffer), Some(&mut bytes_read), None).is_err() || bytes_read == 0 { break; }
        if bytes_read < 6 { continue; }
        let msg_type = buffer[0];
        let key_code = u32::from_le_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]);
        let modifiers = buffer[5];
        let shift = (modifiers & 1) != 0; let ctrl = (modifiers & 2) != 0; let alt = (modifiers & 4) != 0;
        
        if msg_type == 5 { // Activated
            // println!("[TSF] Client activated, showing status bar");
            if let Some(ref tx) = gui_tx {
                let p = processor.lock().unwrap();
                let short = p.get_short_display();
                let enabled = p.chinese_enabled;
                let _ = tx.send(GuiEvent::ShowStatus(short, enabled));
                let _ = tx.send(GuiEvent::SetVisible(true));
            }
            let _ = WriteFile(handle, Some(&[2u8]), Some(&mut 0), None);
            continue;
        }

        if msg_type == 6 { // Deactivated
            // println!("[TSF] Client deactivated, hiding status bar");
            if let Some(ref tx) = gui_tx {
                let _ = tx.send(GuiEvent::SetVisible(false));
            }
            let _ = WriteFile(handle, Some(&[2u8]), Some(&mut 0), None);
            continue;
        }

        if msg_type == 1 && bytes_read >= 14 {
            let mut x = i32::from_le_bytes([buffer[6], buffer[7], buffer[8], buffer[9]]);
            let mut y = i32::from_le_bytes([buffer[10], buffer[11], buffer[12], buffer[13]]);
            if x == 0 && y == 0 { if let Some((sx, sy)) = get_system_cursor_pos() { x = sx; y = sy; } }
            if (x != 0 || y != 0) && gui_tx.is_some() { let _ = gui_tx.as_ref().unwrap().send(crate::ui::GuiEvent::MoveTo { x, y }); }
        }

        // 核心热键判定
        let (enable_tab, enable_ctrl_space, switch_key) = {
            let c = config.read().unwrap();
            (c.hotkeys.enable_tab_toggle, c.hotkeys.enable_ctrl_space_toggle, c.hotkeys.switch_language.key.clone())
        };

        let is_tab_match = enable_tab && is_hk_match(&switch_key, key_code, ctrl, alt, shift);
        let is_ctrl_space_match = enable_ctrl_space && (key_code == 0x20 && ctrl && !alt && !shift);

        if is_tab_match || is_ctrl_space_match {
            if msg_type == 1 {
                let mut p = processor.lock().unwrap(); p.toggle();
                let enabled = p.chinese_enabled; let short = p.get_short_display(); let profile = p.get_current_profile_display();
                drop(p);
                let _ = tray_tx.send(crate::ui::tray::TrayEvent::SyncStatus { chinese_enabled: enabled, active_profile: profile });
                if let Some(ref tx) = gui_tx { let _ = tx.send(crate::ui::GuiEvent::ShowStatus(if enabled { short } else { "英".into() }, enabled)); }
                update_gui_impl(&gui_tx, &processor);
            }
            let _ = WriteFile(handle, Some(&[2u8]), Some(&mut 0), None); continue;
        }

        // Ctrl/Alt 快捷键放行逻辑 (当没输入拼音时)
        if (ctrl || alt) && !is_ctrl_space_match {
            let p = processor.lock().unwrap();
            if p.buffer.is_empty() {
                let _ = WriteFile(handle, Some(&[0u8]), Some(&mut 0), None);
                continue;
            }
        }

        let key = match key_code {
            0x41..=0x5A => crate::engine::keys::VirtualKey::from_u32(key_code - 0x41),
            0x30..=0x39 => crate::engine::keys::VirtualKey::from_u32(key_code - 0x30 + 26),
            0x20 => Some(crate::engine::keys::VirtualKey::Space), 0x08 => Some(crate::engine::keys::VirtualKey::Backspace), 0x0D => Some(crate::engine::keys::VirtualKey::Enter), 0x1B => Some(crate::engine::keys::VirtualKey::Esc), 0x14 => Some(crate::engine::keys::VirtualKey::CapsLock), 0x09 => Some(crate::engine::keys::VirtualKey::Tab),
            0x25 => Some(crate::engine::keys::VirtualKey::Left), 0x26 => Some(crate::engine::keys::VirtualKey::Up), 0x27 => Some(crate::engine::keys::VirtualKey::Right), 0x28 => Some(crate::engine::keys::VirtualKey::Down),
            0xBB => Some(crate::engine::keys::VirtualKey::Equal), 0xBD => Some(crate::engine::keys::VirtualKey::Minus), 0xBC => Some(crate::engine::keys::VirtualKey::Comma), 0xBE => Some(crate::engine::keys::VirtualKey::Dot), 0xBF => Some(crate::engine::keys::VirtualKey::Slash),
            0xBA => Some(crate::engine::keys::VirtualKey::Semicolon), 0xDE => Some(crate::engine::keys::VirtualKey::Apostrophe), 0xDB => Some(crate::engine::keys::VirtualKey::LeftBrace), 0xDD => Some(crate::engine::keys::VirtualKey::RightBrace), 0xDC => Some(crate::engine::keys::VirtualKey::Backslash), 0xC0 => Some(crate::engine::keys::VirtualKey::Grave),
            _ => None,
        };

        if let Some(key) = key {
            let mut response = Vec::new();
            if msg_type == 1 || msg_type == 3 {
                let mut p = processor.lock().unwrap();
                let action = p.handle_key(key, if msg_type == 1 { 1 } else { 0 }, shift, ctrl, alt);
                drop(p);
                match action {
                    Action::Emit(txt) => { response.push(1); response.extend_from_slice(txt.as_bytes()); update_gui_impl(&gui_tx, &processor); }
                    Action::DeleteAndEmit { delete, insert } => { if delete > 0 { response.push(3); response.push(delete as u8); } else { response.push(1); } response.extend_from_slice(insert.as_bytes()); update_gui_impl(&gui_tx, &processor); }
                    Action::Consume => { response.push(2); update_gui_impl(&gui_tx, &processor); }
                    Action::Alert => { response.push(2); update_gui_impl(&gui_tx, &processor); }
                    Action::Notify(summary, _body) => {
                        let (active, profile) = { let p = processor.lock().unwrap(); (p.chinese_enabled, p.get_current_profile_display()) };
                        if let Some(ref tx) = gui_tx { let _ = tx.send(crate::ui::GuiEvent::ShowStatus(summary, active)); }
                        let _ = tray_tx.send(crate::ui::tray::TrayEvent::SyncStatus { chinese_enabled: active, active_profile: profile });
                        update_gui_impl(&gui_tx, &processor);
                        response.push(2); 
                    }
                    _ => { response.push(0); }
                }
            } else {
                let p = processor.lock().unwrap();
                let is_letter = key_code >= 0x41 && key_code <= 0x5A;
                let is_special = key_code == 0x14;
                let is_punct = match key_code { 0x20 | 0xC0 | 0xBD | 0xBB | 0xDB | 0xDD | 0xDC | 0xBA | 0xDE | 0xBC | 0xBE | 0xBF => true, 0x30..=0x39 if shift => true, _ => false };
                if p.chinese_enabled && (!p.buffer.is_empty() || is_letter || is_punct || is_special) { response.push(2); } else { response.push(0); }
            }
            let _ = WriteFile(handle, Some(&response), Some(&mut 0), None);
        } else { let _ = WriteFile(handle, Some(&[0u8]), Some(&mut 0), None); }
    }
}
