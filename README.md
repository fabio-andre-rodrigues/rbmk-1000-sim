# RBMK-1000 Nuclear Reactor Simulation

A real-time simulation of an RBMK-1000 nuclear reactor core in Rust, featuring agent-based neutron physics, fission chain reactions, and the key design flaws that led to the Chernobyl disaster.

![Graphical Mode](https://img.shields.io/badge/renderer-TUI%20%7C%20Graphical-blue)
![Rust](https://img.shields.io/badge/rust-2024%20edition-orange)
![License](https://img.shields.io/badge/license-MIT-green)

## Features

### Physics Engine
- **Agent-based neutron transport** -- individual neutrons tracked as particles with cell-by-cell stepping (no tunneling)
- **Two-state energy model** -- fast (2 MeV) and thermal (0.025 eV) neutrons with distinct interaction rules
- **U-235 fission** -- probabilistic (20% per cell traversal), producing 3 fast neutrons per event
- **Graphite moderation** -- fast-to-thermal conversion with isotropic scattering at moderator columns
- **Iodine-135 / Xenon-135 decay chain** -- iodine accumulates during fission, decays into xenon on compressed timescale, creating the "iodine pit" effect
- **Water cooling** -- cool/warm/vapor state machine with neutron hit counting
- **Positive void coefficient** -- vapor cells increase fission probability (the key RBMK flaw)
- **Delayed neutrons** -- single-group precursor pool (beta=0.0065, lambda=0.08/s) distinguishing controllable from prompt-critical states
- **Spontaneous fission source** -- background neutron generation to bootstrap chain reactions

### Control Systems
- **Independent per-zone rod control** -- 5 absorption rods, each responding to its own zone's activation rate with dead-band and EMA smoothing
- **Gradual SCRAM** -- rods insert at realistic speed (1.5 rows/s, ~13s full insertion) instead of instantly
- **Graphite displacer tip effect** -- brief reactivity spike at the rod's leading edge during insertion (the Chernobyl mechanism)
- **Steam pressure opposes SCRAM** -- high pressure physically pushes rods back up, preventing full insertion
- **Coolant flow control** -- adjustable pump flow rate affecting water cooling and vapor return

### Visualization
- **TUI renderer** (ratatui + crossterm) -- colored Unicode grid, real-time HUD with statistics, legend, and keyboard controls. Adapts to terminal size with dynamic cell width.
- **Graphical renderer** (macroquad) -- pixel-perfect grid with dashboard panel featuring arc gauges (pressure, power, coolant), rod position bars, stat bars, and blinking SCRAM indicator.
- **Gas transparency overlays** -- iodine (orange tint, intensity scales with concentration) and xenon (purple tint, fades as it decays) shown as semi-transparent layers over fuel cells.

### Scenario System
- **JSON scenario files** -- timed sequences of operator actions (rod positions, auto-control, coolant flow)
- **Physics-driven consequences** -- scenarios only script what operators did; the simulation physics produces the disaster naturally
- **Chernobyl scenario included** -- recreates the April 26, 1986 disaster sequence with historically accurate operator actions

## Quick Start

```bash
# TUI mode (default, runs in terminal)
cargo run

# TUI mode with Chernobyl scenario
cargo run -- --scenario scenarios/chernobyl.json

# Graphical mode
cargo run --features gfx -- --gfx

# Graphical mode with Chernobyl scenario
cargo run --features gfx -- --gfx --scenario scenarios/chernobyl.json
```

## Controls

| Key | Action |
|-----|--------|
| Space | Pause / Resume |
| S | SCRAM (emergency shutdown) |
| R | Reset simulation |
| Up / Down | Manual rod control |
| Left / Right | Coolant flow +/- 10% |
| A | Toggle automatic rod control |
| + / - | Simulation speed (0.25x - 4x) |
| N | Inject 5 neutrons |
| Q / Esc | Quit |

## Grid Legend

| Symbol | Color | Meaning |
|--------|-------|---------|
| Solid block | Green | U-235 fuel (active) |
| Light block | Dark gray | U-235 fuel (spent, reactivating) |
| Orange tint | Orange overlay | Iodine-135 gas (precursor to xenon) |
| Purple tint | Purple overlay | Xenon-135 gas (neutron poison) |
| Double line | Blue | Graphite moderator rod |
| Dense block | Red | Absorption control rod |
| Background | Blue | Cool water |
| Background | Red | Warm water |
| Background | Black | Vapor (no water -- void coefficient active) |
| Dot | White | Fast neutron |
| Dot | Gray | Thermal neutron |

## Architecture

```
src/
  main.rs           -- Entry point, game loop, CLI routing
  config.rs         -- All physics constants with real-world references
  grid.rs           -- Grid, CellState, WaterState, iodine tracking
  neutron.rs        -- Neutron struct, movement, spawning
  simulation.rs     -- Core physics engine, all interactions
  controls.rs       -- Rod control system, PID, displacer tip
  renderer.rs       -- Renderer trait, InputEvent enum
  renderer_tui.rs   -- Console TUI (ratatui + crossterm)
  renderer_gfx.rs   -- Graphical dashboard (macroquad)
  scenario.rs       -- JSON scenario loader and event system

scenarios/
  chernobyl.json    -- Chernobyl disaster recreation
```

## Chernobyl Scenario

The included scenario recreates the key events of April 25-26, 1986:

1. **Normal operation** -- reactor at ~100% power with automatic rod control
2. **Power reduction** -- rods inserted to reduce power for turbine test
3. **Over-insertion** -- operator error drops power too far, xenon builds up
4. **Rod withdrawal** -- operators disable auto-control and withdraw nearly all rods to fight xenon poisoning (violating the 30-rod minimum safety rule)
5. **Feedwater increase** -- extra coolant pumps temporarily stabilize the reactor
6. **Turbine test begins** -- coolant flow drops as pumps lose power
7. **Void coefficient runaway** -- water boils, positive feedback loop increases power
8. **AZ-5 SCRAM** -- emergency button pressed, but graphite displacer tips cause initial reactivity spike
9. **Steam pressure blocks rods** -- pressure pushes control rods back up, preventing shutdown
10. **Prompt supercritical excursion** -- reactor destroyed

All consequences emerge from the physics engine -- only operator actions are scripted.

## Physics Model

The simulation uses simplified but physically grounded models based on real RBMK-1000 parameters:

| Real Parameter | Sim Parameter | Value |
|---|---|---|
| Neutrons per fission (nu-bar = 2.43) | `NEUTRONS_PER_FISSION` | 3 |
| Fission cross-section | `FISSION_PROBABILITY` | 0.20 per cell |
| Water absorption | `WATER_NEUTRON_ABSORPTION_PROB` | 0.08 per cell |
| Void coefficient (+2500 pcm) | `VOID_COEFFICIENT_BOOST` | 1.3x |
| Delayed neutron fraction | `DELAYED_NEUTRON_FRACTION` | 0.0065 |
| Control rod speed | `SCRAM_ROD_SPEED` | 1.5 rows/s |
| Displacer tip boost | `DISPLACER_TIP_BOOST` | 1.5x |
| Xe-135 decay | `XENON_DECAY_SECS` | 45s (compressed) |
| I-135 decay rate | `IODINE_DECAY_RATE` | 0.14/s (compressed) |

## Creating Custom Scenarios

Scenarios are JSON files with timed events:

```json
{
  "name": "My Scenario",
  "description": "Description shown on title screen",
  "initial": {
    "seed_neutrons": 30,
    "auto_control": true,
    "rod_positions": [3.0, 3.0, 3.0, 3.0, 3.0],
    "sim_speed": 1.0
  },
  "events": [
    {
      "time": 10.0,
      "action": { "type": "set_rods", "depth": 8.0 },
      "message": "Inserting rods..."
    },
    {
      "time": 20.0,
      "action": { "type": "set_coolant_flow", "flow": 0.5 },
      "message": "Reducing coolant flow"
    },
    {
      "time": 30.0,
      "action": { "type": "scram" },
      "message": "SCRAM!"
    }
  ]
}
```

Available actions: `set_rods`, `set_rods_individual`, `set_auto_control`, `scram`, `inject_neutrons`, `set_speed`, `set_coolant_flow`, `set_water`, `set_water_region`, `set_paused`, `message`.

## Building

Requires Rust 1.85+ (2024 edition).

```bash
# TUI only (default)
cargo build --release

# With graphical renderer
cargo build --release --features gfx

# Run tests (34 tests including headless scenario validation)
cargo test
```

## References

- [RBMK Reactor Design](https://en.wikipedia.org/wiki/RBMK)
- [Chernobyl Accident Sequence](https://en.wikipedia.org/wiki/Chernobyl_disaster)
- [Xenon-135 Reactor Poisoning](https://en.wikipedia.org/wiki/Iodine_pit)
- [Positive Void Coefficient](https://en.wikipedia.org/wiki/Void_coefficient)

## Disclaimer

This is an educational simulation. The physics are simplified for real-time visualization. It is not a nuclear safety analysis tool.
