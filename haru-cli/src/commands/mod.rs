pub mod new;

use clap::{Parser, Subcommand};

use new::NewArgs;

#[derive(Debug, Parser)]
#[command(name = "haru", about = "haru cli", long_about = None)]
pub struct Cli {
	#[command(subcommand)]
	pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
	/// create a new project
	New(NewArgs),
}

