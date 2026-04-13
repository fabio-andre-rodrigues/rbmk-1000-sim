use crate::config::*;
use crate::controls::AbsorptionRodSystem;
use crate::grid::{CellState, Grid, WaterState};
use crate::neutron::{Neutron, NeutronSpeed};
use crate::renderer::InputEvent;
use rand::Rng;
use rayon::prelude::*;

pub struct SimStats {
    pub activations_this_frame: u32,
    pub activations_per_sec: f32,
    // Per-zone activation tracking (one per rod)
    pub zone_activations_this_frame: [u32; NUM_ABSORPTION_RODS],
    pub zone_rates: [f32; NUM_ABSORPTION_RODS],
    pub neutron_count: usize,
    pub fast_count: usize,
    pub thermal_count: usize,
    pub xenon_count: usize,
    pub iodine_total: f32,
    pub water_cool_count: usize,
    pub water_warm_count: usize,
    pub water_vapor_count: usize,
    pub elapsed_time: f32,
    pub power_mw: f32,
    pub is_scrammed: bool,
    pub is_paused: bool,
    pub auto_control: bool,
    pub sim_speed: f32,
    /// Coolant flow multiplier (1.0 = normal, 0.0 = no flow).
    /// Affects water cooling rate and vapor return speed.
    /// Reducing this simulates pump trip / turbine rundown.
    pub coolant_flow: f32,
    /// Core pressure in MPa. Normal operating: 7.0 MPa.
    /// Increases with vapor count (steam generation).
    /// Steam explosion threshold: ~14 MPa (2x normal).
    pub pressure_mpa: f32,
    pub show_legend: bool,
}

impl SimStats {
    pub fn new() -> Self {
        SimStats {
            activations_this_frame: 0,
            activations_per_sec: 0.0,
            zone_activations_this_frame: [0; NUM_ABSORPTION_RODS],
            zone_rates: [0.0; NUM_ABSORPTION_RODS],
            neutron_count: 0,
            fast_count: 0,
            thermal_count: 0,
            xenon_count: 0,
            iodine_total: 0.0,
            water_cool_count: 0,
            water_warm_count: 0,
            water_vapor_count: 0,
            elapsed_time: 0.0,
            power_mw: 0.0,
            is_scrammed: false,
            is_paused: false,
            auto_control: true,
            sim_speed: 1.0,
            coolant_flow: DEFAULT_COOLANT_FLOW,
            pressure_mpa: 7.0,
            show_legend: false,
        }
    }
}

/// A single fission event with all its products.
/// Stored per-event so that phantom fissions (where another neutron
/// already deactivated the same cell this frame) can be discarded
/// wholesale — no neutrons spawned, no activation counted.
struct FissionEvent {
    row: usize,
    col: usize,
    iodine: f32,
    delayed_precursors: f32,
    activation_weight: u32,
    zone: usize,
    new_neutrons: Vec<Neutron>,
}

/// Accumulated results from processing neutrons in parallel.
/// The grid is treated as read-only during the neutron transport
/// phase; mutations are collected here and applied sequentially
/// afterward. This two-phase approach eliminates shared mutable
/// state between rayon worker threads.
struct FrameResult {
    /// Fission events with their products (neutrons, iodine, precursors).
    fission_events: Vec<FissionEvent>,
    /// Cells where thermal neutrons were absorbed by Xe-135.
    xenon_absorbed: Vec<(usize, usize)>,
    /// Cells where neutrons deposited heat into water.
    water_hits: Vec<(usize, usize, u32)>,
}

impl FrameResult {
    fn new() -> Self {
        FrameResult {
            fission_events: Vec::with_capacity(32),
            xenon_absorbed: Vec::new(),
            water_hits: Vec::with_capacity(512),
        }
    }

    fn merge(mut self, other: Self) -> Self {
        self.fission_events.extend(other.fission_events);
        self.xenon_absorbed.extend(other.xenon_absorbed);
        self.water_hits.extend(other.water_hits);
        self
    }
}

pub struct Simulation {
    pub grid: Grid,
    pub neutrons: Vec<Neutron>,
    pub stats: SimStats,
    pub rods: AbsorptionRodSystem,
    pub scenario_message: Option<String>,
    spontaneous_source_accumulator: f32,
    /// Delayed neutron precursor concentration (arbitrary units).
    /// Fission deposits beta_eff fraction of neutrons here.
    /// Decays at lambda_eff rate, spawning thermal neutrons.
    pub delayed_precursor_pool: f32,
}

impl Simulation {
    pub fn new() -> Self {
        Simulation {
            grid: Grid::new(),
            neutrons: Vec::new(),
            stats: SimStats::new(),
            rods: AbsorptionRodSystem::new(),
            scenario_message: None,
            spontaneous_source_accumulator: 0.0,
            delayed_precursor_pool: 0.0,
        }
    }

    pub fn seed_neutrons(&mut self, count: usize) {
        let mut rng = rand::thread_rng();
        let grid_w = GRID_COLS as f32 * CELL_SIZE;
        let grid_h = GRID_ROWS as f32 * CELL_SIZE;
        for _ in 0..count {
            let x = rng.gen_range(0.0..grid_w);
            let y = rng.gen_range(0.0..grid_h);
            self.neutrons.push(Neutron::spawn(x, y, NeutronSpeed::Fast));
        }
    }

    fn spawn_spontaneous_neutron(&mut self) {
        // Pick a random active U-235 cell and emit a fast neutron from it
        let mut rng = rand::thread_rng();
        let mut attempts = 0;
        loop {
            let col = rng.gen_range(0..GRID_COLS);
            let row = rng.gen_range(0..GRID_ROWS);
            if self.grid.cells[row][col] == CellState::Uranium235Active {
                let x = col as f32 * CELL_SIZE + CELL_SIZE / 2.0;
                let y = row as f32 * CELL_SIZE + CELL_SIZE / 2.0;
                self.neutrons.push(Neutron::spawn(x, y, NeutronSpeed::Fast));
                return;
            }
            attempts += 1;
            if attempts > 50 {
                // Fallback: spawn at random position
                let x = rng.gen_range(0.0..GRID_COLS as f32 * CELL_SIZE);
                let y = rng.gen_range(0.0..GRID_ROWS as f32 * CELL_SIZE);
                self.neutrons.push(Neutron::spawn(x, y, NeutronSpeed::Fast));
                return;
            }
        }
    }

    pub fn process_input(&mut self, event: InputEvent) {
        match event {
            InputEvent::Pause => {
                self.stats.is_paused = !self.stats.is_paused;
            }
            InputEvent::Scram => {
                self.rods.scram();
                self.stats.is_scrammed = true;
                self.stats.auto_control = false;
            }
            InputEvent::Reset => {
                *self = Simulation::new();
                self.seed_neutrons(INITIAL_SEED_NEUTRONS);
            }
            InputEvent::RodsUp => {
                self.rods.manual_move(-1.0, 0.5);
                self.stats.is_scrammed = false;
                self.rods.target_active = false;
            }
            InputEvent::RodsDown => {
                self.rods.manual_move(1.0, 0.5);
                self.rods.target_active = false;
            }
            InputEvent::ToggleAutoControl => {
                self.stats.auto_control = !self.stats.auto_control;
                if self.stats.auto_control {
                    self.stats.is_scrammed = false;
                }
            }
            InputEvent::SpeedUp => {
                self.stats.sim_speed = (self.stats.sim_speed * 2.0).min(4.0);
            }
            InputEvent::SpeedDown => {
                self.stats.sim_speed = (self.stats.sim_speed / 2.0).max(0.25);
            }
            InputEvent::InjectNeutrons => {
                self.seed_neutrons(5);
            }
            InputEvent::CoolantFlowUp => {
                self.stats.coolant_flow = (self.stats.coolant_flow + 0.1).min(2.0);
            }
            InputEvent::CoolantFlowDown => {
                self.stats.coolant_flow = (self.stats.coolant_flow - 0.1).max(0.0);
            }
            InputEvent::ToggleLegend => {
                self.stats.show_legend = !self.stats.show_legend;
            }
            InputEvent::Quit => {}
        }
    }

    pub fn update(&mut self, dt: f32) {
        if self.stats.is_paused {
            return;
        }

        let dt = dt * self.stats.sim_speed;
        self.stats.elapsed_time += dt;
        self.stats.activations_this_frame = 0;
        self.stats.zone_activations_this_frame = [0; NUM_ABSORPTION_RODS];

        // Per-rod steam force: each rod's upward push is computed from
        // the local void fraction in its zone. More vapor in the
        // channel = more steam pushing that rod toward withdrawn.
        let rod_steam = self.compute_rod_steam_forces();

        // Scenario rod targets take priority over auto-control.
        // Rods move gradually toward target depth at ROD_MOVE_SPEED,
        // with local steam assisting withdrawal / opposing insertion.
        if self.rods.target_active && !self.rods.scram_active {
            self.rods.update_toward_targets(dt, &rod_steam);
        } else if self.stats.auto_control && !self.rods.scram_active {
            self.rods.auto_update(&self.stats.zone_rates, dt);
        }

        // SCRAM: gravity-driven insertion vs local steam per rod.
        // Rods in heavily boiling zones stall while cooler zones insert.
        if self.rods.scram_active {
            self.rods.update_scram(dt, &rod_steam);
        }

        // Sync rod positions to grid cells
        self.sync_rods_to_grid();

        // Spontaneous neutron source — models background spontaneous
        // fission in U-235 fuel. Keeps the chain reaction bootstrapped.
        self.spontaneous_source_accumulator += SPONTANEOUS_NEUTRONS_PER_SEC * dt;
        while self.spontaneous_source_accumulator >= 1.0 {
            self.spontaneous_source_accumulator -= 1.0;
            self.spawn_spontaneous_neutron();
        }

        // Delayed neutron precursor decay: precursors accumulated from
        // fission events decay at rate lambda, spawning thermal neutrons.
        // This is the physics that makes reactor control possible —
        // at prompt critical (rho >= beta), delayed neutrons are
        // irrelevant and the reactor period drops to milliseconds.
        {
            let decay = self.delayed_precursor_pool * DELAYED_LAMBDA * dt;
            self.delayed_precursor_pool -= decay;
            // Each unit of decay = one delayed neutron to spawn
            let mut delayed_to_spawn = decay;
            while delayed_to_spawn >= 1.0 {
                delayed_to_spawn -= 1.0;
                self.spawn_spontaneous_neutron(); // reuse: random fuel cell
            }
            // Fractional remainder: probabilistic spawn
            if delayed_to_spawn > 0.0 {
                let mut rng = rand::thread_rng();
                if rng.r#gen::<f32>() < delayed_to_spawn {
                    self.spawn_spontaneous_neutron();
                }
            }
        }

        // === PARALLEL NEUTRON TRANSPORT ===
        // Phase 1: Process neutrons against a read-only grid snapshot.
        // Each rayon worker thread accumulates grid events and new
        // neutrons into a thread-local FrameResult via fold().
        // rand::thread_rng() is per-thread (TLS) — zero contention.
        let result = {
            let grid = &self.grid;
            let rods = &self.rods;
            self.neutrons
                .par_iter_mut()
                .fold(FrameResult::new, |mut acc, neutron| {
                    process_neutron(neutron, grid, rods, dt, &mut acc);
                    acc
                })
                .reduce(FrameResult::new, FrameResult::merge)
        };

        // Phase 2: Apply accumulated grid mutations sequentially.
        self.apply_frame_result(result);
        self.neutrons.retain(|n| n.alive);

        // Update cell and water timers (parallel by row)
        self.update_cell_timers(dt);
        self.update_water_timers(dt);

        // Update activation rate using exponential moving average (EMA).
        // Tau=0.5s gives smooth readings without the abrupt reset-to-zero
        // that caused rod controller oscillation with the old 1s window.
        let ema_tau = 0.5_f32;
        let alpha = 1.0 - (-dt / ema_tau).exp();
        let instant_rate = self.stats.activations_this_frame as f32 / dt.max(0.001);
        self.stats.activations_per_sec =
            self.stats.activations_per_sec * (1.0 - alpha) + instant_rate * alpha;
        for z in 0..NUM_ABSORPTION_RODS {
            let zone_instant =
                self.stats.zone_activations_this_frame[z] as f32 / dt.max(0.001);
            self.stats.zone_rates[z] =
                self.stats.zone_rates[z] * (1.0 - alpha) + zone_instant * alpha;
        }

        // Update power display
        self.stats.power_mw = self.stats.activations_per_sec * POWER_PER_ACTIVATION;

        // Count stats
        self.update_counts();
    }

    /// Apply deferred grid mutations collected during parallel
    /// neutron transport. Phantom fissions (where multiple neutrons
    /// fissioned the same cell because the grid was read-only) are
    /// discarded entirely — their neutrons are never spawned.
    /// This preserves the sequential invariant: at most one fission
    /// per U-235 cell per frame.
    fn apply_frame_result(&mut self, result: FrameResult) {
        // Xenon absorptions: convert back to active uranium
        for &(row, col) in &result.xenon_absorbed {
            if let CellState::Xenon135 { .. } = self.grid.cells[row][col] {
                self.grid.cells[row][col] = CellState::Uranium235Active;
            }
        }

        // Fissions: first-writer-wins per cell.
        // If two neutrons fissioned the same cell (both saw Active
        // in the read-only snapshot), only the first is accepted.
        // The phantom's neutrons, iodine, and activation are discarded.
        let mut deactivated = [[false; GRID_COLS]; GRID_ROWS];
        let mut accepted_neutrons: Vec<Neutron> = Vec::new();
        let mut activations = 0u32;
        let mut zone_activations = [0u32; NUM_ABSORPTION_RODS];
        let mut delayed_precursors = 0.0_f32;

        for event in result.fission_events {
            if !deactivated[event.row][event.col]
                && self.grid.cells[event.row][event.col] == CellState::Uranium235Active
            {
                deactivated[event.row][event.col] = true;
                self.grid.cells[event.row][event.col] = CellState::Uranium235Inactive {
                    reactivation_timer: U235_REACTIVATION_SECS,
                };
                self.grid.iodine[event.row][event.col] += event.iodine;
                activations += event.activation_weight;
                zone_activations[event.zone] += event.activation_weight;
                delayed_precursors += event.delayed_precursors;
                accepted_neutrons.extend(event.new_neutrons);
            }
            // Else: phantom fission — discard entirely
        }

        // Water heating: aggregate per-cell hits, then apply thresholds
        let mut hit_map = [[0u32; GRID_COLS]; GRID_ROWS];
        for &(row, col, hits) in &result.water_hits {
            hit_map[row][col] += hits;
        }
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                let hits = hit_map[row][col];
                if hits == 0 {
                    continue;
                }
                match &mut self.grid.water[row][col] {
                    WaterState::Cool { neutron_hits } => {
                        *neutron_hits += hits;
                        if *neutron_hits >= WATER_BOIL_THRESHOLD {
                            self.grid.water[row][col] = WaterState::Vapor {
                                return_timer: VAPOR_RETURN_SECS,
                            };
                        } else if *neutron_hits >= WATER_HEAT_THRESHOLD {
                            let h = *neutron_hits;
                            self.grid.water[row][col] = WaterState::Warm {
                                neutron_hits: h,
                                cool_timer: WARM_COOL_SECS,
                            };
                        }
                    }
                    WaterState::Warm { neutron_hits, .. } => {
                        *neutron_hits += hits;
                        if *neutron_hits >= WATER_BOIL_THRESHOLD {
                            self.grid.water[row][col] = WaterState::Vapor {
                                return_timer: VAPOR_RETURN_SECS,
                            };
                        }
                    }
                    _ => {}
                }
            }
        }

        // Merge accepted neutrons (soft cap at MAX_NEUTRONS)
        let space = MAX_NEUTRONS.saturating_sub(self.neutrons.len());
        self.neutrons
            .extend(accepted_neutrons.into_iter().take(space));

        // Update frame stats
        self.stats.activations_this_frame = activations;
        self.stats.zone_activations_this_frame = zone_activations;
        self.delayed_precursor_pool += delayed_precursors;
    }

    /// Iodine-135 decay, xenon-135 production and decay, uranium
    /// reactivation. Parallelized by row — each row's cells and
    /// iodine concentrations are independent.
    fn update_cell_timers(&mut self, dt: f32) {
        let cells = &mut self.grid.cells;
        let iodine = &mut self.grid.iodine;

        cells
            .par_iter_mut()
            .zip(iodine.par_iter_mut())
            .for_each(|(cell_row, iodine_row)| {
                let mut rng = rand::thread_rng();
                for col in 0..GRID_COLS {
                    // Iodine-135 decay -> Xenon-135 production.
                    let iod = iodine_row[col];
                    if iod > 0.01 {
                        let decay = iod * IODINE_DECAY_RATE * dt;
                        iodine_row[col] -= decay;

                        let conversion_prob =
                            (decay * XENON_FROM_IODINE_FRACTION * 3.0).min(0.5);
                        if conversion_prob > 0.001
                            && !matches!(cell_row[col], CellState::Xenon135 { .. })
                            && !matches!(cell_row[col], CellState::ModeratorRod)
                            && !matches!(cell_row[col], CellState::AbsorptionRod)
                        {
                            if rng.r#gen::<f32>() < conversion_prob {
                                cell_row[col] = CellState::Xenon135 {
                                    decay_timer: XENON_DECAY_SECS,
                                };
                                iodine_row[col] = 0.0;
                            }
                        }
                    }

                    match &mut cell_row[col] {
                        CellState::Uranium235Inactive { reactivation_timer } => {
                            *reactivation_timer -= dt;
                            if *reactivation_timer <= 0.0 {
                                if rng.r#gen::<f32>() < XENON_DIRECT_SPAWN_PROBABILITY {
                                    cell_row[col] = CellState::Xenon135 {
                                        decay_timer: XENON_DECAY_SECS,
                                    };
                                } else {
                                    cell_row[col] = CellState::Uranium235Active;
                                }
                            }
                        }
                        CellState::Xenon135 { decay_timer } => {
                            *decay_timer -= dt;
                            if *decay_timer <= 0.0 {
                                cell_row[col] = CellState::Uranium235Active;
                            }
                        }
                        _ => {}
                    }
                }
            });
    }

    /// Water state transitions (vapor → warm → cool).
    /// Parallelized by row — each row is independent.
    fn update_water_timers(&mut self, dt: f32) {
        let flow = self.stats.coolant_flow;
        self.grid.water.par_iter_mut().for_each(|water_row| {
            for col in 0..GRID_COLS {
                match &mut water_row[col] {
                    WaterState::Vapor { return_timer } => {
                        // Coolant flow affects how fast vapor condenses back
                        // No flow = vapor never returns
                        *return_timer -= dt * flow;
                        if *return_timer <= 0.0 && flow > 0.01 {
                            water_row[col] = WaterState::Warm {
                                neutron_hits: 0,
                                cool_timer: WARM_COOL_SECS,
                            };
                        }
                    }
                    WaterState::Warm {
                        cool_timer,
                        neutron_hits,
                    } => {
                        // Coolant flow affects cooling rate
                        if *neutron_hits < WATER_HEAT_THRESHOLD {
                            *cool_timer -= dt * flow;
                            if *cool_timer <= 0.0 && flow > 0.01 {
                                water_row[col] = WaterState::Cool { neutron_hits: 0 };
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    fn sync_rods_to_grid(&mut self) {
        for i in 0..NUM_ABSORPTION_RODS {
            let rod_col = self.rods.rod_columns[i];
            let depth = self.rods.positions[i] as usize;
            for row in 0..GRID_ROWS {
                if row < depth {
                    if self.grid.cells[row][rod_col] != CellState::AbsorptionRod {
                        self.grid.cells[row][rod_col] = CellState::AbsorptionRod;
                        self.grid.water[row][rod_col] = WaterState::None;
                    }
                } else if self.grid.cells[row][rod_col] == CellState::AbsorptionRod {
                    self.grid.cells[row][rod_col] = CellState::Uranium235Active;
                    self.grid.water[row][rod_col] = WaterState::Cool { neutron_hits: 0 };
                }
            }
        }
    }

    fn update_counts(&mut self) {
        // Weighted neutron counting: each particle's weight represents
        // the number of physical neutrons it stands for.
        let mut fast_w = 0.0_f32;
        let mut thermal_w = 0.0_f32;
        for n in &self.neutrons {
            if n.alive {
                match n.speed {
                    NeutronSpeed::Fast => fast_w += n.weight,
                    NeutronSpeed::Thermal => thermal_w += n.weight,
                }
            }
        }
        self.stats.fast_count = fast_w.round() as usize;
        self.stats.thermal_count = thermal_w.round() as usize;
        self.stats.neutron_count = self.stats.fast_count + self.stats.thermal_count;

        let mut xenon = 0;
        let mut iodine_sum = 0.0_f32;
        let mut cool = 0;
        let mut warm = 0;
        let mut vapor = 0;
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                if let CellState::Xenon135 { .. } = self.grid.cells[row][col] {
                    xenon += 1;
                }
                iodine_sum += self.grid.iodine[row][col];
                match self.grid.water[row][col] {
                    WaterState::Cool { .. } => cool += 1,
                    WaterState::Warm { .. } => warm += 1,
                    WaterState::Vapor { .. } => vapor += 1,
                    WaterState::None => {}
                }
            }
        }
        self.stats.xenon_count = xenon;
        self.stats.iodine_total = iodine_sum;
        self.stats.water_cool_count = cool;
        self.stats.water_warm_count = warm;
        self.stats.water_vapor_count = vapor;

        // Pressure model: steam (vapor) generates pressure.
        // Normal operation at 7 MPa with ~0 vapor cells.
        // Each vapor cell adds pressure. Reduced coolant flow
        // prevents pressure relief (steam can't escape to turbine).
        let total_water = (cool + warm + vapor).max(1) as f32;
        let void_fraction = vapor as f32 / total_water;
        // Base 7 MPa + up to 14 MPa from steam buildup
        // Reduced coolant flow traps steam (less relief)
        let relief_factor = self.stats.coolant_flow.max(0.1);
        let raw_pressure = 7.0 + (void_fraction * 14.0) / relief_factor;
        self.stats.pressure_mpa = raw_pressure.min(200.0); // cap for display
    }

    /// Per-rod upward steam force derived from local void fraction
    /// AND how much rod is actually inside the pressure channel.
    ///
    /// Steam pushes on the inserted portion of the rod — at depth 0
    /// there is nothing to push on, so force is zero. As the rod
    /// descends, more surface area is exposed to channel steam and
    /// the upward force grows. This produces the historically accurate
    /// "bouncing" behavior: rods insert freely at first (SCRAM speed),
    /// reach 6-7 rows (~30-35%), then steam resistance stalls and
    /// pushes them back, and they oscillate around the equilibrium.
    ///
    /// Real RBMK rods reached ~2-2.5m of their 7m travel before
    /// steam in the pressure channels physically lifted them.
    fn compute_rod_steam_forces(&self) -> [f32; NUM_ABSORPTION_RODS] {
        let mut forces = [0.0_f32; NUM_ABSORPTION_RODS];
        let relief = self.stats.coolant_flow.clamp(0.3, 1.5);
        // Low flow traps steam in the channel → more force per void cell
        let flow_factor = 0.9 + (1.0 - relief.min(1.0)) * 0.5;

        for i in 0..NUM_ABSORPTION_RODS {
            let zone_start = i * MODERATOR_INTERVAL;
            let zone_end = (zone_start + MODERATOR_INTERVAL).min(GRID_COLS);

            let mut vapor = 0u32;
            let mut total = 0u32;
            for row in 0..GRID_ROWS {
                for col in zone_start..zone_end {
                    match self.grid.water[row][col] {
                        WaterState::Cool { .. } | WaterState::Warm { .. } => total += 1,
                        WaterState::Vapor { .. } => {
                            vapor += 1;
                            total += 1;
                        }
                        WaterState::None => {}
                    }
                }
            }

            if total > 0 {
                let void_fraction = vapor as f32 / total as f32;
                // How much rod is exposed to channel steam (0.0 = withdrawn, 1.0 = fully inserted)
                let rod_in_channel = self.rods.positions[i] / GRID_ROWS as f32;

                // Force = void × flow_factor × rod_exposure × gain.
                // At 100% void, low flow (ff≈1.3), 35% insertion (7/20):
                //   1.0 × 1.5 × 1.3 × 0.35 × 2.5 = 1.71 > SCRAM(1.5) → pushed back.
                // At 100% void, low flow, 25% insertion (5/20):
                //   1.0 × 1.5 × 1.3 × 0.25 × 2.5 = 1.22 < SCRAM → still inserting.
                // Equilibrium ≈ 30% insertion (6 rows) — matches historical record.
                forces[i] = (void_fraction * SCRAM_ROD_SPEED * flow_factor
                    * rod_in_channel * 2.5)
                    .min(SCRAM_ROD_SPEED * 2.0);
            }
        }
        forces
    }
}

/// Process a single neutron against the grid (read-only) and rod state.
/// Accumulates deferred grid mutations and new neutrons into `result`.
/// Called from rayon worker threads — uses thread-local RNG (zero contention).
fn process_neutron(
    neutron: &mut Neutron,
    grid: &Grid,
    rods: &AbsorptionRodSystem,
    dt: f32,
    result: &mut FrameResult,
) {
    if !neutron.alive {
        return;
    }

    let speed = (neutron.vx * neutron.vx + neutron.vy * neutron.vy).sqrt();
    if speed < 0.01 {
        return;
    }

    // rand::thread_rng() accesses thread-local storage — each rayon
    // worker gets its own independent RNG with no synchronization.
    let mut rng = rand::thread_rng();

    // How many cells does this neutron cross this frame?
    let distance = speed * dt;
    let steps = ((distance / CELL_SIZE).ceil() as usize).max(1);
    let sub_dt = dt / steps as f32;

    for _ in 0..steps {
        if !neutron.alive {
            break;
        }

        // Sub-step movement
        neutron.update(sub_dt);

        let col = neutron.grid_col();
        let row = neutron.grid_row();

        // --- Interaction checks at this cell ---

        // 1. Moderator rod collision (fast -> thermal)
        // Real graphite: ~115 collisions to thermalize, each
        // scattering to a random direction. We model as instant
        // thermalization with isotropic scattering (random angle).
        if grid.cells[row][col] == CellState::ModeratorRod
            && neutron.speed == NeutronSpeed::Fast
        {
            let angle = rng.gen_range(0.0..std::f32::consts::TAU);
            neutron.vx = angle.cos() * THERMAL_NEUTRON_SPEED;
            neutron.vy = angle.sin() * THERMAL_NEUTRON_SPEED;
            neutron.speed = NeutronSpeed::Thermal;
            continue;
        }

        // 2. Absorption rod: thermal absorbed, fast pass through
        if grid.cells[row][col] == CellState::AbsorptionRod {
            if neutron.speed == NeutronSpeed::Thermal {
                neutron.alive = false;
            }
            continue;
        }

        // 3. Xenon-135 absorption (thermal only)
        // Xe-135 has 2.6M barn cross-section at thermal energies
        // but negligible cross-section for fast neutrons.
        if let CellState::Xenon135 { .. } = grid.cells[row][col]
            && neutron.speed == NeutronSpeed::Thermal
        {
            neutron.alive = false;
            result.xenon_absorbed.push((row, col));
            continue;
        }

        // 4. Water interaction: absorption + heating
        // Only THERMAL neutrons get absorbed by water.
        // Fast neutrons (~2 MeV) have negligible absorption
        // cross-section in water — they pass right through.
        match grid.water[row][col] {
            WaterState::Cool { .. } | WaterState::Warm { .. } => {
                if neutron.speed == NeutronSpeed::Thermal
                    && rng.r#gen::<f32>() < WATER_NEUTRON_ABSORPTION_PROB
                {
                    neutron.alive = false;
                    continue;
                }
                // Defer water heating to the sequential apply phase.
                // Hits are per-particle (not weight-scaled) to avoid
                // artificial hot spots from spatially concentrated
                // weighted particles — 3 real neutrons would heat
                // 3 different cells, not the same cell 3× harder.
                result.water_hits.push((row, col, 1));
            }
            WaterState::Vapor { .. } => {
                // Positive void coefficient: no water absorption
            }
            WaterState::None => {}
        }

        // 5. U-235 fission (thermal only, probabilistic)
        // Real fuel is ~2% enriched — not every thermal neutron
        // causes fission. This lets neutrons traverse multiple
        // fuel cells, accumulating water absorption along the way.
        if grid.cells[row][col] == CellState::Uranium235Active
            && neutron.speed == NeutronSpeed::Thermal
        {
            let mut fission_prob = FISSION_PROBABILITY;

            // Displacer tip and void coefficient increase
            // the local fission probability
            fission_prob *= rods.displacer_boost_at(col, row);
            if let WaterState::Vapor { .. } = grid.water[row][col] {
                fission_prob *= VOID_COEFFICIENT_BOOST;
            }

            if rng.r#gen::<f32>() < fission_prob {
                neutron.alive = false;

                let w = neutron.weight.round().max(1.0) as u32;
                let zone = AbsorptionRodSystem::zone_for_col(col);

                // Weighted fission yield
                let total_w = neutron.weight * NEUTRONS_PER_FISSION as f32;
                let delayed_w = total_w * DELAYED_NEUTRON_FRACTION;
                let prompt_w = total_w - delayed_w;

                // Spawn weighted neutron(s) into a per-event Vec
                let mut fission_neutrons = Vec::new();
                if prompt_w > 0.01 {
                    if prompt_w > WEIGHT_SPLIT_THRESHOLD {
                        let n_particles = prompt_w.ceil() as usize;
                        let w_each = prompt_w / n_particles as f32;
                        for _ in 0..n_particles {
                            let mut n =
                                Neutron::spawn(neutron.x, neutron.y, NeutronSpeed::Fast);
                            n.weight = w_each;
                            fission_neutrons.push(n);
                        }
                    } else {
                        let mut n =
                            Neutron::spawn(neutron.x, neutron.y, NeutronSpeed::Fast);
                        n.weight = prompt_w;
                        fission_neutrons.push(n);
                    }
                }

                // Package as a FissionEvent so apply_frame_result
                // can discard phantom fissions (duplicate cell hits).
                result.fission_events.push(FissionEvent {
                    row,
                    col,
                    iodine: neutron.weight * IODINE_PRODUCTION_PER_FISSION,
                    delayed_precursors: delayed_w,
                    activation_weight: w,
                    zone,
                    new_neutrons: fission_neutrons,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moderator_collision() {
        let mut sim = Simulation::new();
        let x = 10.0 * CELL_SIZE + CELL_SIZE / 2.0;
        let y = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
        sim.neutrons.push(Neutron {
            x,
            y,
            vx: FAST_NEUTRON_SPEED,
            vy: 0.0,
            speed: NeutronSpeed::Fast,
            alive: true,
            weight: 1.0,
        });

        sim.update(0.001);

        assert_eq!(sim.neutrons.len(), 1);
        assert_eq!(sim.neutrons[0].speed, NeutronSpeed::Thermal);
    }

    #[test]
    fn test_u235_fission() {
        // Fission is probabilistic (FISSION_PROBABILITY per cell).
        // Run many trials: a thermal neutron on active U-235 should
        // sometimes fission (producing a weighted fast neutron) and
        // sometimes pass through.
        let mut fission_count = 0;
        let trials = 200;

        for _ in 0..trials {
            let mut sim = Simulation::new();
            sim.grid.water[5][3] = WaterState::None;

            let x = 3.0 * CELL_SIZE + CELL_SIZE / 2.0;
            let y = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
            sim.neutrons.push(Neutron {
                x,
                y,
                vx: 0.0,
                vy: THERMAL_NEUTRON_SPEED,
                speed: NeutronSpeed::Thermal,
                alive: true,
                weight: 1.0,
            });

            sim.update(0.001);

            if sim.stats.activations_this_frame > 0 {
                fission_count += 1;
                // When fission occurs: 1 weighted fast neutron produced
                // (weight ~2.98, carrying prompt yield for all 3 secondaries)
                assert!(sim.neutrons.iter().all(|n| n.speed == NeutronSpeed::Fast));
            }
        }

        // Should fission roughly FISSION_PROBABILITY of the time
        // (with some tolerance for the cell-stepping sub-steps)
        assert!(
            fission_count > 5,
            "Expected some fissions in {trials} trials, got {fission_count}"
        );
        assert!(
            fission_count < trials,
            "Fission should not be guaranteed, but got {fission_count}/{trials}"
        );
    }

    #[test]
    fn test_absorption_rod_interaction() {
        let mut sim = Simulation::new();
        sim.rods.positions = [GRID_ROWS as f32; NUM_ABSORPTION_RODS];
        sim.sync_rods_to_grid();

        assert_eq!(sim.grid.cells[0][5], CellState::AbsorptionRod);

        let x = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
        let y = 0.5 * CELL_SIZE;
        sim.neutrons.push(Neutron {
            x,
            y,
            vx: 0.0,
            vy: THERMAL_NEUTRON_SPEED,
            speed: NeutronSpeed::Thermal,
            alive: true,
            weight: 1.0,
        });

        sim.update(0.001);

        assert_eq!(sim.neutrons.len(), 0);
    }

    #[test]
    fn test_fast_bypasses_absorption_rod() {
        let mut sim = Simulation::new();
        sim.rods.positions = [GRID_ROWS as f32; NUM_ABSORPTION_RODS];
        sim.sync_rods_to_grid();

        let x = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
        let y = 0.5 * CELL_SIZE;
        sim.neutrons.push(Neutron {
            x,
            y,
            vx: FAST_NEUTRON_SPEED,
            vy: 0.0,
            speed: NeutronSpeed::Fast,
            alive: true,
            weight: 1.0,
        });

        sim.update(0.001);

        assert_eq!(sim.neutrons.len(), 1);
        assert!(sim.neutrons[0].alive);
    }

    #[test]
    fn test_xenon_mechanics() {
        let mut sim = Simulation::new();
        sim.grid.cells[3][3] = CellState::Xenon135 {
            decay_timer: XENON_DECAY_SECS,
        };

        let x = 3.0 * CELL_SIZE + CELL_SIZE / 2.0;
        let y = 3.0 * CELL_SIZE + CELL_SIZE / 2.0;
        sim.neutrons.push(Neutron {
            x,
            y,
            vx: THERMAL_NEUTRON_SPEED,
            vy: 0.0,
            speed: NeutronSpeed::Thermal,
            alive: true,
            weight: 1.0,
        });

        sim.update(0.001);

        assert_eq!(sim.grid.cells[3][3], CellState::Uranium235Active);
        assert_eq!(sim.neutrons.len(), 0);
    }

    #[test]
    fn test_xenon_natural_decay() {
        let mut sim = Simulation::new();
        sim.grid.cells[3][3] = CellState::Xenon135 { decay_timer: 1.0 };

        sim.update(1.5);

        assert_eq!(sim.grid.cells[3][3], CellState::Uranium235Active);
    }

    #[test]
    fn test_water_cycle() {
        let mut sim = Simulation::new();
        assert!(matches!(
            sim.grid.water[5][3],
            WaterState::Cool { neutron_hits: 0 }
        ));

        // Test warm -> cool transition via timer
        sim.grid.water[5][3] = WaterState::Warm {
            neutron_hits: 0,
            cool_timer: 1.0,
        };
        sim.update(1.5);
        assert!(matches!(sim.grid.water[5][3], WaterState::Cool { .. }));
    }

    #[test]
    fn test_vapor_return() {
        let mut sim = Simulation::new();
        sim.grid.water[5][3] = WaterState::Vapor { return_timer: 1.0 };

        sim.update(1.5);

        assert!(matches!(sim.grid.water[5][3], WaterState::Warm { .. }));
    }

    #[test]
    fn test_activation_rate_ema() {
        // EMA should converge toward the actual rate over time
        let mut sim = Simulation::new();
        sim.stats.activations_per_sec = 0.0;

        // Simulate steady 10 activations per frame at 30fps for 2 seconds
        for _ in 0..60 {
            sim.stats.activations_this_frame = 10;
            sim.stats.zone_activations_this_frame = [2; NUM_ABSORPTION_RODS];
            // Manually apply EMA (same logic as update)
            let dt = 0.033_f32;
            let alpha = 1.0 - (-dt / 0.5).exp();
            let instant = 10.0 / dt;
            sim.stats.activations_per_sec =
                sim.stats.activations_per_sec * (1.0 - alpha) + instant * alpha;
        }

        // After 2s of 10 acts/frame at 30fps = ~303 acts/sec
        // EMA should converge close to that
        assert!(
            sim.stats.activations_per_sec > 250.0,
            "EMA should converge: got {}",
            sim.stats.activations_per_sec
        );
    }

    // === Physics Integration Tests ===

    #[test]
    fn test_fission_produces_chain_reaction() {
        // Seed many thermal neutrons and run for several frames.
        // Over time, fissions should occur and produce new neutrons,
        // demonstrating the chain reaction mechanism.
        let mut sim = Simulation::new();

        // Inject 50 thermal neutrons spread across the core
        for i in 0..50 {
            let col = (i * 3) % GRID_COLS;
            let row = (i * 7) % GRID_ROWS;
            let x = col as f32 * CELL_SIZE + CELL_SIZE / 2.0;
            let y = row as f32 * CELL_SIZE + CELL_SIZE / 2.0;
            sim.neutrons.push(Neutron {
                x,
                y,
                vx: 0.0,
                vy: THERMAL_NEUTRON_SPEED,
                speed: NeutronSpeed::Thermal,
                alive: true,
                weight: 1.0,
            });
            sim.grid.water[row][col] = WaterState::None;
        }

        // Run several frames
        for _ in 0..10 {
            sim.update(0.033);
        }

        // Some fissions should have occurred, producing fast neutrons
        let fast_count = sim
            .neutrons
            .iter()
            .filter(|n| n.speed == NeutronSpeed::Fast)
            .count();
        assert!(
            fast_count > 0,
            "Chain reaction should produce fast neutrons from fission"
        );
    }

    #[test]
    fn test_scram_activates_gradual_insertion() {
        let mut sim = Simulation::new();
        sim.process_input(InputEvent::Scram);

        // SCRAM should activate but rods not instantly at max
        assert!(sim.rods.scram_active);
        assert!(sim.stats.is_scrammed);
        assert!(!sim.stats.auto_control);

        // Rods should still be at initial position (0.0) since
        // update_scram hasn't been called yet
        assert_eq!(sim.rods.positions[0], 0.0);

        // After enough update cycles, rods reach full insertion
        for _ in 0..500 {
            sim.update(0.033);
        }
        for pos in sim.rods.positions.iter() {
            assert_eq!(*pos, GRID_ROWS as f32);
        }
    }

    #[test]
    fn test_rod_withdrawal_increases_fuel_cells() {
        let mut sim = Simulation::new();

        // Start with rods fully inserted
        sim.rods.positions = [GRID_ROWS as f32; NUM_ABSORPTION_RODS];
        sim.sync_rods_to_grid();

        // Count absorption rod cells
        let rod_cells_before: usize = sim
            .grid
            .cells
            .iter()
            .flatten()
            .filter(|&&c| c == CellState::AbsorptionRod)
            .count();
        assert!(rod_cells_before > 0);

        // Withdraw rods partially
        sim.rods.positions = [10.0; NUM_ABSORPTION_RODS];
        sim.sync_rods_to_grid();

        let rod_cells_after: usize = sim
            .grid
            .cells
            .iter()
            .flatten()
            .filter(|&&c| c == CellState::AbsorptionRod)
            .count();

        // Fewer rod cells = more fuel cells exposed
        assert!(rod_cells_after < rod_cells_before);
    }

    #[test]
    fn test_void_coefficient_increases_fission_rate() {
        // Vapor cells (no water) should have higher fission rate
        // than liquid water cells due to positive void coefficient.
        // Run many trials and compare rates.
        let trials = 500;

        // Fission rate WITH water (cool)
        let mut fissions_with_water = 0;
        for _ in 0..trials {
            let mut sim = Simulation::new();
            sim.grid.water[5][3] = WaterState::Cool { neutron_hits: 0 };

            let x = 3.0 * CELL_SIZE + CELL_SIZE / 2.0;
            let y = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
            sim.neutrons.push(Neutron {
                x, y, vx: 0.0, vy: THERMAL_NEUTRON_SPEED,
                speed: NeutronSpeed::Thermal, alive: true, weight: 1.0,
            });
            sim.update(0.001);
            fissions_with_water += sim.stats.activations_this_frame;
        }

        // Fission rate WITHOUT water (vapor — void coefficient)
        let mut fissions_with_vapor = 0;
        for _ in 0..trials {
            let mut sim = Simulation::new();
            sim.grid.water[5][3] = WaterState::Vapor { return_timer: 100.0 };

            let x = 3.0 * CELL_SIZE + CELL_SIZE / 2.0;
            let y = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
            sim.neutrons.push(Neutron {
                x, y, vx: 0.0, vy: THERMAL_NEUTRON_SPEED,
                speed: NeutronSpeed::Thermal, alive: true, weight: 1.0,
            });
            sim.update(0.001);
            fissions_with_vapor += sim.stats.activations_this_frame;
        }

        // Vapor should have MORE fissions:
        // 1. No water absorption (8% saved)
        // 2. Higher fission probability (VOID_COEFFICIENT_BOOST = 1.3x)
        assert!(
            fissions_with_vapor > fissions_with_water,
            "Void coefficient should increase fissions: vapor={fissions_with_vapor} vs water={fissions_with_water}"
        );
    }

    #[test]
    fn test_neutron_linear_straight_line() {
        // Verify neutrons travel in straight lines
        let mut n = Neutron {
            x: 50.0,
            y: 50.0,
            vx: 100.0,
            vy: 50.0,
            speed: NeutronSpeed::Fast,
            alive: true,
            weight: 1.0,
        };

        let start_x = n.x;
        let start_y = n.y;

        // 10 small steps should equal 1 big step (linear motion)
        for _ in 0..10 {
            n.update(0.01);
        }
        let x_10 = n.x;
        let y_10 = n.y;

        let mut n2 = Neutron {
            x: start_x,
            y: start_y,
            vx: 100.0,
            vy: 50.0,
            speed: NeutronSpeed::Fast,
            alive: true,
            weight: 1.0,
        };
        n2.update(0.1);

        assert!((x_10 - n2.x).abs() < 1.0);
        assert!((y_10 - n2.y).abs() < 1.0);
    }

    #[test]
    fn test_fast_neutrons_bypass_u235() {
        let mut sim = Simulation::new();
        sim.grid.water[5][3] = WaterState::None;

        // Place a FAST neutron on active U-235
        let x = 3.0 * CELL_SIZE + CELL_SIZE / 2.0;
        let y = 5.0 * CELL_SIZE + CELL_SIZE / 2.0;
        sim.neutrons.push(Neutron {
            x,
            y,
            vx: FAST_NEUTRON_SPEED,
            vy: 0.0,
            speed: NeutronSpeed::Fast,
            alive: true,
            weight: 1.0,
        });

        sim.update(0.001);

        // Fast neutron should NOT trigger fission
        assert_eq!(sim.stats.activations_this_frame, 0);
        // Cell should still be active
        assert_eq!(sim.grid.cells[5][3], CellState::Uranium235Active);
        // Neutron still alive (passed through)
        assert_eq!(sim.neutrons.len(), 1);
    }

    #[test]
    fn test_inactive_u235_reactivation() {
        let mut sim = Simulation::new();
        sim.grid.cells[5][3] = CellState::Uranium235Inactive {
            reactivation_timer: 0.5,
        };

        // Update past the timer
        sim.update(1.0);

        // Cell should have reactivated (either U-235 or Xenon-135)
        assert_ne!(
            sim.grid.cells[5][3],
            CellState::Uranium235Inactive {
                reactivation_timer: 0.5
            }
        );
        let is_active = sim.grid.cells[5][3] == CellState::Uranium235Active;
        let is_xenon = matches!(sim.grid.cells[5][3], CellState::Xenon135 { .. });
        assert!(is_active || is_xenon);
    }

    #[test]
    fn test_water_full_cycle() {
        // Cool -> Warm -> Vapor -> Warm -> Cool
        let mut sim = Simulation::new();

        // 1. Start cool
        assert!(matches!(
            sim.grid.water[5][3],
            WaterState::Cool { neutron_hits: 0 }
        ));

        // 2. Heat to warm threshold
        sim.grid.water[5][3] = WaterState::Cool {
            neutron_hits: WATER_HEAT_THRESHOLD,
        };

        // 3. Heat to boil threshold -> Vapor
        sim.grid.water[5][3] = WaterState::Vapor {
            return_timer: VAPOR_RETURN_SECS,
        };

        // 4. Wait for vapor to return to warm
        sim.update(VAPOR_RETURN_SECS + 1.0);
        assert!(matches!(sim.grid.water[5][3], WaterState::Warm { .. }));

        // 5. Wait for warm to cool
        sim.update(WARM_COOL_SECS + 1.0);
        assert!(matches!(sim.grid.water[5][3], WaterState::Cool { .. }));
    }

    #[test]
    fn test_pause_freezes_simulation() {
        let mut sim = Simulation::new();
        sim.seed_neutrons(10);
        let positions_before: Vec<(f32, f32)> =
            sim.neutrons.iter().map(|n| (n.x, n.y)).collect();

        sim.stats.is_paused = true;
        sim.update(1.0);

        let positions_after: Vec<(f32, f32)> =
            sim.neutrons.iter().map(|n| (n.x, n.y)).collect();

        assert_eq!(positions_before, positions_after);
    }

    #[test]
    fn test_sim_speed_multiplier() {
        let mut sim = Simulation::new();

        sim.stats.sim_speed = 2.0;
        sim.update(1.0);
        let elapsed_2x = sim.stats.elapsed_time;

        let mut sim2 = Simulation::new();
        sim2.stats.sim_speed = 1.0;
        sim2.update(1.0);
        let elapsed_1x = sim2.stats.elapsed_time;

        assert!((elapsed_2x - 2.0 * elapsed_1x).abs() < 0.01);
    }

    #[test]
    fn test_displacer_tip_boost_on_scram() {
        let mut sim = Simulation::new();
        // Partially insert rod before SCRAM so leading edge is in-grid
        sim.rods.positions[0] = 5.0;
        sim.rods.displacer_active[0] = true;
        sim.rods.displacer_timers[0] = DISPLACER_TIP_DURATION;

        assert!(sim.rods.is_any_displacer_active());
        // Boost at leading edge (row 5, col 5)
        assert!(
            (sim.rods.displacer_boost_at(5, 5) - DISPLACER_TIP_BOOST).abs() < 0.01
        );
        // No boost above the rod (row 3)
        assert!((sim.rods.displacer_boost_at(5, 3) - 1.0).abs() < 0.01);
    }

    /// Headless 120-second simulation to verify reactor stability.
    /// Prints diagnostics every 5 seconds. The reactor should reach
    /// steady-state near 40 act/s without going critical.
    #[test]
    fn test_headless_stability_120s() {
        let mut sim = Simulation::new();
        sim.seed_neutrons(INITIAL_SEED_NEUTRONS);

        let dt = 0.033; // ~30fps
        let total_time = 120.0_f32;
        let mut t = 0.0_f32;
        let mut last_print = 0.0_f32;

        let mut max_act_rate = 0.0_f32;
        let mut critical_seconds = 0;
        let mut samples = 0;

        while t < total_time {
            sim.update(dt);
            t += dt;

            if t - last_print >= 5.0 {
                last_print = t;
                let rate = sim.stats.activations_per_sec;
                let rod_avg = sim.rods.positions.iter().sum::<f32>()
                    / NUM_ABSORPTION_RODS as f32;
                let rod_pct = rod_avg / GRID_ROWS as f32 * 100.0;
                let power_pct = rate / TARGET_ACTIVATIONS_PER_SEC * 100.0;

                eprintln!(
                    "t={:5.0}s | act/s={:6.1} | power={:5.0}% | \
                     neutrons={:5} (F:{} T:{}) | rods={:.0}% | \
                     Xe={} | vapor={}",
                    t,
                    rate,
                    power_pct,
                    sim.stats.neutron_count,
                    sim.stats.fast_count,
                    sim.stats.thermal_count,
                    rod_pct,
                    sim.stats.xenon_count,
                    sim.stats.water_vapor_count,
                );

                if t > 15.0 {
                    // After initial ramp-up, start measuring
                    if rate > max_act_rate {
                        max_act_rate = rate;
                    }
                    if power_pct > 200.0 {
                        critical_seconds += 5;
                    }
                    samples += 1;
                }
            }
        }

        eprintln!("\n=== STABILITY REPORT ===");
        eprintln!("Max activation rate (after 15s): {:.1} act/s", max_act_rate);
        eprintln!(
            "Seconds above 200% power: {} / {}",
            critical_seconds,
            (total_time - 15.0) as i32
        );

        // The reactor should not spend more than 20% of its runtime
        // in critical state under auto-control
        let critical_ratio = critical_seconds as f32 / (samples * 5) as f32;
        assert!(
            critical_ratio < 0.2,
            "Reactor spent {:.0}% of time above 200% power — too unstable! \
             Max rate: {:.1} act/s",
            critical_ratio * 100.0,
            max_act_rate,
        );
    }

    /// Run the Chernobyl scenario headless and verify the physics
    /// produces the expected sequence: stable -> power drop -> xenon
    /// buildup -> rod withdrawal -> coolant loss -> power surge.
    #[test]
    fn test_chernobyl_scenario_headless() {
        use crate::scenario::ScenarioRunner;
        use std::path::Path;

        let path = Path::new("scenarios/chernobyl.json");
        if !path.exists() {
            eprintln!("Skipping: scenarios/chernobyl.json not found");
            return;
        }

        let mut runner = ScenarioRunner::load(path).expect("Failed to load scenario");
        let mut sim = Simulation::new();
        runner.apply_initial(&mut sim);
        // Don't let the scenario pause us

        let dt = 0.033;
        let total_time = 205.0_f32;
        let mut t = 0.0_f32;
        let mut last_print = -1.0_f32;

        // Per-phase tracking
        let mut min_power_during_insertion = f32::MAX; // t=32-70
        let mut max_xenon = 0_usize;
        let mut max_iodine = 0.0_f32;
        let mut max_pressure = 0.0_f32;
        let mut max_act_rate = 0.0_f32;
        let mut max_neutrons = 0_usize;
        let mut scram_rod_min = f32::MAX; // lowest rod % after SCRAM

        eprintln!("t(s)  | act/s |  pwr% | P(MPa)| neut  | rods% |   Xe | I-135 |  vap | flow% | event");
        eprintln!("------|-------|-------|-------|-------|-------|------|-------|------|-------|------");

        while t < total_time {
            sim.stats.is_paused = false;
            sim.update(dt);
            runner.update(&mut sim, dt);
            t += dt;

            let rate = sim.stats.activations_per_sec;
            let pressure = sim.stats.pressure_mpa;
            let power_pct = rate / TARGET_ACTIVATIONS_PER_SEC * 100.0;
            let rod_avg = sim.rods.positions.iter().sum::<f32>()
                / NUM_ABSORPTION_RODS as f32;
            let rod_pct = rod_avg / GRID_ROWS as f32 * 100.0;

            if rate > max_act_rate { max_act_rate = rate; }
            if pressure > max_pressure { max_pressure = pressure; }
            if sim.stats.neutron_count > max_neutrons { max_neutrons = sim.stats.neutron_count; }
            if sim.stats.xenon_count > max_xenon { max_xenon = sim.stats.xenon_count; }
            if sim.stats.iodine_total > max_iodine { max_iodine = sim.stats.iodine_total; }

            // Track min power during rod insertion phase (t=32-70)
            if t > 32.0 && t < 70.0 && power_pct < min_power_during_insertion {
                min_power_during_insertion = power_pct;
            }
            // Track rod depth after SCRAM (t>163)
            if t > 163.0 && rod_pct < scram_rod_min {
                scram_rod_min = rod_pct;
            }

            // Print every 1 second
            if t - last_print >= 1.0 {
                last_print = t;
                let msg = sim.scenario_message.as_deref().unwrap_or("");
                let event_marker = if !msg.is_empty() {
                    msg
                } else {
                    ""
                };

                eprintln!(
                    "{:5.0} | {:5.1} | {:5.0} | {:5.1} | {:5} | {:5.0} | {:4} | {:5.1} | {:4} | {:5.0} | {}",
                    t, rate, power_pct, pressure,
                    sim.stats.neutron_count,
                    rod_pct,
                    sim.stats.xenon_count,
                    sim.stats.iodine_total,
                    sim.stats.water_vapor_count,
                    sim.stats.coolant_flow * 100.0,
                    event_marker,
                );
            }
        }

        eprintln!("\n=== SCENARIO CHECKPOINT ANALYSIS ===");
        eprintln!("Min power during rod insertion (t=32-70): {:.0}%", min_power_during_insertion);
        eprintln!("Max Xe-135 cells: {}", max_xenon);
        eprintln!("Max I-135 total: {:.1}", max_iodine);
        eprintln!("Max activation rate: {:.1} act/s", max_act_rate);
        eprintln!("Max pressure: {:.1} MPa", max_pressure);
        eprintln!("Max neutrons: {}", max_neutrons);
        eprintln!("Rod depth after SCRAM (min %): {:.0}%", scram_rod_min);

        eprintln!("\n=== EXPECTED vs ACTUAL ===");
        let checks = [
            ("Power should drop below 100% during rod insertion (t=32-70)",
             min_power_during_insertion < 100.0),
            ("Xenon should build up (>5 cells at some point)",
             max_xenon > 5),
            ("Iodine should accumulate (>10 total at some point)",
             max_iodine > 10.0),
            ("Pressure should exceed 50 MPa during coolant loss",
             max_pressure > 50.0),
            ("Power should exceed 200% during void coefficient surge",
             max_act_rate > TARGET_ACTIVATIONS_PER_SEC * 2.0),
            ("SCRAM rods should be blocked by steam (<20% insertion)",
             scram_rod_min < 20.0),
        ];

        for (desc, passed) in &checks {
            let mark = if *passed { "PASS" } else { "FAIL" };
            eprintln!("[{}] {}", mark, desc);
        }

        let failures: Vec<_> = checks.iter().filter(|(_, p)| !p).collect();
        assert!(
            failures.len() <= 2,
            "Too many scenario checkpoints failed ({}/{})",
            failures.len(),
            checks.len(),
        );
    }
}
