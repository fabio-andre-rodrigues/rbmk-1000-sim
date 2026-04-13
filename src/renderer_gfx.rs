use crate::config::*;
use crate::grid::{CellState, WaterState};
use crate::neutron::NeutronSpeed;
use crate::renderer::{InputEvent, Renderer};
use crate::simulation::Simulation;
use macroquad::prelude::*;
use std::error::Error;

const HEADER_H: f32 = 30.0;
const DASH_H: f32 = 340.0;
const GRID_PX_W: f32 = GRID_COLS as f32 * CELL_SIZE;
const GRID_PX_H: f32 = GRID_ROWS as f32 * CELL_SIZE;
const WIN_W: f32 = GRID_PX_W + 20.0;
const WIN_H: f32 = HEADER_H + GRID_PX_H + DASH_H + 20.0;

pub struct GfxRenderer;

impl GfxRenderer {
    pub fn new() -> Self {
        GfxRenderer
    }
}

impl Renderer for GfxRenderer {
    fn init(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn render(&mut self, sim: &Simulation) -> Result<(), Box<dyn Error>> {
        clear_background(Color::new(0.05, 0.05, 0.08, 1.0));

        // Header
        draw_text("RBMK-1000 REACTOR SIMULATION", 10.0, 20.0, 20.0, GREEN);
        let elapsed = sim.stats.elapsed_time;
        let time_str = format!(
            "T+{}m {:02}s (t={:.0})",
            (elapsed / 60.0) as u32,
            (elapsed % 60.0) as u32,
            elapsed
        );
        draw_text(&time_str, GRID_PX_W - 80.0, 20.0, 18.0, WHITE);

        // Status badge
        let (status, status_color) = get_status(sim);
        let badge_x = GRID_PX_W / 2.0 - 40.0;
        draw_rectangle(badge_x, 5.0, 100.0, 20.0, status_color);
        draw_text(status, badge_x + 8.0, 20.0, 18.0, BLACK);

        // Grid
        render_grid(sim);
        render_neutrons(&sim.neutrons);

        // Dashboard
        let dash_y = HEADER_H + GRID_PX_H + 5.0;
        draw_rectangle(0.0, dash_y, screen_width(), DASH_H, Color::new(0.08, 0.08, 0.12, 1.0));
        draw_line(0.0, dash_y, screen_width(), dash_y, 2.0, Color::new(0.2, 0.2, 0.3, 1.0));

        render_dashboard(sim, dash_y);

        // Scenario message
        if let Some(ref msg) = sim.scenario_message {
            let msg_y = HEADER_H + GRID_PX_H - 22.0;
            draw_rectangle(0.0, msg_y, GRID_PX_W, 22.0, Color::new(0.1, 0.1, 0.0, 0.85));
            draw_text(msg, 8.0, msg_y + 16.0, 16.0, YELLOW);
        }

        // Legend overlay (toggled with L)
        if sim.stats.show_legend {
            render_legend_panel();
        }

        // Status bar: Auto/Speed + Controls
        let bar_y = screen_height() - 20.0;
        draw_rectangle(0.0, bar_y - 4.0, screen_width(), 24.0, Color::new(0.06, 0.06, 0.1, 1.0));

        let auto_color = if sim.stats.auto_control { GREEN } else { RED };
        draw_circle(10.0, bar_y + 7.0, 4.0, auto_color);
        draw_text(
            if sim.stats.auto_control { "AUTO" } else { "MANUAL" },
            20.0, bar_y + 12.0, 14.0, auto_color,
        );
        draw_text(
            &format!("Speed: {:.1}x", sim.stats.sim_speed),
            85.0, bar_y + 12.0, 14.0, WHITE,
        );

        draw_text(
            "Space=Pause  S=SCRAM  R=Reset  Up/Dn=Rods  Lt/Rt=Coolant  A=Auto  +/-=Speed  N=Neutrons  L=Legend  Q=Quit",
            185.0, bar_y + 12.0, 12.0, Color::new(0.4, 0.4, 0.5, 1.0),
        );

        Ok(())
    }

    fn handle_input(&mut self) -> Result<Option<InputEvent>, Box<dyn Error>> {
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            return Ok(Some(InputEvent::Quit));
        }
        if is_key_pressed(KeyCode::Space) { return Ok(Some(InputEvent::Pause)); }
        if is_key_pressed(KeyCode::S) { return Ok(Some(InputEvent::Scram)); }
        if is_key_pressed(KeyCode::R) { return Ok(Some(InputEvent::Reset)); }
        if is_key_pressed(KeyCode::Up) { return Ok(Some(InputEvent::RodsUp)); }
        if is_key_pressed(KeyCode::Down) { return Ok(Some(InputEvent::RodsDown)); }
        if is_key_pressed(KeyCode::A) { return Ok(Some(InputEvent::ToggleAutoControl)); }
        if is_key_pressed(KeyCode::Equal) { return Ok(Some(InputEvent::SpeedUp)); }
        if is_key_pressed(KeyCode::Minus) { return Ok(Some(InputEvent::SpeedDown)); }
        if is_key_pressed(KeyCode::N) { return Ok(Some(InputEvent::InjectNeutrons)); }
        if is_key_pressed(KeyCode::L) { return Ok(Some(InputEvent::ToggleLegend)); }
        if is_key_pressed(KeyCode::Right) { return Ok(Some(InputEvent::CoolantFlowUp)); }
        if is_key_pressed(KeyCode::Left) { return Ok(Some(InputEvent::CoolantFlowDown)); }
        Ok(None)
    }

    fn cleanup(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

fn get_status(sim: &Simulation) -> (&'static str, Color) {
    let power_pct = sim.stats.activations_per_sec / TARGET_ACTIVATIONS_PER_SEC * 100.0;
    if sim.stats.is_scrammed {
        ("SCRAM", RED)
    } else if sim.stats.is_paused {
        ("PAUSED", YELLOW)
    } else if power_pct > 200.0 {
        ("CRITICAL", RED)
    } else if power_pct > 125.0 {
        ("DANGER", ORANGE)
    } else {
        ("NORMAL", GREEN)
    }
}

fn render_grid(sim: &Simulation) {
    // Build graphite displacer tip map
    let mut is_graphite_tip = [[false; GRID_COLS]; GRID_ROWS];
    for i in 0..NUM_ABSORPTION_RODS {
        let rod_col = sim.rods.rod_columns[i];
        let depth = sim.rods.positions[i] as usize;
        let tip_start = depth.saturating_sub(DISPLACER_TIP_ROWS);
        for row in tip_start..depth {
            is_graphite_tip[row][rod_col] = true;
        }
    }

    for row in 0..GRID_ROWS {
        for col in 0..GRID_COLS {
            let x = col as f32 * CELL_SIZE;
            let y = row as f32 * CELL_SIZE + HEADER_H;

            let bg = match sim.grid.water[row][col] {
                WaterState::Cool { .. } => Color::new(0.0, 0.0, 0.35, 1.0),
                WaterState::Warm { .. } => Color::new(0.4, 0.0, 0.0, 1.0),
                WaterState::Vapor { .. } | WaterState::None => BLACK,
            };
            draw_rectangle(x, y, CELL_SIZE, CELL_SIZE, bg);

            // Base cell color — graphite displacer tips are blue,
            // boron carbide absorber body is dark red.
            let color = match sim.grid.cells[row][col] {
                CellState::Uranium235Active => GREEN,
                CellState::Uranium235Inactive { .. } => DARKGRAY,
                CellState::Xenon135 { .. } => GREEN,
                CellState::ModeratorRod => DARKBLUE,
                CellState::AbsorptionRod => {
                    if is_graphite_tip[row][col] {
                        Color::new(0.25, 0.4, 0.85, 1.0) // graphite tip
                    } else {
                        MAROON // boron absorber
                    }
                }
                CellState::Empty => Color::new(0.0, 0.0, 0.0, 0.0),
            };

            if !matches!(sim.grid.cells[row][col], CellState::Empty) {
                draw_rectangle(x + 1.0, y + 1.0, CELL_SIZE - 2.0, CELL_SIZE - 2.0, color);
            }

            // Gas overlays (semi-transparent, drawn on top of fuel)
            let iodine = sim.grid.iodine[row][col];
            if iodine > 0.02 {
                // Iodine-135 gas: orange tint, opacity scales with concentration
                let alpha = (iodine / 0.4).min(0.7);
                draw_rectangle(
                    x + 1.0, y + 1.0, CELL_SIZE - 2.0, CELL_SIZE - 2.0,
                    Color::new(1.0, 0.6, 0.0, alpha),
                );
            }
            if let CellState::Xenon135 { decay_timer } = sim.grid.cells[row][col] {
                // Xenon-135 gas: purple tint, stronger when fresh
                // (more time remaining = higher concentration)
                let alpha = (decay_timer / XENON_DECAY_SECS).clamp(0.2, 0.8);
                draw_rectangle(
                    x + 1.0, y + 1.0, CELL_SIZE - 2.0, CELL_SIZE - 2.0,
                    Color::new(0.7, 0.0, 0.9, alpha),
                );
            }
        }
    }
}

fn render_neutrons(neutrons: &[crate::neutron::Neutron]) {
    for n in neutrons.iter().filter(|n| n.alive) {
        let color = match n.speed {
            NeutronSpeed::Fast => WHITE,
            NeutronSpeed::Thermal => Color::new(0.5, 0.5, 0.5, 1.0),
        };
        draw_circle(n.x, n.y + HEADER_H, 2.0, color);
    }
}

fn render_legend_panel() {
    let panel_w = 240.0;
    let panel_h = 410.0;
    let px = screen_width() - panel_w - 10.0;
    let py = HEADER_H + 10.0;
    let swatch = 18.0;
    let row_h = 22.0;
    let label_color = Color::new(0.8, 0.8, 0.85, 1.0);
    let section_color = Color::new(0.5, 0.5, 0.6, 1.0);

    // Background
    draw_rectangle(px, py, panel_w, panel_h, Color::new(0.08, 0.08, 0.12, 0.92));
    draw_rectangle_lines(px, py, panel_w, panel_h, 1.0, Color::new(0.3, 0.3, 0.4, 1.0));
    draw_text("LEGEND (L)", px + 8.0, py + 18.0, 16.0, WHITE);

    let mut y = py + 32.0;

    // --- Cell Types ---
    draw_text("CELL TYPES", px + 8.0, y, 13.0, section_color);
    y += row_h;

    // U-235 active
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, GREEN);
    draw_text("U-235 (active)", px + 36.0, y, 14.0, label_color);
    y += row_h;

    // U-235 spent
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, DARKGRAY);
    draw_text("U-235 (spent)", px + 36.0, y, 14.0, label_color);
    y += row_h;

    // Graphite moderator
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, DARKBLUE);
    draw_text("Graphite moderator", px + 36.0, y, 14.0, label_color);
    y += row_h;

    // Boron absorber
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, MAROON);
    draw_text("Boron absorber", px + 36.0, y, 14.0, label_color);
    y += row_h;

    // Graphite displacer tip
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, Color::new(0.25, 0.4, 0.85, 1.0));
    draw_text("Graphite tip", px + 36.0, y, 14.0, label_color);
    y += row_h + 6.0;

    // --- Water / Coolant ---
    draw_text("WATER / COOLANT", px + 8.0, y, 13.0, section_color);
    y += row_h;

    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, Color::new(0.0, 0.0, 0.35, 1.0));
    draw_text("Cool water", px + 36.0, y, 14.0, label_color);
    y += row_h;

    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, Color::new(0.4, 0.0, 0.0, 1.0));
    draw_text("Warm water", px + 36.0, y, 14.0, label_color);
    y += row_h;

    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, BLACK);
    draw_rectangle_lines(px + 10.0, y - 12.0, swatch, swatch - 4.0, 1.0, GRAY);
    draw_text("Steam (void)", px + 36.0, y, 14.0, label_color);
    y += row_h + 6.0;

    // --- Gas Overlays ---
    draw_text("GAS OVERLAYS", px + 8.0, y, 13.0, section_color);
    y += row_h;

    // Xenon-135
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, Color::new(0.7, 0.0, 0.9, 0.7));
    draw_text("Xenon-135", px + 36.0, y, 14.0, label_color);
    y += row_h;

    // Iodine-135
    draw_rectangle(px + 10.0, y - 12.0, swatch, swatch - 4.0, Color::new(1.0, 0.6, 0.0, 0.7));
    draw_text("Iodine-135", px + 36.0, y, 14.0, label_color);
    y += row_h + 6.0;

    // --- Neutrons ---
    draw_text("NEUTRONS", px + 8.0, y, 13.0, section_color);
    y += row_h;

    draw_circle(px + 18.0, y - 5.0, 3.0, WHITE);
    draw_text("Fast (2 MeV)", px + 36.0, y, 14.0, label_color);
    y += row_h;

    draw_circle(px + 18.0, y - 5.0, 3.0, Color::new(0.5, 0.5, 0.5, 1.0));
    draw_text("Thermal (0.025 eV)", px + 36.0, y, 14.0, label_color);
}

// === Dashboard ===

fn render_dashboard(sim: &Simulation, dash_y: f32) {
    let power_pct = sim.stats.activations_per_sec / TARGET_ACTIVATIONS_PER_SEC * 100.0;

    // === Row 1: Arc gauges + Rod indicators ===
    let row1_y = dash_y + 15.0;
    let gauge_r = 50.0;
    let gauge_cy = row1_y + gauge_r + 8.0;
    let gauge_spacing = 160.0;

    // Pressure gauge
    draw_arc_gauge(
        80.0, gauge_cy, gauge_r,
        sim.stats.pressure_mpa, 0.0, 21.0,
        "PRESSURE", &format!("{:.1} MPa", sim.stats.pressure_mpa),
        pressure_color(sim.stats.pressure_mpa),
    );

    // Power gauge
    draw_arc_gauge(
        80.0 + gauge_spacing, gauge_cy, gauge_r,
        power_pct.min(400.0), 0.0, 400.0,
        "POWER", &format!("{:.0}%  {:.0} MW", power_pct, sim.stats.power_mw),
        power_color(power_pct),
    );

    // Coolant flow gauge
    draw_arc_gauge(
        80.0 + gauge_spacing * 2.0, gauge_cy, gauge_r,
        sim.stats.coolant_flow * 100.0, 0.0, 150.0,
        "COOLANT FLOW", &format!("{:.0}%", sim.stats.coolant_flow * 100.0),
        coolant_color(sim.stats.coolant_flow),
    );

    // Rod position indicators (5 vertical bars)
    let rods_x = 80.0 + gauge_spacing * 3.0 - 20.0;
    draw_text("CONTROL RODS", rods_x, row1_y + 4.0, 14.0, GRAY);
    let bar_h = 80.0;
    let bar_w = 16.0;
    let bar_gap = 8.0;
    for i in 0..NUM_ABSORPTION_RODS {
        let bx = rods_x + i as f32 * (bar_w + bar_gap);
        let by = row1_y + 14.0;
        let fill = sim.rods.positions[i] / GRID_ROWS as f32;

        draw_rectangle(bx, by, bar_w, bar_h, Color::new(0.15, 0.15, 0.2, 1.0));
        let fill_h = fill * bar_h;
        let rod_color = if sim.rods.displacer_active[i] {
            YELLOW
        } else {
            RED
        };
        draw_rectangle(bx, by, bar_w, fill_h, rod_color);
        draw_rectangle_lines(bx, by, bar_w, bar_h, 1.0, GRAY);
        draw_text(
            &format!("{:.0}", sim.rods.positions[i]),
            bx, by + bar_h + 14.0, 12.0, GRAY,
        );
    }

    // === Divider line ===
    let div_y = dash_y + gauge_r * 2.0 + 65.0;
    draw_line(10.0, div_y, GRID_PX_W - 10.0, div_y, 1.0, Color::new(0.2, 0.2, 0.3, 1.0));

    // === Row 2: Stat bars (left half) + Text stats (right half) ===
    let row2_y = div_y + 12.0;
    let half_w = GRID_PX_W / 2.0;
    let bar_w_stat = half_w - 120.0;
    let bar_h_stat = 16.0;
    let row_h = 28.0;

    // --- Left column: bars ---
    let total_cells = (GRID_ROWS * GRID_COLS) as f32;
    let water_total = (sim.stats.water_cool_count
        + sim.stats.water_warm_count
        + sim.stats.water_vapor_count)
        .max(1) as f32;
    let void_frac = sim.stats.water_vapor_count as f32 / water_total;

    draw_stat_bar(
        10.0, row2_y, bar_w_stat, bar_h_stat,
        "NEUTRONS", sim.stats.neutron_count as f32, MAX_NEUTRONS as f32,
        &format!("{}", sim.stats.neutron_count),
        Color::new(0.3, 0.6, 1.0, 1.0),
    );
    draw_stat_bar(
        10.0, row2_y + row_h, bar_w_stat, bar_h_stat,
        "Xe-135", sim.stats.xenon_count as f32, total_cells * 0.3,
        &format!("{} ({:.0}%)", sim.stats.xenon_count, sim.stats.xenon_count as f32 / total_cells * 100.0),
        MAGENTA,
    );
    draw_stat_bar(
        10.0, row2_y + row_h * 2.0, bar_w_stat, bar_h_stat,
        "I-135", sim.stats.iodine_total, 300.0,
        &format!("{:.0}", sim.stats.iodine_total),
        Color::new(0.8, 0.4, 0.8, 1.0),
    );
    draw_stat_bar(
        10.0, row2_y + row_h * 3.0, bar_w_stat, bar_h_stat,
        "VOID", sim.stats.water_vapor_count as f32, water_total,
        &format!("{:.0}%", void_frac * 100.0),
        if void_frac > 0.5 { RED } else { Color::new(0.6, 0.3, 0.1, 1.0) },
    );

    // --- Right column: text readouts ---
    let col2_x = half_w + 40.0;
    let label_color = Color::new(0.5, 0.5, 0.6, 1.0);
    let val_x = col2_x + 110.0;

    draw_text("ACT/s", col2_x, row2_y + 12.0, 14.0, label_color);
    draw_text(
        &format!("{:.0} / {:.0}", sim.stats.activations_per_sec, TARGET_ACTIVATIONS_PER_SEC),
        val_x, row2_y + 12.0, 16.0, WHITE,
    );

    draw_text("FAST / THERM", col2_x, row2_y + row_h + 12.0, 14.0, label_color);
    draw_text(
        &format!("{} / {}", sim.stats.fast_count, sim.stats.thermal_count),
        val_x, row2_y + row_h + 12.0, 16.0, Color::new(0.3, 0.6, 1.0, 1.0),
    );

    draw_text("PRECURSORS", col2_x, row2_y + row_h * 2.0 + 12.0, 14.0, label_color);
    draw_text(
        &format!("{:.1}", sim.delayed_precursor_pool),
        val_x, row2_y + row_h * 2.0 + 12.0, 16.0, Color::new(1.0, 0.8, 0.3, 1.0),
    );

    draw_text("WATER", col2_x, row2_y + row_h * 3.0 + 12.0, 14.0, label_color);
    draw_text(
        &format!("C:{} W:{} V:{}", sim.stats.water_cool_count, sim.stats.water_warm_count, sim.stats.water_vapor_count),
        val_x, row2_y + row_h * 3.0 + 12.0, 14.0, SKYBLUE,
    );

    // SCRAM blinking warning
    if sim.rods.scram_active {
        let blink = (sim.stats.elapsed_time * 4.0).sin() > 0.0;
        if blink {
            let scram_y = row2_y + row_h * 4.0 + 8.0;
            draw_rectangle(col2_x - 5.0, scram_y - 14.0, 160.0, 22.0, Color::new(0.3, 0.0, 0.0, 1.0));
            draw_text("!! SCRAM ACTIVE !!", col2_x, scram_y, 18.0, RED);
        }
    }
}

fn draw_arc_gauge(
    cx: f32, cy: f32, radius: f32,
    value: f32, min_val: f32, max_val: f32,
    label: &str, value_text: &str,
    color: Color,
) {
    let start_angle = std::f32::consts::PI * 0.75;
    let end_angle = std::f32::consts::PI * 2.25;
    let total_arc = end_angle - start_angle;

    // Background arc
    let segments = 40;
    for i in 0..segments {
        let t0 = i as f32 / segments as f32;
        let t1 = (i + 1) as f32 / segments as f32;
        let a0 = start_angle + t0 * total_arc;
        let a1 = start_angle + t1 * total_arc;
        draw_line(
            cx + a0.cos() * radius, cy + a0.sin() * radius,
            cx + a1.cos() * radius, cy + a1.sin() * radius,
            3.0, Color::new(0.2, 0.2, 0.25, 1.0),
        );
    }

    // Filled arc
    let frac = ((value - min_val) / (max_val - min_val)).clamp(0.0, 1.0);
    let fill_segments = (frac * segments as f32) as usize;
    for i in 0..fill_segments {
        let t0 = i as f32 / segments as f32;
        let t1 = (i + 1) as f32 / segments as f32;
        let a0 = start_angle + t0 * total_arc;
        let a1 = start_angle + t1 * total_arc;
        draw_line(
            cx + a0.cos() * radius, cy + a0.sin() * radius,
            cx + a1.cos() * radius, cy + a1.sin() * radius,
            5.0, color,
        );
    }

    // Needle
    let needle_angle = start_angle + frac * total_arc;
    let inner_r = radius * 0.3;
    draw_line(
        cx + needle_angle.cos() * inner_r, cy + needle_angle.sin() * inner_r,
        cx + needle_angle.cos() * (radius - 5.0), cy + needle_angle.sin() * (radius - 5.0),
        2.0, WHITE,
    );
    draw_circle(cx, cy, 4.0, WHITE);

    // Label
    draw_text(label, cx - 22.0, cy + radius + 18.0, 14.0, GRAY);
    // Value
    draw_text(value_text, cx - 28.0, cy + 8.0, 16.0, color);

    // Tick marks (min and max)
    let a_min = start_angle;
    draw_text(
        &format!("{:.0}", min_val),
        cx + a_min.cos() * (radius + 8.0) - 8.0,
        cy + a_min.sin() * (radius + 8.0),
        11.0, GRAY,
    );
    let a_max = end_angle;
    draw_text(
        &format!("{:.0}", max_val),
        cx + a_max.cos() * (radius + 8.0) - 8.0,
        cy + a_max.sin() * (radius + 8.0),
        11.0, GRAY,
    );
}

fn draw_stat_bar(
    x: f32, y: f32, w: f32, h: f32,
    label: &str, value: f32, max_val: f32,
    value_text: &str, color: Color,
) {
    let frac = (value / max_val).clamp(0.0, 1.0);
    let label_w = 90.0;
    let bar_x = x + label_w;

    // Label (left of bar)
    draw_text(label, x, y + h - 3.0, 13.0, GRAY);

    // Bar background
    draw_rectangle(bar_x, y, w, h, Color::new(0.15, 0.15, 0.2, 1.0));
    // Bar fill
    draw_rectangle(bar_x, y, w * frac, h, color);
    // Bar border
    draw_rectangle_lines(bar_x, y, w, h, 1.0, Color::new(0.3, 0.3, 0.4, 1.0));
    // Value text (right of bar)
    draw_text(value_text, bar_x + w + 8.0, y + h - 3.0, 14.0, WHITE);
}

fn pressure_color(p: f32) -> Color {
    if p > 14.0 { RED }
    else if p > 10.0 { ORANGE }
    else if p > 8.0 { YELLOW }
    else { Color::new(0.0, 0.8, 0.8, 1.0) }
}

fn power_color(pct: f32) -> Color {
    if pct > 200.0 { RED }
    else if pct > 125.0 { ORANGE }
    else if pct > 80.0 { GREEN }
    else { Color::new(0.3, 0.6, 1.0, 1.0) }
}

fn coolant_color(flow: f32) -> Color {
    if flow < 0.2 { RED }
    else if flow < 0.5 { ORANGE }
    else { Color::new(0.0, 0.7, 1.0, 1.0) }
}

pub fn macroquad_conf() -> Conf {
    Conf {
        window_title: "RBMK-1000 Reactor Simulation".to_string(),
        window_width: WIN_W as i32,
        window_height: WIN_H as i32,
        window_resizable: true,
        ..Default::default()
    }
}
