mod actions;
mod commands;
mod config;
mod curator;
mod parser;
mod tui;

fn main() {
	if let Err(err) = config::ensure() {
		eprintln!("config error: {err}");
	}

	let cli = parser::parse();
	curator::dispatch(cli.command);
}
