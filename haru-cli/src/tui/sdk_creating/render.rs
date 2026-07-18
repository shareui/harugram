use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::fullscreen;
use crate::tui::sdk_creating::state::{Focus, Language, NewProjectState, TextField};
use crate::tui::theme::Theme;

pub const CARD_WIDTH: u16 = 62;

pub fn render(frame: &mut Frame, state: &mut NewProjectState, theme: &Theme) {
	frame.render_widget(Block::new().style(theme.base), frame.area());

	let card_height = card_height();
	let card = fullscreen::centered_rect(CARD_WIDTH, card_height, frame.area());

	fullscreen::render_backdrop(frame, frame.area(), card, theme);
	let outer = Block::new()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(theme.accent))
		.style(theme.base)
		.padding(Padding::symmetric(2, 1));
	let inner = outer.inner(card);
	frame.render_widget(outer, card);

	render_header(frame, inner, theme);

	let body = Rect { y: inner.y + 4, height: inner.height.saturating_sub(4), ..inner };
	render_body(frame, body, state, theme);
}

pub fn card_height() -> u16 {
	let order = Focus::ORDER_FULL;
	let fields_height: u16 = order.iter().map(|f| if f.is_text_field() { 3 } else { 1 }).sum();
	let gaps_between_fields = order.len().saturating_sub(1) as u16;
	// header(2) + gap(2) + fields + gaps between fields + gap + language(3) + gap + button(3)
	let content_height = 2 + 2 + fields_height + gaps_between_fields + 1 + 3 + 1 + 3;
	content_height + 2 + 2
}

fn render_header(frame: &mut Frame, area: Rect, theme: &Theme) {
	let title = Line::from(Span::styled("New Project", theme.title)).alignment(Alignment::Center);
	let subtitle = Line::from(Span::styled(
		"configure the sdk before generation",
		Style::new().fg(theme.muted).add_modifier(Modifier::ITALIC),
	))
	.alignment(Alignment::Center);

	let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]);
	let [title_area, subtitle_area] = area.layout(&layout);

	frame.render_widget(title, title_area);
	frame.render_widget(subtitle, subtitle_area);
}

enum Row {
	Field(Focus),
	Spacer,
	Language,
	Button,
}

fn render_body(frame: &mut Frame, area: Rect, state: &mut NewProjectState, theme: &Theme) {
	let order = Focus::ORDER_FULL;

	let mut plan: Vec<Row> = Vec::new();
	for (i, focus) in order.iter().enumerate() {
		if i > 0 {
			plan.push(Row::Spacer);
		}
		plan.push(Row::Field(*focus));
	}
	plan.push(Row::Spacer);
	plan.push(Row::Language);
	plan.push(Row::Spacer);
	plan.push(Row::Button);

	let constraints: Vec<Constraint> = plan
		.iter()
		.map(|row| match row {
			Row::Button => Constraint::Length(3),
			Row::Field(focus) if focus.is_text_field() => Constraint::Length(3),
			Row::Language => Constraint::Length(3),
			_ => Constraint::Length(1),
		})
		.collect();

	let layout = Layout::vertical(constraints);
	let rows = area.layout_vec(&layout);

	for (row, area) in plan.iter().zip(rows.iter()) {
		match row {
			Row::Field(focus) => render_field(frame, *area, *focus, state, theme),
			Row::Spacer => {}
			Row::Language => render_language_row(frame, *area, state, theme),
			Row::Button => render_button(frame, *area, state, theme),
		}
	}
}

fn render_field(frame: &mut Frame, area: Rect, focus: Focus, state: &mut NewProjectState, theme: &Theme) {
	let enabled = state.is_enabled(focus);
	match focus {
		Focus::SdkId => render_text_row(frame, area, "SDK ID", &state.sdk_id, focus, state.focus, enabled, theme, &mut state.hit_areas.sdk_id),
		Focus::Author => {
			render_text_row(frame, area, "Author", &state.author, focus, state.focus, enabled, theme, &mut state.hit_areas.author)
		}
		Focus::AppVersion => render_text_row(
			frame,
			area,
			"App version",
			&state.app_version,
			focus,
			state.focus,
			enabled,
			theme,
			&mut state.hit_areas.app_version,
		),
		Focus::Source => {
			render_text_row(frame, area, "Source", &state.source, focus, state.focus, enabled, theme, &mut state.hit_areas.source)
		}
		Focus::OpenSource => render_checkbox_row(
			frame,
			area,
			"Open source code",
			state.open_source,
			focus,
			state.focus,
			enabled,
			theme,
			&mut state.hit_areas.open_source,
		),
		Focus::KotlinStdlib => render_checkbox_row(
			frame,
			area,
			"Add kotlin-stdlib.jar",
			state.kotlin_stdlib,
			focus,
			state.focus,
			enabled,
			theme,
			&mut state.hit_areas.kotlin_stdlib,
		),
		Focus::UseCurrentCompiler => render_checkbox_row(
			frame,
			area,
			"Use the current compiler version as the minimum",
			state.use_current_compiler,
			focus,
			state.focus,
			enabled,
			theme,
			&mut state.hit_areas.use_current_compiler,
		),
		Focus::Language | Focus::Create => {}
	}
}

#[allow(clippy::too_many_arguments)]
fn render_text_row(
	frame: &mut Frame,
	area: Rect,
	label: &str,
	field: &TextField,
	row_focus: Focus,
	current_focus: Focus,
	enabled: bool,
	theme: &Theme,
	hit_area: &mut Rect,
) {

	*hit_area = if enabled { area } else { Rect::default() };
	let focused = enabled && row_focus == current_focus;

	let border_color = if !enabled { theme.disabled } else if focused { theme.border_focused } else { theme.border };
	let title_style = if !enabled {
		theme.disabled_style
	} else if focused {
		theme.border_focused_style.add_modifier(Modifier::BOLD)
	} else {
		theme.label
	};
	let title = format!(" {label} ");

	let block = Block::bordered()
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(border_color))
		.title(Span::styled(title, title_style));

	let inner = block.inner(area);
	frame.render_widget(block, area);

	let is_placeholder = field.value.is_empty();
	let shown = if is_placeholder { field.placeholder() } else { field.value.as_str() };
	let value_style = if !enabled {
		theme.disabled_style
	} else if is_placeholder {
		Style::new().fg(theme.muted)
	} else {
		theme.value
	};

	frame.render_widget(Paragraph::new(shown).style(value_style), inner);

	if focused {
		let cursor_x = inner.x + field.cursor_column() as u16;
		let cursor_x = cursor_x.min(inner.right().saturating_sub(1));
		frame.set_cursor_position(Position::new(cursor_x, inner.y));
	}
}

#[allow(clippy::too_many_arguments)]
fn render_checkbox_row(
	frame: &mut Frame,
	area: Rect,
	label: &str,
	checked: bool,
	row_focus: Focus,
	current_focus: Focus,
	enabled: bool,
	theme: &Theme,
	hit_area: &mut Rect,
) {
	*hit_area = if enabled { area } else { Rect::default() };
	let focused = enabled && row_focus == current_focus;

	let glyph = if checked { "◉" } else { "○" };
	let glyph_style = if !enabled {
		theme.disabled_style
	} else if checked {
		Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
	} else {
		Style::new().fg(theme.muted)
	};
	let label_style = if !enabled {
		theme.disabled_style
	} else if focused {
		theme.border_focused_style.add_modifier(Modifier::BOLD)
	} else {
		theme.value
	};
	let marker_style = if !enabled {
		theme.disabled_style
	} else if focused {
		theme.border_focused_style
	} else {
		theme.label
	};
	let marker = if focused { "▸ " } else { "  " };

	let line = Line::from(vec![
		Span::styled(marker, marker_style),
		Span::styled(glyph, glyph_style),
		Span::raw(" "),
		Span::styled(label, label_style),
	]);
	frame.render_widget(line, area);
}

fn render_language_row(frame: &mut Frame, area: Rect, state: &mut NewProjectState, theme: &Theme) {
	let focused = state.focus == Focus::Language;
	let marker = if focused { "▸ " } else { "  " };
	let label_style = if focused { theme.border_focused_style.add_modifier(Modifier::BOLD) } else { theme.label };

	let columns = Layout::horizontal([Constraint::Length(16), Constraint::Length(18)]);
	let [label_area, switch_area] = area.layout(&columns);

	// middle row of the 3-row area
	let label_area = Rect { y: label_area.y + 1, height: 1, ..label_area };
	let label_line = Line::from(vec![Span::styled(marker, label_style), Span::styled("Language", label_style)]);
	frame.render_widget(label_line, label_area);

	render_language_switch(frame, switch_area, state, focused, theme);
}

fn render_language_switch(frame: &mut Frame, area: Rect, state: &mut NewProjectState, row_focused: bool, theme: &Theme) {
	let border_color = if row_focused { theme.border_focused } else { theme.border };
	let block = Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(border_color));
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let halves = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]);
	let [kotlin_area, java_area] = inner.layout(&halves);

	state.hit_areas.language_kotlin = kotlin_area;
	state.hit_areas.language_java = java_area;

	render_language_segment(frame, kotlin_area, Language::Kotlin.label(), state.language == Language::Kotlin, theme);
	render_language_segment(frame, java_area, Language::Java.label(), state.language == Language::Java, theme);
}

fn render_language_segment(frame: &mut Frame, area: Rect, label: &str, selected: bool, theme: &Theme) {
	let style = if selected {
		Style::new().fg(theme.background).bg(theme.accent).add_modifier(Modifier::BOLD)
	} else {
		Style::new().fg(theme.muted)
	};
	frame.render_widget(Block::new().style(style), area);
	let line = Line::from(Span::styled(label, style)).alignment(Alignment::Center);
	frame.render_widget(line, area);
}

fn render_button(frame: &mut Frame, area: Rect, state: &mut NewProjectState, theme: &Theme) {
	let focused = state.focus == Focus::Create;
	state.hit_areas.create = area;

	let border_color = if focused { theme.border_focused } else { theme.accent };
	let block = Block::new()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::new().fg(border_color))
		.style(Style::new().bg(theme.accent));

	let label_style = Style::new().fg(theme.background).bg(theme.accent).add_modifier(Modifier::BOLD);
	let label = if focused { "▶  Create  ◀" } else { "Create" };

	let inner = block.inner(area);
	frame.render_widget(block, area);
	let line = Line::from(Span::styled(label, label_style)).alignment(Alignment::Center);
	frame.render_widget(line, inner);
}
