use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::crossterm::execute;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::actions::new::{Error, Event, Request, Step};
use crate::tui::fullscreen;
use crate::tui::theme::{Theme, ThemeName};

const CARD_WIDTH: u16 = 62;
const LOG_VISIBLE_ROWS: usize = 8;
// header(2) + gap(1) + progress(1) + gap(1) + logs(LOG_VISIBLE_ROWS + 2 border) + gap(1) + button(3),
// plus the outer block's border(2) and vertical padding(2)
const CARD_HEIGHT: u16 = 2 + 1 + 1 + 1 + (LOG_VISIBLE_ROWS as u16 + 2) + 1 + 3 + 2 + 2;
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

pub enum Outcome {
	// after a successful run
	Done,
	// creation failed
	Failed(Error),
}

enum LogLevel {
	Normal,
	Warn,
	Critical,
}

struct LogLine {
	text: String,
	level: LogLevel,
}

struct PendingConfirm {
	prompt: String,
	typed: Option<bool>,
}

// state of the create/progress screen
struct CreatorState {
	total_steps: usize,
	done_steps: usize,
	logs: Vec<LogLine>,
	finished: Option<Result<(), Error>>,
	pending_confirm: Option<PendingConfirm>,
	skip_available: bool,
	button_hit: Rect,
}

impl CreatorState {
	fn new() -> Self {
		Self {
			total_steps: Step::ORDER.len(),
			done_steps: 0,
			logs: Vec::new(),
			finished: None,
			pending_confirm: None,
			skip_available: false,
			button_hit: Rect::default(),
		}
	}

	// each step is an ==
	fn percent(&self) -> u16 {
		if self.total_steps == 0 {
			return 100;
		}
		((self.done_steps * 100) / self.total_steps) as u16
	}

	fn apply(&mut self, event: Event) {
		match event {
			Event::Log(line) => self.logs.push(LogLine { text: line, level: LogLevel::Normal }),
			Event::Warn(line) => self.logs.push(LogLine { text: line, level: LogLevel::Warn }),
			Event::Confirm(prompt) => self.pending_confirm = Some(PendingConfirm { prompt, typed: None }),
			Event::SkipAvailable(available) => self.skip_available = available,
			Event::StepDone => self.done_steps = (self.done_steps + 1).min(self.total_steps),
			Event::Finished(result) => {
				// Terminated already logged its own "Termination..." line, nothing more to add
				if let Err(err) = &result {
					if !matches!(err, Error::Terminated) {
						self.logs.push(LogLine { text: format!("Critical: {err}"), level: LogLevel::Critical });
					}
				}
				self.pending_confirm = None;
				self.finished = Some(result);
			}
		}
	}

	fn succeeded(&self) -> bool {
		matches!(self.finished, Some(Ok(())))
	}

	fn awaiting_confirm(&self) -> bool {
		self.pending_confirm.is_some()
	}
}

pub fn run(request: Request) -> std::io::Result<Outcome> {
	execute!(std::io::stdout(), EnableMouseCapture)?;
	let mut terminal = ratatui::init();

	let result = run_app(&mut terminal, request);

	ratatui::restore();
	execute!(std::io::stdout(), DisableMouseCapture)?;

	result
}

fn run_app(terminal: &mut DefaultTerminal, request: Request) -> std::io::Result<Outcome> {
	let theme = Theme::get(ThemeName::default());
	let (tx, rx) = mpsc::channel();
	let (confirm_tx, confirm_rx) = mpsc::channel();
	let (skip_tx, skip_rx) = mpsc::channel();
	thread::spawn(move || crate::actions::new::run(request, tx, confirm_rx, skip_rx));

	let mut state = CreatorState::new();

	loop {
		drain_events(&rx, &mut state);

		let mut too_small = false;
		terminal.draw(|frame| {
			let area = frame.area();
			too_small = fullscreen::is_too_small(area, CARD_WIDTH, CARD_HEIGHT);
			if too_small {
				fullscreen::render_too_small(frame, area, &theme, "Please open the terminal in full screen");
			} else {
				render(frame, &theme, &mut state);
			}
		})?;

		if state.awaiting_confirm() {
			poll_confirm(&mut state, &confirm_tx)?;
			continue;
		}

		if state.skip_available {
			poll_skip(&mut state, &skip_tx, too_small)?;
			continue;
		}

		if let Some(outcome) = poll_dismiss(&mut state, too_small)? {
			return Ok(outcome);
		}
	}
}

fn drain_events(rx: &Receiver<Event>, state: &mut CreatorState) {
	loop {
		match rx.try_recv() {
			Ok(event) => state.apply(event),
			Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
		}
	}
}

fn poll_confirm(state: &mut CreatorState, confirm_tx: &Sender<bool>) -> std::io::Result<()> {
	if !event::poll(FRAME_INTERVAL)? {
		return Ok(());
	}

	let CrosstermEvent::Key(key) = event::read()? else { return Ok(()) };
	if !key.is_press() {
		return Ok(());
	}
	let KeyCode::Char(c) = key.code else { return Ok(()) };

	let answer = match c.to_ascii_lowercase() {
		'y' => true,
		'n' => false,
		_ => return Ok(()),
	};

	if let Some(pending) = &mut state.pending_confirm {
		pending.typed = Some(answer);
	}
	let _ = confirm_tx.send(answer);
	Ok(())
}

fn poll_skip(state: &mut CreatorState, skip_tx: &Sender<()>, too_small: bool) -> std::io::Result<()> {
	if !event::poll(FRAME_INTERVAL)? {
		return Ok(());
	}

	let pressed = match event::read()? {
		CrosstermEvent::Key(key) if key.is_press() => matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')),
		CrosstermEvent::Mouse(mouse) if !too_small => is_button_click(mouse, state.button_hit),
		_ => false,
	};

	if pressed {
		let _ = skip_tx.send(());
	}
	Ok(())
}

fn poll_dismiss(state: &mut CreatorState, too_small: bool) -> std::io::Result<Option<Outcome>> {
	if !event::poll(FRAME_INTERVAL)? {
		return Ok(None);
	}

	let pressed = match event::read()? {
		CrosstermEvent::Key(key) if key.is_press() => matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')),
		CrosstermEvent::Mouse(mouse) if !too_small => is_button_click(mouse, state.button_hit),
		_ => false,
	};

	if !pressed || state.finished.is_none() {
		return Ok(None);
	} if state.succeeded() { return Ok(Some(Outcome::Done)); }
	let Some(Err(err)) = state.finished.take() else { unreachable!() }; Ok(Some(Outcome::Failed(err)))
}

fn is_button_click(mouse: MouseEvent, button_hit: Rect) -> bool {
	mouse.kind == MouseEventKind::Down(MouseButton::Left) && button_hit.contains(Position::new(mouse.column, mouse.row))
}

fn render(frame: &mut Frame, theme: &Theme, state: &mut CreatorState) {
	frame.render_widget(Block::new().style(theme.base), frame.area());

	let card = fullscreen::centered_rect(CARD_WIDTH, CARD_HEIGHT, frame.area());

	fullscreen::render_backdrop(frame, frame.area(), card, theme);
	let outer = Block::new()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(theme.accent))
		.style(theme.base)
		.padding(Padding::symmetric(2, 1));
	let inner = outer.inner(card);
	frame.render_widget(outer, card);

	render_header(frame, inner, theme);

	let layout = Layout::vertical([
		Constraint::Length(1), // progress bar
		Constraint::Length(1), // gap
		Constraint::Length(LOG_VISIBLE_ROWS as u16 + 2), // logs box
		Constraint::Length(1), // gap
		Constraint::Length(3), // button
	]);
	let body = Rect { y: inner.y + 3, height: inner.height.saturating_sub(3), ..inner };
	let [progress_area, _gap1, logs_area, _gap2, button_area] = body.layout(&layout);

	render_progress(frame, progress_area, theme, state);
	render_logs(frame, logs_area, theme, state);
	render_button(frame, button_area, theme, state);
}

fn render_header(frame: &mut Frame, area: Rect, theme: &Theme) {
	let title = Line::from(Span::styled("New Project", theme.title)).alignment(Alignment::Center);
	let subtitle = Line::from(Span::styled(
		"generating your project",
		Style::new().fg(theme.muted).add_modifier(Modifier::ITALIC),
	))
	.alignment(Alignment::Center);

	let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]);
	let [title_area, subtitle_area] = area.layout(&layout);

	frame.render_widget(title, title_area);
	frame.render_widget(subtitle, subtitle_area);
}

// look like [########------] 57%
fn render_progress(frame: &mut Frame, area: Rect, theme: &Theme, state: &CreatorState) {
	if state.succeeded() {
		let line = Line::from(Span::styled("Successfully created!", Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)))
			.alignment(Alignment::Center);
		frame.render_widget(line, area);
		return;
	}

	let percent = state.percent();
	let label = format!("{percent}%");
	// bar width leaves room for " NNN%" after it, label included in the same line
	let bar_width = area.width.saturating_sub(label.len() as u16 + 2).max(1) as usize;
	let filled = (bar_width * percent as usize) / 100;

	let mut bar = String::with_capacity(bar_width + 2);
	bar.push('[');
	bar.push_str(&"#".repeat(filled));
	bar.push_str(&"-".repeat(bar_width - filled));
	bar.push(']');

	let line = Line::from(vec![
		Span::styled(bar, Style::new().fg(theme.accent)),
		Span::raw(" "),
		Span::styled(label, Style::new().fg(theme.foreground).add_modifier(Modifier::BOLD)),
	])
	.alignment(Alignment::Center);
	frame.render_widget(line, area);
}

fn render_logs(frame: &mut Frame, area: Rect, theme: &Theme, state: &CreatorState) {
	let block = Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(theme.border));
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let confirm = state.pending_confirm.as_ref().map(|pending| confirm_line(pending, theme));
	let mut budget = wrapped_height(confirm.as_ref(), inner.width);

	let visible = inner.height as usize;
	let mut taken = Vec::new();
	for entry in state.logs.iter().rev() {
		let line = log_line(entry, theme);
		budget += wrapped_height(Some(&line), inner.width);
		if budget > visible {
			break;
		}
		taken.push(line);
	}
	taken.reverse();
	taken.extend(confirm);

	frame.render_widget(Paragraph::new(taken).wrap(Wrap { trim: false }), inner);
}

// number of visual rows a line occupies once wrapped to the given width
fn wrapped_height(line: Option<&Line>, width: u16) -> usize {
	let Some(line) = line else { return 0 };
	if width == 0 {
		return 1;
	}
	line.width().div_ceil(width as usize).max(1)
}

fn log_line<'a>(entry: &'a LogLine, theme: &Theme) -> Line<'a> {
	let style = match entry.level {
		LogLevel::Normal => theme.log_style,
		LogLevel::Warn => theme.warn_style,
		LogLevel::Critical => theme.critical_style,
	};
	Line::from(Span::styled(entry.text.as_str(), style))
}

fn confirm_line<'a>(pending: &'a PendingConfirm, theme: &Theme) -> Line<'a> {
	let answer = match pending.typed {
		Some(true) => "Y",
		Some(false) => "N",
		None => "",
	};
	Line::from(vec![Span::styled(pending.prompt.as_str(), theme.value), Span::styled(answer, Style::new().fg(theme.foreground).add_modifier(Modifier::BOLD))])
}

fn render_button(frame: &mut Frame, area: Rect, theme: &Theme, state: &mut CreatorState) {
	let clickable = state.finished.is_some() || state.skip_available;
	state.button_hit = if clickable { area } else { Rect::default() };

	let label = match &state.finished {
		None if state.skip_available => "Skip",
		None => "Waiting...",
		Some(Ok(())) => "Thank you",
		Some(Err(_)) => "Cancel",
	};

	let border_color = if clickable { theme.border_focused } else { theme.disabled };
	let bg = if clickable { theme.accent } else { theme.disabled };
	let fg = if clickable { theme.background } else { theme.muted };

	let block = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::new().fg(border_color)).style(Style::new().bg(bg));

	let label_style = Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD);
	let inner = block.inner(area);
	frame.render_widget(block, area);
	let line = Line::from(Span::styled(label, label_style)).alignment(Alignment::Center);
	frame.render_widget(line, inner);
}
