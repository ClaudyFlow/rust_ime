use tiny_skia::*;
use crate::config::Config;
use fontdue::Font;

pub struct CandidatePainter {
    pub width: u32,
    pub height: u32,
    font: Option<Font>,
}

impl CandidatePainter {
    pub fn new() -> Self {
        // 在 Windows 上尝试加载微软雅黑
        let font_path = "C:\\Windows\\Fonts\\msyh.ttc"; // 微软雅黑
        let font = if let Ok(data) = std::fs::read(font_path) {
            Font::from_bytes(data, fontdue::FontSettings::default()).ok()
        } else {
            None
        };

        Self { 
            width: 600, 
            height: 120,
            font,
        }
    }

    pub fn draw(&self, pinyin: &str, candidates: &[String], hints: &[String], selected: usize, config: &Config) -> Vec<u8> {
        // 1. 动态计算宽度：根据拼音长度和候选词大致预估
        let min_width = 400;
        let pinyin_width = (pinyin.chars().count() * 15) as u32 + 60;
        let mut candidates_width = 0;
        for (i, c) in candidates.iter().enumerate() {
            candidates_width += (c.chars().count() * 25) as u32 + 40;
            if let Some(h) = hints.get(i) {
                candidates_width += (h.chars().count() * 12) as u32;
            }
        }
        let dynamic_width = pinyin_width.max(candidates_width).max(min_width).min(1200); // 限制最大宽度

        let mut pixmap = Pixmap::new(dynamic_width, self.height).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        let margin = 10.0;
        let radius = 12.0;

        // 2. 绘制阴影
        let mut shadow_paint = Paint::default();
        shadow_paint.set_color(Color::from_rgba8(0, 0, 0, 40));
        shadow_paint.anti_alias = true;
        let shadow_rect = Rect::from_xywh(2.0, 2.0, dynamic_width as f32 - 4.0, self.height as f32 - 4.0).unwrap();
        let shadow_path = self.create_rounded_rect_path(shadow_rect, radius + 2.0);
        pixmap.fill_path(&shadow_path, &shadow_paint, FillRule::Winding, Transform::identity(), None);

        // 3. 绘制主背景
        let mut bg_paint = Paint::default();
        bg_paint.set_color(self.parse_color(&config.appearance.candidate_bg_color));
        bg_paint.anti_alias = true;
        let rect = Rect::from_xywh(margin, margin, dynamic_width as f32 - margin * 2.0, self.height as f32 - margin * 2.0).unwrap();
        let path = self.create_rounded_rect_path(rect, radius);
        pixmap.fill_path(&path, &bg_paint, FillRule::Winding, Transform::identity(), None);

        // 4. 绘制文字 (高清晰度渲染)
        if let Some(ref font) = self.font {
            // A. 绘制拼音
            self.draw_text(&mut pixmap, font, pinyin, 25.0, 20.0, 24.0, Color::from_rgba8(0, 113, 227, 255));

            // B. 绘制候选词与提示
            let mut x_offset = 25.0;
            for (i, cand) in candidates.iter().enumerate() {
                let text = format!("{}.{}", i + 1, cand);
                let color = if i == selected {
                    Color::from_rgba8(0, 113, 227, 255)
                } else {
                    Color::BLACK
                };
                
                let adv = self.draw_text(&mut pixmap, font, &text, x_offset, 65.0, 26.0, color);
                x_offset += adv;
                
                if let Some(hint) = hints.get(i) {
                    if !hint.is_empty() {
                        let hint_adv = self.draw_text(&mut pixmap, font, hint, x_offset + 4.0, 65.0, 16.0, Color::from_rgba8(150, 150, 150, 255));
                        x_offset += hint_adv + 4.0;
                    }
                }
                x_offset += 35.0;
            }
        }

        pixmap.data().to_vec()
    }

    fn draw_text(&self, pixmap: &mut Pixmap, font: &Font, text: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
        let mut current_x = x;
        let mut total_advance = 0.0;
        
        for c in text.chars() {
            let (metrics, bitmap) = font.rasterize(c, size);
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col];
                    if alpha > 0 {
                        let px = current_x + col as f32 + metrics.xmin as f32;
                        let py = y + row as f32 - metrics.ymin as f32 - metrics.height as f32; // 调整 Y 轴对齐
                        
                        if px >= 0.0 && px < pixmap.width() as f32 && py >= 0.0 && py < pixmap.height() as f32 {
                            let mut paint = Paint::default();
                            let mut c_with_alpha = color;
                            c_with_alpha.set_alpha(color.alpha() * (alpha as f32 / 255.0));
                            paint.set_color(c_with_alpha);
                            pixmap.fill_rect(Rect::from_xywh(px, py, 1.0, 1.0).unwrap(), &paint, Transform::identity(), None);
                        }
                    }
                }
            }
            current_x += metrics.advance_width;
            total_advance += metrics.advance_width;
        }
        total_advance
    }

    fn create_rounded_rect_path(&self, rect: Rect, radius: f32) -> Path {
        let mut pb = PathBuilder::new();
        pb.move_to(rect.left() + radius, rect.top());
        pb.line_to(rect.right() - radius, rect.top());
        pb.quad_to(rect.right(), rect.top(), rect.right(), rect.top() + radius);
        pb.line_to(rect.right(), rect.bottom() - radius);
        pb.quad_to(rect.right(), rect.bottom(), rect.right() - radius, rect.bottom());
        pb.line_to(rect.left() + radius, rect.bottom());
        pb.quad_to(rect.left(), rect.bottom(), rect.left(), rect.bottom() - radius);
        pb.line_to(rect.left(), rect.top() + radius);
        pb.quad_to(rect.left(), rect.top(), rect.left() + radius, rect.top());
        pb.close();
        pb.finish().unwrap()
    }

    fn parse_color(&self, hex: &str) -> Color {
        if hex.starts_with('#') && hex.len() == 7 {
            let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(255);
            return Color::from_rgba8(r, g, b, 255);
        }
        Color::WHITE
    }
}
