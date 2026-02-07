use tiny_skia::*;
use crate::config::Config;

pub struct CandidatePainter {
    pub width: u32,
    pub height: u32,
}

impl CandidatePainter {
    pub fn new() -> Self {
        Self { width: 600, height: 100 }
    }

    pub fn draw(&self, pinyin: &str, candidates: &[String], hints: &[Vec<String>], selected: usize, config: &Config) -> Vec<u8> {
        let mut pixmap = Pixmap::new(self.width, self.height).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        let mut paint = Paint::default();
        
        // 1. 绘制背景 (带圆角和阴影的逻辑可以在这里实现)
        let rect = Rect::from_xywh(5.0, 5.0, (self.width - 10) as f32, (self.height - 10) as f32).unwrap();
        let path = PathBuilder::from_rect(rect);
        paint.color = self.parse_color(&config.appearance.candidate_bg_color);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);

        // 2. 绘制拼音和候选词 (文字绘制目前可以使用 tiny-skia 的基础路径，或者集成 rusttype)
        // 注意：tiny-skia 原生不支持直接写字，通常需要配合 fontdue 或 rusttype
        
        pixmap.data().to_vec()
    }

    fn parse_color(&self, hex: &str) -> Color {
        // 简单的十六进制颜色转换
        if let Ok(c) = Color::from_rgba8(255, 255, 255, 255).to_color_u8().0.get(0..4) {
             // 占位逻辑
        }
        Color::from_rgba8(255, 255, 255, 255)
    }
}
