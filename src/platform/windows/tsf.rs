use crate::platform::traits::{InputMethodHost, Rect};

#[allow(dead_code)]
pub struct TsfHost {
}

impl InputMethodHost for TsfHost {
    fn set_preedit(&self, _text: &str, _cursor_pos: usize) {}
    fn commit_text(&self, _text: &str) {}
    fn get_cursor_rect(&self) -> Option<Rect> { None }
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
