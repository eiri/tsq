use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::sequencer::{HihatVoice, STEPS, SequencerState, SharedState, random_pattern};

const FRAME_WIDTH: u16 = 12;
const FRAME_HEIGHT: u16 = 8;

const COLOR_HIGHLIGHT: Color = Color::Rgb(255, 160, 100);
const COLOR_STEP: Color = Color::Rgb(210, 120, 80);
const COLOR_STEP_ALT: Color = Color::Rgb(135, 79, 54);
const COLOR_STEP_OFF: Color = Color::DarkGray;
const COLOR_BTN: Color = Color::Rgb(0, 168, 150);
const COLOR_BTN_PRESSED: Color = Color::Rgb(0, 90, 80);

const TRACK_NAMES: [&str; 4] = ["kick", "snare", "hihat", "tone"];
const NUM_TRACKS: usize = 4;
const HALF: usize = STEPS / 2;

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
    let mut selected_track: usize = 0;
    let mut btn_l_at: Option<std::time::Instant> = None;
    let mut btn_r_at: Option<std::time::Instant> = None;

    const PRESS_DURATION: std::time::Duration = std::time::Duration::from_millis(200);

    loop {
        let now = std::time::Instant::now();
        let btn_l_pressed = btn_l_at.map_or(false, |t| now.duration_since(t) < PRESS_DURATION);
        let btn_r_pressed = btn_r_at.map_or(false, |t| now.duration_since(t) < PRESS_DURATION);

        let state = shared.lock().unwrap().clone();
        terminal.draw(|f| draw(f, &state, selected_track, btn_l_pressed, btn_r_pressed))?;

        if event::poll(std::time::Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('r') => {
                    let mut s = shared.lock().unwrap();
                    s.pattern = random_pattern();
                    s.reset = true;
                    btn_l_at = Some(std::time::Instant::now());
                }
                KeyCode::Char('i') => {
                    selected_track = (selected_track + 1) % NUM_TRACKS;
                    btn_r_at = Some(std::time::Instant::now());
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

fn track_pips(selected: usize) -> String {
    (0..NUM_TRACKS)
        .map(|i| if i == selected { '▾' } else { '▿' })
        .collect()
}

fn draw(
    frame: &mut ratatui::Frame,
    state: &SequencerState,
    selected_track: usize,
    btn_l_pressed: bool,
    btn_r_pressed: bool,
) {
    let area = fixed_rect(frame.area());

    let title = format!(" {} ", TRACK_NAMES[selected_track]);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title);

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
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}", track_pips(selected_track)),
            Style::default().fg(COLOR_STEP),
        )])),
        chunks[0],
    );

    match selected_track {
        0 => {
            let (top, bot) = bool_half_lines(&state.pattern.kick, state.current_step);
            frame.render_widget(top, chunks[2]);
            frame.render_widget(bot, chunks[3]);
        }
        1 => {
            let (top, bot) = bool_half_lines(&state.pattern.snare, state.current_step);
            frame.render_widget(top, chunks[2]);
            frame.render_widget(bot, chunks[3]);
        }
        2 => {
            let (top, bot) = hihat_half_lines(&state.pattern.hihat, state.current_step);
            frame.render_widget(top, chunks[2]);
            frame.render_widget(bot, chunks[3]);
        }
        3 => {
            let (top, bot) = bool_half_lines(&state.pattern.tone, state.current_step);
            frame.render_widget(top, chunks[2]);
            frame.render_widget(bot, chunks[3]);
        }
        _ => unreachable!(),
    }

    let controls = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(1)
        .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[5]);

    // ref : ◎ ◉ ○ ● ◯ ⬤ ▢ ▬ ▭ ▰ ▱
    let color_l = if btn_l_pressed {
        COLOR_BTN_PRESSED
    } else {
        COLOR_BTN
    };
    let color_r = if btn_r_pressed {
        COLOR_BTN_PRESSED
    } else {
        COLOR_BTN
    };

    let controls_l = Paragraph::new(
        Line::from(vec![Span::styled("◉", Style::default().fg(color_l))])
            .alignment(Alignment::Left),
    );
    let controls_r = Paragraph::new(
        Line::from(vec![Span::styled("◉", Style::default().fg(color_r))])
            .alignment(Alignment::Right),
    );

    frame.render_widget(controls_l, controls[0]);
    frame.render_widget(controls_r, controls[1]);
}

fn bool_step_spans(
    steps: &[bool; STEPS],
    current: usize,
    range: std::ops::Range<usize>,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let last = range.end - 1;

    for i in range {
        let active = steps[i];
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
        if i < last {
            spans.push(Span::raw(" "));
        }
    }
    spans
}

fn bool_half_lines(
    steps: &[bool; STEPS],
    current: usize,
) -> (Paragraph<'static>, Paragraph<'static>) {
    let top = Paragraph::new(Line::from(bool_step_spans(steps, current, 0..HALF)).centered());
    let bot = Paragraph::new(Line::from(bool_step_spans(steps, current, HALF..STEPS)).centered());
    (top, bot)
}

fn hihat_step_spans(
    steps: &[Option<HihatVoice>; STEPS],
    current: usize,
    range: std::ops::Range<usize>,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let last = range.end - 1;

    for i in range {
        let step = &steps[i];
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
        if i < last {
            spans.push(Span::raw(" "));
        }
    }
    spans
}

fn hihat_half_lines(
    steps: &[Option<HihatVoice>; STEPS],
    current: usize,
) -> (Paragraph<'static>, Paragraph<'static>) {
    let top = Paragraph::new(Line::from(hihat_step_spans(steps, current, 0..HALF)).centered());
    let bot = Paragraph::new(Line::from(hihat_step_spans(steps, current, HALF..STEPS)).centered());
    (top, bot)
}
