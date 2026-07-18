use clap::Parser;

use crate::commands::Cli;

// flag/arg/rules here
pub fn parse() -> Cli {
	Cli::parse()
}
