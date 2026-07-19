use clap::Args;
use owo_colors::OwoColorize;

use crate::actions::build;

#[derive(Debug, Args)]
pub struct BuildArgs {
	// logs
	#[arg(short = 'v', long = "verbose")]
	pub verbose: bool,
}

pub fn run(args: BuildArgs) {
	if let Err(err) = build::run(args.verbose) {
		eprintln!("{}", format!("Error: {err}").red());
	}
}
