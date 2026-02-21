use std::sync::mpsc::Receiver;
use slint::{ComponentHandle, SharedString, ModelRc, VecModel};
use crate::ui::GuiEvent;
use crate::Config;

slint::include_modules!();

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};

pub fn start_gui(rx: Receiver<GuiEvent>, config: Config) {
    let window = CandidateWindow::new().expect("Failed to create CandidateWindow");
    let window_handle = window.as_weak();

    // 初始布局设置
    let initial_horizontal = config.appearance.candidate_layout == "horizontal";
    window.set_is_horizontal(initial_horizontal);

    // 1. 初始化窗口特殊的系统属性 (Windows): 隐藏任务栏图标与不抢焦点
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            unsafe {
                let title = "RustImeCandidateWindow\0".encode_utf16().collect::<Vec<u16>>();
                let hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                if hwnd.0 != 0 {
                    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex_style as u32 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0) as isize);
                }
            }
        });
    }

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
                            let mut final_x = x;
                            let mut final_y = y;

                            #[cfg(target_os = "windows")]
                            unsafe {
                                let win_size = w.window().size();
                                let width = win_size.width as i32;
                                let height = win_size.height as i32;

                                let monitor = MonitorFromPoint(windows::Win32::Foundation::POINT { x, y }, MONITOR_DEFAULTTONEAREST);
                                let mut mi = MONITORINFO {
                                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                                    ..Default::default()
                                };
                                if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                                    if final_x + width > mi.rcMonitor.right {
                                        final_x = mi.rcMonitor.right - width - 10;
                                    }
                                    if final_y + height > mi.rcMonitor.bottom {
                                        final_y = mi.rcMonitor.bottom - height - 10;
                                    }
                                    if final_x < mi.rcMonitor.left { final_x = mi.rcMonitor.left + 5; }
                                    if final_y < mi.rcMonitor.top { final_y = mi.rcMonitor.top + 5; }
                                }
                            }
                            w.window().set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(final_x, final_y)));
                        }
                        GuiEvent::ApplyConfig(new_conf) => {
                            w.set_is_horizontal(new_conf.appearance.candidate_layout == "horizontal");
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

    window.run().expect("Failed to run Slint event loop");
}
