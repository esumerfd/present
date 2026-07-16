mod app;
mod assets;
mod claude;
mod export;
mod firework;
mod markdown;
mod mermaid;
mod notes;
mod state;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

enum Args {
    Help,
    Version,
    Export { assets_dir: String, output: String },
    Notes { assets_dir: String },
    Run { assets_dir: String, reset: bool },
}

fn parse_args(args: Vec<String>) -> Args {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Args::Help;
    }
    if args.iter().any(|a| a == "--version" || a == "-v") {
        return Args::Version;
    }

    let mut assets_dir: Option<String> = None;
    let mut export_output: Option<String> = None;
    let mut reset = false;
    let mut notes = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--export" => {
                export_output = args.get(i + 1).cloned();
                i += 2;
            }
            "--reset" => {
                reset = true;
                i += 1;
            }
            "--notes" => {
                notes = true;
                i += 1;
            }
            other => {
                if assets_dir.is_none() {
                    assets_dir = Some(other.to_string());
                }
                i += 1;
            }
        }
    }

    let assets_dir = assets_dir.unwrap_or_else(|| "assets".to_string());

    if notes {
        return Args::Notes { assets_dir };
    }
    if let Some(output) = export_output {
        return Args::Export { assets_dir, output };
    }
    Args::Run { assets_dir, reset }
}

fn help_text() -> &'static str {
    "Usage: cinnug-presentation [ASSETS_DIR] [OPTIONS]

Arguments:
  [ASSETS_DIR]         Path to assets directory [default: assets]

Options:
  --export <FILE>      Export all topic text and prompts to a file and exit
  --reset              Clear saved position and start from the beginning
  --notes              Run as a presenter-notes display (second monitor)
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
        Args::Export { assets_dir, output } => {
            export::export_to_file(&assets_dir, &output)?;
            println!("Exported to {output}");
            Ok(())
        }
        Args::Notes { assets_dir } => run_notes(&assets_dir),
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

fn run_notes(assets_dir: &str) -> Result<()> {
    let mut app = notes::NotesApp::new(assets_dir)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_notes_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_notes_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut notes::NotesApp,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    loop {
        terminal.draw(|f| ui::render_notes(f, app))?;

        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    return Ok(());
                }
            }
        }

        app.tick();
    }
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
    fn assets_dir_positional_arg() {
        let args = vec!["/path/to/assets".to_string()];
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
        let args = vec!["/tmp".to_string(), "--reset".to_string()];
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

    #[test]
    fn export_flag_parses_output_filename() {
        let args = vec!["--export".to_string(), "output.txt".to_string()];
        let Args::Export { output, .. } = parse_args(args) else {
            panic!("expected Export variant");
        };
        assert_eq!(output, "output.txt");
    }

    #[test]
    fn export_flag_uses_default_assets_dir() {
        let args = vec!["--export".to_string(), "out.txt".to_string()];
        let Args::Export { assets_dir, .. } = parse_args(args) else {
            panic!("expected Export variant");
        };
        assert_eq!(assets_dir, "assets");
    }

    #[test]
    fn export_flag_combines_with_assets_dir() {
        let args = vec!["/tmp".to_string(), "--export".to_string(), "out.txt".to_string()];
        let Args::Export { assets_dir, output } = parse_args(args) else {
            panic!("expected Export variant");
        };
        assert_eq!(assets_dir, "/tmp");
        assert_eq!(output, "out.txt");
    }

    #[test]
    fn positional_assets_dir_can_follow_flags() {
        let args = vec!["--reset".to_string(), "/tmp".to_string()];
        let Args::Run { assets_dir, reset } = parse_args(args) else {
            panic!("expected Run variant");
        };
        assert_eq!(assets_dir, "/tmp");
        assert!(reset);
    }

    #[test]
    fn notes_flag_parses_to_notes_variant() {
        let args = vec!["--notes".to_string()];
        assert!(matches!(parse_args(args), Args::Notes { .. }));
    }

    #[test]
    fn notes_flag_uses_default_assets_dir() {
        let args = vec!["--notes".to_string()];
        let Args::Notes { assets_dir } = parse_args(args) else {
            panic!("expected Notes variant");
        };
        assert_eq!(assets_dir, "assets");
    }

    #[test]
    fn notes_flag_combines_with_assets_dir() {
        let args = vec!["/tmp".to_string(), "--notes".to_string()];
        let Args::Notes { assets_dir } = parse_args(args) else {
            panic!("expected Notes variant");
        };
        assert_eq!(assets_dir, "/tmp");
    }
}
