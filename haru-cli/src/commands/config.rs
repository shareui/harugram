use clap::Args;
use owo_colors::OwoColorize;

use crate::actions::config;

#[derive(Debug, Args)]
pub struct ConfigArgs {
	// set an existing field, value type must match the current one
	#[arg(short = 's', long = "set", num_args = 2, value_names = ["FIELD", "VALUE"])]
	pub set: Option<Vec<String>>,

	// create a new field
	#[arg(short = 'n', long = "new", num_args = 2, value_names = ["FIELD", "VALUE"])]
	pub new: Option<Vec<String>>,

	// delete a field
	#[arg(short = 'd', long = "del", value_name = "FIELD")]
	pub del: Option<String>,
}

pub fn run(args: ConfigArgs) {
	let result = match (args.set, args.new, args.del) {
		(Some(set), None, None) => config::set(&set[0], &set[1]),
		(None, Some(new), None) => config::new(&new[0], &new[1]),
		(None, None, Some(del)) => config::del(&del),
		_ => {
			print_usage();
			return;
		}
	};

	if let Err(err) = result {
		eprintln!("{}", format!("Error: {err}").red());
	}
}

fn print_usage() {
	eprintln!("Please use exactly one of the following flags:");
	eprintln!("    -s/--set {{field}} {{value}} - update an existing field");
	eprintln!("    -n/--new {{field}} {{value}} - create a new field");
	eprintln!("    -d/--del {{field}} - delete a field");
}
