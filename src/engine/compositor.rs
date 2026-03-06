use crate::engine::processor::{Processor, ImeState};

pub struct Compositor;

impl Compositor {
    pub fn get_preedit(p: &Processor) -> String {
        if p.ctx.buffer.is_empty() || !p.chinese_enabled {
            return String::new();
        }

        let mut pinyin = if p.best_segmentation.is_empty() {
            p.ctx.buffer.clone()
        } else {
            p.best_segmentation.join(" ")
        };

        if p.ctx.nav_mode {
            pinyin.push_str(" [H:左 J:下 K:上 L:右]");
        }

        if !p.ctx.aux_filter.is_empty() {
            let mut display_aux = String::new();
            for (i, c) in p.ctx.aux_filter.chars().enumerate() {
                if i == 0 {
                    for uc in c.to_uppercase() { display_aux.push(uc); }
                } else {
                    for lc in c.to_lowercase() { display_aux.push(lc); }
                }
            }
            pinyin.push_str(&display_aux);
        }

        pinyin
    }

    pub fn get_phantom_text(p: &Processor) -> String {
        if p.ctx.state == ImeState::Direct { return String::new(); }
        
        if p.ctx.switch_mode {
            return "[方案切换]".to_string();
        }

        if p.preview_selected_candidate && !p.ctx.candidates.is_empty() {
            p.ctx.candidates[p.ctx.selected.min(p.ctx.candidates.len() - 1)].text.clone()
        } else if !p.ctx.joined_sentence.is_empty() {
            p.ctx.joined_sentence.clone()
        } else if !p.ctx.candidates.is_empty() {
            p.ctx.candidates[0].text.clone()
        } else {
            p.ctx.buffer.clone()
        }
    }
}
