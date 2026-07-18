use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};

const JAR_NAME: &str = "kotlin-stdlib.jar";

// result of one search round
pub enum Outcome {
	Found(PathBuf),
	// skip pressed
	Skipped,
	// the search completed everywhere it looks and found nothing
	NotFound,
}

pub fn search(excluded: &[PathBuf], skip_rx: &Receiver<()>) -> Outcome {
	if cfg!(windows) { search_windows(excluded, skip_rx) } else { search_unix(excluded, skip_rx) }
}

fn skip_requested(skip_rx: &Receiver<()>) -> bool {
	match skip_rx.try_recv() {
		Ok(()) => true,
		Err(TryRecvError::Empty) => false,
		Err(TryRecvError::Disconnected) => true,
	}
}

fn is_excluded(path: &Path, excluded: &[PathBuf]) -> bool {
	excluded.iter().any(|p| p == path)
}

#[cfg(not(windows))]
fn search_unix(excluded: &[PathBuf], skip_rx: &Receiver<()>) -> Outcome {
	if let Some(home) = std::env::var_os("HOME") {
		match find_via_command(Path::new(&home), excluded, skip_rx) {
			Outcome::NotFound => {}
			outcome => return outcome,
		}
	}
	find_via_command(Path::new("/"), excluded, skip_rx)
}

#[cfg(windows)]
fn search_unix(_excluded: &[PathBuf], _skip_rx: &Receiver<()>) -> Outcome {
	unreachable!("search_unix is only called on unix-like systems")
}

#[cfg(not(windows))]
fn find_via_command(start: &Path, excluded: &[PathBuf], skip_rx: &Receiver<()>) -> Outcome {
	let Ok(mut child) = Command::new("find").arg(start).arg("-name").arg(JAR_NAME).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
		return Outcome::NotFound;
	};
	let Some(stdout) = child.stdout.take() else {
		return Outcome::NotFound;
	};
	let reader = std::io::BufRead::lines(std::io::BufReader::new(stdout));

	let mut outcome = Outcome::NotFound;
	for line in reader {
		if skip_requested(skip_rx) {
			outcome = Outcome::Skipped;
			break;
		}
		let Ok(line) = line else { break };
		let path = PathBuf::from(line);
		if !is_excluded(&path, excluded) {
			outcome = Outcome::Found(path);
			break;
		}
	}

	let _ = child.kill();
	let _ = child.wait();
	outcome
}

#[cfg(windows)]
fn search_windows(excluded: &[PathBuf], skip_rx: &Receiver<()>) -> Outcome {
	for drive in ["C:\\", "D:\\"] {
		let root = Path::new(drive);
		if !root.exists() {
			continue;
		}
		match walk_drive(root, excluded, skip_rx) {
			Outcome::NotFound => {}
			outcome => return outcome,
		}
	}
	Outcome::NotFound
}

#[cfg(not(windows))]
fn search_windows(_excluded: &[PathBuf], _skip_rx: &Receiver<()>) -> Outcome {
	unreachable!("search_windows is only called on windows")
}

#[cfg(windows)]
fn walk_drive(root: &Path, excluded: &[PathBuf], skip_rx: &Receiver<()>) -> Outcome {
	for entry in walkdir::WalkDir::new(root).into_iter().filter_map(std::result::Result::ok) {
		if skip_requested(skip_rx) {
			return Outcome::Skipped;
		}
		if entry.file_name() != JAR_NAME {
			continue;
		}
		let path = entry.into_path();
		if !is_excluded(&path, excluded) {
			return Outcome::Found(path);
		}
	}
	Outcome::NotFound
}
