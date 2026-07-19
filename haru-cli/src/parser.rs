use clap::Parser;
use owo_colors::OwoColorize;
use ratatui::style::Color;

use crate::commands::Cli;
use crate::tui::theme::{Theme, ThemeName};

const HARU_VERSION: &str = env!("CARGO_PKG_VERSION");

const HARU_ART: [&str; 6] = [
	"██╗  ██╗ █████╗ ██████╗ ██╗   ██╗",
	"██║  ██║██╔══██╗██╔══██╗██║   ██║",
	"███████║███████║██████╔╝██║   ██║",
	"██╔══██║██╔══██║██╔══██╗██║   ██║",
	"██║  ██║██║  ██║██║  ██║╚██████╔╝",
	"╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝",
];

// flag/arg/rules here
pub fn parse() -> Cli {
	Cli::parse()
}

// diag gradient
pub fn print_version() {
	let theme = Theme::get(ThemeName::default());
	let stops = [to_rgb(theme.accent), to_rgb(theme.accent_alt), to_rgb(theme.border_focused)];

	let widest = HARU_ART.iter().map(|line| line.chars().count()).max().unwrap_or(0);
	let max_diagonal = (HARU_ART.len() - 1 + widest.saturating_sub(1)).max(1) as f64;

	for (row, line) in HARU_ART.iter().enumerate() {
		for (col, ch) in line.chars().enumerate() {
			let t = (row + col) as f64 / max_diagonal;
			let (r, g, b) = gradient_color(&stops, t);
			print!("{}", ch.to_string().truecolor(r, g, b));
		}
		println!();
	}

	let title = format!("Haru console utility v{HARU_VERSION}");
	print_gradient_text(&title, &stops);
	println!();
}

fn print_gradient_text(text: &str, stops: &[(u8, u8, u8)]) {
	let len = text.chars().count().max(1) as f64;
	for (i, ch) in text.chars().enumerate() {
		let t = i as f64 / (len - 1.0).max(1.0);
		let (r, g, b) = gradient_color(stops, t);
		print!("{}", ch.to_string().truecolor(r, g, b));
	}
}

fn gradient_color(stops: &[(u8, u8, u8)], t: f64) -> (u8, u8, u8) {
	let t = t.clamp(0.0, 1.0);
	let segments = stops.len() - 1;
	let scaled = t * segments as f64;
	let index = (scaled.floor() as usize).min(segments - 1);
	let local_t = scaled - index as f64;

	let (r1, g1, b1) = stops[index];
	let (r2, g2, b2) = stops[index + 1];
	(lerp(r1, r2, local_t), lerp(g1, g2, local_t), lerp(b1, b2, local_t))
}

// larp
fn lerp(a: u8, b: u8, t: f64) -> u8 {
	(a as f64 + (b as f64 - a as f64) * t).round() as u8
}

fn to_rgb(color: Color) -> (u8, u8, u8) {
	match color {
		Color::Rgb(r, g, b) => (r, g, b),
		_ => (255, 255, 255),
	}
}
