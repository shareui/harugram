mod actions;
mod commands;
mod config;
mod curator;
mod parser;
mod tui;

use tui::theme::ThemeName;

fn main() {
	init();

	let cli = parser::parse();
	curator::dispatch(cli.command);
}

fn init() {
	if let Err(err) = config::ensure() {
		eprintln!("config error: {err}");
	}

	let theme_name = match ThemeName::default() {
		ThemeName::AshViolet => "ash violet",
	};
	if let Err(err) = config::save_theme(theme_name) {
		eprintln!("config error: {err}");
	}
}
