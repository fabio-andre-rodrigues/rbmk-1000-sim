use crate::config::*;
use crate::simulation::Simulation;
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub initial: InitialConditions,
    pub events: Vec<ScenarioEvent>,
}

#[derive(Debug, Deserialize)]
pub struct InitialConditions {
    #[serde(default = "default_seed_neutrons")]
    pub seed_neutrons: usize,
    #[serde(default = "default_true")]
    pub auto_control: bool,
    #[serde(default)]
    pub rod_positions: Option<[f32; NUM_ABSORPTION_RODS]>,
    #[serde(default = "default_sim_speed")]
    pub sim_speed: f32,
}

impl Default for InitialConditions {
    fn default() -> Self {
        InitialConditions {
            seed_neutrons: INITIAL_SEED_NEUTRONS,
            auto_control: true,
            rod_positions: None,
            sim_speed: 1.0,
        }
    }
}

fn default_seed_neutrons() -> usize {
    INITIAL_SEED_NEUTRONS
}
fn default_true() -> bool {
    true
}
fn default_sim_speed() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct ScenarioEvent {
    /// Simulation time in seconds when this event fires
    pub time: f32,
    /// The action to perform
    pub action: EventAction,
    /// Optional message to display on HUD
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventAction {
    /// Set all rod positions to a uniform depth
    SetRods { depth: f32 },
    /// Set individual rod positions [rod0..rod4]
    SetRodsIndividual { positions: [f32; NUM_ABSORPTION_RODS] },
    /// Enable or disable automatic rod control
    SetAutoControl { enabled: bool },
    /// Trigger SCRAM (emergency full rod insertion)
    Scram,
    /// Inject fast neutrons at random fuel positions
    InjectNeutrons { count: usize },
    /// Change simulation speed multiplier
    SetSpeed { speed: f32 },
    /// Force all water to a specific state
    SetWater { state: WaterAction },
    /// Force water in a column range to a specific state
    SetWaterRegion {
        col_start: usize,
        col_end: usize,
        state: WaterAction,
    },
    /// Set coolant flow rate (1.0 = normal, 0.0 = no flow)
    /// Simulates pump trip, turbine rundown, valve closure
    SetCoolantFlow { flow: f32 },
    /// Pause or unpause
    SetPaused { paused: bool },
    /// Display a message (no simulation effect)
    Message,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum WaterAction {
    Cool,
    Warm,
    Vapor,
}

/// Tracks scenario playback state
pub struct ScenarioRunner {
    pub scenario: Scenario,
    pub next_event_index: usize,
    pub active_message: Option<String>,
    pub message_timer: f32,
}

impl ScenarioRunner {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let scenario: Scenario = serde_json::from_str(&content)?;
        Ok(ScenarioRunner {
            scenario,
            next_event_index: 0,
            active_message: None,
            message_timer: 0.0,
        })
    }

    pub fn apply_initial(&self, sim: &mut Simulation) {
        let init = &self.scenario.initial;
        sim.seed_neutrons(init.seed_neutrons);
        sim.stats.auto_control = init.auto_control;
        sim.stats.sim_speed = init.sim_speed;
        if let Some(positions) = init.rod_positions {
            sim.rods.positions = positions;
        }
    }

    pub fn update(&mut self, sim: &mut Simulation, dt: f32) {
        // Fade message using real elapsed time
        if self.active_message.is_some() {
            self.message_timer -= dt * sim.stats.sim_speed;
            if self.message_timer <= 0.0 {
                self.active_message = None;
                sim.scenario_message = None;
            }
        }

        let sim_time = sim.stats.elapsed_time;

        while self.next_event_index < self.scenario.events.len() {
            let event = &self.scenario.events[self.next_event_index];
            if sim_time < event.time {
                break;
            }

            // Set message if present — stored on both runner and sim
            // so any renderer can read it
            if let Some(ref msg) = event.message {
                self.active_message = Some(msg.clone());
                sim.scenario_message = Some(msg.clone());
                self.message_timer = 8.0;
            }

            // Execute action
            match &event.action {
                EventAction::SetRods { depth } => {
                    sim.rods.set_targets(*depth);
                    sim.stats.is_scrammed = false;
                }
                EventAction::SetRodsIndividual { positions } => {
                    sim.rods.set_individual_targets(*positions);
                    sim.stats.is_scrammed = false;
                }
                EventAction::SetAutoControl { enabled } => {
                    sim.stats.auto_control = *enabled;
                    if *enabled {
                        sim.rods.target_active = false;
                    }
                }
                EventAction::Scram => {
                    sim.rods.scram();
                    sim.stats.is_scrammed = true;
                    sim.stats.auto_control = false;
                }
                EventAction::InjectNeutrons { count } => {
                    sim.seed_neutrons(*count);
                }
                EventAction::SetSpeed { speed } => {
                    sim.stats.sim_speed = speed.clamp(0.25, 8.0);
                }
                EventAction::SetWater { state } => {
                    apply_water_state(sim, 0, GRID_COLS, *state);
                }
                EventAction::SetWaterRegion {
                    col_start,
                    col_end,
                    state,
                } => {
                    apply_water_state(sim, *col_start, *col_end, *state);
                }
                EventAction::SetCoolantFlow { flow } => {
                    sim.stats.coolant_flow = flow.clamp(0.0, 2.0);
                }
                EventAction::SetPaused { paused } => {
                    sim.stats.is_paused = *paused;
                }
                EventAction::Message => {
                    // message-only, already handled above
                }
            }

            self.next_event_index += 1;
        }
    }

}

fn apply_water_state(sim: &mut Simulation, col_start: usize, col_end: usize, action: WaterAction) {
    use crate::grid::{Grid, WaterState};

    let col_end = col_end.min(GRID_COLS);
    for row in 0..GRID_ROWS {
        for col in col_start..col_end {
            if Grid::is_absorption_rod_column(col) {
                continue; // don't put water on rod columns when rods inserted
            }
            if sim.grid.cells[row][col] == crate::grid::CellState::ModeratorRod {
                continue;
            }
            match action {
                WaterAction::Cool => {
                    sim.grid.water[row][col] = WaterState::Cool { neutron_hits: 0 };
                }
                WaterAction::Warm => {
                    sim.grid.water[row][col] = WaterState::Warm {
                        neutron_hits: WATER_HEAT_THRESHOLD,
                        cool_timer: WARM_COOL_SECS,
                    };
                }
                WaterAction::Vapor => {
                    sim.grid.water[row][col] = WaterState::Vapor {
                        return_timer: VAPOR_RETURN_SECS,
                    };
                }
            }
        }
    }
}
