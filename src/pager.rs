use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use ansi_to_tui::IntoText;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use crate::engine::Engine;
use crate::filter::LineFilter;
use crate::md_table::{FeedResult, FlushResult, TableAssembler};
use crate::trigger::{OutputDecision, TriggerFilter};

/// Drop guard that ensures terminal state is restored even on panic.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Run the interactive pager for the given file content.
///
/// `raw_lines` should contain the full file content (all lines).
/// Lines are filtered, colorized, and displayed interactively.
#[allow(clippy::too_many_arguments)]
pub fn run(
    file_path: &Path,
    engine: &mut Engine,
    filter: &LineFilter,
    trigger: &mut TriggerFilter,
    profile_name: Option<&str>,
    rule_count: usize,
    raw_lines: &[String],
    table: Option<&mut TableAssembler>,
) -> Result<()> {
    // Apply filter + engine + trigger to produce colorized ANSI lines
    let colored_lines = colorize_lines(raw_lines, engine, filter, trigger, table);
    let tui_lines = to_tui_lines(&colored_lines);
    display(file_path, profile_name, rule_count, tui_lines)
}

/// Pager over lines that are already fully styled (include expansion).
pub fn run_prerendered(
    file_path: &Path,
    profile_name: Option<&str>,
    rule_count: usize,
    colored_lines: &[String],
) -> Result<()> {
    display(file_path, profile_name, rule_count, to_tui_lines(colored_lines))
}

/// Convert ANSI-styled strings to ratatui lines for display.
fn to_tui_lines(colored_lines: &[String]) -> Vec<Line<'static>> {
    colored_lines
        .iter()
        .flat_map(|s| s.as_bytes().into_text().unwrap_or_default().lines)
        .collect()
}

/// Set up the terminal and run the event loop over pre-converted lines.
fn display(
    file_path: &Path,
    profile_name: Option<&str>,
    rule_count: usize,
    tui_lines: Vec<Line<'static>>,
) -> Result<()> {
    if tui_lines.is_empty() {
        return Ok(());
    }

    // Setup terminal
    terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)
        .context("failed to enter alternate screen")?;

    // Drop guard ensures cleanup even on panic
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    // Run the event loop
    event_loop(
        &mut terminal,
        &tui_lines,
        file_path,
        profile_name,
        rule_count,
    )
}

/// Colorize lines through the existing Engine/Filter/Trigger pipeline.
fn colorize_lines(
    raw_lines: &[String],
    engine: &mut Engine,
    filter: &LineFilter,
    trigger: &mut TriggerFilter,
    mut table: Option<&mut TableAssembler>,
) -> Vec<String> {
    let mut result = Vec::new();

    if trigger.is_active() {
        for line in raw_lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            let apply_result = engine.apply(line);
            match trigger.process_line(line, apply_result.flatten()) {
                OutputDecision::Pass(v) => result.extend(v),
                OutputDecision::Flush(lines) => result.extend(lines),
                OutputDecision::Suppress => {}
            }
        }
    } else {
        for line in raw_lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            match table.as_deref_mut() {
                None => result.extend(engine.apply(line).flatten()),
                Some(t) => match t.feed(line) {
                    FeedResult::Pass(raw) => {
                        for r in raw {
                            result.extend(engine.apply(&r).flatten());
                        }
                    }
                    FeedResult::Buffered => {}
                    FeedResult::Table { rendered, trailing } => {
                        result.extend(rendered);
                        if let Some(r) = trailing {
                            result.extend(engine.apply(&r).flatten());
                        }
                    }
                },
            }
        }
        if let Some(t) = table {
            match t.flush() {
                FlushResult::Nothing => {}
                FlushResult::Raw(r) => result.extend(engine.apply(&r).flatten()),
                FlushResult::Table(rendered) => result.extend(rendered),
            }
        }
    }

    result
}

/// Main event loop: render and handle input.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    lines: &[Line<'static>],
    file_path: &Path,
    profile_name: Option<&str>,
    rule_count: usize,
) -> Result<()> {
    let mut offset: usize = 0;
    let total_lines = lines.len();
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    loop {
        // Draw
        terminal.draw(|frame| {
            let area = frame.area();
            let [content_area, status_area] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(area);

            let content_height = content_area.height as usize;

            // Clamp offset
            if total_lines > content_height {
                if offset > total_lines - content_height {
                    offset = total_lines - content_height;
                }
            } else {
                offset = 0;
            }

            // Render content — no border, no Block, just a bare Paragraph
            let end = (offset + content_height).min(total_lines);
            let visible = &lines[offset..end];
            let content = Paragraph::new(visible.to_vec());
            frame.render_widget(content, content_area);

            // Render status bar
            let last_visible = end;
            let first_visible = offset + 1;
            let pct = if total_lines == 0 {
                0
            } else {
                (last_visible * 100) / total_lines
            };

            let left = match profile_name {
                Some(name) => format!(
                    " {} | profile: {} | {} rules",
                    file_name, name, rule_count
                ),
                None => format!(" {} | {} rules", file_name, rule_count),
            };
            let right = format!(
                "{}-{}/{} ({}%) q:quit Space:next b:prev ",
                first_visible, last_visible, total_lines, pct
            );

            let status_width = status_area.width as usize;
            let left_len = left.len();
            let right_len = right.len();
            let padding = if status_width > left_len + right_len {
                status_width - left_len - right_len
            } else {
                1
            };
            let status_text = format!(
                "{}{:>width$}{}",
                left,
                "",
                right,
                width = padding
            );

            let status = Paragraph::new(Line::raw(status_text)).style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
            frame.render_widget(status, status_area);
        })?;

        // Handle input — single read, match all event types
        let content_height = terminal.size()?.height.saturating_sub(1) as usize;
        match event::read()? {
            Event::Key(key) => match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) => break,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                (KeyCode::Char(' '), _) => {
                    offset = offset.saturating_add(content_height);
                }
                (KeyCode::Char('b'), _) => {
                    offset = offset.saturating_sub(content_height);
                }
                (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                    offset = offset.saturating_add(1);
                }
                (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                    offset = offset.saturating_sub(1);
                }
                (KeyCode::Char('g'), _) => {
                    offset = 0;
                }
                (KeyCode::Char('G'), _) => {
                    offset = total_lines; // clamped in draw
                }
                _ => {}
            },
            Event::Resize(_, _) => {
                // Next draw picks up new dimensions automatically
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_table::TableAssembler;

    #[test]
    fn to_tui_lines_preserves_line_count_and_text() {
        let lines = vec![
            "plain".to_string(),
            "\x1b[31mred\x1b[0m".to_string(),
            "│ gutter".to_string(),
        ];
        let tui = to_tui_lines(&lines);
        assert_eq!(tui.len(), 3);
        let texts: Vec<String> = tui
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(texts, vec!["plain", "red", "│ gutter"]);
    }

    #[test]
    fn colorize_lines_renders_tables() {
        let mut engine = Engine::new(vec![], true, None);
        let filter = LineFilter::new(&[], &[], true).unwrap();
        let mut trigger = TriggerFilter::new(&[], "20", "20", true).unwrap();
        let lines: Vec<String> = vec![
            "| a | b |".into(),
            "|---|---|".into(),
            "| 1 | 2 |".into(),
        ];
        let mut table = TableAssembler::new();
        let out = colorize_lines(&lines, &mut engine, &filter, &mut trigger, Some(&mut table));
        let joined = out.join("\n");
        assert!(joined.contains('┌'), "pager should box-draw tables: {joined}");
    }
}
