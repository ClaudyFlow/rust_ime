use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::UI::TextServices::*,
};
use std::cell::RefCell;

#[implement(ITfTextInputProcessor, ITfKeyEventSink)]
pub struct TextService {
    inner: RefCell<TextServiceInner>,
}

struct TextServiceInner {
    client_id: u32,
    thread_mgr: Option<ITfThreadMgr>,
}

impl TextService {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(TextServiceInner {
                client_id: 0,
                thread_mgr: None,
            }),
        }
    }

    // 辅助函数：将文本上屏
    fn commit_text(&self, context: &ITfContext, text: &str) -> Result<()> {
        let client_id = self.inner.borrow().client_id;
        unsafe {
            let session = EditSession::new(context.clone(), text.to_string());
            let session_ptr: ITfEditSession = session.into();
            let _ = context.RequestEditSession(client_id, &session_ptr, TF_ES_READWRITE);
        }
        Ok(())
    }
}

impl ITfTextInputProcessor_Impl for TextService {
    fn Activate(&self, thread_mgr: Option<&ITfThreadMgr>, client_id: u32) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        inner.client_id = client_id;
        inner.thread_mgr = thread_mgr.cloned();
        
        if let Some(mgr) = thread_mgr {
            unsafe {
                let keystroke_mgr: ITfKeystrokeMgr = mgr.cast()?;
                keystroke_mgr.AdviseKeyEventSink(client_id, &self.cast::<ITfKeyEventSink>()?, true)?;
            }
        }
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        if let Some(mgr) = &inner.thread_mgr {
            unsafe {
                if let Ok(keystroke_mgr) = mgr.cast::<ITfKeystrokeMgr>() {
                    let _ = keystroke_mgr.UnadviseKeyEventSink(inner.client_id);
                }
            }
        }
        inner.thread_mgr = None;
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> { Ok(()) }
    
    fn OnTestKeyDown(&self, _context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        // 暂时只拦截 'A' 键 (0x41) 做测试
        if key_code == 0x41 {
            return Ok(TRUE);
        }
        Ok(FALSE)
    }

    fn OnKeyDown(&self, context: Option<&ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let key_code = wparam.0 as u32;
        if key_code == 0x41 { // 如果是 'A' 键
            if let Some(ctx) = context {
                let _ = self.commit_text(ctx, "你好");
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
            let source: ITfInsertAtSelection = self.context.cast()?;
            let text_w = crate::registry::to_pcwstr(&self.text);
            let _ = source.InsertTextAtSelection(ec, TF_IAS_NOQUERY, &text_w)?;
        }
        Ok(())
    }
}