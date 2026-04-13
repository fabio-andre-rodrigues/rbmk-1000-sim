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

fn find_scenarios() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir("scenarios") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

fn interactive_menu() -> (bool, Option<PathBuf>) {
    println!();
    println!("  ╔══════════════════════════════════════════╗");
    println!("  ║   RBMK-1000 NUCLEAR REACTOR SIMULATION  ║");
    println!("  ╚══════════════════════════════════════════╝");
    println!();

    // Renderer selection
    let use_gfx;
    #[cfg(all(feature = "tui", feature = "gfx"))]
    {
        println!("  Select renderer:");
        println!("    [1] Terminal (TUI)");
        println!("    [2] Graphical (windowed)");
        println!();
        use_gfx = loop {
            eprint!("  > ");
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_err() {
                break false;
            }
            match input.trim() {
                "1" => break false,
                "2" => break true,
                _ => println!("  Enter 1 or 2"),
            }
        };
        println!();
    }
    #[cfg(all(feature = "tui", not(feature = "gfx")))]
    {
        use_gfx = false;
    }
    #[cfg(all(feature = "gfx", not(feature = "tui")))]
    {
        use_gfx = true;
    }
    #[cfg(not(any(feature = "tui", feature = "gfx")))]
    {
        use_gfx = false;
    }

    // Scenario selection
    let scenarios = find_scenarios();
    let scenario_path = if scenarios.is_empty() {
        println!("  No scenarios found. Starting free play.");
        println!();
        None
    } else {
        println!("  Select scenario:");
        println!("    [0] Free play (no scenario)");
        for (i, path) in scenarios.iter().enumerate() {
            let name = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
            println!("    [{}] {}", i + 1, name);
        }
        println!();
        loop {
            eprint!("  > ");
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_err() {
                break None;
            }
            let trimmed = input.trim();
            if trimmed == "0" {
                break None;
            }
            if let Ok(n) = trimmed.parse::<usize>() {
                if n >= 1 && n <= scenarios.len() {
                    break Some(scenarios[n - 1].clone());
                }
            }
            println!("  Enter 0-{}", scenarios.len());
        }
    };

    (use_gfx, scenario_path)
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

    let mut scenario_runner = match &scenario_path {
        Some(path) => {
            let runner = ScenarioRunner::load(path)?;
            Some(runner)
        }
        None => None,
    };

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
    let has_flags = args.iter().any(|a| a.starts_with("--"));

    // If CLI flags are provided, use them directly (backwards compatible).
    // Otherwise, show interactive menu.
    let (use_gfx, scenario_path) = if has_flags {
        let use_gfx = args.iter().any(|a| a == "--gfx");
        let scenario_path = parse_scenario_arg();
        (use_gfx, scenario_path)
    } else {
        interactive_menu()
    };

    if use_gfx {
        #[cfg(feature = "gfx")]
        {
            macroquad::Window::from_config(renderer_gfx::macroquad_conf(), run_gfx(scenario_path));
        }
        #[cfg(not(feature = "gfx"))]
        {
            eprintln!("Graphical renderer not available in this build.");
            std::process::exit(1);
        }
    } else {
        #[cfg(feature = "tui")]
        {
            run_tui(scenario_path)?;
        }
        #[cfg(not(feature = "tui"))]
        {
            eprintln!("TUI renderer not available in this build.");
            std::process::exit(1);
        }
    }

    Ok(())
}
