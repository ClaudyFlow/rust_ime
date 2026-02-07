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

    fn send_key_to_server(&self, msg_type: u8, key_code: u32, modifiers: u8, context: Option<&ITfContext>) -> (u8, String) {
        let mut x = 0i32;
        let mut y = 0i32;

        if let Some(ctx) = context {
            unsafe {
                if let Ok(view) = ctx.GetActiveView() {
                    let mut rect = RECT::default();
                    let mut selection = [TF_SELECTION { ..Default::default() }];
                    let mut fetched = 0;
                    
                    // 获取当前选区 (光标位置)
                    if ctx.GetSelection(0, TF_DEFAULT_SELECTION, &mut selection, &mut fetched).is_ok() && fetched > 0 {
                        if let Some(range) = &*selection[0].range {
                            let mut clipped = BOOL(0);
                            if view.GetTextExt(range, &mut rect, &mut clipped).is_ok() {
                                x = rect.left;
                                y = rect.bottom;
                            }
                        }
                    }
                }
            }
        }

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
                if handle.is_invalid() { return (0, String::new()); }

                let mut request = [0u8; 14]; // 增加长度存放 x, y (各 4 字节)
                request[0] = msg_type;
                let code_bytes = key_code.to_le_bytes();
                request[1..5].copy_from_slice(&code_bytes);
                request[5] = modifiers;
                request[6..10].copy_from_slice(&x.to_le_bytes());
                request[10..14].copy_from_slice(&y.to_le_bytes());

                let mut bytes_written = 0;
                let _ = WriteFile(handle, Some(&request), Some(&mut bytes_written), None);

                let mut response = [0u8; 1024];
                let mut bytes_read = 0;
                if ReadFile(handle, Some(&mut response), Some(&mut bytes_read), None).is_ok() && bytes_read > 0 {
                    let action = response[0];
                    if action == 1 { // Commit
                        let text = String::from_utf8_lossy(&response[1..bytes_read as usize]).to_string();
                        let _ = CloseHandle(handle);
                        return (action, text);
                    }
                    let _ = CloseHandle(handle);
                    return (action, String::new());
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
        if (key_code >= 0x30 && key_code <= 0x39) || (key_code >= 0x41 && key_code <= 0x5A) ||
           (key_code >= 0x60 && key_code <= 0x69) ||
           key_code == 0x20 || key_code == 0x0D || key_code == 0x08 || key_code == 0x1B || key_code == 0x09 ||
           (key_code >= 0x21 && key_code <= 0x28) ||
           key_code == 0x10 || key_code == 0x11 || key_code == 0x12 || key_code == 0x14 ||
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
            let (action, _) = self.send_key_to_server(2, key_code, modifiers, context); // 2 = Test
            if action != 0 { return Ok(TRUE); }
        }
        Ok(FALSE)
    }

    fn OnKeyDown(&self, context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        if (key_code >= 0x30 && key_code <= 0x39) || (key_code >= 0x41 && key_code <= 0x5A) ||
           (key_code >= 0x60 && key_code <= 0x69) ||
           key_code == 0x20 || key_code == 0x0D || key_code == 0x08 || key_code == 0x1B || key_code == 0x09 ||
           (key_code >= 0x21 && key_code <= 0x28) ||
           key_code == 0x10 || key_code == 0x11 || key_code == 0x12 || key_code == 0x14 ||
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
            let (action, text) = self.send_key_to_server(1, key_code, modifiers, context); // 1 = Actual
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
        unsafe {
            let text_w: Vec<u16> = self.text.encode_utf16().collect();
            if text_w.is_empty() { return Ok(()); }

            // 方案：获取当前选区并直接设置文本
            let mut selection = [TF_SELECTION { ..Default::default() }];
            let mut fetched = 0;
            
            // 1. 获取当前插入点
            if self.context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched).is_ok() && fetched > 0 {
                if let Some(range) = &*selection[0].range {
                    // 2. 将文本设置到该位置
                    let _ = range.SetText(ec, 0, &text_w);
                    
                    // 3. 将光标移至文本末尾
                    let _ = range.Collapse(ec, TF_ANCHOR_END);
                    let _ = self.context.SetSelection(ec, &[TF_SELECTION {
                        range: std::mem::ManuallyDrop::new(Some(range.clone())),
                        style: selection[0].style,
                    }]);
                }
            } else {
                // 备选方案：如果 GetSelection 失败，尝试原始插入接口
                let source_res: Result<ITfInsertAtSelection> = self.context.cast();
                if let Ok(source) = source_res {
                    let _ = source.InsertTextAtSelection(ec, TF_IAS_NOQUERY, &text_w);
                }
            }
        }
        Ok(())
    }
}
