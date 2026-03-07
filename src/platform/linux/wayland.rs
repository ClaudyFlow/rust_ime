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
    found_input_method: bool,
}

impl WaylandHost {
    pub fn new(processor: Arc<Mutex<Processor>>, gui_tx: Option<Sender<GuiEvent>>) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();

        let mut state = WaylandState {
            seat: None,
            found_input_method: false,
        };

        let display = conn.display();
        display.get_registry(&qh, ());

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
        println!("[WaylandHost] 原生探测服务已就绪。");
        loop {
            self.event_queue.blocking_dispatch(&mut self.state)?;
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { interface, .. } = event {
            if interface.contains("input_method") {
                println!("[Wayland Discovery] 发现特权接口: {}", interface);
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
