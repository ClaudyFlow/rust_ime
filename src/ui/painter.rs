use tiny_skia::*;
use crate::config::Config;
use fontdue::Font;

pub struct CandidatePainter {
    font: Option<Font>,
}

impl CandidatePainter {
    pub fn new() -> Self {
        let font_path = "C:\\Windows\\Fonts\\msyh.ttc"; // 微软雅黑
        let font = if let Ok(data) = std::fs::read(font_path) {
            Font::from_bytes(data, fontdue::FontSettings::default()).ok()
        } else {
            None
        };

        Self { font }
    }

    pub fn draw(&self, pinyin: &str, candidates: &[String], hints: &[String], selected: usize, config: &Config) -> (Vec<u8>, u32, u32) {
        let padding = 16.0;
        let corner_radius = 6.0;
        let font_size_pinyin = 18.0;
        let font_size_cand = 20.0;
        let line_height_pinyin = font_size_pinyin * 1.4;
        let line_height_cand = font_size_cand * 1.4;
        let spacing_v = 12.0;
        let item_spacing_h = 24.0;

        let mut cand_widths = Vec::new();
        let mut total_width = 300.0;
        let mut total_height = 100.0;
        
        if let Some(ref font) = self.font {
            let pinyin_w = self.measure_text(font, pinyin, font_size_pinyin);
            let mut row_width = 0.0;
            for (i, cand) in candidates.iter().enumerate() {
                let text = format!("{}.{}", i + 1, cand);
                let w = self.measure_text(font, &text, font_size_cand);
                let hint_w = if let Some(h) = hints.get(i) {
                    if !h.is_empty() { self.measure_text(font, h, font_size_cand * 0.75) + 6.0 } else { 0.0 }
                } else { 0.0 };
                let total_item_w = w + hint_w;
                cand_widths.push(total_item_w);
                row_width += total_item_w + if i < candidates.len() - 1 { item_spacing_h } else { 0.0 };
            }
            total_width = (pinyin_w + padding * 2.0).max(row_width + padding * 2.0).max(300.0).min(1200.0);
            total_height = padding * 2.0 + line_height_pinyin + spacing_v + line_height_cand;
        }

        let mut pixmap = Pixmap::new(total_width as u32, total_height as u32).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        // 多层阴影
        for i in 1..=3 {
            let offset = i as f32 * 2.0;
            let mut shadow_paint = Paint::default();
            shadow_paint.set_color(Color::from_rgba8(0, 0, 0, (15 / i) as u8));
            shadow_paint.anti_alias = true;
            let shadow_rect = Rect::from_xywh(offset, offset, total_width - offset, total_height - offset).unwrap();
            let shadow_path = self.create_rounded_rect_path(shadow_rect, corner_radius + offset);
            pixmap.fill_path(&shadow_path, &shadow_paint, FillRule::Winding, Transform::identity(), None);
        }

        // 背景
        let mut bg_paint = Paint::default();
        bg_paint.set_color(self.parse_color(&config.appearance.candidate_bg_color));
        bg_paint.anti_alias = true;
        let main_rect = Rect::from_xywh(0.0, 0.0, total_width - 10.0, total_height - 10.0).unwrap();
        let main_path = self.create_rounded_rect_path(main_rect, corner_radius);
        pixmap.fill_path(&main_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

        if let Some(ref font) = self.font {
            let pinyin_y = padding + line_height_pinyin * 0.8;
            self.draw_text(&mut pixmap, font, pinyin, padding, pinyin_y, font_size_pinyin, Color::from_rgba8(100, 100, 100, 255));

            let cand_y_base = padding + line_height_pinyin + spacing_v;
            let mut x_cursor = padding;
            for (i, cand) in candidates.iter().enumerate() {
                let is_selected = i == selected;
                if is_selected {
                    let mut hp = Paint::default();
                    hp.set_color(Color::from_rgba8(0, 120, 215, 40));
                    let hr = Rect::from_xywh(x_cursor - 4.0, cand_y_base, cand_widths[i] + 8.0, line_height_cand).unwrap();
                    pixmap.fill_path(&self.create_rounded_rect_path(hr, 4.0), &hp, FillRule::Winding, Transform::identity(), None);
                }
                let text = format!("{}.{}", i + 1, cand);
                let adv = self.draw_text(&mut pixmap, font, &text, x_cursor, cand_y_base + line_height_cand * 0.75, font_size_cand, if is_selected { Color::from_rgba8(0, 102, 204, 255) } else { Color::BLACK });
                if let Some(hint) = hints.get(i) {
                    if !hint.is_empty() {
                        self.draw_text(&mut pixmap, font, hint, x_cursor + adv + 4.0, cand_y_base + line_height_cand * 0.75, font_size_cand * 0.75, Color::from_rgba8(150, 150, 150, 255));
                    }
                }
                x_cursor += cand_widths[i] + item_spacing_h;
            }
        }

        (pixmap.data().to_vec(), total_width as u32, total_height as u32)
    }

    fn measure_text(&self, font: &Font, text: &str, size: f32) -> f32 {
        let mut width = 0.0;
        for c in text.chars() {
            let metrics = font.metrics(c, size);
            width += metrics.advance_width;
        }
        width
    }

    fn draw_text(&self, pixmap: &mut Pixmap, font: &Font, text: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
        let mut cx = x;
        for c in text.chars() {
            let (metrics, bitmap) = font.rasterize(c, size);
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col];
                    if alpha > 0 {
                        let px = cx + col as f32 + metrics.xmin as f32;
                        let py = y + row as f32 - metrics.ymin as f32 - metrics.height as f32;
                        if px >= 0.0 && px < pixmap.width() as f32 && py >= 0.0 && py < pixmap.height() as f32 {
                            let mut p = Paint::default();
                            p.set_color(Color::from_rgba(color.red(), color.green(), color.blue(), color.alpha() * (alpha as f32 / 255.0)).unwrap());
                            pixmap.fill_rect(Rect::from_xywh(px, py, 1.0, 1.0).unwrap(), &p, Transform::identity(), None);
                        }
                    }
                }
            }
            cx += metrics.advance_width;
        }
        cx - x
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