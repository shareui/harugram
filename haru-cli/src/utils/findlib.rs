use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

pub enum Outcome {
	Found(PathBuf),
	Cancelled,
	NotFound,
}

pub fn find(file_name: &str, cancel_flag: &AtomicBool) -> Outcome {
	if cfg!(windows) { find_windows(file_name, cancel_flag) } else { find_unix(file_name, cancel_flag) }
}

fn cancelled(cancel_flag: &AtomicBool) -> bool {
	cancel_flag.load(Ordering::Relaxed)
}

#[cfg(not(windows))]
fn find_unix(file_name: &str, cancel_flag: &AtomicBool) -> Outcome {
	if let Some(home) = std::env::var_os("HOME") {
		match find_via_command(Path::new(&home), file_name, cancel_flag) {
			Outcome::NotFound => {}
			outcome => return outcome,
		}
	}
	find_via_command(Path::new("/"), file_name, cancel_flag)
}

#[cfg(windows)]
fn find_unix(_file_name: &str, _cancel_flag: &AtomicBool) -> Outcome {
	unreachable!("find_unix is only called on unix-like systems")
}

#[cfg(not(windows))]
fn find_via_command(start: &Path, file_name: &str, cancel_flag: &AtomicBool) -> Outcome {
	let Ok(mut child) = Command::new("find").arg(start).arg("-name").arg(file_name).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
		return Outcome::NotFound;
	};
	let Some(stdout) = child.stdout.take() else {
		return Outcome::NotFound;
	};
	let reader = std::io::BufRead::lines(std::io::BufReader::new(stdout));

	let mut outcome = Outcome::NotFound;
	for line in reader {
		if cancelled(cancel_flag) {
			outcome = Outcome::Cancelled;
			break;
		}
		let Ok(line) = line else { break };
		let path = PathBuf::from(line);
		outcome = Outcome::Found(path);
		break;
	}

	let _ = child.kill();
	let _ = child.wait();
	outcome
}

#[cfg(windows)]
fn find_windows(file_name: &str, cancel_flag: &AtomicBool) -> Outcome {
	for drive in ["C:\\", "D:\\"] {
		let root = Path::new(drive);
		if !root.exists() {
			continue;
		}
		match walk_drive(root, file_name, cancel_flag) {
			Outcome::NotFound => {}
			outcome => return outcome,
		}
	}
	Outcome::NotFound
}

#[cfg(not(windows))]
fn find_windows(_file_name: &str, _cancel_flag: &AtomicBool) -> Outcome {
	unreachable!("find_windows is only called on windows")
}

#[cfg(windows)]
fn walk_drive(root: &Path, file_name: &str, cancel_flag: &AtomicBool) -> Outcome {
	for entry in walkdir::WalkDir::new(root).into_iter().filter_map(std::result::Result::ok) {
		if cancelled(cancel_flag) {
			return Outcome::Cancelled;
		}
		if entry.file_name() == file_name {
			return Outcome::Found(entry.into_path());
		}
	}
	Outcome::NotFound
}
