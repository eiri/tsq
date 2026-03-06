use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::sequencer::{STEPS, SequencerState, SharedState, ToneVoice, random_pattern};

// 1 space + 12 label + 1 space + 8*3 steps + 7 spaces between steps + 2 border cols
const FRAME_WIDTH: u16 = 47;
// 4 track rows + 1 empty row + 1 status row + 2 border rows
const FRAME_HEIGHT: u16 = 8;

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
        track_line(
            "hh closed:",
            &state.pattern.hihat_closed,
            state.current_step,
        ),
        chunks[1],
    );
    frame.render_widget(
        track_line("hh open:  ", &state.pattern.hihat_open, state.current_step),
        chunks[2],
    );
    frame.render_widget(
        track_line("tone:     ", &state.pattern.tone, state.current_step),
        chunks[3],
    );

    let voice_name = match state.pattern.tone_voice {
        ToneVoice::Sine => "sine",
        ToneVoice::Square => "square",
    };

    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {:.0} BPM", state.bpm),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!("  voice: {voice_name}"),
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
        let text = if active { "[x]" } else { "[ ]" };

        let style = match (active, is_current) {
            (_, true) => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            (true, false) => Style::default().fg(Color::Green),
            (false, false) => Style::default().fg(Color::DarkGray),
        };

        spans.push(Span::styled(text, style));
        if i < STEPS - 1 {
            spans.push(Span::raw(" "));
        }
    }

    Paragraph::new(Line::from(spans))
}
