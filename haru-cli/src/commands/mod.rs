pub mod build;
pub mod config;
pub mod new;

use clap::{Parser, Subcommand};

use build::BuildArgs;
use config::ConfigArgs;
use new::NewArgs;

#[derive(Debug, Parser)]
#[command(name = "haru", about = "haru cli", long_about = None, disable_version_flag = true)]
pub struct Cli {
	#[command(subcommand)]
	pub command: Option<Commands>,
	
	#[arg(short = 'v', long = "version")]
	pub version: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
	// create a new project
	New(NewArgs),

	// edit the config file
	Config(ConfigArgs),

	// run the build pipeline
	Build(BuildArgs),
}

