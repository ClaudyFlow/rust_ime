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

    pub fn draw(&self, _pinyin: &str, _candidates: &[String], _hints: &[Vec<String>], _selected: usize, config: &Config) -> Vec<u8> {
        let mut pixmap = Pixmap::new(self.width, self.height).unwrap();
        pixmap.fill(Color::TRANSPARENT);

        let mut paint = Paint::default();
        
        // 1. 绘制背景
        let rect = Rect::from_xywh(5.0, 5.0, (self.width - 10) as f32, (self.height - 10) as f32).unwrap();
        let path = PathBuilder::from_rect(rect);
        paint.set_color(self.parse_color(&config.appearance.candidate_bg_color));
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);

        pixmap.data().to_vec()
    }

    fn parse_color(&self, _hex: &str) -> Color {
        // 暂时返回白色，后续实现完整的十六进制解析
        Color::from_rgba8(255, 255, 255, 255)
    }
}
