# RBMK-1000 Nuclear Reactor Simulation

A real-time simulation of an RBMK-1000 nuclear reactor core in Rust, featuring agent-based neutron physics, fission chain reactions, and the key design flaws that led to the Chernobyl disaster.

![Graphical Mode](https://img.shields.io/badge/renderer-TUI%20%7C%20Graphical-blue)
![Rust](https://img.shields.io/badge/rust-2024%20edition-orange)
![License](https://img.shields.io/badge/license-MIT-green)

## Download

Pre-built binaries for Windows, Linux, and macOS are available on the [Releases](https://github.com/fabio-andre-rodrigues/rbmk-1000-sim/releases) page. Each archive includes the binary, scenario files, and documentation.

## Features

### Physics Engine
- **Agent-based neutron transport** -- individual neutrons tracked as particles with cell-by-cell stepping, parallelized across CPU cores via [rayon](https://docs.rs/rayon)
- **Two-phase parallel architecture** -- neutrons processed against a read-only grid snapshot, then grid mutations applied sequentially with phantom fission filtering
- **Weighted particles** -- one particle can represent multiple neutrons, reducing allocations while maintaining spatial diversity via weight splitting
- **Two-state energy model** -- fast (2 MeV) and thermal (0.025 eV) neutrons with distinct interaction rules
- **U-235 fission** -- probabilistic (20% per cell traversal), producing weighted fast neutrons per event
- **Graphite moderation** -- fast-to-thermal conversion with isotropic scattering at moderator columns
- **Iodine-135 / Xenon-135 decay chain** -- iodine accumulates during fission, decays into xenon on compressed timescale, creating the "iodine pit" effect
- **Water cooling** -- cool/warm/vapor state machine with neutron hit counting
- **Positive void coefficient** -- vapor cells increase fission probability (the key RBMK flaw)
- **Delayed neutrons** -- single-group precursor pool (beta=0.005 for burned fuel with Pu-239/241, lambda=0.08/s) distinguishing controllable from prompt-critical states
- **Spontaneous fission source** -- background neutron generation to bootstrap chain reactions

### Control Systems
- **Independent per-zone rod control** -- 5 absorption rods, each responding to its own zone's activation rate with dead-band and EMA smoothing
- **Gradual rod movement** -- scenario-driven rods move toward targets at realistic speed, not instantly
- **Gradual SCRAM** -- rods insert at 1.5 rows/s (~13s full insertion) with gravity-driven mechanics
- **Graphite displacer tip effect** -- brief reactivity spike at the rod's leading edge during insertion (the Chernobyl mechanism), rendered in distinct blue color
- **Channel deformation model** -- during a power excursion, per-zone resistance increases with local activation rate, physically jamming rods. Control rod channels had separate low-pressure cooling that did not boil (INSAG-7).
- **SCRAM rod jamming** -- rods insert freely at first, then stall at ~30% depth (6-7 rows) as the power excursion deforms fuel channels, matching the historical 2-2.5m of 7m travel at Chernobyl. Buckled channels jam rods in place but cannot push them back out (speed clamped to >= 0).
- **Coolant flow control** -- adjustable pump flow rate affecting water cooling and vapor return

### Visualization
- **TUI renderer** (ratatui + crossterm) -- colored Unicode grid, real-time HUD with statistics and keyboard controls. Adapts to terminal size with dynamic cell width.
- **Graphical renderer** (macroquad) -- pixel-perfect grid with dashboard panel featuring arc gauges (pressure, power, coolant), rod position bars, stat bars, and blinking SCRAM indicator.
- **Gas transparency overlays** -- iodine (orange tint, intensity scales with concentration) and xenon (purple tint, fades as it decays) shown as semi-transparent layers over fuel cells.
- **Toggle-able color legend** -- press `L` to show/hide a detailed panel with actual render-color swatches for all cell types, water states, gas overlays, and neutron types (both renderers).
- **Graphite tip visualization** -- the bottom 2 rows of each control rod rendered in blue (graphite displacer) vs red (boron absorber), making the fatal design flaw visible.

### Scenario System
- **JSON scenario files** -- timed sequences of operator actions (rod targets, auto-control, coolant flow)
- **Gradual rod movement** -- `set_rods` events set targets that rods move toward at realistic speed
- **Physics-driven consequences** -- scenarios only script what operators did; the simulation physics produces the disaster naturally
- **Chernobyl scenario included** -- recreates the April 26, 1986 disaster sequence with historically accurate operator actions, cross-referenced with INSAG-7 and WNA sources

## Quick Start

```bash
# TUI mode (default, runs in terminal)
cargo run --release

# TUI mode with Chernobyl scenario
cargo run --release -- --scenario scenarios/chernobyl.json

# Graphical mode
cargo run --release --no-default-features --features gfx -- --gfx

# Graphical mode with Chernobyl scenario
cargo run --release --no-default-features --features gfx -- --gfx --scenario scenarios/chernobyl.json
```

Or download a pre-built binary from [Releases](https://github.com/fabio-andre-rodrigues/rbmk-1000-sim/releases).

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
| L | Toggle color legend |
| Q / Esc | Quit |

## Grid Legend

| Symbol | Color | Meaning |
|--------|-------|---------|
| Solid block | Green | U-235 fuel (active) |
| Light block | Dark gray | U-235 fuel (spent, reactivating) |
| Orange tint | Orange overlay | Iodine-135 gas (precursor to xenon) |
| Purple tint | Purple overlay | Xenon-135 gas (neutron poison) |
| Double line | Blue | Graphite moderator rod |
| Dense block | Red | Boron carbide absorber (control rod body) |
| Dense block | Blue | Graphite displacer tip (rod leading edge) |
| Background | Dark blue | Cool water |
| Background | Dark red | Warm water |
| Background | Black | Vapor (no water -- void coefficient active) |
| Dot | White | Fast neutron |
| Dot | Gray | Thermal neutron |

Press `L` in-game to see these with actual render colors.

## Architecture

```
src/
  main.rs           -- Entry point, game loop, CLI routing
  config.rs         -- All physics constants with real-world references
  grid.rs           -- Grid, CellState, WaterState, iodine tracking
  neutron.rs        -- Neutron struct, movement, spawning, weight
  simulation.rs     -- Core physics engine, rayon-parallel neutron loop,
                       two-phase grid updates, channel deformation model
  controls.rs       -- Rod control system, targets, displacer tip,
                       SCRAM with channel deformation resistance
  renderer.rs       -- Renderer trait, InputEvent enum
  renderer_tui.rs   -- Console TUI (ratatui + crossterm)
  renderer_gfx.rs   -- Graphical dashboard (macroquad)
  scenario.rs       -- JSON scenario loader and event system

scenarios/
  chernobyl.json            -- Chernobyl disaster recreation
  CHERNOBYL.txt             -- Full historical reference with timeline,
                               key personnel, points of contention,
                               INSAG-1 vs INSAG-7 debate, and sources
  SIMULATION-TRADEOFFS.txt  -- What the sim gets right, what it gets
                               wrong, why each tradeoff was made,
                               constants vs. published data, and
                               what each of the 34 tests validates

.github/workflows/
  release.yml        -- CI pipeline: builds Windows/Linux/macOS binaries,
                        packages with scenarios and docs, creates release
```

## Chernobyl Scenario

The included scenario recreates the key events of April 25-26, 1986, cross-referenced with the IAEA INSAG-7 report and World Nuclear Association sources:

1. **Normal operation** -- reactor at ~50% power (1,600 MWt) with automatic rod control
2. **9-hour delay** -- grid controller holds power for evening demand; Xe-135 in equilibrium at 1,600 MWt
3. **Power collapse** -- at 00:28, power crashes to 30 MWt (~1%) during control transfer
4. **Reactivity losses** -- void collapse (less boiling = more water absorption), graphite cooling, and rising Xe-135 all demand rod withdrawal
5. **Rod withdrawal** -- operators disable auto-control and withdraw nearly all rods against multiple reactivity losses (ORM falls to 6-8 rods; minimum required: 15 per INSAG-7)
6. **Feedwater increase** -- extra coolant pumps temporarily suppress boiling
7. **Turbine test begins** -- 01:23:04, coolant pumps coast down as turbine decelerates
8. **Void coefficient** -- coolant boils, void fraction rises, positive void coefficient slowly adds reactivity
9. **AZ-5 pressed** -- 01:23:40, routine or precautionary. Graphite displacer tips enter core first, causing reactivity spike ("positive scram" effect, known since 1983 at Ignalina). This was the trigger for the catastrophic excursion.
10. **Rods jam** -- power excursion deforms fuel channels, physically jamming rods at ~30% insertion (historically ~2-2.5m of 7m travel)
11. **Prompt supercritical** -- 01:23:47, power reaches ~30,000 MWt. Two explosions destroy the reactor.

All consequences emerge from the physics engine -- only operator actions are scripted. See `scenarios/CHERNOBYL.txt` for the full historical reference and `scenarios/SIMULATION-TRADEOFFS.txt` for what the simulation gets right vs. where it diverges from reality.

## Physics Model

| Real Parameter | Sim Constant | Value |
|---|---|---|
| Neutrons per fission (nu-bar = 2.43) | `NEUTRONS_PER_FISSION` | 3 |
| Fission cross-section | `FISSION_PROBABILITY` | 0.20 per cell |
| Water absorption | `WATER_NEUTRON_ABSORPTION_PROB` | 0.08 per cell |
| Void coefficient (+2500 pcm) | `VOID_COEFFICIENT_BOOST` | 1.3x |
| Delayed neutron fraction | `DELAYED_NEUTRON_FRACTION` | 0.005 (burned fuel, Pu mix) |
| Control rod speed | `SCRAM_ROD_SPEED` | 1.5 rows/s |
| Displacer tip boost | `DISPLACER_TIP_BOOST` | 1.5x |
| Displacer tip length | `DISPLACER_TIP_ROWS` | 2 rows |
| Weight split threshold | `WEIGHT_SPLIT_THRESHOLD` | 2.0 |
| Xe-135 decay | `XENON_DECAY_SECS` | 45s (compressed from 9.2h) |
| I-135 decay rate | `IODINE_DECAY_RATE` | 0.14/s (compressed from 6.6h) |

## Creating Custom Scenarios

Scenarios are JSON files with timed events. Rod positions are **targets** -- rods move gradually toward the specified depth at `ROD_MOVE_SPEED`:

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
      "message": "Inserting rods (gradual)..."
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
cargo build --release --no-default-features --features gfx

# Run tests (34 tests including headless 120s stability + full Chernobyl scenario)
cargo test
```

Optional: install [sccache](https://github.com/mozilla/sccache) for faster rebuilds:
```bash
cargo install sccache
export RUSTC_WRAPPER=sccache  # add to shell profile
```

## References

- [IAEA INSAG-7 (1992) -- "The Chernobyl Accident: Updating of INSAG-1"](https://pub.iaea.org/MTCD/publications/PDF/Pub913e_web.pdf)
- [World Nuclear Association -- Chernobyl Accident 1986](https://world-nuclear.org/information-library/safety-and-security/safety-of-plants/chernobyl-accident)
- [World Nuclear Association -- Sequence of Events](https://world-nuclear.org/information-library/appendices/chernobyl-accident-appendix-1-sequence-of-events)
- [World Nuclear Association -- RBMK Reactors](https://world-nuclear.org/information-library/appendices/rbmk-reactors)
- [OECD NEA -- Chernobyl: The Site and Accident Sequence](https://www.oecd-nea.org/jcms/pl_28271/chernobyl-chapter-i-the-site-and-accident-sequence)

## Disclaimer

This is an educational simulation. The physics are simplified for real-time visualization. It is not a nuclear safety analysis tool. See `scenarios/SIMULATION-TRADEOFFS.txt` for a detailed analysis of where the model matches and diverges from reality.
