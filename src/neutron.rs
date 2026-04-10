use crate::config::*;
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NeutronSpeed {
    Fast,
    Thermal,
}

#[derive(Clone)]
pub struct Neutron {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub speed: NeutronSpeed,
    pub alive: bool,
}

impl Neutron {
    pub fn spawn(x: f32, y: f32, speed: NeutronSpeed) -> Self {
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(0.0..std::f32::consts::TAU);
        let magnitude = match speed {
            NeutronSpeed::Fast => FAST_NEUTRON_SPEED,
            NeutronSpeed::Thermal => THERMAL_NEUTRON_SPEED,
        };
        Neutron {
            x,
            y,
            vx: angle.cos() * magnitude,
            vy: angle.sin() * magnitude,
            speed,
            alive: true,
        }
    }

    pub fn update(&mut self, dt: f32) {
        if !self.alive {
            return;
        }

        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Elastic bounce off grid boundaries
        let grid_w = GRID_COLS as f32 * CELL_SIZE;
        let grid_h = GRID_ROWS as f32 * CELL_SIZE;

        if self.x < 0.0 {
            self.x = -self.x;
            self.vx = -self.vx;
        } else if self.x >= grid_w {
            self.x = 2.0 * grid_w - self.x;
            self.vx = -self.vx;
        }

        if self.y < 0.0 {
            self.y = -self.y;
            self.vy = -self.vy;
        } else if self.y >= grid_h {
            self.y = 2.0 * grid_h - self.y;
            self.vy = -self.vy;
        }

        // Clamp to grid bounds as safety
        self.x = self.x.clamp(0.0, grid_w - 0.01);
        self.y = self.y.clamp(0.0, grid_h - 0.01);
    }

    pub fn grid_col(&self) -> usize {
        ((self.x / CELL_SIZE) as usize).min(GRID_COLS - 1)
    }

    pub fn grid_row(&self) -> usize {
        ((self.y / CELL_SIZE) as usize).min(GRID_ROWS - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neutron_movement() {
        let mut n = Neutron {
            x: 100.0,
            y: 100.0,
            vx: 200.0,
            vy: 0.0,
            speed: NeutronSpeed::Fast,
            alive: true,
        };
        n.update(0.1);
        assert!((n.x - 120.0).abs() < 0.01);
        assert!((n.y - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_neutron_boundary_bounce() {
        let grid_w = GRID_COLS as f32 * CELL_SIZE;
        let mut n = Neutron {
            x: grid_w - 1.0,
            y: 100.0,
            vx: 200.0,
            vy: 0.0,
            speed: NeutronSpeed::Fast,
            alive: true,
        };
        n.update(0.1); // moves +20px, should bounce
        assert!(n.vx < 0.0); // direction reversed
        assert!(n.x < grid_w); // still in bounds
    }

    #[test]
    fn test_neutron_spawn_direction() {
        // Spawn many neutrons, verify speed magnitude is consistent
        for _ in 0..100 {
            let n = Neutron::spawn(100.0, 100.0, NeutronSpeed::Fast);
            let speed = (n.vx * n.vx + n.vy * n.vy).sqrt();
            assert!((speed - FAST_NEUTRON_SPEED).abs() < 1.0);
        }
        for _ in 0..100 {
            let n = Neutron::spawn(100.0, 100.0, NeutronSpeed::Thermal);
            let speed = (n.vx * n.vx + n.vy * n.vy).sqrt();
            assert!((speed - THERMAL_NEUTRON_SPEED).abs() < 1.0);
        }
    }

    #[test]
    fn test_grid_position() {
        let n = Neutron {
            x: 25.0,
            y: 33.0,
            vx: 0.0,
            vy: 0.0,
            speed: NeutronSpeed::Thermal,
            alive: true,
        };
        assert_eq!(n.grid_col(), 1); // 25/16 = 1
        assert_eq!(n.grid_row(), 2); // 33/16 = 2
    }
}
