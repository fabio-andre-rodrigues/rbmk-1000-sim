mod config;
mod controls;
mod grid;
mod neutron;
mod renderer;
#[cfg(feature = "gfx")]
mod renderer_gfx;
#[cfg(feature = "tui")]
mod renderer_tui;
mod scenario;
mod simulation;

use renderer::InputEvent;
use scenario::ScenarioRunner;
use simulation::Simulation;
use std::path::PathBuf;

fn parse_scenario_arg() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    for (i, arg) in args.iter().enumerate() {
        if arg == "--scenario" {
            if let Some(path) = args.get(i + 1) {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

#[cfg(feature = "tui")]
fn run_tui(scenario_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    use renderer::Renderer;
    use std::time::{Duration, Instant};

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let mut renderer = renderer_tui::TuiRenderer::new()?;
    renderer.init()?;

    // Load scenario if provided
    let mut scenario_runner = match &scenario_path {
        Some(path) => {
            let runner = ScenarioRunner::load(path)?;
            eprintln!("Loaded scenario: {}", runner.scenario.name);
            Some(runner)
        }
        None => None,
    };

    // Title screen
    if let Some(ref runner) = scenario_runner {
        renderer.render_scenario_title(&runner.scenario)?;
    } else {
        renderer.render_title()?;
    }
    if !renderer.wait_for_start()? {
        renderer.cleanup()?;
        return Ok(());
    }

    let mut sim = Simulation::new();

    // Apply scenario initial conditions, or default seed
    if let Some(ref runner) = scenario_runner {
        runner.apply_initial(&mut sim);
    } else {
        sim.seed_neutrons(config::INITIAL_SEED_NEUTRONS);
    }

    let tick_rate = Duration::from_millis(33);
    let mut last_tick = Instant::now();

    loop {
        if let Some(event) = renderer.handle_input()? {
            match event {
                InputEvent::Quit => break,
                other => sim.process_input(other),
            }
        }

        let dt = last_tick.elapsed().as_secs_f32();
        last_tick = Instant::now();
        sim.update(dt);

        // Advance scenario events
        if let Some(ref mut runner) = scenario_runner {
            runner.update(&mut sim, dt);
        }

        renderer.render_with_message(&sim, sim.scenario_message.as_deref())?;

        let elapsed = last_tick.elapsed();
        if elapsed < tick_rate {
            std::thread::sleep(tick_rate - elapsed);
        }
    }

    renderer.cleanup()?;
    Ok(())
}

#[cfg(feature = "gfx")]
async fn run_gfx(scenario_path: Option<PathBuf>) {
    use renderer::Renderer;

    let mut renderer = renderer_gfx::GfxRenderer::new();
    let _ = renderer.init();

    let mut sim = Simulation::new();

    let mut scenario_runner = match &scenario_path {
        Some(path) => ScenarioRunner::load(path).ok(),
        None => None,
    };

    if let Some(ref runner) = scenario_runner {
        runner.apply_initial(&mut sim);
    } else {
        sim.seed_neutrons(config::INITIAL_SEED_NEUTRONS);
    }

    loop {
        if let Ok(Some(event)) = renderer.handle_input() {
            match event {
                InputEvent::Quit => break,
                other => sim.process_input(other),
            }
        }

        let dt = macroquad::time::get_frame_time();
        sim.update(dt);

        if let Some(ref mut runner) = scenario_runner {
            runner.update(&mut sim, dt);
        }

        let _ = renderer.render(&sim);

        macroquad::window::next_frame().await;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let use_gfx = args.iter().any(|a| a == "--gfx");
    let scenario_path = parse_scenario_arg();

    if use_gfx {
        #[cfg(feature = "gfx")]
        {
            macroquad::Window::from_config(renderer_gfx::macroquad_conf(), run_gfx(scenario_path));
        }
        #[cfg(not(feature = "gfx"))]
        {
            eprintln!(
                "Graphical renderer not available. Build with: cargo run --features gfx -- --gfx"
            );
            std::process::exit(1);
        }
    } else {
        #[cfg(feature = "tui")]
        {
            run_tui(scenario_path)?;
        }
        #[cfg(not(feature = "tui"))]
        {
            eprintln!("TUI renderer not available. Build with default features or --features tui");
            std::process::exit(1);
        }
    }

    Ok(())
}
