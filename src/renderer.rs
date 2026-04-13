use crate::simulation::Simulation;
use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    Pause,
    Scram,
    Reset,
    Quit,
    RodsUp,
    RodsDown,
    ToggleAutoControl,
    SpeedUp,
    SpeedDown,
    InjectNeutrons,
    CoolantFlowUp,
    CoolantFlowDown,
    ToggleLegend,
}

#[allow(dead_code)]
pub trait Renderer {
    fn init(&mut self) -> Result<(), Box<dyn Error>>;
    fn render(&mut self, sim: &Simulation) -> Result<(), Box<dyn Error>>;
    fn handle_input(&mut self) -> Result<Option<InputEvent>, Box<dyn Error>>;
    fn cleanup(&mut self) -> Result<(), Box<dyn Error>>;
}
