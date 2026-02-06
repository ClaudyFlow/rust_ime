use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::Com::*,
    Win32::UI::TextServices::*,
    Win32::UI::WindowsAndMessaging::*,
};
use std::sync::atomic::{AtomicU32, Ordering};

#[implement(ITfTextInputProcessor, ITfKeyEventSink)]
pub struct TextService {
    ref_count: AtomicU32,
    thread_mgr: Option<ITfThreadMgr>,
    client_id: TfClientId,
}

impl TextService {
    pub fn new() -> Self {
        Self {
            ref_count: AtomicU32::new(1),
            thread_mgr: None,
            client_id: 0,
        }
    }
}

impl ITfTextInputProcessor_Impl for TextService {
    fn Activate(&self, thread_mgr: Option<&ITfThreadMgr>, client_id: TfClientId) -> Result<()> {
        println!("[RustIME] Activate called!");
        // 保存 ThreadMgr 和 ClientId，后续需要用它们来操作文本
        // 由于 self 是不可变的（COM 规则），这里通常需要内部可变性 (RefCell/Mutex) 
        // 或者简单点，我们这里只是演示，先绕过 unsafe 修改
        unsafe {
            let self_ptr = self as *const _ as *mut Self;
            (*self_ptr).thread_mgr = thread_mgr.cloned();
            (*self_ptr).client_id = client_id;
            
            // 注册键盘事件监听
            if let Some(mgr) = thread_mgr {
                let keystroke_mgr: ITfKeystrokeMgr = mgr.cast()?;
                keystroke_mgr.AdviseKeyEventSink(
                    client_id, 
                    &self.cast::<ITfKeyEventSink>()?, 
                    true
                )?;
            }
        }
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        println!("[RustIME] Deactivate called!");
        unsafe {
            let self_ptr = self as *const _ as *mut Self;
            
            // 取消注册键盘事件
            if let Some(mgr) = &(*self_ptr).thread_mgr {
                if let Ok(keystroke_mgr) = mgr.cast::<ITfKeystrokeMgr>() {
                     let _ = keystroke_mgr.UnadviseKeyEventSink((*self_ptr).client_id);
                }
            }
            
            (*self_ptr).thread_mgr = None;
            (*self_ptr).client_id = 0;
        }
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> { Ok(()) }
    fn OnTestKeyDown(&self, _context: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        // 返回 TRUE 表示我们要处理这个按键，返回 FALSE 表示忽略
        // 这里我们简单测试：拦截所有 'A' 键 (0x41)
        if _wparam.0 == 0x41 {
            return Ok(TRUE);
        }
        Ok(FALSE)
    }

    fn OnKeyDown(&self, _context: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        // 真正的按键处理逻辑
        if _wparam.0 == 0x41 {
            // 这里可以调用 Win32 API 弹窗或者写日志证明我们收到了
            unsafe {
                // 为了避免卡死界面，这里只打印调试信息
                // 在 Windows 上需用 DebugView 查看，或者写文件
                // OutputDebugStringA(PCSTR(b"RustIME: 'A' Key Pressed!\0".as_ptr()));
            }
            return Ok(TRUE); // 吃掉这个按键
        }
        Ok(FALSE)
    }

    fn OnTestKeyUp(&self, _context: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> { Ok(FALSE) }
    fn OnKeyUp(&self, _context: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> { Ok(FALSE) }
    fn OnPreservedKey(&self, _context: Option<&ITfContext>, _guid: *const GUID) -> Result<BOOL> { Ok(FALSE) }
}
