mod app;
mod assets;
mod claude;
mod firework;
mod markdown;
mod mermaid;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

enum Args {
    Help,
    Run { assets_dir: String },
}

fn parse_args(args: Vec<String>) -> Args {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Args::Help;
    }
    let assets_dir = args
        .windows(2)
        .find(|w| w[0] == "--assets-dir")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "assets".to_string());
    Args::Run { assets_dir }
}

fn help_text() -> &'static str {
    "Usage: cinnug-presentation [OPTIONS]

Options:
  --assets-dir <PATH>  Path to assets directory [default: assets]
  -h, --help           Print help
"
}

fn main() -> Result<()> {
    match parse_args(std::env::args().skip(1).collect()) {
        Args::Help => {
            print!("{}", help_text());
            Ok(())
        }
        Args::Run { assets_dir } => run(&assets_dir),
    }
}

fn run(assets_dir: &str) -> Result<()> {
    let mut app = App::new(assets_dir)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.handle_key(key.code) {
                        return Ok(());
                    }
                }
            }
        }

        app.tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_short_flag() {
        assert!(matches!(parse_args(vec!["-h".to_string()]), Args::Help));
    }

    #[test]
    fn help_long_flag() {
        assert!(matches!(parse_args(vec!["--help".to_string()]), Args::Help));
    }

    #[test]
    fn assets_dir_arg() {
        let args = vec!["--assets-dir".to_string(), "/path/to/assets".to_string()];
        let Args::Run { assets_dir } = parse_args(args) else {
            panic!("expected Run variant");
        };
        assert_eq!(assets_dir, "/path/to/assets");
    }

    #[test]
    fn default_assets_dir() {
        let Args::Run { assets_dir } = parse_args(vec![]) else {
            panic!("expected Run variant");
        };
        assert_eq!(assets_dir, "assets");
    }
}
