use ratatui::layout::Rect;

// language choice for the generated project
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
	Kotlin,
	Java,
}

impl Language {
	pub fn label(self) -> &'static str {
		match self {
			Self::Kotlin => "Kotlin",
			Self::Java => "Java",
		}
	}
}

// which field currently owns keyboard input, in tab order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
	SdkId,
	Author,
	AppVersion,
	Source,
	OpenSource,
	KotlinStdlib,
	UseCurrentCompiler,
	Language,
	Create,
}

impl Focus {
	// full tab order
	pub const ORDER_FULL: [Self; 9] = [
		Self::SdkId,
		Self::Author,
		Self::AppVersion,
		Self::Source,
		Self::OpenSource,
		Self::KotlinStdlib,
		Self::UseCurrentCompiler,
		Self::Language,
		Self::Create,
	];

	// a text input field the user can type into
	pub fn is_text_field(self) -> bool {
		matches!(self, Self::SdkId | Self::Author | Self::AppVersion | Self::Source)
	}
}

// all rows are always rendered
fn is_enabled(focus: Focus, state: &NewProjectState) -> bool {
	match focus {
		Focus::Source => state.open_source,
		Focus::KotlinStdlib => state.language == Language::Kotlin,
		_ => true,
	}
}

// a single line text field with cursor position
#[derive(Debug, Clone)]
pub struct TextField {
	pub value: String,
	cursor: usize,
	placeholder: &'static str,
}

impl TextField {
	fn new(placeholder: &'static str) -> Self {
		Self { value: String::new(), cursor: 0, placeholder }
	}

	pub fn placeholder(&self) -> &'static str {
		self.placeholder
	}

	pub fn insert_char(&mut self, c: char) {
		self.value.insert(self.cursor, c);
		self.cursor += c.len_utf8();
	}

	pub fn backspace(&mut self) {
		if self.cursor == 0 {
			return;
		}
		let prev = self.value[..self.cursor]
			.char_indices()
			.next_back()
			.map(|(i, _)| i)
			.unwrap_or(0);
		self.value.drain(prev..self.cursor);
		self.cursor = prev;
	}

	pub fn delete_forward(&mut self) {
		if self.cursor >= self.value.len() {
			return;
		}
		let next = self.value[self.cursor..]
			.char_indices()
			.nth(1)
			.map(|(i, _)| self.cursor + i)
			.unwrap_or(self.value.len());
		self.value.drain(self.cursor..next);
	}

	pub fn move_left(&mut self) {
		if self.cursor == 0 {
			return;
		}
		self.cursor = self.value[..self.cursor]
			.char_indices()
			.next_back()
			.map(|(i, _)| i)
			.unwrap_or(0);
	}

	pub fn move_right(&mut self) {
		if self.cursor >= self.value.len() {
			return;
		}
		self.cursor = self.value[self.cursor..]
			.char_indices()
			.nth(1)
			.map(|(i, _)| self.cursor + i)
			.unwrap_or(self.value.len());
	}

	pub fn move_home(&mut self) {
		self.cursor = 0;
	}

	pub fn move_end(&mut self) {
		self.cursor = self.value.len();
	}

	// caret column visible to the user, used for placeholder + cursor rendering
	pub fn cursor_column(&self) -> usize {
		self.value[..self.cursor].chars().count()
	}
}

// clickable areas recorded during the last render, used for mouse hit-testing
#[derive(Debug, Clone, Copy, Default)]
pub struct HitAreas {
	pub sdk_id: Rect,
	pub author: Rect,
	pub app_version: Rect,
	pub source: Rect,
	pub open_source: Rect,
	pub kotlin_stdlib: Rect,
	pub use_current_compiler: Rect,
	pub language_kotlin: Rect,
	pub language_java: Rect,
	pub create: Rect,
}

// full state of the new-project form
pub struct NewProjectState {
	pub sdk_id: TextField,
	pub author: TextField,
	pub app_version: TextField,
	pub source: TextField,

	pub open_source: bool,
	pub kotlin_stdlib: bool,
	pub use_current_compiler: bool,

	pub language: Language,
	pub focus: Focus,

	pub hit_areas: HitAreas,
	pub submitted: bool,
	pub should_quit: bool,
}

impl Default for NewProjectState {
	fn default() -> Self {
		Self {
			sdk_id: TextField::new("com.example.mysdk"),
			author: TextField::new(""),
			app_version: TextField::new(">=0.8.6"),
			source: TextField::new("github.com/username/my-sdk"),

			open_source: false,
			kotlin_stdlib: false,
			use_current_compiler: false,

			language: Language::Kotlin,
			focus: Focus::SdkId,

			hit_areas: HitAreas::default(),
			submitted: false,
			should_quit: false,
		}
	}
}

impl NewProjectState {
	// fields that can currently hold focus, in tab order; disabled fields are skipped but still rendered
	pub fn enabled_focus_order(&self) -> Vec<Focus> {
		Focus::ORDER_FULL
			.into_iter()
			.filter(|f| self.is_enabled(*f))
			.collect()
	}

	pub fn is_enabled(&self, focus: Focus) -> bool {
		is_enabled(focus, self)
	}

	pub fn focus_next(&mut self) {
		let order = self.enabled_focus_order();
		let pos = order.iter().position(|f| *f == self.focus).unwrap_or(0);
		self.focus = order[(pos + 1) % order.len()];
	}

	pub fn focus_prev(&mut self) {
		let order = self.enabled_focus_order();
		let pos = order.iter().position(|f| *f == self.focus).unwrap_or(0);
		self.focus = order[(pos + order.len() - 1) % order.len()];
	}

	pub fn set_focus(&mut self, focus: Focus) {
		if self.is_enabled(focus) {
			self.focus = focus;
		}
	}

	// active text field for the current focus, if any
	pub fn active_text_field(&mut self) -> Option<&mut TextField> {
		match self.focus {
			Focus::SdkId => Some(&mut self.sdk_id),
			Focus::Author => Some(&mut self.author),
			Focus::AppVersion => Some(&mut self.app_version),
			Focus::Source if self.open_source => Some(&mut self.source),
			_ => None,
		}
	}

	pub fn toggle_focused_checkbox(&mut self) {
		match self.focus {
			Focus::OpenSource => self.open_source = !self.open_source,
			Focus::KotlinStdlib => self.kotlin_stdlib = !self.kotlin_stdlib,
			Focus::UseCurrentCompiler => self.use_current_compiler = !self.use_current_compiler,
			_ => {}
		}
		self.reconcile_focus_state();
	}

	pub fn toggle_language(&mut self) {
		self.language = match self.language {
			Language::Kotlin => Language::Java,
			Language::Java => Language::Kotlin,
		};
		self.reconcile_focus_state();
	}

	pub fn set_language(&mut self, language: Language) {
		self.language = language;
		self.reconcile_focus_state();
	}

	// drop state that no longer applies when a field becomes disabled, and move focus off it
	fn reconcile_focus_state(&mut self) {
		if self.language != Language::Kotlin {
			self.kotlin_stdlib = false;
		}
		if self.is_enabled(self.focus) {
			return;
		}
		let order = self.enabled_focus_order();
		self.focus = order[0];
	}

	pub fn activate_focus(&mut self) {
		match self.focus {
			Focus::OpenSource | Focus::KotlinStdlib | Focus::UseCurrentCompiler => {
				self.toggle_focused_checkbox();
			}
			Focus::Language => self.toggle_language(),
			Focus::Create => self.submitted = true,
			_ => {}
		}
	}
}
