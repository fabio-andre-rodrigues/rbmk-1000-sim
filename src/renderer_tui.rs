use crate::config::*;
use crate::grid::{CellState, WaterState};
use crate::neutron::NeutronSpeed;
use crate::renderer::{InputEvent, Renderer};
use crate::simulation::Simulation;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use std::error::Error;
use std::io::{self, Stdout};
use std::time::Duration;

pub struct TuiRenderer {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TuiRenderer {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(TuiRenderer { terminal })
    }

    pub fn render_title(&mut self) -> Result<(), Box<dyn Error>> {
        self.terminal.draw(|f| {
            let area = f.area();
            let block = Block::default().borders(Borders::ALL);
            let inner = block.inner(area);
            f.render_widget(block, area);

            let title_lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "    RBMK-1000 NUCLEAR REACTOR SIMULATION",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "    Reaktor Bolshoy Moshchnosti Kanalnyy",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "    \"High-Power Channel Reactor\"",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from("    Specifications:"),
                Line::from(Span::styled(
                    "    Thermal Power:  3200 MW",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    "    Electrical:     1000 MW",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    "    Moderator:      Graphite",
                    Style::default().fg(Color::Blue),
                )),
                Line::from(Span::styled(
                    "    Coolant:        Light water",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(Span::styled(
                    "    Fuel:           Enriched UO2 (U-235)",
                    Style::default().fg(Color::Green),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "    CAUTION: Positive void coefficient of reactivity",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "    Loss of coolant INCREASES reactor power",
                    Style::default().fg(Color::Red),
                )),
                Line::from(""),
                Line::from("    This simulation demonstrates the physics behind the"),
                Line::from("    RBMK-1000 reactor design, including the mechanisms"),
                Line::from("    that contributed to the Chernobyl disaster of 1986."),
                Line::from(""),
                Line::from(Span::styled(
                    "    Press ENTER to start simulation...",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "    Press Q to quit",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let paragraph = Paragraph::new(title_lines);
            f.render_widget(paragraph, inner);
        })?;
        Ok(())
    }

    pub fn wait_for_start(&mut self) -> Result<bool, Box<dyn Error>> {
        loop {
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Enter => return Ok(true),
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn render_with_message(
        &mut self,
        sim: &Simulation,
        message: Option<&str>,
    ) -> Result<(), Box<dyn Error>> {
        let msg = message.map(|s| s.to_string());
        self.terminal.draw(|f| {
            draw_ui(f, sim, msg.as_deref());
        })?;
        Ok(())
    }

    pub fn render_scenario_title(
        &mut self,
        scenario: &crate::scenario::Scenario,
    ) -> Result<(), Box<dyn Error>> {
        let name = scenario.name.clone();
        let desc = scenario.description.clone();
        self.terminal.draw(|f| {
            let area = f.area();
            let block = Block::default().borders(Borders::ALL);
            let inner = block.inner(area);
            f.render_widget(block, area);

            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("    SCENARIO: {}", name),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
            ];

            // Word-wrap description at ~70 chars
            for chunk in desc.as_bytes().chunks(70) {
                let s = String::from_utf8_lossy(chunk);
                lines.push(Line::from(format!("    {}", s.trim())));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "    Press ENTER to start scenario...",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "    Press Q to quit",
                Style::default().fg(Color::DarkGray),
            )));

            let paragraph = Paragraph::new(lines);
            f.render_widget(paragraph, inner);
        })?;
        Ok(())
    }
}

impl Renderer for TuiRenderer {
    fn init(&mut self) -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        self.terminal.clear()?;
        Ok(())
    }

    fn render(&mut self, sim: &Simulation) -> Result<(), Box<dyn Error>> {
        self.terminal.draw(|f| {
            draw_ui(f, sim, None);
        })?;
        Ok(())
    }

    fn handle_input(&mut self) -> Result<Option<InputEvent>, Box<dyn Error>> {
        if event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key) => return Ok(map_key(key)),
                Event::Resize(_, _) => {
                    // Terminal resized — clear and force full redraw
                    self.terminal.clear()?;
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn cleanup(&mut self) -> Result<(), Box<dyn Error>> {
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        Ok(())
    }
}

fn map_key(key: KeyEvent) -> Option<InputEvent> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(InputEvent::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(InputEvent::Quit)
        }
        KeyCode::Char(' ') => Some(InputEvent::Pause),
        KeyCode::Char('s') | KeyCode::Char('S') => Some(InputEvent::Scram),
        KeyCode::Char('r') | KeyCode::Char('R') => Some(InputEvent::Reset),
        KeyCode::Up => Some(InputEvent::RodsUp),
        KeyCode::Down => Some(InputEvent::RodsDown),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(InputEvent::ToggleAutoControl),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(InputEvent::SpeedUp),
        KeyCode::Char('-') | KeyCode::Char('_') => Some(InputEvent::SpeedDown),
        KeyCode::Char('n') | KeyCode::Char('N') => Some(InputEvent::InjectNeutrons),
        KeyCode::Right => Some(InputEvent::CoolantFlowUp),
        KeyCode::Left => Some(InputEvent::CoolantFlowDown),
        _ => None,
    }
}

fn draw_ui(f: &mut Frame, sim: &Simulation, scenario_msg: Option<&str>) {
    let term_w = f.area().width;
    let term_h = f.area().height;

    // Calculate cell width: use 2 chars per cell if terminal is wide enough
    let min_hud_width = 36_u16;
    let available_for_grid = term_w.saturating_sub(min_hud_width + 2);
    let cell_w: u16 = if available_for_grid >= (GRID_COLS as u16) * 2 + 2 {
        2
    } else {
        1
    };
    let grid_panel_w = (GRID_COLS as u16) * cell_w + 2; // +2 for borders

    let msg_height = if scenario_msg.is_some() { 3_u16 } else { 0 };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(msg_height),
        ])
        .split(f.area());

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(grid_panel_w.min(term_w.saturating_sub(min_hud_width))),
            Constraint::Min(min_hud_width),
        ])
        .split(outer[0]);

    draw_grid(f, chunks[0], sim, cell_w, term_h);
    draw_hud(f, chunks[1], sim);

    // Scenario event message banner at bottom
    if let Some(msg) = scenario_msg {
        let msg_block = Block::default()
            .borders(Borders::ALL)
            .title(" Scenario ")
            .style(Style::default().fg(Color::Yellow));
        let msg_para = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )))
        .block(msg_block);
        f.render_widget(msg_para, outer[1]);
    }
}

fn draw_grid(f: &mut Frame, area: Rect, sim: &Simulation, cell_w: u16, _term_h: u16) {
    let block = Block::default().borders(Borders::ALL).title(" Reactor Core ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build a neutron density map for overlay
    let mut neutron_map = [[0u16; GRID_COLS]; GRID_ROWS];
    let mut neutron_speed_map = [[NeutronSpeed::Fast; GRID_COLS]; GRID_ROWS];
    for n in &sim.neutrons {
        if n.alive {
            let col = n.grid_col();
            let row = n.grid_row();
            if row < GRID_ROWS && col < GRID_COLS {
                neutron_map[row][col] += 1;
                neutron_speed_map[row][col] = n.speed;
            }
        }
    }

    let visible_rows = inner.height as usize;
    let visible_cols = (inner.width as usize) / (cell_w as usize);

    let mut lines: Vec<Line> = Vec::new();

    for row in 0..GRID_ROWS.min(visible_rows) {
        let mut spans: Vec<Span> = Vec::new();

        for col in 0..GRID_COLS.min(visible_cols) {
            let cell = sim.grid.cells[row][col];
            let water = sim.grid.water[row][col];
            let n_count = neutron_map[row][col];

            // Determine background color from water state
            let bg = match water {
                WaterState::Cool { .. } => Color::Rgb(0, 0, 80),
                WaterState::Warm { .. } => Color::Rgb(100, 0, 0),
                WaterState::Vapor { .. } => Color::Black,
                WaterState::None => Color::Black,
            };

            // Determine character, foreground, and effective background
            let (ch, fg, cell_bg) = if n_count > 0 {
                let (nch, nfg) = if n_count > 9 {
                    ('*', Color::Yellow)
                } else if n_count > 1 {
                    (char::from_digit(n_count as u32, 10).unwrap_or('*'), Color::White)
                } else {
                    match neutron_speed_map[row][col] {
                        NeutronSpeed::Fast => ('\u{00B7}', Color::White),
                        NeutronSpeed::Thermal => ('\u{00B7}', Color::DarkGray),
                    }
                };
                (nch, nfg, bg)
            } else {
                let iodine = sim.grid.iodine[row][col];

                let (base_ch, base_fg) = match cell {
                    CellState::Uranium235Active => ('\u{2588}', Color::Green),
                    CellState::Uranium235Inactive { .. } => ('\u{2591}', Color::DarkGray),
                    CellState::Xenon135 { .. } => ('\u{2588}', Color::Green),
                    CellState::ModeratorRod => ('\u{2551}', Color::Blue),
                    CellState::AbsorptionRod => ('\u{2593}', Color::Red),
                    CellState::Empty => (' ', Color::Black),
                };

                // Gas overlay tints the background
                let gas_bg = if let CellState::Xenon135 { .. } = cell {
                    // Xenon: purple tint, stronger = more time left
                    Color::Rgb(100, 0, 130)
                } else if iodine > 0.02 {
                    // Iodine: orange tint, stronger with concentration
                    let t = (iodine / 0.4).min(1.0);
                    let r = (60.0 + 140.0 * t) as u8;
                    let g = (30.0 + 40.0 * t) as u8;
                    Color::Rgb(r, g, 0)
                } else {
                    bg
                };

                (base_ch, base_fg, gas_bg)
            };

            // Render cell_w characters per grid cell
            let cell_str: String = std::iter::repeat(ch).take(cell_w as usize).collect();
            spans.push(Span::styled(
                cell_str,
                Style::default().fg(fg).bg(cell_bg),
            ));
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn draw_hud(f: &mut Frame, area: Rect, sim: &Simulation) {
    let block = Block::default().borders(Borders::ALL).title(" RBMK-1000 HUD ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Power gauge
            Constraint::Length(13), // Stats
            Constraint::Length(12), // Legend
            Constraint::Min(1),    // Controls
        ])
        .split(inner);

    // Power gauge
    let power_pct = (sim.stats.activations_per_sec / TARGET_ACTIVATIONS_PER_SEC * 100.0)
        .min(300.0)
        .max(0.0);
    let power_color = if power_pct > 200.0 {
        Color::Red
    } else if power_pct > 125.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    let gauge = Gauge::default()
        .block(Block::default().title(" Power ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(power_color).add_modifier(Modifier::BOLD))
        .percent((power_pct.min(100.0)) as u16)
        .label(format!(
            "{:.0} MW ({:.0}%)",
            sim.stats.power_mw, power_pct
        ));
    f.render_widget(gauge, chunks[0]);

    // Stats
    let status = if sim.stats.is_scrammed {
        Span::styled("SCRAM", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    } else if sim.stats.is_paused {
        Span::styled("PAUSED", Style::default().fg(Color::Yellow))
    } else if power_pct > 200.0 {
        Span::styled("CRITICAL", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    } else if power_pct > 125.0 {
        Span::styled("DANGER", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else if sim.stats.xenon_count as f32 / (GRID_ROWS * GRID_COLS) as f32 > 0.3 {
        Span::styled("XENON POISONING", Style::default().fg(Color::Magenta))
    } else {
        Span::styled("NORMAL", Style::default().fg(Color::Green))
    };

    let rod_avg = sim.rods.positions.iter().sum::<f32>() / NUM_ABSORPTION_RODS as f32;
    let rod_pct = rod_avg / GRID_ROWS as f32 * 100.0;

    let elapsed = sim.stats.elapsed_time;
    let mins = (elapsed / 60.0) as u32;
    let secs = (elapsed % 60.0) as u32;

    let auto_str = if sim.stats.auto_control { "ON" } else { "OFF" };

    let stats_lines = vec![
        Line::from(vec![Span::raw("Status: "), status]),
        Line::from(format!(
            "Act/s: {:.0} (target: {:.0})",
            sim.stats.activations_per_sec, TARGET_ACTIVATIONS_PER_SEC
        )),
        Line::from(format!(
            "Neutrons: {} (F:{} T:{})",
            sim.stats.neutron_count, sim.stats.fast_count, sim.stats.thermal_count
        )),
        Line::from(format!("Rods: {:.0}% inserted", rod_pct)),
        Line::from(format!(
            "Auto-ctrl: {}  Speed: {:.1}x",
            auto_str, sim.stats.sim_speed
        )),
        Line::from(format!(
            "Xe-135: {} cells ({:.1}%)",
            sim.stats.xenon_count,
            sim.stats.xenon_count as f32 / (GRID_ROWS * GRID_COLS) as f32 * 100.0
        )),
        Line::from(format!(
            "Water: C:{} W:{} V:{}",
            sim.stats.water_cool_count, sim.stats.water_warm_count, sim.stats.water_vapor_count
        )),
        {
            let p = sim.stats.pressure_mpa;
            let p_color = if p > 12.0 {
                Color::Red
            } else if p > 9.0 {
                Color::Yellow
            } else {
                Color::Cyan
            };
            Line::from(vec![
                Span::raw("Pressure: "),
                Span::styled(
                    format!("{:.1} MPa", p),
                    Style::default().fg(p_color),
                ),
                Span::raw(format!(" (norm: 7.0) Flow: {:.0}%", sim.stats.coolant_flow * 100.0)),
            ])
        },
        Line::from(format!("Time: {}m {:02}s", mins, secs)),
    ];
    let stats_para = Paragraph::new(stats_lines)
        .block(Block::default().title(" Statistics ").borders(Borders::ALL));
    f.render_widget(stats_para, chunks[1]);

    // Legend
    let legend_lines = vec![
        Line::from(vec![
            Span::styled("\u{2588}", Style::default().fg(Color::Green)),
            Span::raw(" U-235 (active)"),
        ]),
        Line::from(vec![
            Span::styled("\u{2591}", Style::default().fg(Color::DarkGray)),
            Span::raw(" U-235 (spent)"),
        ]),
        Line::from(vec![
            Span::styled("\u{2588}", Style::default().fg(Color::Rgb(255, 160, 0))),
            Span::raw(" Iodine-135 (decays to Xe)"),
        ]),
        Line::from(vec![
            Span::styled("\u{2588}", Style::default().fg(Color::Magenta)),
            Span::raw(" Xenon-135"),
        ]),
        Line::from(vec![
            Span::styled("\u{2551}", Style::default().fg(Color::Blue)),
            Span::raw(" Moderator (graphite)"),
        ]),
        Line::from(vec![
            Span::styled("\u{2593}", Style::default().fg(Color::Red)),
            Span::raw(" Absorption rod"),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(Color::Rgb(0, 0, 80))),
            Span::raw(" Cool water"),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default().bg(Color::Rgb(100, 0, 0))),
            Span::raw(" Warm water"),
        ]),
        Line::from(vec![
            Span::styled("\u{00B7}", Style::default().fg(Color::White)),
            Span::raw(" Fast neutron  "),
            Span::styled("\u{00B7}", Style::default().fg(Color::DarkGray)),
            Span::raw(" Thermal"),
        ]),
    ];
    let legend_para = Paragraph::new(legend_lines)
        .block(Block::default().title(" Legend ").borders(Borders::ALL));
    f.render_widget(legend_para, chunks[2]);

    // Controls
    let controls_lines = vec![
        Line::from("Space=Pause  S=SCRAM  R=Reset"),
        Line::from("Up/Down=Rods  Left/Right=Coolant"),
        Line::from("A=AutoCtrl  +/-=Speed  N=Neutrons"),
        Line::from("Q=Quit"),
    ];
    let controls_para = Paragraph::new(controls_lines)
        .block(Block::default().title(" Controls ").borders(Borders::ALL));
    f.render_widget(controls_para, chunks[3]);
}
