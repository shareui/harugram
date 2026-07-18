use std::time::{Duration, Instant};

use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

const VISIBLE_FOR: Duration = Duration::from_millis(300);
const MESSAGE: &str = "Please, change your keyboard layout to English.";

#[derive(Debug, Default)]
pub struct Attention {
	visible_until: Option<Instant>,
}

impl Attention {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn notify_char(&mut self, c: char) {
		if c.is_ascii() {
			return;
		}
		self.visible_until = Some(Instant::now() + VISIBLE_FOR);
	}

	pub fn is_active(&self) -> bool {
		self.visible_until.is_some_and(|until| Instant::now() < until)
	}

	pub fn render(&self, frame: &mut Frame, theme: &Theme) {
		if !self.is_active() {
			return;
		}

		let area = bottom_bar_area(frame.area());
		let block = Block::new().style(theme.selection);
		let paragraph = Paragraph::new(MESSAGE)
			.block(block)
			.style(theme.selection)
			.alignment(Alignment::Center);

		frame.render_widget(Clear, area);
		frame.render_widget(paragraph, area);
	}
}

fn bottom_bar_area(area: Rect) -> Rect {
	let height = 1;
	Rect::new(area.x, area.bottom().saturating_sub(height), area.width, height)
}
