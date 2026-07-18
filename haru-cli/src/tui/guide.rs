use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::crossterm::execute;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use crate::tui::fullscreen;
use crate::tui::theme::Theme;

const CARD_WIDTH: u16 = 58;
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const LINE_REVEAL_INTERVAL: Duration = Duration::from_millis(90);
const PULSE_PERIOD: Duration = Duration::from_millis(1400);

struct Tip {
	keys: &'static str,
	text: &'static str,
}

const TIPS: [Tip; 5] = [
	Tip { keys: "Tab", text: "move to the next field" },
	Tip { keys: "Shift+Tab", text: "move to the previous field" },
	Tip { keys: "< >", text: "switch between choices, like language" },
	Tip { keys: "Mouse", text: "click any field, checkbox, or button directly" },
	Tip { keys: "Esc", text: "exit" },
];

fn card_height() -> u16 {
	// header(2) + gap(1) + tips + gap(1) + button(3), plus block border(2) and padding(2)
	let content_height = 2 + 1 + TIPS.len() as u16 + 1 + 3;
	content_height + 2 + 2
}

pub fn run(theme: &Theme) -> std::io::Result<()> {
	execute!(std::io::stdout(), EnableMouseCapture)?;
	let mut terminal = ratatui::init();

	let result = run_app(&mut terminal, theme);

	ratatui::restore();
	execute!(std::io::stdout(), DisableMouseCapture)?;

	result
}

fn run_app(terminal: &mut DefaultTerminal, theme: &Theme) -> std::io::Result<()> {
	let started_at = Instant::now();
	let mut button_hit: Rect = Rect::default();
	let mut dismissed = false;

	while !dismissed {
		let mut too_small = false;
		terminal.draw(|frame| {
			let area = frame.area();
			too_small = fullscreen::is_too_small(area, CARD_WIDTH, card_height());
			if too_small {
				fullscreen::render_too_small(frame, area, theme, "Please open the terminal in full screen");
			} else {
				render(frame, theme, started_at, &mut button_hit);
			}
		})?;

		if event::poll(FRAME_INTERVAL)? {
			match event::read()? {
				Event::Key(key) => {
					if dismiss_on_key(key) {
						dismissed = true;
					}
				}
				Event::Mouse(mouse) if !too_small => {
					if dismiss_on_mouse(mouse, button_hit) {
						dismissed = true;
					}
				}
				_ => {}
			}
		}
	}

	Ok(())
}

fn dismiss_on_key(key: KeyEvent) -> bool {
	if !key.is_press() {
		return false;
	}
	matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' '))
}

fn dismiss_on_mouse(mouse: MouseEvent, button_hit: Rect) -> bool {
	if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
		return false;
	}
	button_hit.contains(Position::new(mouse.column, mouse.row))
}

fn render(frame: &mut Frame, theme: &Theme, started_at: Instant, button_hit: &mut Rect) {
	frame.render_widget(Block::new().style(theme.base), frame.area());

	let card = fullscreen::centered_rect(CARD_WIDTH, card_height(), frame.area());

	fullscreen::render_backdrop(frame, frame.area(), card, theme);
	let outer = Block::new()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(theme.accent))
		.style(theme.base)
		.padding(Padding::symmetric(2, 1));
	let inner = outer.inner(card);
	frame.render_widget(outer, card);

	let elapsed = started_at.elapsed();
	render_header(frame, inner, theme);

	let tips_area = Rect { y: inner.y + 3, height: TIPS.len() as u16, ..inner };
	render_tips(frame, tips_area, theme, elapsed);

	let button_area = Rect { y: inner.y + 4 + TIPS.len() as u16, height: 3, ..inner };
	render_button(frame, button_area, theme, elapsed, button_hit);
}

fn render_header(frame: &mut Frame, area: Rect, theme: &Theme) {
	let title = Line::from(Span::styled("Welcome", theme.title)).alignment(Alignment::Center);
	let subtitle = Line::from(Span::styled(
		"a quick look at the controls",
		Style::new().fg(theme.muted).add_modifier(Modifier::ITALIC),
	))
	.alignment(Alignment::Center);

	let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]);
	let [title_area, subtitle_area] = area.layout(&layout);

	frame.render_widget(title, title_area);
	frame.render_widget(subtitle, subtitle_area);
}

fn render_tips(frame: &mut Frame, area: Rect, theme: &Theme, elapsed: Duration) {
	let layout = Layout::vertical(vec![Constraint::Length(1); TIPS.len()]);
	let rows = area.layout_vec(&layout);

	for (i, (tip, row)) in TIPS.iter().zip(rows.iter()).enumerate() {
		let reveal_at = LINE_REVEAL_INTERVAL * i as u32;
		if elapsed < reveal_at {
			continue;
		}

		let keycap_style = Style::new().fg(theme.background).bg(theme.accent_alt).add_modifier(Modifier::BOLD);

		let line = Line::from(vec![
			Span::raw(" "),
			Span::styled(format!(" {} ", tip.keys), keycap_style),
			Span::raw("  "),
			Span::styled(tip.text, theme.value),
		]);
		frame.render_widget(line, *row);
	}
}

fn render_button(frame: &mut Frame, area: Rect, theme: &Theme, elapsed: Duration, hit_area: &mut Rect) {
	let phase = (elapsed.as_millis() % PULSE_PERIOD.as_millis()) as f64 / PULSE_PERIOD.as_millis() as f64;
	let pulse = (phase * std::f64::consts::TAU).sin() * 0.5 + 0.5;
	let border_color = lerp_color(theme.accent, theme.border_focused, pulse);

	*hit_area = area;

	let block = Block::new()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(border_color))
		.style(Style::new().bg(theme.accent));

	let label_style = Style::new().fg(theme.background).bg(theme.accent).add_modifier(Modifier::BOLD);
	let inner = block.inner(area);
	frame.render_widget(block, area);
	let line = Line::from(Span::styled("Fine", label_style)).alignment(Alignment::Center);
	frame.render_widget(Paragraph::new(line), inner);
}

fn lerp_color(a: ratatui::style::Color, b: ratatui::style::Color, t: f64) -> ratatui::style::Color {
	let (ar, ag, ab) = color_rgb(a);
	let (br, bg, bb) = color_rgb(b);
	let lerp = |x: u8, y: u8| -> u8 { (x as f64 + (y as f64 - x as f64) * t).round() as u8 };
	ratatui::style::Color::Rgb(lerp(ar, br), lerp(ag, bg), lerp(ab, bb))
}

fn color_rgb(color: ratatui::style::Color) -> (u8, u8, u8) {
	match color {
		ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
		_ => (255, 255, 255),
	}
}
