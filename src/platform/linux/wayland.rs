use crate::engine::Processor;
use crate::platform::traits::{InputMethodHost, Rect};
use crate::ui::GuiEvent;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use wayland_client::{
    protocol::{wl_registry, wl_seat},
    Connection, EventQueue, QueueHandle, Dispatch,
};

pub struct WaylandHost {
    _processor: Arc<Mutex<Processor>>,
    _gui_tx: Option<Sender<GuiEvent>>,
    event_queue: EventQueue<WaylandState>,
    state: WaylandState,
}

struct WaylandState {
    seat: Option<wl_seat::WlSeat>,
    found_im_manager: bool,
    found_text_input: bool,
}

impl WaylandHost {
    pub fn new(processor: Arc<Mutex<Processor>>, gui_tx: Option<Sender<GuiEvent>>) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();

        let mut state = WaylandState {
            seat: None,
            found_im_manager: false,
            found_text_input: false,
        };

        let display = conn.display();
        display.get_registry(&qh, ());

        // 第一轮同步：搜集所有全局接口
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
        println!("[WaylandHost] 原生协议服务已就绪。");
        
        if self.state.found_im_manager {
            println!("[WaylandHost] 状态: 核心输入法接口已激活。");
        } else if self.state.found_text_input {
            println!("[WaylandHost] 状态: 仅探测到文本输入接口，可能需要 KDE 额外授权。");
        } else {
            println!("[WaylandHost] 状态: 未发现任何输入相关接口。");
        }

        loop {
            self.event_queue.blocking_dispatch(&mut self.state)?;
        }
    }
}

// 实现 Registry 监听
impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_seat" => {
                    state.seat = Some(proxy.bind::<wl_seat::WlSeat, _, _>(name, version, qh, ()));
                }
                s if s.contains("input_method_manager") => {
                    state.found_im_manager = true;
                    println!("[Wayland Discovery] 确认核心特权接口: {}", s);
                }
                s if s.contains("text_input_manager") => {
                    state.found_text_input = true;
                    println!("[Wayland Discovery] 发现客户端接口: {}", s);
                }
                _ => {}
            }
        }
    }
}

// 实现 Seat 监听
impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // 处理键盘/鼠标能力的动态增减
    }
}
