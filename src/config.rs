// === Grid Layout ===
pub const GRID_COLS: usize = 50;
pub const GRID_ROWS: usize = 20;
pub const CELL_SIZE: f32 = 16.0;
pub const MODERATOR_INTERVAL: usize = 10; // cols 10, 20, 30, 40
pub const NUM_ABSORPTION_RODS: usize = 5; // cols 5, 15, 25, 35, 45

// === Neutron Physics ===
pub const FAST_NEUTRON_SPEED: f32 = 300.0; // px/s
pub const THERMAL_NEUTRON_SPEED: f32 = 80.0; // px/s
pub const NEUTRONS_PER_FISSION: usize = 3;

// === Delayed Neutrons ===
// At the time of the accident, significant fuel burnup had reduced
// effective U-235 enrichment to ~1.3% while breeding fissile Pu-239
// and Pu-241. Plutonium isotopes have a much lower delayed neutron
// fraction (~0.21%) than U-235 (~0.65%). The weighted average for
// the mixed fuel gives beta_eff ≈ 0.005 (0.50%).
// Lambda_eff = 0.08 s^-1 (weighted average of 6 precursor groups,
// corresponding to ~8s average precursor half-life).
// Lower beta means prompt supercritical is reached at lower
// reactivity insertion — making the displacer tip effect more
// dangerous than it would be with fresh fuel.
pub const DELAYED_NEUTRON_FRACTION: f32 = 0.0050;
pub const DELAYED_LAMBDA: f32 = 0.08; // s^-1 (precursor decay rate)

// === Fission & Cross-Sections ===
// Fresh RBMK fuel is ~2% enriched UO2, but by the time of the
// accident significant burnup had reduced effective enrichment to
// ~1.3% (with Pu-239/241 contributing to the fissile inventory).
// Not every thermal neutron hitting a fuel cell causes fission —
// the macroscopic fission cross-section per cell width gives a
// probability per traversal. At 20%, a thermal neutron travels
// ~5 fuel cells on average before fission, accumulating water
// absorption along the way.
pub const FISSION_PROBABILITY: f32 = 0.20;
pub const WATER_NEUTRON_ABSORPTION_PROB: f32 = 0.08;

// === Spontaneous Neutron Source ===
// Real U-235 has spontaneous fission rate ~0.0003 n/s/g.
// With ~190 tonnes of UO2 in an RBMK core, there is a
// continuous background neutron source. We model this as
// random fast neutron spawns on active fuel cells to keep
// the chain reaction bootstrapped and prevent die-out.
pub const SPONTANEOUS_NEUTRONS_PER_SEC: f32 = 8.0;
pub const INITIAL_SEED_NEUTRONS: usize = 20;
pub const MAX_NEUTRONS: usize = 5000; // performance cap

// === U-235 Reactivation ===
pub const U235_REACTIVATION_SECS: f32 = 10.0;

// === Absorption Rod Control ===
pub const TARGET_ACTIVATIONS_PER_SEC: f32 = 40.0;
pub const ROD_MOVE_SPEED: f32 = 2.0; // rows/sec (faster response)
// SCRAM: real RBMK rods took 18-21s for full insertion.
// At SCRAM speed of 1.5 rows/s, 20 rows takes ~13s.
// The gradual insertion means the displacer tip traverses the
// core over several seconds — the key Chernobyl mechanism.
pub const SCRAM_ROD_SPEED: f32 = 1.5; // rows/sec during SCRAM

// === Iodine-135 / Xenon-135 Dynamics ===
// Real: I-135 produced by 6.3% of fissions, half-life 6.6h.
// I-135 decays into Xe-135 (half-life 9.2h, sigma_a = 2.6M barns).
// After shutdown, Xe-135 peaks ~11h later ("iodine pit").
// Simulation: compressed timescales for visibility.
pub const IODINE_PRODUCTION_PER_FISSION: f32 = 0.3;
// Faster decay = iodine converts to xenon more visibly.
// Real half-life 6.6h compressed to ~5s (lambda = ln2/5 = 0.14)
pub const IODINE_DECAY_RATE: f32 = 0.14;
pub const XENON_FROM_IODINE_FRACTION: f32 = 0.95;
pub const XENON_DIRECT_SPAWN_PROBABILITY: f32 = 0.02;
pub const XENON_DECAY_SECS: f32 = 45.0; // compressed from 9.2h

// === Water / Coolant ===
// Thresholds are high because cell-stepping means each neutron
// increments the counter at every cell it crosses (many hits/sec).
// At steady-state ~40 act/s, a fuel cell sees ~5-15 neutron
// traversals per second, so 80 hits ≈ 5-16 seconds to warm.
pub const WATER_HEAT_THRESHOLD: u32 = 80;
pub const WATER_BOIL_THRESHOLD: u32 = 160;
pub const VAPOR_RETURN_SECS: f32 = 8.0;
pub const WARM_COOL_SECS: f32 = 5.0;
pub const DEFAULT_COOLANT_FLOW: f32 = 1.0;

// === Void Coefficient ===
pub const VOID_COEFFICIENT_BOOST: f32 = 1.3;

// === Channel Deformation ===
// During a power excursion, fuel rods fragment and swell, pressure
// tubes rupture, and graphite blocks shift — physically deforming
// control rod channels. This is what jammed the rods at Chernobyl
// (INSAG-7), NOT steam pressure in the rod channels (which had
// separate low-pressure cooling circuits that did not boil).
// Resistance activates per-zone when zone activation rate exceeds
// this threshold, and scales with power above it.
pub const CHANNEL_DEFORM_ZONE_THRESHOLD: f32 = 20.0; // act/s per zone (~2.5x normal)
pub const CHANNEL_DEFORM_GAIN: f32 = 1.5;

// === Power Display ===
pub const POWER_PER_ACTIVATION: f32 = 80.0; // 3200/40

// === Graphite Displacer Tip ===
pub const DISPLACER_TIP_DURATION: f32 = 2.0; // seconds
pub const DISPLACER_TIP_BOOST: f32 = 1.5; // +50% local reactivity
pub const DISPLACER_TIP_ROWS: usize = 2; // rows at rod leading edge

// === Weighted Particles ===
// When a fission product's weight exceeds this threshold,
// split it into multiple particles to maintain spatial diversity.
// With NEUTRONS_PER_FISSION=3, a weight-1 fission yields ~2.98.
// At threshold 2.0, first-generation products split to ~3 particles
// of weight ~0.99 — preserving the original spatial distribution
// while the weight infrastructure enables rayon parallelization
// and future variance reduction techniques.
pub const WEIGHT_SPLIT_THRESHOLD: f32 = 2.0;
