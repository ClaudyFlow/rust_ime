use crate::engine::Processor;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::ui::GuiEvent;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use wayland_client::{
    protocol::wl_registry,
    Connection, EventQueue, QueueHandle,
};

// 使用 wlr_layer_shell 进行 UI 渲染（如果将来需要）
// 目前我们先专注于 input_method 协议的发现

pub struct WaylandHost {
    _processor: Arc<Mutex<Processor>>,
    _gui_tx: Option<Sender<GuiEvent>>,
    event_queue: EventQueue<WaylandState>,
    state: WaylandState,
}

struct WaylandState {
    found_input_method: bool,
}

impl WaylandHost {
    pub fn new(processor: Arc<Mutex<Processor>>, gui_tx: Option<Sender<GuiEvent>>) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();

        let mut state = WaylandState {
            found_input_method: false,
        };

        let display = conn.display();
        display.get_registry(&qh, ());

        // 进行一轮同步，触发 Registry 扫描
        event_queue.roundtrip(&mut state)?;

        Ok(Self {
            _processor: processor,
            _gui_tx: gui_tx,
            event_queue,
            state,
        })
    }
}

impl InputMethodHost for WaylandHost {
    fn set_preedit(&self, _text: &str, _cursor_pos: usize) {}
    fn commit_text(&self, _text: &str) {}
    fn get_cursor_rect(&self) -> Option<Rect> { None }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[WaylandHost] 原生协议模式已启动。");
        if self.state.found_input_method {
            println!("[WaylandHost] 探测到系统支持 input-method 协议。");
        } else {
            println!("[WaylandHost] 警告：当前 Wayland 复合器未导出 input-method 接口。");
            println!("[WaylandHost] 这通常是因为权限限制，请尝试在 KDE 设置中明确启用 Wayland 输入法支持。");
        }
        
        loop {
            // 这里会阻塞，监听系统事件
            self.event_queue.blocking_dispatch(&mut self.state)?;
        }
    }
}

// 基础的 Registry 监听实现，仅用于探测
impl wayland_client::Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, .. } = event {
            if interface.contains("input_method") {
                println!("[Wayland] 发现可用接口: {}", interface);
                state.found_input_method = true;
            }
        }
    }
}
