use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::crossterm::execute;
use ratatui::layout::Position;
use ratatui::DefaultTerminal;

use crate::tui::attention::Attention;
use crate::tui::fullscreen;
use crate::tui::sdk_creating::render;
use crate::tui::sdk_creating::state::{Focus, Language, NewProjectState};
use crate::tui::theme::{Theme, ThemeName};

// outcome of running
pub enum Outcome {
	Cancelled,
	Submitted(NewProjectState),
}

pub fn run() -> std::io::Result<Outcome> {
	execute!(std::io::stdout(), EnableMouseCapture)?;
	let mut terminal = ratatui::init();

	let result = run_app(&mut terminal);

	ratatui::restore();
	execute!(std::io::stdout(), DisableMouseCapture)?;

	result
}

fn run_app(terminal: &mut DefaultTerminal) -> std::io::Result<Outcome> {
	let theme = Theme::get(ThemeName::default());
	let mut state = NewProjectState::default();
	let mut attention = Attention::new();

	loop {
		let mut too_small = false;
		terminal.draw(|frame| {
			let area = frame.area();
			too_small = fullscreen::is_too_small(area, render::CARD_WIDTH, render::card_height());
			if too_small {
				fullscreen::render_too_small(frame, area, &theme, "Please open the terminal in full screen");
			} else {
				render::render(frame, &mut state, &theme);
				attention.render(frame, &theme);
			}
		})?;

		match event::read()? {
			Event::Key(key) => {
				if !key.is_press() {
					continue;
				}
				if let KeyCode::Char(c) = key.code {
					attention.notify_char(c);
				}
				handle_key(&mut state, key);
			}
			// hit areas are stale from the last full render, so ignore clicks while too small
			Event::Mouse(mouse) if !too_small => handle_mouse(&mut state, mouse),
			_ => {}
		}

		if state.should_quit {
			return Ok(Outcome::Cancelled);
		}
		if state.submitted {
			return Ok(Outcome::Submitted(state));
		}
	}
}

fn handle_key(state: &mut NewProjectState, key: KeyEvent) {
	match key.code {
		KeyCode::Esc => state.should_quit = true,
		KeyCode::Tab => state.focus_next(),
		KeyCode::BackTab => state.focus_prev(),
		KeyCode::Enter => state.activate_focus(),
		KeyCode::Char(' ') if !state.focus.is_text_field() => state.activate_focus(),
		KeyCode::Left if state.focus == Focus::Language => state.set_language(Language::Kotlin),
		KeyCode::Right if state.focus == Focus::Language => state.set_language(Language::Java),
		_ => handle_text_input(state, key),
	}
}

fn handle_text_input(state: &mut NewProjectState, key: KeyEvent) {
	if !state.focus.is_text_field() {
		return;
	}
	let Some(field) = state.active_text_field() else { return };
	match key.code {
		KeyCode::Char(c) => field.insert_char(c),
		KeyCode::Backspace => field.backspace(),
		KeyCode::Delete => field.delete_forward(),
		KeyCode::Left => field.move_left(),
		KeyCode::Right => field.move_right(),
		KeyCode::Home => field.move_home(),
		KeyCode::End => field.move_end(),
		_ => {}
	}
}

fn handle_mouse(state: &mut NewProjectState, mouse: MouseEvent) {
	if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
		return;
	}
	let position = Position::new(mouse.column, mouse.row);
	let hit = state.hit_areas;

	if hit.sdk_id.contains(position) {
		state.set_focus(Focus::SdkId);
	} else if hit.author.contains(position) {
		state.set_focus(Focus::Author);
	} else if hit.app_version.contains(position) {
		state.set_focus(Focus::AppVersion);
	} else if hit.source.contains(position) {
		state.set_focus(Focus::Source);
	} else if hit.open_source.contains(position) {
		state.set_focus(Focus::OpenSource);
		state.toggle_focused_checkbox();
	} else if hit.kotlin_stdlib.contains(position) {
		state.set_focus(Focus::KotlinStdlib);
		state.toggle_focused_checkbox();
	} else if hit.use_current_compiler.contains(position) {
		state.set_focus(Focus::UseCurrentCompiler);
		state.toggle_focused_checkbox();
	} else if hit.language_kotlin.contains(position) {
		state.set_focus(Focus::Language);
		state.set_language(Language::Kotlin);
	} else if hit.language_java.contains(position) {
		state.set_focus(Focus::Language);
		state.set_language(Language::Java);
	} else if hit.create.contains(position) {
		state.set_focus(Focus::Create);
		state.submitted = true;
	}
}
