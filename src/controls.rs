use crate::config::*;

/// Each rod controls a 10-column zone of the core.
/// Rod at col 5  -> zone 0 (cols 0-9)
/// Rod at col 15 -> zone 1 (cols 10-19)
/// Rod at col 25 -> zone 2 (cols 20-29)
/// Rod at col 35 -> zone 3 (cols 30-39)
/// Rod at col 45 -> zone 4 (cols 40-49)
pub struct AbsorptionRodSystem {
    pub positions: [f32; NUM_ABSORPTION_RODS],
    pub rod_columns: [usize; NUM_ABSORPTION_RODS],
    pub was_inserting: [bool; NUM_ABSORPTION_RODS],
    pub displacer_timers: [f32; NUM_ABSORPTION_RODS],
    pub displacer_active: [bool; NUM_ABSORPTION_RODS],
    pub scram_active: bool,
    /// Per-zone target: each rod aims for TARGET / NUM_RODS
    pub zone_target: f32,
    /// Scenario-driven rod depth targets. When active, rods move
    /// toward these positions at ROD_MOVE_SPEED, with steam pressure
    /// assisting withdrawal and opposing insertion.
    pub targets: [f32; NUM_ABSORPTION_RODS],
    pub target_active: bool,
}

impl AbsorptionRodSystem {
    pub fn new() -> Self {
        AbsorptionRodSystem {
            positions: [0.0; NUM_ABSORPTION_RODS],
            rod_columns: [5, 15, 25, 35, 45],
            was_inserting: [false; NUM_ABSORPTION_RODS],
            displacer_timers: [0.0; NUM_ABSORPTION_RODS],
            displacer_active: [false; NUM_ABSORPTION_RODS],
            scram_active: false,
            zone_target: TARGET_ACTIVATIONS_PER_SEC / NUM_ABSORPTION_RODS as f32,
            targets: [0.0; NUM_ABSORPTION_RODS],
            target_active: false,
        }
    }

    /// Each rod independently responds to its own zone's activation rate.
    /// zone_rates[i] = activations/sec in zone i.
    pub fn auto_update(&mut self, zone_rates: &[f32; NUM_ABSORPTION_RODS], dt: f32) {
        for i in 0..NUM_ABSORPTION_RODS {
            let error = zone_rates[i] - self.zone_target;

            // Dead-band: don't move rods when error is small (within 10%
            // of zone target). Prevents constant oscillation near setpoint
            // and avoids spurious displacer tip activations.
            let dead_band = self.zone_target * 0.10;
            if error.abs() < dead_band {
                continue;
            }

            let direction = error.signum();
            let magnitude = ((error.abs() - dead_band) / self.zone_target).min(1.0);

            let old_pos = self.positions[i];
            self.positions[i] += direction * ROD_MOVE_SPEED * magnitude * dt;
            self.positions[i] = self.positions[i].clamp(0.0, GRID_ROWS as f32);

            // Track insertion state for displacer tip
            let now_inserting = self.positions[i] > old_pos;
            if now_inserting && !self.was_inserting[i] {
                self.displacer_timers[i] = DISPLACER_TIP_DURATION;
                self.displacer_active[i] = true;
            }
            self.was_inserting[i] = now_inserting;
        }

        self.update_displacer_timers(dt);
    }

    pub fn manual_move(&mut self, direction: f32, dt: f32) {
        for i in 0..NUM_ABSORPTION_RODS {
            let old_pos = self.positions[i];
            self.positions[i] += direction * ROD_MOVE_SPEED * 5.0 * dt;
            self.positions[i] = self.positions[i].clamp(0.0, GRID_ROWS as f32);

            let now_inserting = self.positions[i] > old_pos;
            if now_inserting && !self.was_inserting[i] {
                self.displacer_timers[i] = DISPLACER_TIP_DURATION;
                self.displacer_active[i] = true;
            }
            self.was_inserting[i] = now_inserting;
        }

        self.update_displacer_timers(dt);
    }

    /// Set all rods to move toward `depth` at ROD_MOVE_SPEED.
    /// Used by scenario events — rods move gradually, not instantly.
    pub fn set_targets(&mut self, depth: f32) {
        let d = depth.clamp(0.0, GRID_ROWS as f32);
        for t in &mut self.targets {
            *t = d;
        }
        self.target_active = true;
    }

    /// Set individual rod targets.
    pub fn set_individual_targets(&mut self, positions: [f32; NUM_ABSORPTION_RODS]) {
        for (i, &p) in positions.iter().enumerate() {
            self.targets[i] = p.clamp(0.0, GRID_ROWS as f32);
        }
        self.target_active = true;
    }

    /// Move rods toward their targets at ROD_MOVE_SPEED.
    /// Rod channels have separate low-pressure cooling that does
    /// not impede movement during normal operation.
    pub fn update_toward_targets(&mut self, dt: f32) {
        if !self.target_active {
            return;
        }

        let mut all_reached = true;
        for i in 0..NUM_ABSORPTION_RODS {
            let diff = self.targets[i] - self.positions[i];
            if diff.abs() < 0.05 {
                self.positions[i] = self.targets[i];
                continue;
            }
            all_reached = false;

            let old_pos = self.positions[i];
            let drive = diff.signum() * ROD_MOVE_SPEED;
            self.positions[i] += drive * dt;
            self.positions[i] = self.positions[i].clamp(0.0, GRID_ROWS as f32);

            // Don't overshoot target
            let new_diff = self.targets[i] - self.positions[i];
            if new_diff * diff < 0.0 {
                self.positions[i] = self.targets[i];
            }

            // Track insertion for displacer tip
            let now_inserting = self.positions[i] > old_pos;
            if now_inserting && !self.was_inserting[i] {
                self.displacer_timers[i] = DISPLACER_TIP_DURATION;
                self.displacer_active[i] = true;
            }
            self.was_inserting[i] = now_inserting;
        }

        self.update_displacer_timers(dt);

        if all_reached {
            self.target_active = false;
        }
    }

    /// Initiate SCRAM — sets flag for gradual rod insertion.
    /// Overrides any active scenario targets.
    pub fn scram(&mut self) {
        self.scram_active = true;
        self.target_active = false;
        // Activate displacer tips for rods not already fully inserted
        for i in 0..NUM_ABSORPTION_RODS {
            if self.positions[i] < GRID_ROWS as f32 {
                self.displacer_timers[i] = DISPLACER_TIP_DURATION;
                self.displacer_active[i] = true;
                self.was_inserting[i] = true;
            }
        }
    }

    /// Drive SCRAM rods down at SCRAM_ROD_SPEED against channel resistance.
    /// `channel_resistance[i]`: per-rod resistance from fuel channel
    /// deformation during a power excursion. At Chernobyl, the power
    /// surge caused fuel to fragment and channels to buckle, physically
    /// jamming the rods at ~2-2.5m of 7m travel (INSAG-7).
    pub fn update_scram(&mut self, dt: f32, channel_resistance: &[f32; NUM_ABSORPTION_RODS]) {
        if !self.scram_active {
            return;
        }

        let mut all_inserted = true;
        for i in 0..NUM_ABSORPTION_RODS {
            let old_pos = self.positions[i];

            // SCRAM drives down (+), channel deformation resists that motion.
            // If resistance exceeds the gravity-driven insertion speed, the rod
            // stalls instead of reversing direction — a buckled channel jams
            // the rod in place, it does not push it back out.
            let net_speed = (SCRAM_ROD_SPEED - channel_resistance[i]).max(0.0);
            self.positions[i] += net_speed * dt;
            self.positions[i] = self.positions[i].clamp(0.0, GRID_ROWS as f32);

            if self.positions[i] < GRID_ROWS as f32 {
                all_inserted = false;
            }

            // Displacer tip activates at the leading edge as rod descends
            let now_inserting = self.positions[i] > old_pos;
            if now_inserting && !self.was_inserting[i] {
                self.displacer_timers[i] = DISPLACER_TIP_DURATION;
                self.displacer_active[i] = true;
            }
            self.was_inserting[i] = now_inserting;
        }

        self.update_displacer_timers(dt);

        if all_inserted {
            self.scram_active = false;
        }
    }

    fn update_displacer_timers(&mut self, dt: f32) {
        for i in 0..NUM_ABSORPTION_RODS {
            if self.displacer_active[i] {
                self.displacer_timers[i] -= dt;
                if self.displacer_timers[i] <= 0.0 {
                    self.displacer_active[i] = false;
                    self.displacer_timers[i] = 0.0;
                }
            }
        }
    }

    #[cfg(test)]
    pub fn is_any_displacer_active(&self) -> bool {
        self.displacer_active.iter().any(|&a| a)
    }

    /// Returns the displacer tip boost multiplier at the given col and row.
    /// The boost only applies at the rod's leading edge (the rows just
    /// below the current rod position) where the graphite tip displaces
    /// water before the boron absorber arrives.
    pub fn displacer_boost_at(&self, col: usize, row: usize) -> f32 {
        for i in 0..NUM_ABSORPTION_RODS {
            if self.rod_columns[i] == col && self.displacer_active[i] {
                // Boost only at the leading edge: the 2 rows just below
                // the rod's current insertion depth
                let rod_depth = self.positions[i] as usize;
                if row >= rod_depth && row < rod_depth + DISPLACER_TIP_ROWS {
                    return DISPLACER_TIP_BOOST;
                }
            }
        }
        1.0
    }

    /// Map a grid column to its rod zone index (0..NUM_ABSORPTION_RODS)
    pub fn zone_for_col(col: usize) -> usize {
        (col / MODERATOR_INTERVAL).min(NUM_ABSORPTION_RODS - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_independent_rod_control() {
        let mut rods = AbsorptionRodSystem::new();
        assert_eq!(rods.positions[0], 0.0);

        // Zone 0 hot, zone 1 cold — rods should diverge
        let mut zone_rates = [0.0; NUM_ABSORPTION_RODS];
        zone_rates[0] = 20.0; // way above zone target (8.0)
        zone_rates[1] = 2.0; // below zone target

        rods.auto_update(&zone_rates, 1.0);

        // Rod 0 should insert (high local activity)
        assert!(rods.positions[0] > 0.0);
        // Rod 1 should stay at 0 or withdraw (already at 0)
        assert_eq!(rods.positions[1], 0.0);
    }

    #[test]
    fn test_zone_mapping() {
        assert_eq!(AbsorptionRodSystem::zone_for_col(0), 0);
        assert_eq!(AbsorptionRodSystem::zone_for_col(5), 0);
        assert_eq!(AbsorptionRodSystem::zone_for_col(9), 0);
        assert_eq!(AbsorptionRodSystem::zone_for_col(10), 1);
        assert_eq!(AbsorptionRodSystem::zone_for_col(15), 1);
        assert_eq!(AbsorptionRodSystem::zone_for_col(25), 2);
        assert_eq!(AbsorptionRodSystem::zone_for_col(35), 3);
        assert_eq!(AbsorptionRodSystem::zone_for_col(45), 4);
        assert_eq!(AbsorptionRodSystem::zone_for_col(49), 4);
    }

    #[test]
    fn test_scram_gradual_insertion() {
        let mut rods = AbsorptionRodSystem::new();
        rods.scram();
        assert!(rods.scram_active);

        // Simulate 15 seconds of SCRAM with no steam (all zones dry)
        let no_steam = [0.0; NUM_ABSORPTION_RODS];
        for _ in 0..450 {
            rods.update_scram(0.033, &no_steam);
        }

        // After ~15s at 1.5 rows/s, rods should be fully inserted
        for pos in &rods.positions {
            assert_eq!(*pos, GRID_ROWS as f32);
        }
        assert!(!rods.scram_active);
    }

    #[test]
    fn test_displacer_tip_activates_on_insertion() {
        let mut rods = AbsorptionRodSystem::new();
        assert!(!rods.is_any_displacer_active());

        rods.scram();
        assert!(rods.is_any_displacer_active());

        rods.update_displacer_timers(DISPLACER_TIP_DURATION + 0.1);
        assert!(!rods.is_any_displacer_active());
    }

    #[test]
    fn test_rod_position_clamped() {
        let mut rods = AbsorptionRodSystem::new();
        let zero_rates = [0.0; NUM_ABSORPTION_RODS];
        rods.auto_update(&zero_rates, 100.0);
        for pos in &rods.positions {
            assert!(*pos >= 0.0);
        }

        let high_rates = [1000.0; NUM_ABSORPTION_RODS];
        rods.auto_update(&high_rates, 100.0);
        for pos in &rods.positions {
            assert!(*pos <= GRID_ROWS as f32);
        }
    }

    #[test]
    fn test_displacer_boost_at_leading_edge() {
        let mut rods = AbsorptionRodSystem::new();
        // Insert rod 0 to depth 5
        rods.positions[0] = 5.0;
        rods.displacer_active[0] = true;
        rods.displacer_timers[0] = 1.0;

        // Row 5 (at leading edge) should get boost
        assert!((rods.displacer_boost_at(5, 5) - DISPLACER_TIP_BOOST).abs() < 0.01);
        // Row 6 (within DISPLACER_TIP_ROWS=2) should get boost
        assert!((rods.displacer_boost_at(5, 6) - DISPLACER_TIP_BOOST).abs() < 0.01);
        // Row 7 (beyond tip) should NOT get boost
        assert!((rods.displacer_boost_at(5, 7) - 1.0).abs() < 0.01);
        // Row 3 (above rod, already absorbed) should NOT get boost
        assert!((rods.displacer_boost_at(5, 3) - 1.0).abs() < 0.01);

        // Column 3 (not a rod column) — no boost
        assert!((rods.displacer_boost_at(3, 5) - 1.0).abs() < 0.01);
    }
}
