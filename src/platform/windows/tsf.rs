use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::engine::Processor;
use crate::ui::gui::GuiEvent;
use crate::{Config, NotifyEvent};

#[allow(dead_code)]
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
}

impl InputMethodHost for TsfHost {
    fn set_preedit(&self, _text: &str, _cursor_pos: usize) {
        // 在 Windows TSF 中，这通常通过 ITfEditSession 修改 Context 的 Text 缓冲区实现
    }
    fn commit_text(&self, _text: &str) {
        // 通过 ITfEditSession 提交文本
    }
    fn get_cursor_rect(&self) -> Option<Rect> { None }
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Com::*;
            use windows::Win32::UI::TextServices::*;

            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED)?;
                println!("[TSF] 已初始化 COM。");
                
                /* 
                   下一步计划：
                   1. 实现 ITfTextInputProcessor 接口：这是 TSF 输入法的核心接口。
                   2. 实现 ITfThreadMgrEventSink：用于监听焦点变化。
                   3. 注册 COM 类：Windows 需要在注册表中找到你的 DLL。
                   4. 类别注册：使用 ITfInputProcessorProfiles 注册语言配置文件。
                   5. 交互：通过 ITfContext 获取当前输入框的文本和光标位置。
                   
                   注意：标准的 TSF 输入法通常是一个 DLL (In-process Server)。
                   目前的架构是一个独立的 EXE，可能需要采用 "Bridge" 模式：
                   - 一个轻量的 DLL 注入到目标进程。
                   - 通过 IPC (如 Named Pipes 或 RPC) 与这个 Rust EXE 通信。
                */
                
                println!("[TSF] 实验性模式运行中... 按下 Ctrl+C 退出。");
            }
            
            // 简单的消息循环
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err("TsfHost 仅支持 Windows。".into())
        }
    }
}
