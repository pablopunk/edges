use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::{info, warn};

mod border;
mod events;
mod ffi;
mod renderer;
mod settings;
mod window_manager;

use events::WindowEvent;
use settings::Settings;
use window_manager::WindowManager;

#[derive(Parser, Debug)]
#[command(name = "edges", about = "Lightweight window borders for macOS", version)]
struct Cli {
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,
    #[arg(long)]
    style: Option<String>,
    #[arg(long)]
    width: Option<f32>,
    #[arg(long)]
    active_color: Option<String>,
    #[arg(long)]
    inactive_color: Option<String>,
    #[arg(long)]
    hidpi: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "Starting edges");

    let cli = Cli::parse();
    let mut settings = Settings::default();

    // Load config file: --config path, or ~/.config/edges/edges.toml
    let config_path = cli.config.clone().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(format!("{}/.config/edges/edges.toml", home))
    });
    if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<settings::ConfigFile>(&content) {
                Ok(cfg) => {
                    cfg.apply(&mut settings);
                    info!(?config_path, "Loaded config file");
                }
                Err(e) => warn!(?config_path, error = %e, "Failed to parse config"),
            },
            Err(e) => warn!(?config_path, error = %e, "Failed to read config"),
        }
    }

    // CLI args override config file
    if let Some(style) = cli.style {
        settings.style = match style.as_str() {
            "round"   => settings::BorderStyle::Round,
            "square"  => settings::BorderStyle::Square,
            "uniform" => settings::BorderStyle::Uniform,
            _ => { warn!("Unknown style '{}', using default", style); settings.style }
        };
    }
    if let Some(w) = cli.width { settings.width = w; }
    if cli.hidpi { settings.hidpi = true; }
    if let Some(ref c) = cli.active_color {
        if let Some(hex) = settings::parse_hex(c) {
            settings.colors.active = settings::ColorSpec::Solid { color: hex };
        }
    }
    if let Some(ref c) = cli.inactive_color {
        if let Some(hex) = settings::parse_hex(c) {
            settings.colors.inactive = settings::ColorSpec::Solid { color: hex };
        }
    }

    info!(style = ?settings.style, width = settings.width, "Configuration loaded");

    let settings = Arc::new(settings);
    let mut wm = WindowManager::new(Arc::clone(&settings))?;

    let cid = ffi::skylight::main_connection();
    info!("Window server connection: {}", cid);

    // Add borders for all existing windows (matches JB main.c flow)
    wm.add_existing_windows();

    ctrlc::set_handler(|| {
        info!("Shutdown signal received");
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    info!("Edges is running. Press Ctrl+C to stop.");

    // Enter event loop (blocks forever)
    unsafe {
        events::run_event_loop(cid, Box::new(move |event| {
            handle_event(&mut wm, event);
        }));
    }

    Ok(())
}

fn handle_event(wm: &mut WindowManager, event: WindowEvent) {
    use WindowEvent::*;
    match event {
        Created(wid, sid)    => wm.window_created(wid, sid),
        Destroyed(wid, sid)  => wm.window_destroyed(wid, sid),
        Moved(wid)           => wm.window_moved(wid),
        Resized(wid)         => wm.window_updated(wid),
        Reordered(wid)       => wm.window_reordered(wid),
        LevelChanged(wid)    => wm.window_updated(wid),
        Hidden(wid)          => wm.window_hidden(wid),
        Unhidden(wid)        => wm.window_unhidden(wid),
        TitleChanged(_)      => wm.focus_changed(),
        WindowUpdate(_)      => wm.focus_changed(),
        WindowClose(wid)     => wm.window_closed(wid),
        SpaceChanged         => wm.space_changed(),
        FrontChanged         => wm.focus_changed(),
    }
}
