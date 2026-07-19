use crate::commands;
use crate::commands::Commands;

pub fn dispatch(command: Commands) {
	match command {
		Commands::New(args) => commands::new::run(args),
		Commands::Config(args) => commands::config::run(args),
		Commands::Build(args) => commands::build::run(args),
	}
}
