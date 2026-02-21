use std::sync::mpsc::Receiver;
use slint::{ComponentHandle, SharedString, ModelRc, VecModel};
use crate::ui::GuiEvent;
use crate::Config;

slint::include_modules!();

pub fn start_gui(rx: Receiver<GuiEvent>, _config: Config) {
    let window = CandidateWindow::new().expect("Failed to create CandidateWindow");
    let window_handle = window.as_weak();

    // 2. 启动事件轮询线程，监控来自引擎的消息
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let handle = window_handle.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = handle.upgrade() {
                    match event {
                        GuiEvent::Update { pinyin, candidates, selected, .. } => {
                            if pinyin.is_empty() && candidates.is_empty() {
                                w.set_is_visible(false);
                            } else {
                                w.set_pinyin(SharedString::from(pinyin));
                                let cands: Vec<SharedString> = candidates.into_iter().take(5).map(SharedString::from).collect();
                                w.set_candidates(ModelRc::new(VecModel::from(cands)));
                                w.set_selected_index(selected as i32);
                                w.set_is_visible(true);
                            }
                        }
                        GuiEvent::MoveTo { x, y } => {
                            w.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(x, y)));
                        }
                        GuiEvent::Exit => {
                            let _ = w.window().hide();
                        }
                        _ => {}
                    }
                }
            });
        }
    });

    // 3. 启动 Slint 主循环
    window.run().expect("Failed to run Slint event loop");
}
