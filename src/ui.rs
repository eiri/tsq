use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::sequencer::{HihatVoice, STEPS, SequencerState, SharedState, ToneVoice, random_pattern};

// 1 space + 12 label + 1 space + 8*3 steps + 7 spaces between steps + 2 border cols
const FRAME_WIDTH: u16 = 47;
// 4 track rows + 1 empty row + 1 status row + 2 border rows
const FRAME_HEIGHT: u16 = 8;

const COLOR_HIGHLIGHT: Color = Color::Rgb(255, 160, 100);
const COLOR_STEP: Color = Color::Rgb(210, 120, 80);
const COLOR_STEP_ALT: Color = Color::Rgb(135, 79, 54);
const COLOR_STEP_OFF: Color = Color::DarkGray;

pub fn run(shared: SharedState) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, shared);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    shared: SharedState,
) -> Result<()> {
    loop {
        let state = shared.lock().unwrap().clone();
        terminal.draw(|f| draw(f, &state))?;

        if event::poll(std::time::Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('r') => {
                    let mut s = shared.lock().unwrap();
                    s.pattern = random_pattern();
                    s.reset = true;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn fixed_rect(area: Rect) -> Rect {
    Rect {
        x: area.x,
        y: area.y,
        width: FRAME_WIDTH.min(area.width),
        height: FRAME_HEIGHT.min(area.height),
    }
}

fn draw(frame: &mut ratatui::Frame, state: &SequencerState) {
    let area = fixed_rect(frame.area());

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" tsq ");

    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        track_line("kick:     ", &state.pattern.kick, state.current_step),
        chunks[0],
    );
    frame.render_widget(
        track_line("snare:    ", &state.pattern.snare, state.current_step),
        chunks[1],
    );
    frame.render_widget(
        hihat_line("hihat:    ", &state.pattern.hihat, state.current_step),
        chunks[2],
    );
    frame.render_widget(
        track_line("tone:     ", &state.pattern.tone, state.current_step),
        chunks[3],
    );

    let tone_name = match state.pattern.tone_voice {
        ToneVoice::Sine => "sine",
        ToneVoice::Square => "square",
    };

    // ref : ◎ ◉ ○ ● ◯ ⬤
    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {:.0} BPM", state.bpm),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!("  tone: {tone_name}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("    r", Style::default().fg(Color::Yellow)),
        Span::styled(" randomize  ", Style::default().fg(Color::DarkGray)),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::styled(" quit", Style::default().fg(Color::DarkGray)),
    ]));

    frame.render_widget(status, chunks[5]);
}

fn track_line<'a>(label: &'a str, steps: &'a [bool; STEPS], current: usize) -> Paragraph<'a> {
    let mut spans = vec![Span::raw(format!(" {label} "))];

    for (i, &active) in steps.iter().enumerate() {
        let is_current = i == current;

        let (text, style) = match (active, is_current) {
            (_, true) => (
                "●",
                Style::default()
                    .fg(COLOR_HIGHLIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            (true, false) => ("●", Style::default().fg(COLOR_STEP)),
            (false, false) => ("○", Style::default().fg(COLOR_STEP_OFF)),
        };

        spans.push(Span::styled(text, style));
        if i < STEPS - 1 {
            spans.push(Span::raw(" "));
        }
    }

    Paragraph::new(Line::from(spans))
}

// fixme - this needs smth smarter
fn hihat_line<'a>(
    label: &'a str,
    steps: &'a [Option<HihatVoice>; STEPS],
    current: usize,
) -> Paragraph<'a> {
    let mut spans = vec![Span::raw(format!(" {label} "))];

    for (i, step) in steps.iter().enumerate() {
        let is_current = i == current;

        let (text, style) = match (step, is_current) {
            (None, true) => (
                "○",
                Style::default()
                    .fg(COLOR_HIGHLIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            (_, true) => (
                "●",
                Style::default()
                    .fg(COLOR_HIGHLIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            (Some(HihatVoice::Open), false) => ("●", Style::default().fg(COLOR_STEP)),
            (Some(HihatVoice::Closed), false) => ("●", Style::default().fg(COLOR_STEP_ALT)),
            (None, false) => ("○", Style::default().fg(COLOR_STEP_OFF)),
        };

        spans.push(Span::styled(text, style));
        if i < STEPS - 1 {
            spans.push(Span::raw(" "));
        }
    }

    Paragraph::new(Line::from(spans))
}
