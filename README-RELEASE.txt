===============================================
  RBMK-1000 NUCLEAR REACTOR SIMULATION
  Reaktor Bolshoy Moshchnosti Kanalnyy
  "High-Power Channel Reactor"
===============================================

An agent-based neutron transport simulation of the RBMK-1000 reactor,
the design involved in the 1986 Chernobyl disaster. Written in Rust.

The simulation models individual neutrons, graphite moderation,
control rod mechanics (including graphite displacer tips), the
I-135/Xe-135 decay chain, water coolant states, the positive void
coefficient, and steam pressure effects on control rods.

QUICK START (from release zip)
------------------------------

  1. Terminal (TUI) mode — runs in your terminal:

     Windows:   rbmk-1000-sim.exe
     Linux:     ./rbmk-1000-sim
     macOS:     ./rbmk-1000-sim

  2. With the Chernobyl scenario:

     Windows:   rbmk-1000-sim.exe --scenario scenarios\chernobyl.json
     Linux:     ./rbmk-1000-sim --scenario scenarios/chernobyl.json
     macOS:     ./rbmk-1000-sim --scenario scenarios/chernobyl.json

  Note: on Linux/macOS you may need to:  chmod +x rbmk-1000-sim

BUILD FROM SOURCE
-----------------

  Prerequisites: Rust toolchain (https://rustup.rs)

  Clone and run:

     git clone https://github.com/fabio-andre-rodrigues/rbmk-1000-sim.git
     cd rbmk-1000-sim
     cargo run --release

  With the Chernobyl scenario:

     cargo run --release -- --scenario scenarios/chernobyl.json

  Graphical mode (macroquad — experimental):

     cargo run --release --no-default-features --features gfx -- --gfx
     cargo run --release --no-default-features --features gfx -- --gfx --scenario scenarios/chernobyl.json

  Run tests:

     cargo test

CONTROLS
--------

  Space       Pause / Resume
  S           SCRAM (emergency rod insertion)
  R           Reset simulation
  Up/Down     Manual rod insertion / withdrawal
  Left/Right  Decrease / Increase coolant flow
  A           Toggle automatic rod control
  +/-         Speed up / Slow down simulation
  N           Inject neutrons
  L           Toggle color legend
  Q / Esc     Quit

WHAT YOU'RE SEEING
------------------

  Green blocks    Active U-235 fuel cells
  Gray blocks     Spent fuel (reactivating)
  Blue columns    Graphite moderator rods
  Red blocks      Boron carbide control rods (absorber)
  Blue blocks     Graphite displacer tips (bottom of control rods)

  Dark blue bg    Cool water (neutron absorber + coolant)
  Dark red bg     Warm water (heating up)
  Black bg        Steam / void (no neutron absorption!)

  Purple overlay  Xenon-135 (neutron poison, 2.6M barn cross-section)
  Orange overlay  Iodine-135 (decays into Xe-135)

  White dots      Fast neutrons (2 MeV, need moderation)
  Gray dots       Thermal neutrons (0.025 eV, can cause fission)

THE CHERNOBYL SCENARIO
----------------------

  The scenario recreates the sequence of events on April 26, 1986:

  1. Power reduction for turbine rundown test
  2. Operator overshoots — power collapses
  3. Xe-135 poisoning builds at low power
  4. Operators withdraw rods past safety limits to fight xenon
  5. Turbine test begins — coolant pumps lose power
  6. Steam builds — positive void coefficient — power surge
  7. AZ-5 (SCRAM) pressed — graphite tips cause initial spike
  8. Power excursion limits rod insertion — rods reach only ~30%
  9. Prompt supercritical excursion — reactor destroyed

  Watch the control rods during SCRAM — they insert a few rows,
  then the power excursion deforms channels, slowing insertion.
  Synchro indicators in the control room showed the rods at
  2-2.5 meters of their full 7-meter travel when the reactor
  was destroyed. Whether the rods physically jammed or were
  simply still in transit during the 8-10 seconds before
  destruction is unproven — simulations produce the same
  explosion regardless. The "positive scram" effect — where
  graphite displacer tips initially increase reactivity by displacing
  neutron-absorbing water — had been discovered in 1983 at the
  Ignalina Nuclear Power Plant, but countermeasures were never
  implemented.

  References:

  - IAEA INSAG-7 (1992), "The Chernobyl Accident: Updating of INSAG-1"
    https://pub.iaea.org/MTCD/publications/PDF/Pub913e_web.pdf

  - World Nuclear Association, "Chernobyl Accident 1986"
    https://world-nuclear.org/information-library/safety-and-security/safety-of-plants/chernobyl-accident

  - World Nuclear Association, "Sequence of Events — Chernobyl Accident"
    https://world-nuclear.org/information-library/appendices/chernobyl-accident-appendix-1-sequence-of-events

SOURCE CODE
-----------

  https://github.com/fabio-andre-rodrigues/rbmk-1000-sim

LICENSE
-------

  See the repository for license information.
