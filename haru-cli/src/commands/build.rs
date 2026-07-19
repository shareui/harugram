use clap::Args;
use owo_colors::OwoColorize;

use crate::actions::build;

#[derive(Debug, Args)]
pub struct BuildArgs {
	// logs
	#[arg(short = 'v', long = "verbose")]
	pub verbose: bool,

	// uses --release when merging the dexes with d8
	#[arg(short = 'r', long = "release")]
	pub release: bool,

	// deflate compression level, 0 (store) to 9 (max), sdk-only
	#[arg(short = 'c', long = "compression", value_name = "LEVEL")]
	pub compression: Option<u8>,

	// encrypts the archive, sdk-only
	#[arg(short = 'p', long = "password", num_args = 2, value_names = ["ALGORITHM", "PASSWORD"])]
	pub password: Option<Vec<String>>,
}

pub fn run(args: BuildArgs) {
	let options = build::BuildOptions { verbose: args.verbose, release: args.release, compression: args.compression, password: args.password };

	if let Err(err) = build::run(options) {
		let hint = err.hint();
		eprintln!("{}", format!("Error: {err}").red());
		if let Some(hint) = hint {
			eprintln!("{}", hint.bright_black());
		}
	}
}
