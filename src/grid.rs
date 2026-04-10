use crate::config::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellState {
    Uranium235Active,
    Uranium235Inactive { reactivation_timer: f32 },
    Xenon135 { decay_timer: f32 },
    ModeratorRod,
    AbsorptionRod,
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaterState {
    Cool { neutron_hits: u32 },
    Warm { neutron_hits: u32, cool_timer: f32 },
    Vapor { return_timer: f32 },
    None, // no water in this cell (moderator/absorption rod columns)
}

pub struct Grid {
    pub cells: [[CellState; GRID_COLS]; GRID_ROWS],
    pub water: [[WaterState; GRID_COLS]; GRID_ROWS],
    /// Per-cell iodine-135 concentration. Accumulates during fission,
    /// decays into xenon-135 at IODINE_DECAY_RATE. This creates the
    /// "iodine pit" effect: xenon peaks hours after power reduction.
    pub iodine: [[f32; GRID_COLS]; GRID_ROWS],
}

impl Grid {
    pub fn new() -> Self {
        let mut cells = [[CellState::Empty; GRID_COLS]; GRID_ROWS];
        let mut water = [[WaterState::None; GRID_COLS]; GRID_ROWS];

        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                if Self::is_moderator_or_reflector(col) {
                    // Moderator columns: 10, 20, 30, 40
                    // Reflector columns: 0, 49 (radial graphite reflector)
                    // Real RBMK: graphite reflector surrounds the core,
                    // ensuring edge fuel zones have equal moderation to
                    // central zones.
                    cells[row][col] = CellState::ModeratorRod;
                } else if Self::is_absorption_rod_column(col) {
                    cells[row][col] = CellState::Uranium235Active;
                    water[row][col] = WaterState::Cool { neutron_hits: 0 };
                } else {
                    cells[row][col] = CellState::Uranium235Active;
                    water[row][col] = WaterState::Cool { neutron_hits: 0 };
                }
            }
        }

        Grid {
            cells,
            water,
            iodine: [[0.0; GRID_COLS]; GRID_ROWS],
        }
    }

    pub fn is_absorption_rod_column(col: usize) -> bool {
        // Columns 5, 15, 25, 35, 45
        col >= 5 && (col % 10) == 5
    }

    pub fn is_moderator_or_reflector(col: usize) -> bool {
        // Interior moderator columns every MODERATOR_INTERVAL
        if col > 0 && col < GRID_COLS - 1 && col % MODERATOR_INTERVAL == 0 {
            return true;
        }
        // Edge reflector columns (radial graphite reflector)
        col == 0 || col == GRID_COLS - 1
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_init() {
        let grid = Grid::new();

        // Check moderator rods at correct columns
        for row in 0..GRID_ROWS {
            assert_eq!(grid.cells[row][10], CellState::ModeratorRod);
            assert_eq!(grid.cells[row][20], CellState::ModeratorRod);
            assert_eq!(grid.cells[row][30], CellState::ModeratorRod);
            assert_eq!(grid.cells[row][40], CellState::ModeratorRod);
        }

        // Column 0 and 49 are graphite reflectors
        assert_eq!(grid.cells[0][0], CellState::ModeratorRod);
        assert_eq!(grid.cells[0][GRID_COLS - 1], CellState::ModeratorRod);

        // Check fuel cells
        assert_eq!(grid.cells[0][1], CellState::Uranium235Active);
        assert_eq!(grid.cells[5][7], CellState::Uranium235Active);

        // Check water on fuel cells
        assert_eq!(grid.water[0][1], WaterState::Cool { neutron_hits: 0 });

        // Check no water on moderator columns
        assert_eq!(grid.water[0][10], WaterState::None);

        // Check absorption rod columns are fuel initially
        assert_eq!(grid.cells[0][5], CellState::Uranium235Active);
        assert_eq!(grid.cells[0][15], CellState::Uranium235Active);
    }

    #[test]
    fn test_absorption_rod_columns() {
        assert!(Grid::is_absorption_rod_column(5));
        assert!(Grid::is_absorption_rod_column(15));
        assert!(Grid::is_absorption_rod_column(25));
        assert!(Grid::is_absorption_rod_column(35));
        assert!(Grid::is_absorption_rod_column(45));
        assert!(!Grid::is_absorption_rod_column(0));
        assert!(!Grid::is_absorption_rod_column(10));
        assert!(!Grid::is_absorption_rod_column(3));
    }

}
