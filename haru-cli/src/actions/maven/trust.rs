use std::io::BufRead;

use owo_colors::OwoColorize;

use crate::actions::maven::coordinate::ResolvedCoordinate;
use crate::actions::maven::manifest;
use crate::progress::Logger;

const WHITE: (u8, u8, u8) = (255, 255, 255);

pub fn ask_and_remember(logger: &mut Logger, dependency: &ResolvedCoordinate, transit: &ResolvedCoordinate) -> bool {
	let color = logger.next_gradient_color();
	let question = format!("Does library {dependency} need library {transit} add it to trust and install?");
	if !ask_yes_no(logger, color, &question) {
		return false;
	}

	if let Err(err) = manifest::add_trusted(transit) {
		logger.log(&format!("Warn: failed to save {transit} to trusted list: {err}"));
	}
	true
}

fn read_line_lower() -> String {
	let mut line = String::new();
	let stdin = std::io::stdin();
	let _ = stdin.lock().read_line(&mut line);
	line.trim().to_lowercase()
}

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
