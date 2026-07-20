use clap::Args;
use owo_colors::OwoColorize;

use crate::actions::build;

#[derive(Debug, Args)]
pub struct BuildArgs {
	// -v = level 1 (as before), -v 2 = verbose debug log from haru's internals
	#[arg(short = 'v', long = "verbose", value_name = "LEVEL", num_args = 0..=1, default_missing_value = "1")]
	pub verbose: Option<u8>,

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
	let level = args.verbose.unwrap_or(0);
	let options = build::BuildOptions { verbose_level: level, release: args.release, compression: args.compression, password: args.password };

	if let Err(err) = build::run(options) {
		let hint = err.hint();
		eprintln!("{}", format!("Error: {err}").red());
		if let Some(hint) = hint {
			eprintln!("{}", hint.bright_black());
		}
	}
}
