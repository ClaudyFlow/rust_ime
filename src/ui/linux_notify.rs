use notify_rust::{Notification, NotificationHandle};
use crate::ui::CandidateDisplay;
use crate::config::Config;

pub struct LinuxNotifyDisplay {
    active_notification: Option<NotificationHandle>,
    config: Config,
}

impl LinuxNotifyDisplay {
    pub fn new(config: Config) -> Self {
        Self {
            active_notification: None,
            config,
        }
    }
}

impl CandidateDisplay for LinuxNotifyDisplay {
    fn update_candidates(&mut self, pinyin: &str, candidates: Vec<String>, hints: Vec<String>, selected: usize) {
        if !self.config.input.enable_notification_candidates {
            if let Some(h) = self.active_notification.take() {
                h.close();
            }
            return;
        }

        if pinyin.is_empty() {
            if let Some(h) = self.active_notification.take() {
                h.close();
            }
            return;
        }

        let page_size = self.config.appearance.page_size;
        let page = (selected / page_size) * page_size;
    
        let mut notify_body = String::new();
        for i in page..(page + page_size).min(candidates.len()) {
            let cand = &candidates[i];
            let hint = hints.get(i).cloned().unwrap_or_default();
            
            // 提取辅助码/提示逻辑 (保持与原 gui_slint.rs 一致)
            let mut aux = String::new();
            if !hint.is_empty() {
                if hint.contains('/') {
                    let parts: Vec<&str> = hint.split('/').collect();
                    aux = parts[0].trim().to_string();
                } else { aux = hint.clone(); }
            }

            let display_idx = (i % page_size) + 1;
            let entry = if !aux.is_empty() {
                format!("{}.{}({})", display_idx, cand, aux)
            } else {
                format!("{}.{}", display_idx, cand)
            };

            if i == selected {
                notify_body.push_str(&format!("【{}】 ", entry));
            } else {
                notify_body.push_str(&format!("{} ", entry));
            }
        }

        if let Some(ref mut h) = self.active_notification {
            h.summary(pinyin);
            h.body(&notify_body);
            h.update();
        } else {
            self.active_notification = Notification::new()
                .summary(pinyin)
                .body(&notify_body)
                .appname("rust-ime")
                .timeout(0) 
                .show()
                .ok();
        }
    }

    fn update_status(&mut self, text: &str, _chinese_enabled: bool) {
        // Linux 下状态切换也用通知显示，或者仅在需要时显示
        let _ = Notification::new()
            .summary("Rust IME")
            .body(text)
            .appname("rust-ime")
            .timeout(1500)
            .show();
    }

    fn move_to(&mut self, _x: i32, _y: i32) {
        // 通知无法移动位置
    }

    fn set_visible(&mut self, visible: bool) {
        if !visible {
            if let Some(h) = self.active_notification.take() {
                h.close();
            }
        }
    }

    fn apply_config(&mut self, config: &Config) {
        self.config = config.clone();
    }

    fn is_visible(&self) -> bool {
        self.active_notification.is_some()
    }

    fn close(&mut self) {
        if let Some(h) = self.active_notification.take() {
            h.close();
        }
    }
}
