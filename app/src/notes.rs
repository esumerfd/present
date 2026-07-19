use crate::assets::{self, Panel, Topic};
use crate::state;
use anyhow::Result;
use std::time::{Instant, SystemTime};

const ASSET_POLL_SECS: u64 = 5;
const STATE_POLL_MILLIS: u128 = 200;

pub struct NotesApp {
    pub topics: Vec<Topic>,
    pub assets_dir: String,
    pub current_topic: usize,
    pub current_panel: usize,
    pub last_asset_poll: Instant,
    pub last_state_poll: Instant,
    pub last_dir_signature: (usize, SystemTime),
    pub started_at: Instant,
}

impl NotesApp {
    pub fn new(assets_dir: &str) -> Result<Self> {
        let topics = assets::load_topics(assets_dir)?;
        let last_dir_signature =
            assets::dir_signature(assets_dir).unwrap_or((0, SystemTime::UNIX_EPOCH));
        let mut app = Self {
            topics,
            assets_dir: assets_dir.to_string(),
            current_topic: 0,
            current_panel: 0,
            last_asset_poll: Instant::now(),
            last_state_poll: Instant::now(),
            last_dir_signature,
            started_at: Instant::now(),
        };
        app.sync_position();
        Ok(app)
    }

    fn sync_position(&mut self) {
        let Ok(Some(state)) = state::load_state(&self.assets_dir) else { return };
        self.current_topic = state.current_topic.min(self.topics.len().saturating_sub(1));
        let panel_count = self
            .topics
            .get(self.current_topic)
            .map(|t| t.panels.len())
            .unwrap_or(0);
        let panel = state.panel_per_topic.get(self.current_topic).copied().unwrap_or(0);
        self.current_panel = panel.min(panel_count.saturating_sub(1));
    }

    fn poll_for_asset_changes(&mut self) {
        let Ok(signature) = assets::dir_signature(&self.assets_dir) else { return };
        if signature != self.last_dir_signature {
            self.last_dir_signature = signature;
            if let Ok(new_topics) = assets::load_topics(&self.assets_dir) {
                self.topics = new_topics;
                self.sync_position();
            }
        }
    }

    pub fn tick(&mut self) {
        if self.last_state_poll.elapsed().as_millis() >= STATE_POLL_MILLIS {
            self.last_state_poll = Instant::now();
            self.sync_position();
        }
        if self.last_asset_poll.elapsed().as_secs() >= ASSET_POLL_SECS {
            self.last_asset_poll = Instant::now();
            self.poll_for_asset_changes();
        }
    }

    pub fn current_panel(&self) -> Option<&Panel> {
        self.topics.get(self.current_topic).and_then(|t| t.panels.get(self.current_panel))
    }

    pub fn current_topic_label(&self) -> Option<&str> {
        self.topics.get(self.current_topic).map(|t| t.label.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{save_state, PresentationState};
    use std::fs;
    use std::time::Instant;

    fn tempdir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("present-notes-test-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        let _ = fs::remove_dir_all(dir);
    }

    fn write_panel(topic_dir: &std::path::Path, panel: &str, notes: Option<&str>) {
        let panel_dir = topic_dir.join(panel);
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("text.md"), "slide content").unwrap();
        if let Some(n) = notes {
            fs::write(panel_dir.join("notes.md"), n).unwrap();
        }
    }

    #[test]
    fn notes_app_new_reads_initial_position_from_state_file() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("initial-position");
        write_panel(&dir.join("01-topic"), "1", None);
        write_panel(&dir.join("01-topic"), "2", None);
        write_panel(&dir.join("02-topic"), "1", None);

        save_state(
            dir.to_str().unwrap(),
            &PresentationState { current_topic: 1, panel_per_topic: vec![1, 0], visited: vec![0, 1] },
        )
        .unwrap();

        let app = NotesApp::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(app.current_topic, 1);
        assert_eq!(app.current_panel, 0);
        cleanup(&dir);
    }

    #[test]
    fn tick_updates_position_when_state_file_changes() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("tick-updates-position");
        write_panel(&dir.join("01-topic"), "1", None);
        write_panel(&dir.join("01-topic"), "2", None);
        let _ = crate::state::clear_state(dir.to_str().unwrap());

        let mut app = NotesApp::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(app.current_panel, 0);

        save_state(
            dir.to_str().unwrap(),
            &PresentationState { current_topic: 0, panel_per_topic: vec![1], visited: vec![0] },
        )
        .unwrap();
        app.last_state_poll = Instant::now() - std::time::Duration::from_millis(300);
        app.tick();

        assert_eq!(app.current_panel, 1, "should pick up the new panel position");
        cleanup(&dir);
    }

    #[test]
    fn tick_does_not_poll_state_before_interval_elapses() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("tick-too-soon");
        write_panel(&dir.join("01-topic"), "1", None);
        write_panel(&dir.join("01-topic"), "2", None);
        let _ = crate::state::clear_state(dir.to_str().unwrap());

        let mut app = NotesApp::new(dir.to_str().unwrap()).unwrap();

        save_state(
            dir.to_str().unwrap(),
            &PresentationState { current_topic: 0, panel_per_topic: vec![1], visited: vec![0] },
        )
        .unwrap();
        app.last_state_poll = Instant::now();
        app.tick();

        assert_eq!(app.current_panel, 0, "should not pick up the change before the poll interval elapses");
        cleanup(&dir);
    }

    #[test]
    fn tick_reloads_topics_on_asset_dir_change() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("tick-reloads-topics");
        write_panel(&dir.join("01-topic"), "1", None);

        let mut app = NotesApp::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(app.topics[0].panels.len(), 1);

        write_panel(&dir.join("01-topic"), "2", None);
        app.last_asset_poll = Instant::now() - std::time::Duration::from_secs(6);
        app.tick();

        assert_eq!(app.topics[0].panels.len(), 2, "should reload to pick up the new panel");
        cleanup(&dir);
    }

    #[test]
    fn notes_app_clamps_out_of_range_position() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("clamps-out-of-range");
        write_panel(&dir.join("01-topic"), "1", None);

        save_state(
            dir.to_str().unwrap(),
            &PresentationState { current_topic: 5, panel_per_topic: vec![99], visited: vec![0] },
        )
        .unwrap();

        let app = NotesApp::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(app.current_topic, 0, "should clamp to the last available topic");
        assert_eq!(app.current_panel, 0, "should clamp to the last available panel");
        cleanup(&dir);
    }

    #[test]
    fn current_panel_returns_the_panel_at_current_position() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("current-panel-accessor");
        write_panel(&dir.join("01-topic"), "1", Some("first notes"));
        write_panel(&dir.join("01-topic"), "2", Some("second notes"));

        let mut app = NotesApp::new(dir.to_str().unwrap()).unwrap();
        app.current_panel = 1;

        let panel = app.current_panel().expect("should find a panel");
        let asset = panel.notes().expect("should find notes asset");
        let crate::assets::AssetKind::Notes { content } = &asset.kind else {
            panic!("expected Notes asset");
        };
        assert_eq!(content, "second notes");
        cleanup(&dir);
    }
}
