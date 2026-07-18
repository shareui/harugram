use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

// centers a box
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
	let width = width.min(area.width);
	let height = height.min(area.height);
	let x = area.x + (area.width.saturating_sub(width)) / 2;
	let y = area.y + (area.height.saturating_sub(height)) / 2;
	Rect { x, y, width, height }
}

pub fn is_too_small(area: Rect, required_width: u16, required_height: u16) -> bool {
	area.width < required_width || area.height < required_height
}

// faint dotted texture
pub fn render_backdrop(frame: &mut Frame, screen: Rect, card: Rect, theme: &Theme) {
	let above = Rect { height: card.y.saturating_sub(screen.y), ..screen };
	let below_y = card.bottom();
	let below = Rect { y: below_y, height: screen.bottom().saturating_sub(below_y), ..screen };
	let left = Rect { y: card.y, height: card.height, width: card.x.saturating_sub(screen.x), ..screen };
	let right_x = card.right();
	let right = Rect { x: right_x, y: card.y, width: screen.right().saturating_sub(right_x), height: card.height };

	for band in [above, below, left, right] {
		render_dots(frame, band, theme);
	}
}

fn render_dots(frame: &mut Frame, area: Rect, theme: &Theme) {
	if area.width == 0 || area.height == 0 {
		return;
	}
	let style = Style::new().fg(theme.border);
	let mut lines = Vec::with_capacity(area.height as usize);
	for y in 0..area.height {
		let mut spans = Vec::new();
		for x in 0..area.width {
			let dot = (x % 4 == 0) && (y % 2 == 0);
			spans.push(Span::styled(if dot { "·" } else { " " }, style));
		}
		lines.push(Line::from(spans));
	}
	let paragraph = Paragraph::new(lines);
	frame.render_widget(paragraph, area);
}

// dialog
pub fn render_too_small(frame: &mut Frame, area: Rect, theme: &Theme, message: &str) {
	frame.render_widget(Block::new().style(theme.base), area);

	// dialog width follows the message, with padding for the border and margins
	let dialog_width = message.chars().count() as u16 + 6;
	let dialog_height = 3;
	let dialog = centered_rect(dialog_width, dialog_height, area);

	render_backdrop(frame, area, dialog, theme);
	let block = Block::new()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(theme.accent))
		.style(theme.base);
	let inner = block.inner(dialog);
	frame.render_widget(block, dialog);

	let line = Line::styled(message, Style::new().fg(theme.accent_alt).add_modifier(Modifier::BOLD)).alignment(Alignment::Center);
	frame.render_widget(Paragraph::new(line), inner);
}
