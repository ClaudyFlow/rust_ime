use tiny_skia::*;
use crate::config::Config;
use fontdue::Font;
use std::path::PathBuf;

pub struct CandidatePainter {
    font_zh: Option<Font>,
    font_en: Option<Font>,
}

impl CandidatePainter {
    pub fn new() -> Self {
        let root = crate::find_project_root();
        
        // 1. 中文名流字体：优先 Windows 微软雅黑，其次 Linux Noto CJK，最后用本地自带
        let font_zh = Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc"))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc")))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/google-noto-cjk-fonts/NotoSansCJK-Regular.ttc")))
            .or_else(|| Self::load_font(&root.join("fonts/NotoSansCJKsc-Regular.otf")))
            .or_else(|| Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\msyh.ttf")));

        // 2. 英文名流字体：优先 Windows Segoe UI，其次本地 Inter，最后是 Linux 系统字体
        let font_en = Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\segoeui.ttf"))
            .or_else(|| Self::load_font(&root.join("fonts/Inter-Regular.ttf")))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/TTF/Inter-Regular.ttf")))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/noto/NotoSans-Regular.ttf")));

        Self { font_zh, font_en }
    }

    fn load_font(path: &std::path::Path) -> Option<Font> {
        if let Ok(data) = std::fs::read(path) {
            Font::from_bytes(data, fontdue::FontSettings::default()).ok()
        } else {
            None
        }
    }

    pub fn draw(&self, pinyin: &str, candidates: &[String], hints: &[String], selected: usize, config: &Config) -> (Vec<u8>, u32, u32) {
        let padding = 16.0;
        let corner_radius = config.appearance.corner_radius;
        let font_size_pinyin = config.appearance.pinyin_font_size as f32;
        let font_size_cand = config.appearance.candidate_font_size as f32;
        let line_height_pinyin = font_size_pinyin * 1.4;
        let line_height_cand = font_size_cand * 1.4;
        let spacing_v = 12.0;
        let item_spacing_h = 24.0;

        let mut cand_widths = Vec::new();
        let mut total_width = 300.0;
        let mut total_height = 100.0;
        
        // 预计算布局
        if let (Some(f_zh), Some(f_en)) = (&self.font_zh, &self.font_en) {
            // 拼音用英文字体测量
            let pinyin_w = self.measure_text(f_en, pinyin, font_size_pinyin);
            
            let mut row_width = 0.0;
            for (i, cand) in candidates.iter().enumerate() {
                let prefix = format!("{}.", i + 1);
                let w_prefix = self.measure_text(f_en, &prefix, font_size_cand);
                let w_cand = self.measure_text(f_zh, cand, font_size_cand);
                
                let hint_w = if let Some(h) = hints.get(i) {
                    if !h.is_empty() { self.measure_text(f_en, h, font_size_cand * 0.75) + 8.0 } else { 0.0 }
                } else { 0.0 };
                
                let total_item_w = w_prefix + w_cand + hint_w;
                cand_widths.push(total_item_w);
                row_width += total_item_w + if i < candidates.len() - 1 { item_spacing_h } else { 0.0 };
            }
            total_width = (pinyin_w + padding * 2.0).max(row_width + padding * 2.0).max(300.0).min(1200.0);
            total_height = padding * 2.0 + line_height_pinyin + spacing_v + line_height_cand;
        }

        let mut pixmap = Pixmap::new(total_width as u32, total_height as u32).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        // 绘制阴影 (高级感：更淡、分布更自然的柔和阴影)
        for i in 1..=8 {
            let offset = i as f32 * 1.0;
            let mut sp = Paint::default();
            // 阴影颜色更加深邃且透明度递减
            let alpha = (10 - i) as u8;
            sp.set_color(Color::from_rgba8(0, 0, 0, alpha));
            sp.anti_alias = true;
            let sr = Rect::from_xywh(offset, offset, total_width - offset, total_height - offset).unwrap();
            pixmap.fill_path(&self.create_rounded_rect_path(sr, corner_radius + offset), &sp, FillRule::Winding, Transform::identity(), None);
        }

        // 主背景
        let mut bg_paint = Paint::default();
        bg_paint.set_color(self.parse_color(&config.appearance.candidate_bg_color));
        bg_paint.anti_alias = true;
        let main_rect = Rect::from_xywh(0.0, 0.0, total_width - 10.0, total_height - 10.0).unwrap();
        pixmap.fill_path(&self.create_rounded_rect_path(main_rect, corner_radius), &bg_paint, FillRule::Winding, Transform::identity(), None);

        // 边框 (1px 深黑色，增强视觉边界感)
        let mut border_paint = Paint::default();
        border_paint.set_color(Color::from_rgba8(30, 30, 30, 255)); // 优雅的深黑
        border_paint.anti_alias = true;
        let border_stroke = Stroke {
            width: 1.0,
            ..Default::default()
        };
        pixmap.stroke_path(&self.create_rounded_rect_path(main_rect, corner_radius), &border_paint, &border_stroke, Transform::identity(), None);

        if let (Some(f_zh), Some(f_en)) = (&self.font_zh, &self.font_en) {
            // 1. 绘制拼音 (强制英文字体)
            let py_color = self.parse_color(&config.appearance.pinyin_color);
            let pinyin_y = padding + line_height_pinyin * 0.8;
            self.draw_mixed_text(&mut pixmap, f_zh, f_en, pinyin, padding, pinyin_y, font_size_pinyin, py_color, true);

            // 2. 绘制候选词
            let cand_y_base = padding + line_height_pinyin + spacing_v;
            let mut x_cursor = padding;
            let text_color = self.parse_color(&config.appearance.candidate_text_color);
            let highlight_color = self.parse_color(&config.appearance.candidate_highlight_color);

            for (i, cand) in candidates.iter().enumerate() {
                let is_selected = i == selected;
                if is_selected {
                    let mut hp = Paint::default();
                    let mut hc = highlight_color;
                    hc.set_alpha(0.12);
                    hp.set_color(hc);
                    let hr = Rect::from_xywh(x_cursor - 6.0, cand_y_base, cand_widths[i] + 12.0, line_height_cand).unwrap();
                    pixmap.fill_path(&self.create_rounded_rect_path(hr, 4.0), &hp, FillRule::Winding, Transform::identity(), None);
                }
                
                let prefix = format!("{}.", i + 1);
                let current_color = if is_selected { highlight_color } else { text_color };
                let text_y = cand_y_base + line_height_cand * 0.75;
                
                // 序号 (英文)
                let adv1 = self.draw_text(&mut pixmap, f_en, &prefix, x_cursor, text_y, font_size_cand, current_color);
                // 汉字 (中文)
                let adv2 = self.draw_text(&mut pixmap, f_zh, cand, x_cursor + adv1, text_y, font_size_cand, current_color);
                
                // 提示词 (英文)
                if let Some(hint) = hints.get(i) {
                    if !hint.is_empty() {
                        let mut hc = text_color;
                        hc.set_alpha(0.4);
                        self.draw_text(&mut pixmap, f_en, hint, x_cursor + adv1 + adv2 + 6.0, text_y, font_size_cand * 0.75, hc);
                    }
                }
                x_cursor += cand_widths[i] + item_spacing_h;
            }
        }

        (pixmap.data().to_vec(), total_width as u32, total_height as u32)
    }

    pub fn draw_keystrokes(&self, keys: &[String], config: &Config) -> (Vec<u8>, u32, u32) {
        let padding = config.appearance.keystroke_margin_x as f32; // Reuse margin_x for internal padding
        let font_size = config.appearance.keystroke_font_size as f32;
        let item_spacing = 8.0;
        let corner_radius = 6.0;

        let mut total_width = padding * 2.0;
        let mut widths = Vec::new();

        let f_main = self.font_en.as_ref().or(self.font_zh.as_ref());
        
        if let Some(font) = f_main {
            for key in keys {
                let w = self.measure_text(font, key, font_size);
                widths.push(w);
                total_width += w + padding * 2.0 + item_spacing;
            }
        }
        total_width = total_width.max(10.0);
        let total_height = font_size * 1.5 + padding * 2.0 + 10.0;

        let mut pixmap = Pixmap::new(total_width as u32, total_height as u32).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        let mut x_cursor = padding;
        if let Some(font) = f_main {
            for (i, key) in keys.iter().enumerate() {
                let item_w = widths[i] + padding * 2.0;
                let item_h = font_size * 1.5;
                
                // 背景
                let mut bg_paint = Paint::default();
                bg_paint.set_color(self.parse_color(&config.appearance.keystroke_bg_color));
                bg_paint.anti_alias = true;
                let rect = Rect::from_xywh(x_cursor, padding, item_w, item_h).unwrap();
                pixmap.fill_path(&self.create_rounded_rect_path(rect, corner_radius), &bg_paint, FillRule::Winding, Transform::identity(), None);
                
                // 边框
                let mut border_paint = Paint::default();
                border_paint.set_color(Color::from_rgba8(200, 200, 200, 100));
                let stroke = Stroke { width: 1.0, ..Default::default() };
                pixmap.stroke_path(&self.create_rounded_rect_path(rect, corner_radius), &border_paint, &stroke, Transform::identity(), None);

                // 文字
                self.draw_text(&mut pixmap, font, key, x_cursor + padding, padding + item_h * 0.75, font_size, Color::WHITE);
                
                x_cursor += item_w + item_spacing;
            }
        }

        (pixmap.data().to_vec(), total_width as u32, total_height as u32)
    }

    pub fn draw_learning(&self, word: &str, hint: &str, config: &Config) -> (Vec<u8>, u32, u32) {
        let padding = 16.0;
        let font_size_word = config.appearance.learning_font_size as f32;
        let font_size_hint = font_size_word * 0.6;
        let corner_radius = 8.0;

        let mut total_width = 200.0;
        let mut total_height = 80.0;

        if let (Some(f_zh), Some(f_en)) = (&self.font_zh, &self.font_en) {
            let w_word = self.measure_text(f_zh, word, font_size_word);
            let w_hint = self.measure_text(f_en, hint, font_size_hint);
            total_width = (w_word.max(w_hint) + padding * 2.0).max(150.0);
            total_height = font_size_word + font_size_hint + padding * 2.0 + 10.0 + 10.0;
        }

        let mut pixmap = Pixmap::new(total_width as u32, total_height as u32).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        // 背景
        let mut bg_paint = Paint::default();
        bg_paint.set_color(self.parse_color(&config.appearance.learning_bg_color));
        bg_paint.anti_alias = true;
        let main_rect = Rect::from_xywh(0.0, 0.0, total_width - 10.0, total_height - 10.0).unwrap();
        pixmap.fill_path(&self.create_rounded_rect_path(main_rect, corner_radius), &bg_paint, FillRule::Winding, Transform::identity(), None);

        if let (Some(f_zh), Some(f_en)) = (&self.font_zh, &self.font_en) {
            // 汉字
            self.draw_text(&mut pixmap, f_zh, word, padding, padding + font_size_word * 0.8, font_size_word, Color::WHITE);
            // 提示
            self.draw_text(&mut pixmap, f_en, hint, padding, padding + font_size_word + font_size_hint * 1.0, font_size_hint, Color::from_rgba8(171, 178, 191, 255));
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
            // 检查当前字体是否包含该字符，如果不包含且有备选字体，则尝试备选
            let mut target_font = font;
            if font.lookup_glyph_index(c) == 0 {
                if let Some(ref fallback) = self.font_zh {
                    if fallback.lookup_glyph_index(c) != 0 {
                        target_font = fallback;
                    }
                }
            }

            let (metrics, bitmap) = target_font.rasterize(c, size);
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

    fn draw_mixed_text(&self, pixmap: &mut Pixmap, f_zh: &Font, f_en: &Font, text: &str, x: f32, y: f32, size: f32, color: Color, force_en: bool) {
        let mut cx = x;
        for c in text.chars() {
            let is_latin = c.is_ascii() || force_en;
            let font = if is_latin { f_en } else { f_zh };
            let adv = self.draw_text(pixmap, font, &c.to_string(), cx, y, size, color);
            cx += adv;
        }
    }

    fn create_rounded_rect_path(&self, rect: Rect, radius: f32) -> Path {
        let mut pb = PathBuilder::new();
        let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
        pb.move_to(rect.left() + r, rect.top());
        pb.line_to(rect.right() - r, rect.top());
        pb.quad_to(rect.right(), rect.top(), rect.right(), rect.top() + r);
        pb.line_to(rect.right(), rect.bottom() - r);
        pb.quad_to(rect.right(), rect.bottom(), rect.right() - r, rect.bottom());
        pb.line_to(rect.left() + r, rect.bottom());
        pb.quad_to(rect.left(), rect.bottom(), rect.left(), rect.bottom() - r);
        pb.line_to(rect.left(), rect.top() + r);
        pb.quad_to(rect.left(), rect.top(), rect.left() + r, rect.top());
        pb.close();
        pb.finish().unwrap()
    }

    fn parse_color(&self, color_str: &str) -> Color {
        let s = color_str.trim().to_lowercase();
        
        // 1. 处理 #RRGGBB
        if s.starts_with('#') && s.len() == 7 {
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(255);
            return Color::from_rgba8(r, g, b, 255);
        }
        
        // 2. 处理 rgba(r, g, b, a)
        if s.starts_with("rgba") {
            let content = s.trim_start_matches("rgba(").trim_end_matches(')');
            let parts: Vec<&str> = content.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 4 {
                let r = parts[0].parse::<u8>().unwrap_or(255);
                let g = parts[1].parse::<u8>().unwrap_or(255);
                let b = parts[2].parse::<u8>().unwrap_or(255);
                let a = (parts[3].parse::<f32>().unwrap_or(1.0) * 255.0) as u8;
                return Color::from_rgba8(r, g, b, a);
            }
        }

        // 3. 处理 rgb(r, g, b)
        if s.starts_with("rgb") {
            let content = s.trim_start_matches("rgb(").trim_end_matches(')');
            let parts: Vec<&str> = content.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 3 {
                let r = parts[0].parse::<u8>().unwrap_or(255);
                let g = parts[1].parse::<u8>().unwrap_or(255);
                let b = parts[2].parse::<u8>().unwrap_or(255);
                return Color::from_rgba8(r, g, b, 255);
            }
        }

        Color::WHITE
    }
}
