use clap::Args;

use crate::actions::new::Request;
use crate::config;
use crate::tui;
use crate::tui::theme::{Theme, ThemeName};

#[derive(Debug, Args)]
pub struct NewArgs {
	/// create a sdk
	#[arg(short = 's', long = "sdk")]
	pub sdk: bool,

	/// create an extension
	#[arg(short = 'e', long = "extension")]
	pub extension: bool,
}

pub fn run(args: NewArgs) {
	match (args.sdk, args.extension) {
		(true, false) => run_sdk(),
		(false, true) => run_extension(),
		_ => print_usage(),
	}
}

fn run_sdk() {
	if let Err(err) = maybe_show_guide() {
		eprintln!("guide error: {err}");
	}

	match tui::sdk_creating::run() {
		Ok(tui::sdk_creating::Outcome::Submitted(state)) => run_creator(&state),
		Ok(tui::sdk_creating::Outcome::Cancelled) => {}
		Err(err) => eprintln!("tui error: {err}"),
	}
}

fn run_creator(state: &tui::sdk_creating::state::NewProjectState) {
	let request = Request::from_state(state);
	match tui::sdk_creating::creator::run(request) {
		Ok(tui::sdk_creating::creator::Outcome::Done) => {}
		// error handling and its ui are not implemented yet
		Ok(tui::sdk_creating::creator::Outcome::Failed(err)) => eprintln!("create error: {err}"),
		Err(err) => eprintln!("tui error: {err}"),
	}
}

// maybe
// ну как тебе сказать... вроде да, а вроде и нет
fn maybe_show_guide() -> std::io::Result<()> {
	let mut cfg = config::load();
	if !cfg.first_opening {
		return Ok(());
	}

	let theme = Theme::get(ThemeName::default());
	tui::guide::run(&theme)?;

	cfg.first_opening = false;
	if let Err(err) = config::save(&cfg) {
		eprintln!("config error: {err}");
	}
	Ok(())
}

fn run_extension() {
	// TODO: extension flag not implemented yet
}

fn print_usage() {
	eprintln!("Please use one of the following flags:");
	eprintln!("    -s/--sdk - create a SDK");
	eprintln!("    -e/--extension - create an extension");
}
