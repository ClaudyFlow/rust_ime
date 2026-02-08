use tiny_skia::*;
use crate::config::Config;
use fontdue::Font;
use std::path::PathBuf;

use std::collections::HashMap;
use std::sync::Mutex;

pub struct CandidatePainter {
    font_zh: Option<Font>,
    font_en: Option<Font>,
    glyph_cache: Mutex<HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>>,
    custom_fonts: Mutex<HashMap<String, Font>>,
    font_map: HashMap<String, String>, // Name -> Path
}

impl CandidatePainter {
    pub fn new() -> Self {
        let root = crate::find_project_root();
        
        // 预先扫描系统字体
        let system_fonts = crate::platform::fonts::list_system_fonts();
        let mut font_map = HashMap::new();
        for f in system_fonts {
            font_map.insert(f.name.to_lowercase(), f.path);
        }

        // 1. 中文名流字体：优先 Windows 微软雅黑，其次 Linux Noto CJK，最后用本地自带
        let font_zh = Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc"))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc")))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/google-noto-cjk-fonts/NotoSansCJK-Regular.ttc")))
            .or_else(|| font_map.get("microsoft yahei").and_then(|p| Self::load_font(&PathBuf::from(p))))
            .or_else(|| font_map.get("noto sans cjk sc").and_then(|p| Self::load_font(&PathBuf::from(p))))
            .or_else(|| Self::load_font(&root.join("fonts/NotoSansCJKsc-Regular.otf")))
            .or_else(|| Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\msyh.ttf")));

        // 2. 英文名流字体：优先 Windows Segoe UI，其次本地 Inter，最后是 Linux 系统字体
        let font_en = Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\segoeui.ttf"))
            .or_else(|| font_map.get("segoe ui").and_then(|p| Self::load_font(&PathBuf::from(p))))
            .or_else(|| Self::load_font(&root.join("fonts/Inter-Regular.ttf")))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/TTF/Inter-Regular.ttf")))
            .or_else(|| Self::load_font(&PathBuf::from("/usr/share/fonts/noto/NotoSans-Regular.ttf")));

        Self { 
            font_zh, 
            font_en, 
            glyph_cache: Mutex::new(HashMap::new()),
            custom_fonts: Mutex::new(HashMap::new()),
            font_map,
        }
    }

    fn load_font(path: &std::path::Path) -> Option<Font> {
        if let Ok(data) = std::fs::read(path) {
            Font::from_bytes(data, fontdue::FontSettings::default()).ok()
        } else {
            None
        }
    }

    fn get_font_by_family(&self, family: &str) -> Option<Font> {
        let family_lower = family.to_lowercase();
        let mut cache = self.custom_fonts.lock().unwrap();
        if let Some(f) = cache.get(&family_lower) {
            return Some(f.clone());
        }

        // 1. 检查已知别名映射
        let mut path = match family_lower.as_str() {
            "simhei" | "黑体" => Some("C:\\Windows\\Fonts\\simhei.ttf".to_string()),
            "microsoft yahei" | "微软雅黑" => Some("C:\\Windows\\Fonts\\msyh.ttc".to_string()),
            "simsun" | "宋体" => Some("C:\\Windows\\Fonts\\simsun.ttc".to_string()),
            _ => None,
        };

        // 2. 如果别名没命中，在系统扫描结果中查找
        if path.is_none() {
            path = self.font_map.get(&family_lower).cloned();
        }

        let f = if let Some(p) = path {
            Self::load_font(&PathBuf::from(p))
        } else {
            None
        };

        if let Some(ref font) = f {
            cache.insert(family_lower, font.clone());
            // 如果更换了字体，清空字形缓存
            self.glyph_cache.lock().unwrap().clear();
        }
        f
    }

    pub fn draw(&self, pinyin: &str, candidates: &[String], hints: &[String], selected: usize, config: &Config) -> (Vec<u8>, u32, u32) {
        let appearance = &config.appearance;
        let padding_x = appearance.window_padding_x as f32;
        let padding_y = appearance.window_padding_y as f32;
        let corner_radius = appearance.corner_radius;
        let item_spacing = appearance.item_spacing;
        let row_spacing = appearance.row_spacing;

        let font_size_pinyin = appearance.pinyin_text.font_size as f32;
        let font_size_cand = appearance.candidate_text.font_size as f32;
        let font_size_hint = appearance.hint_text.font_size as f32;
        
        let line_height_pinyin = font_size_pinyin * 1.4;
        let line_height_cand = font_size_cand * 1.5;

        // 动态加载字体
        let f_pinyin_custom = self.get_font_by_family(&appearance.pinyin_text.font_family);
        let f_cand_custom = self.get_font_by_family(&appearance.candidate_text.font_family);
        let f_hint_custom = self.get_font_by_family(&appearance.hint_text.font_family);
        
        let f_pinyin = f_pinyin_custom.as_ref().or(self.font_en.as_ref()).or(self.font_zh.as_ref());
        let f_cand = f_cand_custom.as_ref().or(self.font_zh.as_ref()).or(self.font_en.as_ref());
        let f_hint = f_hint_custom.as_ref().or(self.font_en.as_ref()).or(self.font_zh.as_ref());

        let mut cand_widths = Vec::new();
        let mut total_width = 300.0;
        let mut total_height = 100.0;
        
        // 预计算布局
        if let (Some(f_py), Some(f_zh), Some(f_ht)) = (f_pinyin, f_cand, f_hint) {
            let pinyin_w = self.measure_text(f_py, pinyin, font_size_pinyin);
            
            let mut row_width = 0.0;
            for (i, cand) in candidates.iter().enumerate() {
                let prefix = format!("{}.", i + 1);
                let w_prefix = self.measure_text(f_py, &prefix, font_size_cand);
                let w_cand = self.measure_text(f_zh, cand, font_size_cand);
                let hint_w = if let Some(h) = hints.get(i) {
                    if !h.is_empty() { self.measure_text(f_ht, h, font_size_hint) + 8.0 } else { 0.0 }
                } else { 0.0 };
                
                let total_item_w = w_prefix + w_cand + hint_w + 12.0;
                cand_widths.push(total_item_w);
                row_width += total_item_w + if i < candidates.len() - 1 { item_spacing } else { 0.0 };
            }
            total_width = (pinyin_w + padding_x * 2.0).max(row_width + padding_x * 2.0).max(320.0).min(1200.0);
            total_height = padding_y * 2.0 + line_height_pinyin + row_spacing + line_height_cand;
        }

        let mut pixmap = Pixmap::new(total_width as u32, total_height as u32).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        // 移除所有阴影逻辑，直接绘制主背景
        let main_rect = Rect::from_xywh(0.0, 0.0, total_width, total_height).unwrap();
        let mut bg_paint = Paint::default();
        bg_paint.set_color(self.parse_color(&appearance.window_bg_color));
        bg_paint.anti_alias = true;
        pixmap.fill_path(&self.create_rounded_rect_path(main_rect, corner_radius), &bg_paint, FillRule::Winding, Transform::identity(), None);

        // 边框
        let mut border_paint = Paint::default();
        border_paint.set_color(self.parse_color(&appearance.window_border_color));
        border_paint.anti_alias = true;
        pixmap.stroke_path(&self.create_rounded_rect_path(main_rect, corner_radius), &border_paint, &Stroke { width: 1.0, ..Default::default() }, Transform::identity(), None);

        if let (Some(f_py), Some(f_zh), Some(f_ht)) = (f_pinyin, f_cand, f_hint) {
            // 1. 绘制拼音
            let mut py_color = self.parse_color(&appearance.pinyin_text.color);
            py_color.set_alpha(appearance.pinyin_text.alpha);
            
            let pinyin_y = padding_y + line_height_pinyin * 0.7;
            self.draw_mixed_text(&mut pixmap, f_zh, f_py, pinyin, padding_x, pinyin_y, font_size_pinyin, py_color, false);

            // 2. 绘制候选词
            let cand_y_base = padding_y + line_height_pinyin + row_spacing;
            let mut x_cursor = padding_x;
            
            let mut text_color = self.parse_color(&appearance.candidate_text.color);
            text_color.set_alpha(appearance.candidate_text.alpha);
            
            let mut highlight_color = self.parse_color(&appearance.window_highlight_color);
            
            let mut hint_color = self.parse_color(&appearance.hint_text.color);
            hint_color.set_alpha(appearance.hint_text.alpha);

            for (i, cand) in candidates.iter().enumerate() {
                let is_selected = i == selected;
                let item_w = cand_widths[i];
                
                if is_selected {
                    let mut hp = Paint::default();
                    let mut hc = highlight_color;
                    hc.set_alpha(0.15); // Highlight background alpha
                    hp.set_color(hc);
                    let hr = Rect::from_xywh(x_cursor - 6.0, cand_y_base, item_w, line_height_cand).unwrap();
                    pixmap.fill_path(&self.create_rounded_rect_path(hr, 6.0), &hp, FillRule::Winding, Transform::identity(), None);
                }
                
                let prefix = format!("{}.", i + 1);
                let current_color = if is_selected { highlight_color } else { text_color };
                let text_y = cand_y_base + line_height_cand * 0.7;
                
                let mut prefix_color = current_color;
                if !is_selected { prefix_color.set_alpha(0.6); }
                
                // 绘制序号 (使用拼音字体或候选词字体? 一般候选词字体更统一，但这里保留用 f_py 处理数字)
                let adv1 = self.draw_text(&mut pixmap, f_py, &prefix, x_cursor, text_y, font_size_cand, prefix_color);
                // 绘制候选词
                let adv2 = self.draw_text(&mut pixmap, f_zh, cand, x_cursor + adv1, text_y, font_size_cand, current_color);
                
                if let Some(hint) = hints.get(i) {
                    if !hint.is_empty() {
                        let mut hc = if is_selected { highlight_color } else { hint_color };
                        if is_selected { hc.set_alpha(0.6); }
                        self.draw_text(&mut pixmap, f_ht, hint, x_cursor + adv1 + adv2 + 6.0, text_y, font_size_hint, hc);
                    }
                }
                x_cursor += item_w + item_spacing;
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

    fn get_glyph(&self, font: &Font, c: char, size: f32) -> (fontdue::Metrics, Vec<u8>) {
        let size_key = (size * 10.0) as u32; // 保留一位小数的精度
        let key = (c, size_key);
        
        {
            let cache = self.glyph_cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return cached.clone();
            }
        }
        
        let (metrics, bitmap) = font.rasterize(c, size);
        let mut cache = self.glyph_cache.lock().unwrap();
        cache.insert(key, (metrics, bitmap.clone()));
        (metrics, bitmap)
    }

    fn draw_text(&self, pixmap: &mut Pixmap, font: &Font, text: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
        let mut cx = x;
        let pixmap_width = pixmap.width() as i32;
        let pixmap_height = pixmap.height() as i32;
        let pixels = pixmap.pixels_mut();

        for c in text.chars() {
            let mut target_font = font;
            if font.lookup_glyph_index(c) == 0 {
                if let Some(ref fallback) = self.font_zh {
                    if fallback.lookup_glyph_index(c) != 0 {
                        target_font = fallback;
                    }
                }
            }

            let (metrics, bitmap) = self.get_glyph(target_font, c, size);
            let r = (color.red() * 255.0) as u32;
            let g = (color.green() * 255.0) as u32;
            let b = (color.blue() * 255.0) as u32;
            let a_base = color.alpha();

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha_val = bitmap[row * metrics.width + col];
                    if alpha_val > 0 {
                        let px = (cx + col as f32 + metrics.xmin as f32) as i32;
                        let py = (y + row as f32 - metrics.ymin as f32 - metrics.height as f32) as i32;
                        
                        if px >= 0 && px < pixmap_width && py >= 0 && py < pixmap_height {
                            let idx = (py * pixmap_width + px) as usize;
                            let alpha = (a_base * (alpha_val as f32 / 255.0) * 255.0) as u32;
                            
                            // 预乘 Alpha 混合
                            let old_pixel = pixels[idx];
                            let old_r = old_pixel.red() as u32;
                            let old_g = old_pixel.green() as u32;
                            let old_b = old_pixel.blue() as u32;
                            let old_a = old_pixel.alpha() as u32;

                            let new_r = (r * alpha + old_r * (255 - alpha)) / 255;
                            let new_g = (g * alpha + old_g * (255 - alpha)) / 255;
                            let new_b = (b * alpha + old_b * (255 - alpha)) / 255;
                            let new_a = (alpha * 255 + old_a * (255 - alpha)) / 255;

                            pixels[idx] = PremultipliedColorU8::from_rgba(new_r as u8, new_g as u8, new_b as u8, new_a as u8).unwrap();
                        }
                    }
                }
            }
            cx += metrics.advance_width;
        }
        cx - x
    }

    fn draw_mixed_text(&self, pixmap: &mut Pixmap, f_zh: &Font, f_en: &Font, text: &str, x: f32, y: f32, size: f32, color: Color, _force_en: bool) {
        let mut cx = x;
        let mut current_batch = String::new();
        let mut last_was_latin = true;

        for (i, c) in text.chars().enumerate() {
            let is_latin = c.is_ascii();
            if i == 0 { last_was_latin = is_latin; }

            if is_latin != last_was_latin {
                let font = if last_was_latin { f_en } else { f_zh };
                cx += self.draw_text(pixmap, font, &current_batch, cx, y, size, color);
                current_batch.clear();
                last_was_latin = is_latin;
            }
            current_batch.push(c);
        }

        if !current_batch.is_empty() {
            let font = if last_was_latin { f_en } else { f_zh };
            self.draw_text(pixmap, font, &current_batch, cx, y, size, color);
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
