use std::sync::mpsc::{Receiver, Sender};
use crate::ui::{GuiEvent, CandidateDisplay};
use crate::Config;
use crate::ui::tray::TrayEvent;
use crate::ui::linux_notify::LinuxNotifyDisplay;
use crate::ui::slint_window::SlintDisplay;

pub fn start_gui(rx: Receiver<GuiEvent>, config: Config, _tray_tx: Sender<TrayEvent>) {
    // 决定使用哪种显示方式
    // 在 Linux 上，如果配置了通知模式，则优先使用。否则使用 Slint 窗口。
    let mut display: Box<dyn CandidateDisplay> = if cfg!(target_os = "linux") {
        if config.input.enable_notification_candidates {
            Box::new(LinuxNotifyDisplay::new(config.clone()))
        } else {
            Box::new(SlintDisplay::new(config.clone()))
        }
    } else {
        Box::new(SlintDisplay::new(config.clone()))
    };

    while let Ok(event) = rx.recv() {
        match event {
            GuiEvent::Update { pinyin, candidates, hints, selected, .. } => {
                display.update_candidates(&pinyin, candidates, hints, selected);
            }
            GuiEvent::SyncState(state) => {
                // 将状态更新分发到显示器
                display.update_status(&state.status_text, state.chinese_enabled);
                display.update_candidates(&state.pinyin, state.candidates, state.hints, state.selected_index);
            }
            GuiEvent::ShowStatus(text, chinese_enabled) => {
                display.update_status(&text, chinese_enabled);
            }
            GuiEvent::MoveTo { x, y } => {
                display.move_to(x, y);
            }
            GuiEvent::SetVisible(visible) => {
                display.set_visible(visible);
            }
            GuiEvent::ApplyConfig(new_config) => {
                display.apply_config(&new_config);
            }
            GuiEvent::UpdateStatusBarVisible(visible) => {
                // 某些显示器可能不支持独立控制状态栏，这里简单处理
                display.update_status("", visible); 
            }
            GuiEvent::ForceStatusVisible(visible) => {
                display.update_status("", visible);
            }
            GuiEvent::Exit => {
                display.close();
                break;
            }
            _ => {}
        }
    }
}
