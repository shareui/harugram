use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeName {
	#[default]
	AshViolet,
}


#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Theme {
	pub background: Color,
	pub surface: Color,
	pub foreground: Color,
	pub muted: Color,
	pub border: Color,
	pub border_focused: Color,
	pub accent: Color,
	pub accent_alt: Color,
	pub disabled: Color,
	pub log: Color,
	pub critical: Color,
	pub warn: Color,

	pub base: Style,
	pub title: Style,
	pub label: Style,
	pub value: Style,
	pub border_style: Style,
	pub border_focused_style: Style,
	pub selection: Style,
	pub disabled_style: Style,
	pub log_style: Style,
	pub critical_style: Style,
	pub warn_style: Style,
}

impl Theme {
	pub fn get(name: ThemeName) -> Self {
		match name {
			ThemeName::AshViolet => Self::ash_violet(),
		}
	}

	fn ash_violet() -> Self {
		let background = ash_violet::VOID_VIOLET;
		let surface = ash_violet::PANEL_VIOLET;
		let foreground = ash_violet::MIST_WHITE;
		let muted = ash_violet::DUSK_VIOLET;
		let border = ash_violet::EDGE_VIOLET;
		let border_focused = ash_violet::GLOW_LILAC;
		let accent = ash_violet::LILAC;
		let accent_alt = ash_violet::PERIWINKLE;
		let disabled = ash_violet::SLATE_VIOLET;
		let log = ash_violet::GLOW_LILAC;
		let critical = ash_violet::CRIMSON;
		let warn = ash_violet::AMBER;

		Self {
			background,
			surface,
			foreground,
			muted,
			border,
			border_focused,
			accent,
			accent_alt,
			disabled,
			log,
			critical,
			warn,

			base: Style::new().fg(foreground).bg(background),
			title: Style::new().fg(accent).add_modifier(Modifier::BOLD),
			label: Style::new().fg(muted),
			value: Style::new().fg(foreground),
			border_style: Style::new().fg(border),
			border_focused_style: Style::new().fg(border_focused),
			selection: Style::new().fg(background).bg(accent),
			disabled_style: Style::new().fg(disabled),
			log_style: Style::new().fg(log),
			critical_style: Style::new().fg(critical),
			warn_style: Style::new().fg(warn),
		}
	}
}

// palette of ash violet
mod ash_violet {
	use ratatui::style::Color;

	// near-black violet, deep enough to give every lighter tone real contrast
	pub const VOID_VIOLET: Color = Color::from_u32(0x14101F);
	// slightly lifted panel tone, used behind grouped content
	pub const PANEL_VIOLET: Color = Color::from_u32(0x1E1830);
	// off-white with a violet cast, used for primary text
	pub const MIST_WHITE: Color = Color::from_u32(0xE8E4F5);
	// readable secondary text, still clearly dimmer than foreground
	pub const DUSK_VIOLET: Color = Color::from_u32(0x8B84AD);
	// visible but calm border on the dark background
	pub const EDGE_VIOLET: Color = Color::from_u32(0x4A4166);
	// bright focus ring, distinct from both border and accent
	pub const GLOW_LILAC: Color = Color::from_u32(0xC9A8FF);
	// vivid accent for titles, checks, and the primary button
	pub const LILAC: Color = Color::from_u32(0xA684FF);
	// secondary accent for less prominent highlights
	pub const PERIWINKLE: Color = Color::from_u32(0x7C9CFF);
	// desaturated violet-gray for disabled or inactive fields
	pub const SLATE_VIOLET: Color = Color::from_u32(0x5C5770);
	// red for critical errors
	pub const CRIMSON: Color = Color::from_u32(0xE05561);
	// orange for non-critical warnings
	pub const AMBER: Color = Color::from_u32(0xE0A155);
}
