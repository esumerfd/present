mod app;
mod assets;
mod claude;
mod export;
mod firework;
mod git;
mod iterm2;
mod markdown;
mod mermaid;
mod notes;
mod state;
mod ui;

use anyhow::Result;
use app::{App, EditorLaunch, Screen};
use crossterm::{
    cursor::MoveTo,
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

/// Whether to wipe the real terminal before drawing this frame, to clear stale image
/// bytes from an earlier raw OSC write that ratatui's own buffer diffing doesn't know
/// about.
fn should_clear_stale_image(had_image_escape: bool, panel_has_image: bool, panel_changed: bool) -> bool {
    had_image_escape && (!panel_has_image || panel_changed)
}

fn run_app<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    // Tracks the raw OSC-1337 escape sequence last written to the real terminal (if
    // any), so we don't resend an unchanged image every tick, plus the previous
    // frame's screen, so we can tell when a popup (Help/Confirm/Countdown) just
    // closed. Both live outside ui::render because they describe what's actually on
    // the physical terminal, not app/UI state -- ratatui's own buffer diffing has no
    // idea these raw-painted pixels exist.
    let mut last_image_escape: Option<String> = None;
    let mut last_screen: Option<Screen> = None;
    let mut last_panel_position: Option<(usize, usize)> = None;

    loop {
        let panel_has_image = app.screen == Screen::Topic
            && app
                .topics
                .get(app.current_topic)
                .and_then(|t| t.current_panel())
                .map(|p| p.has_image())
                .unwrap_or(false);

        let panel_position = (app.screen == Screen::Topic)
            .then(|| app.topics.get(app.current_topic).map(|t| (app.current_topic, t.current_panel)))
            .flatten();
        let panel_changed = panel_position != last_panel_position;
        last_panel_position = panel_position;

        // If this frame's panel won't have an image, or the panel changed since the
        // last image was painted, wipe the real terminal *before* drawing so any stale
        // image bytes left by an earlier raw write are cleared and the fresh content
        // lands on a clean screen in the same pass -- clearing after the draw would
        // flash the just-drawn frame blank for a tick. The panel-changed case matters
        // even between two image-bearing panels: the image Rect can move (e.g. a
        // shrink-to-fit text block above it grows or shrinks), so the old pixels can
        // sit outside the new image's coordinates and never get overwritten.
        if should_clear_stale_image(last_image_escape.is_some(), panel_has_image, panel_changed) {
            terminal.clear()?;
            last_image_escape = None;
        }

        let mut pending_image = None;
        terminal.draw(|f| {
            pending_image = ui::render(f, app);
        })?;

        if app.screen == Screen::Topic {
            if let Some(image) = &pending_image {
                // A popup that was covering (part of) the image area leaves a gap
                // when it closes, even though the image itself hasn't changed, so
                // force a repaint on the first frame back on Topic.
                let just_returned_to_topic = last_screen != Some(Screen::Topic);
                if just_returned_to_topic || last_image_escape.as_deref() != Some(image.escape.as_str()) {
                    paint_image(terminal, image)?;
                    last_image_escape = Some(image.escape.clone());
                }
            }
        }
        last_screen = Some(app.screen);

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.handle_key(key.code) {
                        return Ok(());
                    }
                    if let Some(launch) = app.pending_editor.take() {
                        match run_editor(terminal, &launch)? {
                            Ok(status) => {
                                if !status.success() {
                                    app.status_message = Some(format!("nvim exited with {status}"));
                                }
                                // Whatever nvim left behind, commit it -- a no-op if
                                // nothing actually changed under the panel's dir.
                                app.commit_editor_changes(&launch);
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Failed to launch nvim: {e}"));
                            }
                        }
                        // nvim owned the real screen; force a full repaint (and, if
                        // the current panel has one, a fresh image write) next frame.
                        terminal.clear()?;
                        last_image_escape = None;
                    }
                }
            }
        }

        app.tick();
    }
}

/// Suspends the TUI's terminal state, runs `nvim` (or whatever `launch.args`
/// specify) synchronously with inherited stdio so it gets a normal interactive
/// terminal, then restores raw mode/alternate screen/mouse capture once it exits.
fn run_editor<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    launch: &EditorLaunch,
) -> Result<io::Result<std::process::ExitStatus>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    let result = std::process::Command::new("nvim")
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .status();

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
    Ok(result)
}

/// Writes a raw terminal-graphics-protocol escape sequence directly to the real
/// terminal, positioned at the image's cell coordinates. This bypasses ratatui's
/// buffer/diffing entirely -- iTerm2 decodes and rasterizes the embedded image data
/// itself once it receives the sequence.
fn paint_image<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    image: &ui::PendingImage,
) -> Result<()> {
    execute!(terminal.backend_mut(), MoveTo(image.x, image.y))?;
    write!(terminal.backend_mut(), "{}", image.escape)?;
    std::io::Write::flush(terminal.backend_mut())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_clear_stale_image_when_new_panel_has_no_image() {
        assert!(should_clear_stale_image(true, false, false));
    }

    #[test]
    fn should_clear_stale_image_when_panel_changed_even_if_new_panel_has_an_image() {
        // The image Rect can move between two image-bearing panels (e.g. shrink-to-fit
        // text above it grows/shrinks), so a same-has-image panel change must still clear.
        assert!(should_clear_stale_image(true, true, true));
    }

    #[test]
    fn should_not_clear_stale_image_when_same_panel_still_has_the_same_image() {
        assert!(!should_clear_stale_image(true, true, false));
    }

    #[test]
    fn should_not_clear_when_nothing_was_ever_painted() {
        assert!(!should_clear_stale_image(false, false, false));
        assert!(!should_clear_stale_image(false, true, true));
    }

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
