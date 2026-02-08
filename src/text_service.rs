use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::UI::TextServices::*,
    Win32::UI::WindowsAndMessaging::GetWindowRect,
    Win32::UI::Input::KeyboardAndMouse::{GetFocus, VK_SHIFT, VK_CONTROL, VK_MENU},
    Win32::System::Diagnostics::Debug::OutputDebugStringW,
    Win32::Storage::FileSystem::*,
    Win32::System::Pipes::WaitNamedPipeW,
    Win32::Foundation::{ERROR_PIPE_BUSY, GetLastError},
};
use std::sync::atomic::{AtomicU32, Ordering};

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
        log("RustIME: TextService::new - BUILD 2045");
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

    fn send_key_to_server(&self, msg_type: u8, key_code: u32, modifiers: u8, _context: Option<&ITfContext>) -> (u8, String) {
        let mut x = 0i32;
        let mut y = 0i32;

        unsafe {
            let hwnd = GetFocus();
            if hwnd.0 != 0 {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    x = rect.left;
                    y = rect.top;
                }
            }
        }

        let pipe_name = crate::registry::to_pcwstr("\\\\.\\pipe\\rust_ime_pipe");
        let pipe_pcwstr = PCWSTR(pipe_name.as_ptr());
        unsafe {
            // 增加重试逻辑，如果管道忙碌则等待
            let mut retry_count = 0;
            let h_pipe = loop {
                let handle = CreateFileW(
                    pipe_pcwstr,
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE, // 允许共享
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                );

                if let Ok(h) = handle {
                    if !h.is_invalid() { break Ok(h); }
                }

                if GetLastError().is_err_and(|e| e.code() == ERROR_PIPE_BUSY.to_hresult()) && retry_count < 3 {
                    let _ = WaitNamedPipeW(pipe_pcwstr, 100);
                    retry_count += 1;
                    continue;
                }
                break handle;
            };

            if let Ok(handle) = h_pipe {
                if handle.is_invalid() { return (0, String::new()); }

                let mut request = [0u8; 14];
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
                    } else if action == 2 { // Consume (拦截且不提交文本)
                        let _ = CloseHandle(handle);
                        return (2, String::new());
                    }
                    // action == 0 (PassThrough)
                    let _ = CloseHandle(handle);
                    return (0, String::new());
                }
                let _ = CloseHandle(handle);
            }
        }
        (0, String::new())
    }
}

impl ITfTextInputProcessor_Impl for TextService {
    fn Activate(&self, thread_mgr: Option<&ITfThreadMgr>, client_id: u32) -> Result<()> {
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

    fn Deactivate(&self) -> Result<()> { Ok(()) }
}

impl ITfKeyEventSink_Impl for TextService {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> { Ok(()) }
    
    fn OnTestKeyDown(&self, context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        let mut modifiers = 0u8;
        unsafe {
            if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 1; }
            if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 2; }
            if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 4; }
        }
        let (action, _) = self.send_key_to_server(2, key_code, modifiers, context);
        if action != 0 { return Ok(TRUE); }
        Ok(FALSE)
    }

    fn OnKeyDown(&self, context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        let mut modifiers = 0u8;
        unsafe {
            if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 1; }
            if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 2; }
            if (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 { modifiers |= 4; }
        }
        let (action, text) = self.send_key_to_server(1, key_code, modifiers, context);
        if action != 0 {
            if action == 1 {
                if let Some(ctx) = context {
                    let _ = self.commit_text(ctx, &text);
                }
            }
            return Ok(TRUE);
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
            let mut selection = [TF_SELECTION { ..Default::default() }];
            let mut fetched = 0;
            if self.context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched).is_ok() && fetched > 0 {
                if let Some(range) = &*selection[0].range {
                    let _ = range.SetText(ec, 0, &text_w);
                    let _ = range.Collapse(ec, TF_ANCHOR_END);
                    let _ = self.context.SetSelection(ec, &[TF_SELECTION {
                        range: std::mem::ManuallyDrop::new(Some(range.clone())),
                        style: selection[0].style,
                    }]);
                }
            } else {
                let source_res: Result<ITfInsertAtSelection> = self.context.cast();
                if let Ok(source) = source_res {
                    let _ = source.InsertTextAtSelection(ec, TF_IAS_NOQUERY, &text_w);
                }
            }
        }
        Ok(())
    }
}
