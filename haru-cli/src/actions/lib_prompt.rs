use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use owo_colors::OwoColorize;

use crate::progress::Logger;
use crate::utils::findlib;

// what the missing entry is called in prompts and error messages
#[derive(Debug, Clone, Copy)]
pub enum Kind {
	Library,
	Stub,
}

impl Kind {
	fn label(self) -> &'static str {
		match self {
			Self::Library => "Library",
			Self::Stub => "Stub",
		}
	}
}

// outcome of the whole interactive flow for one missing path
pub enum Resolution {
	// the file was found and copied to the originally configured path, build can continue
	Resolved,
	// user declined, cancelled, or nothing was found
	Aborted,
}

const BRIGHT_GREEN: (u8, u8, u8) = (0, 255, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

// asks the user whether to search the system for a missing lib/stub, then drives the whole flow
pub fn resolve_missing(logger: &mut Logger, kind: Kind, path: &str) -> Resolution {
	let ask_color = logger.next_gradient_color();
	let prompt = format!("{} {path} not found, look for it in the system?", kind.label());
	if !ask_yes_no(logger, ask_color, &prompt) {
		return Resolution::Aborted;
	}

	let mut excluded: Vec<PathBuf> = Vec::new();
	loop {
		let cancel_color = logger.next_gradient_color();
		print_themed(logger, cancel_color, "Press Q to cancel the build");

		match search_with_cancel(&file_name(path)) {
			SearchOutcome::Cancelled => return Resolution::Aborted,
			SearchOutcome::NotFound => {
				let err_line = format!("{} not found: {path}", kind.label());
				logger.print_always(&err_line.red().bold().to_string());
				return Resolution::Aborted;
			}
			SearchOutcome::Found(found) => {
				if is_excluded(&found, &excluded) {
					excluded.push(found);
					continue;
				}

				let found_color = logger.next_gradient_color();
				print_found(logger, found_color, &found);

				let choice_color = logger.next_gradient_color();
				match ask_use_cancel_look(logger, choice_color) {
					UseChoice::Use => {
						if copy_into_place(&found, path).is_err() {
							return Resolution::Aborted;
						}
						return Resolution::Resolved;
					}
					UseChoice::Cancel => return Resolution::Aborted,
					UseChoice::LookForAnother => {
						excluded.push(found);
						continue;
					}
				}
			}
		}
	}
}

fn file_name(path: &str) -> String {
	Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

fn is_excluded(path: &Path, excluded: &[PathBuf]) -> bool {
	excluded.iter().any(|p| p == path)
}

fn copy_into_place(found: &Path, dest: &str) -> std::io::Result<()> {
	if let Some(parent) = Path::new(dest).parent() {
		if !parent.as_os_str().is_empty() {
			fs::create_dir_all(parent)?;
		}
	}
	fs::copy(found, dest)?;
	Ok(())
}

enum SearchOutcome {
	Found(PathBuf),
	NotFound,
	Cancelled,
}

// runs the filesystem search on a background thread while the main thread watches for a Q keypress
fn search_with_cancel(file_name: &str) -> SearchOutcome {
	let cancel_flag = Arc::new(AtomicBool::new(false));

	let search_flag = Arc::clone(&cancel_flag);
	let owned_name = file_name.to_string();
	let handle = thread::spawn(move || findlib::find(&owned_name, &search_flag));

	watch_for_cancel_key(&handle, &cancel_flag);

	match handle.join() {
		Ok(findlib::Outcome::Found(path)) => SearchOutcome::Found(path),
		Ok(findlib::Outcome::NotFound) => SearchOutcome::NotFound,
		Ok(findlib::Outcome::Cancelled) => SearchOutcome::Cancelled,
		Err(_) => SearchOutcome::NotFound,
	}
}

// polls for a Q keypress in raw mode until the search thread finishes
fn watch_for_cancel_key(handle: &thread::JoinHandle<findlib::Outcome>, cancel_flag: &Arc<AtomicBool>) {
	use ratatui::crossterm::event::{self, Event, KeyCode};
	use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};

	let raw_mode_enabled = enable_raw_mode().is_ok();

	while !handle.is_finished() {
		let key_pressed = raw_mode_enabled
			&& matches!(event::poll(std::time::Duration::from_millis(50)), Ok(true))
			&& matches!(
				event::read(),
				Ok(Event::Key(key)) if matches!(
					key.code,
					KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Char('й') | KeyCode::Char('Й')
				)
			);

		if key_pressed {
			cancel_flag.store(true, Ordering::Relaxed);
			break;
		}
		if !raw_mode_enabled {
			thread::sleep(std::time::Duration::from_millis(50));
		}
	}

	if raw_mode_enabled {
		let _ = disable_raw_mode();
	}
}

fn print_themed(logger: &Logger, color: (u8, u8, u8), message: &str) {
	let (r, g, b) = color;
	logger.print_always(&message.truecolor(r, g, b).to_string());
}

fn print_found(logger: &Logger, color: (u8, u8, u8), path: &Path) {
	let (r, g, b) = color;
	let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
	let (gr, gg, gb) = BRIGHT_GREEN;
	let line = format!("{} {}", "Found:".truecolor(r, g, b), absolute.display().truecolor(gr, gg, gb));
	logger.print_always(&line);
}

// reads one line from stdin, trims and lowercases it
fn read_line_lower() -> String {
	let mut line = String::new();
	let stdin = std::io::stdin();
	let _ = stdin.lock().read_line(&mut line);
	line.trim().to_lowercase()
}

// [Y/n] prompt, accepts y/n and the matching cyrillic keys on the same physical layout position
fn ask_yes_no(logger: &Logger, prompt_color: (u8, u8, u8), question: &str) -> bool {
	loop {
		let (r, g, b) = prompt_color;
		let (wr, wg, wb) = WHITE;
		let line = format!("{} {}", question.truecolor(r, g, b), "[Y/n]: ".truecolor(wr, wg, wb));
		let _guard = logger.print_prompt_awaiting(&line);

		let answer = read_line_lower();
		match answer.as_str() {
			"" | "y" | "н" => return true,
			"n" | "т" => return false,
			_ => {
				let line = "Invalid option".red().to_string();
				logger.print_always(&line);
			}
		}
	}
}

enum UseChoice {
	Use,
	Cancel,
	LookForAnother,
}

// [U/c/l] prompt
fn ask_use_cancel_look(logger: &Logger, prompt_color: (u8, u8, u8)) -> UseChoice {
	loop {
		let (r, g, b) = prompt_color;
		let (wr, wg, wb) = WHITE;
		let question = "Use and add to project, cancel, look for another?";
		let line = format!("{} {}", question.truecolor(r, g, b), "[U/c/l]: ".truecolor(wr, wg, wb));
		let _guard = logger.print_prompt_awaiting(&line);

		let answer = read_line_lower();
		match answer.as_str() {
			"" | "u" | "г" => return UseChoice::Use,
			"c" | "с" => return UseChoice::Cancel,
			"l" | "д" => return UseChoice::LookForAnother,
			_ => {
				let line = "Invalid option".red().to_string();
				logger.print_always(&line);
			}
		}
	}
}
