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
    custom_fonts: Mutex<HashMap<String, Option<Font>>>,
    font_map: HashMap<String, String>, // Name -> Path
}

#[allow(dead_code)]
impl CandidatePainter {
    pub fn new() -> Self {
        let root = crate::find_project_root();
        
        // 预先扫描系统字体
        let system_fonts = crate::platform::fonts::list_system_fonts();
        let mut font_map = HashMap::new();
        for f in system_fonts {
            font_map.insert(f.name.to_lowercase(), f.path);
        }

        // 核心改动：优先使用本地高性能字体 NotoSansSC-Bold.ttf
        let local_bold = root.join("fonts/NotoSansSC-Bold.ttf");
        let local_reg = root.join("fonts/NotoSansCJKsc-Regular.otf");

        let font_zh = Self::load_font(&local_bold)
            .or_else(|| Self::load_font(&local_reg))
            .or_else(|| Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc")));

        let font_en = Self::load_font(&local_bold)
            .or_else(|| Self::load_font(&local_reg))
            .or_else(|| Self::load_font(&PathBuf::from("C:\\Windows\\Fonts\\segoeui.ttf")));

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
        if family.is_empty() { return None; }
        let family_lower = family.to_lowercase();
        
        {
            let cache = self.custom_fonts.lock().unwrap();
            if let Some(f) = cache.get(&family_lower) {
                // 我们现在存的是 Option<Font>，避免重复查找找不到的字体
                return f.clone();
            }
        }

        // 如果缓存没命中，执行查找
        let mut path = match family_lower.as_str() {
            "simhei" | "黑体" => Some("C:\\Windows\\Fonts\\simhei.ttf".to_string()),
            "microsoft yahei" | "微软雅黑" => Some("C:\\Windows\\Fonts\\msyh.ttc".to_string()),
            _ => None,
        };

        if path.is_none() {
            path = self.font_map.get(&family_lower).cloned();
        }

        let f = if let Some(p) = path {
            Self::load_font(&PathBuf::from(p))
        } else {
            None
        };

        // 存入缓存（哪怕是 None 也要存，防止重复搜索）
        let mut cache = self.custom_fonts.lock().unwrap();
        cache.insert(family_lower, f.clone());
        if f.is_some() {
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
        let is_vertical = appearance.candidate_layout == "vertical";

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
            
            if is_vertical {
                let mut max_item_w = pinyin_w;
                for (i, cand) in candidates.iter().enumerate() {
                    let prefix = format!("{}.", i + 1);
                    let w_prefix = self.measure_text(f_py, &prefix, font_size_cand);
                    let w_cand = self.measure_text(f_zh, cand, font_size_cand);
                    let hint_w = if let Some(h) = hints.get(i) {
                        if !h.is_empty() { self.measure_text(f_ht, h, font_size_hint) + 12.0 } else { 0.0 }
                    } else { 0.0 };
                    let total_item_w = w_prefix + w_cand + hint_w + 12.0;
                    cand_widths.push(total_item_w);
                    if total_item_w > max_item_w { max_item_w = total_item_w; }
                }
                total_width = (max_item_w + padding_x * 2.0).max(200.0).min(1000.0);
                total_height = padding_y * 2.0 + line_height_pinyin + row_spacing + (candidates.len() as f32 * line_height_cand) + ((candidates.len().saturating_sub(1)) as f32 * item_spacing);
            } else {
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
        }

        let pix_w = (total_width as u32).max(1);
        let pix_h = (total_height as u32).max(1);
        let mut pixmap = Pixmap::new(pix_w, pix_h).expect("Failed to create candidate pixmap");
        pixmap.fill(Color::TRANSPARENT);

        let main_rect = Rect::from_xywh(0.0, 0.0, total_width.max(1.0), total_height.max(1.0)).expect("Failed to create main rect");
        let mut bg_paint = Paint::default();
        bg_paint.set_color(self.parse_color(&appearance.window_bg_color));
        bg_paint.anti_alias = true;
        pixmap.fill_path(&self.create_rounded_rect_path(main_rect, corner_radius), &bg_paint, FillRule::Winding, Transform::identity(), None);

        let mut border_paint = Paint::default();
        border_paint.set_color(self.parse_color(&appearance.window_border_color));
        border_paint.anti_alias = true;
        pixmap.stroke_path(&self.create_rounded_rect_path(main_rect, corner_radius), &border_paint, &Stroke { width: 1.0, ..Default::default() }, Transform::identity(), None);

        if let (Some(f_py), Some(f_zh), Some(f_ht)) = (f_pinyin, f_cand, f_hint) {
            let mut py_color = self.parse_color(&appearance.pinyin_text.color);
            py_color.set_alpha(appearance.pinyin_text.alpha);
            
            let pinyin_y = padding_y + line_height_pinyin * 0.7;
            self.draw_mixed_text(&mut pixmap, f_zh, f_py, pinyin, padding_x, pinyin_y, font_size_pinyin, py_color, false);

            let cand_y_start = padding_y + line_height_pinyin + row_spacing;
            let mut x_cursor = padding_x;
            let mut y_cursor = cand_y_start;
            
            let text_color = self.parse_color(&appearance.candidate_text.color);
            let mut text_color = text_color;
            text_color.set_alpha(appearance.candidate_text.alpha);
            
            let highlight_color = self.parse_color(&appearance.window_highlight_color);
            let mut hint_color = self.parse_color(&appearance.hint_text.color);
            hint_color.set_alpha(appearance.hint_text.alpha);

            for (i, cand) in candidates.iter().enumerate() {
                let is_selected = i == selected;
                let item_w = if is_vertical { total_width - padding_x * 2.0 } else { cand_widths[i] };
                
                if is_selected {
                    let mut hp = Paint::default();
                    let mut hc = highlight_color;
                    hc.set_alpha(0.15);
                    hp.set_color(hc);
                    let hr = Rect::from_xywh(x_cursor - 6.0, y_cursor, item_w + 12.0, line_height_cand).unwrap();
                    pixmap.fill_path(&self.create_rounded_rect_path(hr, 6.0), &hp, FillRule::Winding, Transform::identity(), None);
                }
                
                let prefix = format!("{}.", i + 1);
                let current_color = if is_selected { highlight_color } else { text_color };
                let text_y = y_cursor + line_height_cand * 0.7;
                
                let mut prefix_color = current_color;
                if !is_selected { prefix_color.set_alpha(0.6); }
                
                let adv1 = self.draw_text(&mut pixmap, f_py, &prefix, x_cursor, text_y, font_size_cand, prefix_color);
                let adv2 = self.draw_text(&mut pixmap, f_zh, cand, x_cursor + adv1, text_y, font_size_cand, current_color);
                
                if let Some(hint) = hints.get(i) {
                    if !hint.is_empty() {
                        let mut hc = if is_selected { highlight_color } else { hint_color };
                        if is_selected { hc.set_alpha(0.6); }
                        self.draw_text(&mut pixmap, f_ht, hint, x_cursor + adv1 + adv2 + 8.0, text_y, font_size_hint, hc);
                    }
                }

                if is_vertical {
                    y_cursor += line_height_cand + item_spacing;
                } else {
                    x_cursor += item_w + item_spacing;
                }
            }
        }

        (pixmap.data().to_vec(), total_width as u32, total_height as u32)
    }

    pub fn draw_status(&self, text: &str, config: &Config) -> (Vec<u8>, u32, u32) {
        let padding = 12.0;
        let font_size = config.appearance.pinyin_text.font_size as f32 * 1.5;
        let corner_radius = 8.0;

        let f_custom = self.get_font_by_family(&config.appearance.pinyin_text.font_family);
        let f_main = f_custom.as_ref().or(self.font_zh.as_ref()).or(self.font_en.as_ref());
        
        let mut total_width = 60.0;
        let mut total_height = 60.0;

        if let Some(font) = f_main {
            let w = self.measure_text(font, text, font_size);
            total_width = w + padding * 2.0;
            total_height = font_size + padding * 2.0;
        }

        let mut pixmap = Pixmap::new(total_width as u32, total_height as u32).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        // 背景 (使用 Highlight 颜色但增加透明度)
        let mut bg_color = self.parse_color(&config.appearance.window_highlight_color);
        bg_color.set_alpha(0.8);
        
        let mut bg_paint = Paint::default();
        bg_paint.set_color(bg_color);
        bg_paint.anti_alias = true;
        let rect = Rect::from_xywh(0.0, 0.0, total_width, total_height).unwrap();
        pixmap.fill_path(&self.create_rounded_rect_path(rect, corner_radius), &bg_paint, FillRule::Winding, Transform::identity(), None);

        if let Some(font) = f_main {
            self.draw_text(&mut pixmap, font, text, padding, padding + font_size * 0.8, font_size, Color::WHITE);
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
        
        let r_val = (color.red() * 255.0) as u32;
        let g_val = (color.green() * 255.0) as u32;
        let b_val = (color.blue() * 255.0) as u32;
        let a_base = color.alpha();

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
            
            // 批量获取像素引用，减少 lock() 次数是不可能的，因为 get_glyph 已经处理了
            // 但我们可以优化像素写入逻辑
            let pixels = pixmap.pixels_mut();

            for row in 0..metrics.height {
                let py = (y + row as f32 - metrics.ymin as f32 - metrics.height as f32) as i32;
                if py < 0 || py >= pixmap_height { continue; }
                
                let row_offset = py * pixmap_width;
                let bitmap_row_offset = row * metrics.width;

                for col in 0..metrics.width {
                    let alpha_val = bitmap[bitmap_row_offset + col];
                    if alpha_val > 0 {
                        let px = (cx + col as f32 + metrics.xmin as f32) as i32;
                        if px >= 0 && px < pixmap_width {
                            let idx = (row_offset + px) as usize;
                            let alpha = (a_base * (alpha_val as f32 / 255.0) * 255.0) as u32;
                            
                            let old_pixel = pixels[idx];
                            let inv_alpha = 255 - alpha;

                            let new_r = (r_val * alpha + old_pixel.red() as u32 * inv_alpha) / 255;
                            let new_g = (g_val * alpha + old_pixel.green() as u32 * inv_alpha) / 255;
                            let new_b = (b_val * alpha + old_pixel.blue() as u32 * inv_alpha) / 255;
                            let new_a = (alpha * 255 + old_pixel.alpha() as u32 * inv_alpha) / 255;

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

    fn create_rounded_rect_path(&self, rect: Rect, _radius: f32) -> Path {
        let mut pb = PathBuilder::new();
        pb.move_to(rect.left(), rect.top());
        pb.line_to(rect.right(), rect.top());
        pb.line_to(rect.right(), rect.bottom());
        pb.line_to(rect.left(), rect.bottom());
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
