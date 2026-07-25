use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use owo_colors::OwoColorize;
use ratatui::style::Color;

use crate::tui::theme::{Theme, ThemeName};

const BAR_WIDTH: usize = 30;
const AWAITING_TEXT: &str = "Awaiting response...";
const AWAITING_SWEEP_MS: u128 = 1400;
const AWAITING_TICK: Duration = Duration::from_millis(50);
const BAR_TICK: Duration = Duration::from_millis(200);

pub struct Logger {
	level: u8,
	bar: ProgressBar,
	log_color_a: (u8, u8, u8),
	log_color_b: (u8, u8, u8),
	log_count: u32,
}

impl Logger {
	pub fn new(level: u8, total_steps: u32) -> Self {
		let theme = Theme::get(ThemeName::default());
		let logger = Self {
			level,
			bar: ProgressBar::new(total_steps),
			log_color_a: to_rgb(theme.log),
			log_color_b: to_rgb(theme.accent_alt),
			log_count: 0,
		};
		logger.bar.render();
		logger
	}

	pub fn log(&mut self, message: &str) {
		if self.level < 1 {
			return;
		}
		let (r, g, b) = self.next_gradient_color();
		self.bar.log_line(&message.truecolor(r, g, b).to_string());
	}

	// -v 2
	pub fn debug(&mut self, message: &str) {
		if self.level < 2 {
			return;
		}
		let (r, g, b) = self.next_gradient_color();
		self.bar.log_line(&format!("[debug] {message}").truecolor(r, g, b).to_string());
	}

	pub fn next_gradient_color(&mut self) -> (u8, u8, u8) {
		let color = gradient_step(self.log_color_a, self.log_color_b, self.log_count);
		self.log_count += 1;
		color
	}

	pub fn print_always(&self, message: &str) {
		self.bar.log_line(message);
	}

	pub fn print_prompt(&self, message: &str) {
		self.bar.clear_and_print(message);
	}

	// "Awaiting response..."
	pub fn print_prompt_awaiting(&self, message: &str) -> AwaitingGuard {
		self.bar.clear_and_print_awaiting(message)
	}

	// advances the bar by one completed step
	pub fn step(&self) {
		self.bar.step();
	}

	// grows the total step count, used once the number of files to process becomes known
	pub fn extend_total(&self, extra_steps: u32) {
		self.bar.extend_total(extra_steps);
	}

	// shows the [installed/total] maven counter right of the bar, called once discovery has counted all transitive deps
	pub fn set_maven_total(&self, total: u32) {
		self.bar.set_maven_total(total);
	}

	// advances the maven installed counter by one, called after an artifact is downloaded or taken from a valid cache entry
	pub fn maven_installed_step(&self) {
		self.bar.maven_installed_step();
	}

	// hides the maven counter again once resolution is done
	pub fn clear_maven_total(&self) {
		self.bar.clear_maven_total();
	}

	// stops the background ticker and clears the bar line once the run is done
	pub fn finish(&mut self) {
		self.bar.finish();
	}
}

pub struct AwaitingGuard {
	stop: Arc<AtomicBool>,
	handle: Option<thread::JoinHandle<()>>,
	snapshot: RenderSnapshot,
}

impl Drop for AwaitingGuard {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Relaxed);
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
		print!("\x1b[s\x1b[1B");
		self.snapshot.render_with_suffix(None);
		print!("\x1b[u");
		let _ = std::io::Write::flush(&mut std::io::stdout());
	}
}

struct BarCount {
	total: u32,
	current: u32,
	// maven installed/total
	maven_installed: u32,
	maven_total: u32,
}

struct ProgressBar {
	count: Arc<Mutex<BarCount>>,
	start: Instant,
	fill_color_a: (u8, u8, u8),
	fill_color_b: (u8, u8, u8),
	track_color: (u8, u8, u8),
	frame_color: (u8, u8, u8),
	awaiting_color_a: (u8, u8, u8),
	awaiting_color_b: (u8, u8, u8),
	tick_stop: Arc<AtomicBool>,
	tick_handle: Option<thread::JoinHandle<()>>,
}

impl ProgressBar {
	fn new(total: u32) -> Self {
		let theme = Theme::get(ThemeName::default());
		let mut bar = Self {
			count: Arc::new(Mutex::new(BarCount { total, current: 0, maven_installed: 0, maven_total: 0 })),
			start: Instant::now(),
			fill_color_a: to_rgb(theme.accent),
			fill_color_b: to_rgb(theme.border_focused),
			track_color: to_rgb(theme.disabled),
			frame_color: to_rgb(theme.border_focused),
			awaiting_color_a: to_rgb(theme.accent_alt),
			awaiting_color_b: to_rgb(theme.border_focused),
			tick_stop: Arc::new(AtomicBool::new(false)),
			tick_handle: None,
		};
		bar.spawn_ticker();
		bar
	}
	fn spawn_ticker(&mut self) {
		let snapshot = self.snapshot();
		let stop = Arc::clone(&self.tick_stop);
		let handle = thread::spawn(move || {
			while !stop.load(Ordering::Relaxed) {
				thread::sleep(BAR_TICK);
				if stop.load(Ordering::Relaxed) {
					break;
				}
				snapshot.render_with_suffix(None);
			}
		});
		self.tick_handle = Some(handle);
	}

	fn step(&self) {
		{
			let mut count = self.count.lock().unwrap();
			count.current = (count.current + 1).min(count.total);
		}
		self.render();
	}

	fn extend_total(&self, extra_steps: u32) {
		{
			let mut count = self.count.lock().unwrap();
			count.total += extra_steps;
		}
		self.render();
	}

	// sets the maven installed/total counter, total=0 hides it again
	fn set_maven_total(&self, total: u32) {
		{
			let mut count = self.count.lock().unwrap();
			count.maven_total = total;
			count.maven_installed = 0;
		}
		self.render();
	}

	fn maven_installed_step(&self) {
		{
			let mut count = self.count.lock().unwrap();
			count.maven_installed = (count.maven_installed + 1).min(count.maven_total);
		}
		self.render();
	}

	fn clear_maven_total(&self) {
		{
			let mut count = self.count.lock().unwrap();
			count.maven_total = 0;
			count.maven_installed = 0;
		}
		self.render();
	}

	fn log_line(&self, message: &str) {
		clear_line();
		println!("{message}");
		self.render();
	}

	fn clear_and_print(&self, message: &str) {
		clear_line();
		println!("{message}");
		self.render();
		move_cursor_up_to_end_of_prompt(message);
	}

	fn clear_and_print_awaiting(&self, message: &str) -> AwaitingGuard {
		clear_line();
		println!("{message}");
		self.render();
		move_cursor_up_to_end_of_prompt(message);

		let snapshot = self.snapshot();
		let since = Instant::now();
		let stop = Arc::new(AtomicBool::new(false));

		let tick_snapshot = snapshot.clone();
		let tick_stop = Arc::clone(&stop);
		let handle = thread::spawn(move || {
			// save the cursor on the prompt line
			while !tick_stop.load(Ordering::Relaxed) {
				thread::sleep(AWAITING_TICK);
				if tick_stop.load(Ordering::Relaxed) {
					break;
				}
				print!("\x1b[s\x1b[1B");
				tick_snapshot.render_with_suffix(Some(since));
				print!("\x1b[u");
				let _ = std::io::stdout().flush();
			}
		});

		AwaitingGuard { stop, handle: Some(handle), snapshot }
	}

	// stops the background ticker before the caller clears the line, so the ticker can't redraw after
	fn finish(&mut self) {
		self.tick_stop.store(true, Ordering::Relaxed);
		if let Some(handle) = self.tick_handle.take() {
			let _ = handle.join();
		}
		clear_line();
		println!();
	}

	fn render(&self) {
		self.snapshot().render_with_suffix(None);
	}

	fn snapshot(&self) -> RenderSnapshot {
		RenderSnapshot {
			count: Arc::clone(&self.count),
			start: self.start,
			fill_color_a: self.fill_color_a,
			fill_color_b: self.fill_color_b,
			track_color: self.track_color,
			frame_color: self.frame_color,
			awaiting_color_a: self.awaiting_color_a,
			awaiting_color_b: self.awaiting_color_b,
		}
	}
}

// immut render params, current/total read through the shared mutex at render time
#[derive(Clone)]
struct RenderSnapshot {
	count: Arc<Mutex<BarCount>>,
	start: Instant,
	fill_color_a: (u8, u8, u8),
	fill_color_b: (u8, u8, u8),
	track_color: (u8, u8, u8),
	frame_color: (u8, u8, u8),
	awaiting_color_a: (u8, u8, u8),
	awaiting_color_b: (u8, u8, u8),
}

impl RenderSnapshot {
	// draws the bar
	fn render_with_suffix(&self, awaiting_since: Option<Instant>) {
		let (total, current, maven_installed, maven_total) = {
			let count = self.count.lock().unwrap();
			(count.total, count.current, count.maven_installed, count.maven_total)
		};

		clear_line();

		let ratio = if total == 0 { 1.0 } else { current as f64 / total as f64 };
		let filled = (ratio * BAR_WIDTH as f64).round() as usize;
		let filled = filled.min(BAR_WIDTH);
		let empty = BAR_WIDTH - filled;

		let filled_part = self.render_filled(filled, empty);
		let (tr, tg, tb) = self.track_color;
		let empty_part = " ".repeat(empty).truecolor(tr, tg, tb).to_string();

		let percent = (ratio * 100.0).round() as u32;
		let elapsed = format_duration(self.start.elapsed());
		let label = format!("[{percent}%, {current}/{total}, {elapsed}]");

		let (fr, fg, fb) = self.frame_color;
		print!(
			"{}{}{}{} {}",
			"[".truecolor(fr, fg, fb),
			filled_part,
			empty_part,
			"]".truecolor(fr, fg, fb),
			label.truecolor(fr, fg, fb),
		);

		if maven_total > 0 {
			let maven_label = format!("[{maven_installed}/{maven_total}]");
			print!(" {}", maven_label.truecolor(fr, fg, fb));
		}

		if let Some(since) = awaiting_since {
			print!(" {}", self.render_awaiting(since));
		}
		let _ = std::io::stdout().flush();
	}

	fn render_awaiting(&self, since: Instant) -> String {
		let elapsed_ms = since.elapsed().as_millis();
		let mut out = String::new();
		for (i, ch) in AWAITING_TEXT.chars().enumerate() {
			if ch == ' ' {
				out.push(ch);
				continue;
			}
			let t = awaiting_wave(elapsed_ms, i);
			let (r, g, b) = lerp_rgb(self.awaiting_color_a, self.awaiting_color_b, t);
			out.push_str(&ch.to_string().truecolor(r, g, b).to_string());
		}
		out
	}

	fn render_filled(&self, filled: usize, empty: usize) -> String {
		let mut out = String::new();
		for i in 0..filled {
			let is_head = i == filled - 1 && empty > 0;
			let ch = if is_head { '>' } else { '=' };
			let t = if filled <= 1 { 0.0 } else { i as f64 / (filled - 1) as f64 };
			let (r, g, b) = lerp_rgb(self.fill_color_a, self.fill_color_b, t);
			out.push_str(&ch.truecolor(r, g, b).to_string());
		}
		out
	}
}

fn clear_line() {
	print!("\r\x1b[2K");
}

fn move_cursor_up_to_end_of_prompt(message: &str) {
	let visible_width = visible_len(message);
	print!("\x1b[1A\r\x1b[{}C", visible_width);
	let _ = std::io::stdout().flush();
}

fn visible_len(text: &str) -> usize {
	let mut count = 0;
	let mut in_escape = false;
	for ch in text.chars() {
		if in_escape {
			if ch == 'm' {
				in_escape = false;
			}
			continue;
		}
		if ch == '\x1b' {
			in_escape = true;
			continue;
		}
		count += 1;
	}
	count
}

fn to_rgb(color: Color) -> (u8, u8, u8) {
	match color {
		Color::Rgb(r, g, b) => (r, g, b),
		_ => (255, 255, 255),
	}
}

const GRADIENT_STEPS: u32 = 8;

fn gradient_step(from: (u8, u8, u8), to: (u8, u8, u8), index: u32) -> (u8, u8, u8) {
	let period = GRADIENT_STEPS * 2;
	let position = index % period;
	let phase = if position <= GRADIENT_STEPS { position } else { period - position };
	let t = phase as f64 / GRADIENT_STEPS as f64;
	lerp_rgb(from, to, t)
}

const AWAITING_WAVE_SPAN: f64 = 6.0;

fn awaiting_wave(elapsed_ms: u128, i: usize) -> f64 {
	let phase = (elapsed_ms % AWAITING_SWEEP_MS) as f64 / AWAITING_SWEEP_MS as f64;
	let offset = i as f64 / AWAITING_WAVE_SPAN;
	let x = (phase + offset).fract();
	1.0 - (2.0 * x - 1.0).abs()
}

// linearly interpolates between two colors, t in [0.0, 1.0]
fn lerp_rgb(from: (u8, u8, u8), to: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
	let (fr, fg, fb) = from;
	let (tr, tg, tb) = to;
	let lerp = |a: u8, b: u8| -> u8 { (a as f64 + (b as f64 - a as f64) * t).round() as u8 };
	(lerp(fr, tr), lerp(fg, tg), lerp(fb, tb))
}

// 1.2s / 2m 1.5s / 1h 3m
fn format_duration(duration: Duration) -> String {
	let total_seconds = duration.as_secs_f64();

	if total_seconds < 60.0 {
		return format!("{total_seconds:.1}s");
	}

	let whole_seconds = duration.as_secs();
	let hours = whole_seconds / 3600;
	let minutes = (whole_seconds % 3600) / 60;

	if hours == 0 {
		let seconds = total_seconds - (minutes * 60) as f64;
		return format!("{minutes}m {seconds:.1}s");
	}

	format!("{hours}h {minutes}m")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn formats_seconds() {
		assert_eq!(format_duration(Duration::from_millis(1200)), "1.2s");
	}

	#[test]
	fn formats_minutes() {
		assert_eq!(format_duration(Duration::from_millis(121_500)), "2m 1.5s");
	}

	#[test]
	fn formats_hours() {
		assert_eq!(format_duration(Duration::from_secs(3780)), "1h 3m");
	}
}
