use crate::commands;
use crate::commands::Commands;

pub fn dispatch(command: Option<Commands>) {
	let Some(command) = command else {
		print_usage();
		return;
	};
	match command {
		Commands::New(args) => commands::new::run(args),
		Commands::Config(args) => commands::config::run(args),
		Commands::Build(args) => commands::build::run(args),
	}
}

fn print_usage() {
	eprintln!("Please use one of the following commands:");
	eprintln!("    new - create a new project");
	eprintln!("    config - edit the config file");
	eprintln!("    build - run the build pipeline");
}
