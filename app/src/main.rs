mod app;
mod assets;
mod claude;
mod firework;
mod markdown;
mod mermaid;
mod state;
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
    Version,
    Run { assets_dir: String, reset: bool },
}

fn parse_args(args: Vec<String>) -> Args {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Args::Help;
    }
    if args.iter().any(|a| a == "--version" || a == "-v") {
        return Args::Version;
    }
    let assets_dir = args
        .windows(2)
        .find(|w| w[0] == "--assets-dir")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "assets".to_string());
    let reset = args.iter().any(|a| a == "--reset");
    Args::Run { assets_dir, reset }
}

fn help_text() -> &'static str {
    "Usage: cinnug-presentation [OPTIONS]

Options:
  --assets-dir <PATH>  Path to assets directory [default: assets]
  --reset              Clear saved position and start from the beginning
  -h, --help           Print help
  -v, --version        Print version
"
}

fn main() -> Result<()> {
    match parse_args(std::env::args().skip(1).collect()) {
        Args::Help => {
            print!("{}", help_text());
            Ok(())
        }
        Args::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Args::Run { assets_dir, reset } => run(&assets_dir, reset),
    }
}

fn run(assets_dir: &str, reset: bool) -> Result<()> {
    if reset {
        let _ = crate::state::clear_state(assets_dir);
    }
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
        let Args::Run { assets_dir, .. } = parse_args(args) else {
            panic!("expected Run variant");
        };
        assert_eq!(assets_dir, "/path/to/assets");
    }

    #[test]
    fn default_assets_dir() {
        let Args::Run { assets_dir, .. } = parse_args(vec![]) else {
            panic!("expected Run variant");
        };
        assert_eq!(assets_dir, "assets");
    }

    #[test]
    fn reset_flag_sets_reset_true() {
        let Args::Run { reset, .. } = parse_args(vec!["--reset".to_string()]) else {
            panic!("expected Run variant");
        };
        assert!(reset);
    }

    #[test]
    fn reset_flag_absent_by_default() {
        let Args::Run { reset, .. } = parse_args(vec![]) else {
            panic!("expected Run variant");
        };
        assert!(!reset);
    }

    #[test]
    fn reset_flag_combines_with_assets_dir() {
        let args = vec!["--assets-dir".to_string(), "/tmp".to_string(), "--reset".to_string()];
        let Args::Run { assets_dir, reset } = parse_args(args) else {
            panic!("expected Run variant");
        };
        assert_eq!(assets_dir, "/tmp");
        assert!(reset);
    }

    #[test]
    fn version_long_flag() {
        assert!(matches!(parse_args(vec!["--version".to_string()]), Args::Version));
    }

    #[test]
    fn version_short_flag() {
        assert!(matches!(parse_args(vec!["-v".to_string()]), Args::Version));
    }
}
