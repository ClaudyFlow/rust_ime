use crate::engine::Processor;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::ui::GuiEvent;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub struct WaylandHost {
    _processor: Arc<Mutex<Processor>>,
    _gui_tx: Option<Sender<GuiEvent>>,
}

impl WaylandHost {
    pub fn new(processor: Arc<Mutex<Processor>>, gui_tx: Option<Sender<GuiEvent>>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            _processor: processor,
            _gui_tx: gui_tx,
        })
    }
}

impl InputMethodHost for WaylandHost {
    fn set_preedit(&self, _text: &str, _cursor_pos: usize) {}
    fn commit_text(&self, _text: &str) {}
    fn get_cursor_rect(&self) -> Option<Rect> { None }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[WaylandHost] 原生协议模式 (KDE Plasma 6) 基础框架已就绪。");
        println!("[WaylandHost] 正在等待协议连接...");
        // 暂时挂起，防止退出
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}
