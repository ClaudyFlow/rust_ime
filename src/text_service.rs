use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::UI::TextServices::*,
    Win32::System::Diagnostics::Debug::OutputDebugStringW,
    Win32::Storage::FileSystem::*,
    Win32::System::Pipes::*,
    Win32::UI::Input::KeyboardAndMouse::{VK_SHIFT, VK_CONTROL, VK_MENU},
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::io::{Read, Write};

fn log(msg: &str) {
    let mut v: Vec<u16> = msg.encode_utf16().collect();
    v.push(0);
    unsafe { OutputDebugStringW(PCWSTR(v.as_ptr())); }
}

#[implement(ITfTextInputProcessor, ITfKeyEventSink)]
pub struct TextService {
    client_id: AtomicU32,
}

impl TextService {
    pub fn new() -> Self {
        log("RustIME: TextService::new");
        Self {
            client_id: AtomicU32::new(0),
        }
    }

    fn commit_text(&self, context: &ITfContext, text: &str) -> Result<()> {
        let client_id = self.client_id.load(Ordering::SeqCst);
        unsafe {
            let session = EditSession::new(context.clone(), text.to_string());
            let session_ptr: ITfEditSession = session.into();
            let _ = context.RequestEditSession(client_id, &session_ptr, TF_ES_READWRITE);
        }
        Ok(())
    }

    fn send_key_to_server(&self, key_code: u32, modifiers: u8) -> (u8, String) {
        let pipe_name = crate::registry::to_pcwstr("\\\\.\\pipe\\rust_ime_pipe");
        unsafe {
            let h_pipe = CreateFileW(
                PCWSTR(pipe_name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            );

            if let Ok(handle) = h_pipe {
                if handle.is_invalid() {
                    return (0, String::new());
                }

                let mut request = [0u8; 6];
                request[0] = 1; // KeyDown
                let code_bytes = key_code.to_le_bytes();
                request[1..5].copy_from_slice(&code_bytes);
                request[5] = modifiers;

                let mut bytes_written = 0;
                let _ = WriteFile(handle, Some(&request), Some(&mut bytes_written), None);

                let mut response = [0u8; 1024];
                let mut bytes_read = 0;
                if ReadFile(handle, Some(&mut response), Some(&mut bytes_read), None).is_ok() && bytes_read > 0 {
                    let action = response[0];
                    let text = if action == 1 && bytes_read > 1 {
                        String::from_utf8_lossy(&response[1..bytes_read as usize]).to_string()
                    } else {
                        String::new()
                    };
                    let _ = CloseHandle(handle);
                    return (action, text);
                }
                let _ = CloseHandle(handle);
            }
        }
        (0, String::new())
    }
}

impl ITfTextInputProcessor_Impl for TextService {
    fn Activate(&self, thread_mgr: Option<&ITfThreadMgr>, client_id: u32) -> Result<()> {
        log(&format!("RustIME: Activate client_id: {}", client_id));
        self.client_id.store(client_id, Ordering::SeqCst);
        
        if let Some(mgr) = thread_mgr {
            unsafe {
                let keystroke_mgr: ITfKeystrokeMgr = mgr.cast()?;
                let sink: ITfKeyEventSink = self.cast()?;
                keystroke_mgr.AdviseKeyEventSink(client_id, &sink, true)?;
            }
        }
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        log("RustIME: Deactivate");
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> { Ok(()) }
    
    fn OnTestKeyDown(&self, _context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        // 允许大多数按键通过管道咨询服务器
        // A-Z (0x41-0x5A), 0-9 (0x30-0x39), Numpad 0-9 (0x60-0x69)
        // Space (0x20), Enter (0x0D), Backspace (0x08), ESC (0x1B), Tab (0x09)
        // Arrows (0x25-0x28), PgUp/Dn (0x21-0x22), Home/End (0x23-0x24)
        // Punctuation and others...
        if (key_code >= 0x30 && key_code <= 0x39) || (key_code >= 0x41 && key_code <= 0x5A) ||
           (key_code >= 0x60 && key_code <= 0x69) ||
           key_code == 0x20 || key_code == 0x0D || key_code == 0x08 || key_code == 0x1B || key_code == 0x09 ||
           (key_code >= 0x21 && key_code <= 0x28) ||
           key_code == 0x10 || key_code == 0x11 || key_code == 0x12 || // Shift, Ctrl, Alt
           key_code == 0xBB || key_code == 0xBD || key_code == 0xBC || key_code == 0xBE || // = - , .
           key_code == 0xBF || key_code == 0xBA || key_code == 0xDE || // / ; '
           key_code == 0xDB || key_code == 0xDD || key_code == 0xDC || // [ ] \
           key_code == 0xC0 // `
        {
            let mut modifiers = 0u8;
            unsafe {
                if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 1; }
                if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 2; }
                if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 4; }
            }
            let (action, _) = self.send_key_to_server(key_code, modifiers);
            if action != 0 {
                return Ok(TRUE);
            }
        }
        Ok(FALSE)
    }

    fn OnKeyDown(&self, context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        if (key_code >= 0x30 && key_code <= 0x39) || (key_code >= 0x41 && key_code <= 0x5A) ||
           (key_code >= 0x60 && key_code <= 0x69) ||
           key_code == 0x20 || key_code == 0x0D || key_code == 0x08 || key_code == 0x1B || key_code == 0x09 ||
           (key_code >= 0x21 && key_code <= 0x28) ||
           key_code == 0x10 || key_code == 0x11 || key_code == 0x12 ||
           key_code == 0xBB || key_code == 0xBD || key_code == 0xBC || key_code == 0xBE ||
           key_code == 0xBF || key_code == 0xBA || key_code == 0xDE ||
           key_code == 0xDB || key_code == 0xDD || key_code == 0xDC ||
           key_code == 0xC0
        {
            let mut modifiers = 0u8;
            unsafe {
                if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 1; }
                if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 2; }
                if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 4; }
            }
            let (action, text) = self.send_key_to_server(key_code, modifiers);
            if action != 0 {
                if action == 1 {
                    if let Some(ctx) = context {
                        let _ = self.commit_text(ctx, &text);
                    }
                }
                return Ok(TRUE);
            }
        }
        Ok(FALSE)
    }

    fn OnTestKeyUp(&self, _context: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> { Ok(FALSE) }
    fn OnKeyUp(&self, _context: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> { Ok(FALSE) }
    fn OnPreservedKey(&self, _context: Option<&ITfContext>, _guid: *const GUID) -> Result<BOOL> { Ok(FALSE) }
}

#[implement(ITfEditSession)]
struct EditSession {
    context: ITfContext,
    text: String,
}

impl EditSession {
    fn new(context: ITfContext, text: String) -> Self {
        Self { context, text }
    }
}

impl ITfEditSession_Impl for EditSession {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        log("RustIME: DoEditSession start");
        unsafe {
            // 重点 1: 手动转换接口，并检查是否为 null
            let source_res: Result<ITfInsertAtSelection> = self.context.cast();
            if let Ok(source) = source_res {
                // 重点 2: 确保 Vec 存活到调用结束
                let mut text_w: Vec<u16> = self.text.encode_utf16().collect();
                text_w.push(0);
                
                log("RustIME: Invoking InsertTextAtSelection");
                // 重点 3: 严格遵循 windows-rs 0.52.0 的 3 参数签名
                let res = source.InsertTextAtSelection(ec, TF_IAS_NOQUERY, &text_w);
                
                match res {
                    Ok(_) => log("RustIME: Insert Success"),
                    Err(e) => log(&format!("RustIME: Insert Error: {:?}", e)),
                }
            } else {
                log("RustIME: Failed to cast ITfInsertAtSelection");
            }
        }
        Ok(())
    }
}
