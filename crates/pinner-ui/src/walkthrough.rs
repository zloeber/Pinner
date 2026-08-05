use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use pinner_core::{PinDecision, WalkthroughOutcome, apply_walkthrough_decisions};
use pinner_ecosystem::Pin;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::{DefaultTerminal, Frame};

enum Mode {
    Browse,
    Edit { buffer: String },
}

struct App {
    pins: Vec<Pin>,
    decisions: Vec<Option<PinDecision>>,
    selected: usize,
    mode: Mode,
    done: Option<WalkthroughOutcome>,
}

impl App {
    fn new(pins: &[Pin]) -> Self {
        Self {
            pins: pins.to_vec(),
            decisions: vec![None; pins.len()],
            selected: 0,
            mode: Mode::Browse,
            done: None,
        }
    }

    fn accepted_count(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| matches!(d, Some(PinDecision::Accept | PinDecision::Edit { .. })))
            .count()
    }

    fn skipped_count(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| matches!(d, Some(PinDecision::Skip)))
            .count()
    }

    fn decided_count(&self) -> usize {
        self.decisions.iter().filter(|d| d.is_some()).count()
    }

    fn set_decision(&mut self, decision: PinDecision) {
        if self.pins.is_empty() {
            return;
        }
        self.decisions[self.selected] = Some(decision);
        if self.decided_count() == self.pins.len() {
            self.finish();
            return;
        }
        self.advance();
    }

    fn advance(&mut self) {
        let n = self.pins.len();
        if n == 0 {
            return;
        }
        for offset in 1..=n {
            let idx = (self.selected + offset) % n;
            if self.decisions[idx].is_none() {
                self.selected = idx;
                return;
            }
        }
    }

    fn finish(&mut self) {
        let decisions: Vec<PinDecision> = self
            .decisions
            .iter()
            .cloned()
            .map(|d| d.expect("all decisions set"))
            .collect();
        match apply_walkthrough_decisions(&self.pins, &decisions) {
            Ok(outcome) => self.done = Some(outcome),
            Err(_) => {
                // Length mismatch is unreachable when decisions match pins.
                self.done = Some(WalkthroughOutcome::Aborted);
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        match &mut self.mode {
            Mode::Edit { buffer } => match code {
                KeyCode::Esc => self.mode = Mode::Browse,
                KeyCode::Enter => {
                    let pinned = buffer.trim().to_string();
                    if pinned.is_empty() {
                        return;
                    }
                    self.mode = Mode::Browse;
                    self.set_decision(PinDecision::Edit { pinned });
                }
                KeyCode::Backspace => {
                    buffer.pop();
                }
                KeyCode::Char(c) => buffer.push(c),
                _ => {}
            },
            Mode::Browse => match code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.done = Some(WalkthroughOutcome::Aborted);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if !self.pins.is_empty() {
                        self.selected = self.selected.saturating_sub(1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !self.pins.is_empty() && self.selected + 1 < self.pins.len() {
                        self.selected += 1;
                    }
                }
                KeyCode::Enter | KeyCode::Char('a') => {
                    self.set_decision(PinDecision::Accept);
                }
                KeyCode::Char('s') => {
                    self.set_decision(PinDecision::Skip);
                }
                KeyCode::Char('e') => {
                    if self.pins.is_empty() {
                        return;
                    }
                    let initial = self.pins[self.selected].pinned.clone();
                    self.mode = Mode::Edit { buffer: initial };
                }
                _ => {}
            },
        }
    }
}

/// Interactive compact-list walkthrough. `q` / Esc → [`WalkthroughOutcome::Aborted`].
///
/// Empty pin lists return [`WalkthroughOutcome::Continue`] immediately (no TUI).
pub fn run_compact_walkthrough(pins: &[Pin]) -> io::Result<WalkthroughOutcome> {
    if pins.is_empty() {
        return Ok(WalkthroughOutcome::Continue { pins: Vec::new() });
    }

    let mut terminal = ratatui::try_init()?;
    let outcome = run_app(&mut terminal, pins);
    ratatui::restore();
    outcome
}

fn run_app(terminal: &mut DefaultTerminal, pins: &[Pin]) -> io::Result<WalkthroughOutcome> {
    let mut app = App::new(pins);
    loop {
        if let Some(outcome) = app.done.take() {
            return Ok(outcome);
        }
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(std::time::Duration::from_millis(200))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            app.handle_key(key.code);
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .split(area);

    let header = format!(
        "Walkthrough  {}/{} decided  ·  accepted {}  skipped {}  ·  ↑↓ move  Enter/a accept  s skip  e edit  q quit",
        app.decided_count(),
        app.pins.len(),
        app.accepted_count(),
        app.skipped_count(),
    );
    frame.render_widget(
        Paragraph::new(header).block(Block::default().borders(Borders::ALL).title("pinner")),
        chunks[0],
    );

    let rows: Vec<Row> = app
        .pins
        .iter()
        .enumerate()
        .map(|(i, pin)| {
            let status = match &app.decisions[i] {
                None => "·",
                Some(PinDecision::Accept) => "✓",
                Some(PinDecision::Skip) => "skip",
                Some(PinDecision::Edit { .. }) => "edit",
            };
            let style = if i == app.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                status.to_string(),
                pin.ecosystem.as_str().to_string(),
                pin.name.clone(),
                format_pin_transition(pin),
                pin.path.display().to_string(),
            ])
            .style(style)
        })
        .collect();

    let upgrade_mode = any_upgrade_pins(&app.pins);
    let transition_header = if upgrade_mode {
        "current → proposed"
    } else {
        "requested → proposed"
    };
    let table_title = if upgrade_mode {
        "proposed upgrades"
    } else {
        "proposed pins"
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(24),
            Constraint::Min(20),
            Constraint::Min(16),
        ],
    )
    .header(
        Row::new(["", "eco", "name", transition_header, "path"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title(table_title));
    frame.render_widget(table, chunks[1]);

    let help = match &app.mode {
        Mode::Browse => Line::from("Select a row and choose accept / skip / edit."),
        Mode::Edit { buffer } => Line::from(vec![
            Span::raw("Edit pinned value: "),
            Span::styled(
                buffer.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  (Enter confirm, Esc cancel)"),
        ]),
    };
    frame.render_widget(Paragraph::new(help), chunks[2]);

    if let Mode::Edit { buffer } = &app.mode {
        let popup = centered_rect(60, 5, area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(buffer.as_str())
                .block(Block::default().borders(Borders::ALL).title("edit pin")),
            popup,
        );
    }
}

fn is_upgrade_pin(pin: &Pin) -> bool {
    pin.metadata
        .get("upgrade")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn any_upgrade_pins(pins: &[Pin]) -> bool {
    pins.iter().any(is_upgrade_pin)
}

fn format_pin_transition(pin: &Pin) -> String {
    let current = pin
        .metadata
        .get("upgrade")
        .and_then(|v| v.as_bool())
        .filter(|u| *u)
        .and_then(|_| pin.metadata.get("previous").and_then(|v| v.as_str()))
        .unwrap_or(pin.requested.as_str());
    format!("{current} → {}", pin.pinned)
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let top = area.height.saturating_sub(height) / 2;
    let vertical = Layout::vertical([
        Constraint::Length(top),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use pinner_core::WalkthroughOutcome;
    use pinner_ecosystem::{EcosystemKind, EvidenceKind};
    use serde_json::Value;

    fn sample_pin(requested: &str, pinned: &str) -> Pin {
        Pin {
            ecosystem: EcosystemKind::Mise,
            name: "test".into(),
            requested: requested.into(),
            pinned: pinned.into(),
            path: PathBuf::from(".mise.toml"),
            evidence: EvidenceKind::Tool,
            metadata: Default::default(),
        }
    }

    #[test]
    fn empty_pins_continue_without_tui() {
        let outcome = run_compact_walkthrough(&[]).unwrap();
        assert_eq!(outcome, WalkthroughOutcome::Continue { pins: vec![] });
    }

    #[test]
    fn format_pin_transition_prefers_previous_for_upgrade() {
        let mut pin = sample_pin("1.0.0", "2.0.0");
        pin.metadata.insert("upgrade".into(), Value::Bool(true));
        pin.metadata
            .insert("previous".into(), Value::String("1.0.0".into()));
        assert_eq!(format_pin_transition(&pin), "1.0.0 → 2.0.0");
    }
}
