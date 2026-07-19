use crate::assets::{AssetKind, Topic};
use crate::firework::Firework;
use crate::state::PresentationState;
use crossterm::event::KeyCode;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

/// A queued external-editor invocation: `nvim -o <files...>`, run with `cwd` as its
/// working directory. Built by `App::nvim_launch` and drained by the main loop, which
/// suspends the TUI's terminal state around the actual `Command::spawn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorLaunch {
    pub cwd: PathBuf,
    pub args: Vec<String>,
    /// The edited panel's directory, relative to the assets dir (e.g. `01-topic/1`),
    /// for scoping the post-edit git commit to just that folder.
    pub panel_rel_dir: String,
}

fn dbg(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/present.log") {
        let _ = writeln!(f, "{msg}");
    }
}

const COUNTDOWN_SECS: u64 = 3;
const JOKE_SECS: u64 = 5;
const POLL_SECS: u64 = 5;

pub const JOKES: &[&str] = &[
    "Warning: This is tonight's presenter.",
    "He asked Claude to write this joke.",
    "Warning: may hallucinate. Just like his AI.",
    "100% human. Claude won't confirm or deny.",
    "He writes the prompts. Claude does the work.",
    "Passed the Turing test. Barely.",
    "Not a large language model. Just large opinions.",
    "Context window: 8 hours of sleep, 3 coffees.",
    "His commit messages are written by Claude.",
    "Available via API. No rate limits.",
    "Slides? Where we're going, we don't need slides.",
    "This talk was peer-reviewed by an LLM.",
    "His rubber duck is a language model.",
    "Debugs with Claude. Ships with confidence.",
    "Stack Overflow? He hasn't visited in months.",
    "His IDE autocompletes his thoughts before he has them.",
    "Background: .NET developer. Foreground: AI enthusiast.",
    "Charges by the token. Tonight is free.",
    "He doesn't Google anymore. He prompts.",
    "His favorite design pattern is the prompt pattern.",
    "He once had a conversation with Claude. It lasted 3 days.",
    "His code reviews are done by Claude. He just nods along.",
    "He doesn't write unit tests. He writes prompts and hopes for the best.",
    "His favorite programming language is the one Claude responds best to.",
    "He doesn't have bugs. Just unexpected features that Claude will fix.",
    "He once tried to fine-tune a model. It fine-tuned him instead.",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Intro,
    Topic,
    Help,
    Confirm,
    Countdown,
}

pub struct App {
    pub screen: Screen,
    pub topics: Vec<Topic>,
    pub current_topic: usize,
    pub visited: HashSet<usize>,
    pub firework: Option<Firework>,
    pub status_message: Option<String>,
    pub pending_label: String,
    pub pending_content: String,
    pub selected_line: usize,
    pub selected_lines: HashSet<usize>,
    pub countdown_start: Option<Instant>,
    pub joke_index: usize,
    pub joke_timer: Instant,
    pub assets_dir: String,
    pub last_poll: Instant,
    pub last_dir_signature: (usize, SystemTime),
    pub started_at: SystemTime,
    pub pending_editor: Option<EditorLaunch>,
    sender: Box<dyn crate::claude::ClaudeSender>,
    git: Box<dyn crate::git::GitCommitter>,
}

impl App {
    pub fn new(assets_dir: &str) -> anyhow::Result<Self> {
        let topics = crate::assets::load_topics(assets_dir)?;
        for (i, t) in topics.iter().enumerate() {
            dbg(&format!("topic[{}] {} panels={}", i, t.name, t.panels.len()));
        }
        let last_dir_signature = crate::assets::dir_signature(assets_dir)
            .unwrap_or((0, SystemTime::UNIX_EPOCH));
        let mut app = Self {
            screen: Screen::Intro,
            topics,
            current_topic: 0,
            visited: HashSet::new(),
            firework: None,
            status_message: None,
            pending_label: String::new(),
            pending_content: String::new(),
            selected_line: 0,
            selected_lines: HashSet::new(),
            countdown_start: None,
            joke_index: 0,
            joke_timer: Instant::now(),
            assets_dir: assets_dir.to_string(),
            last_poll: Instant::now(),
            last_dir_signature,
            started_at: SystemTime::now(),
            pending_editor: None,
            sender: Box::new(crate::claude::RealSender),
            git: Box::new(crate::git::RealGit),
        };
        if let Ok(Some(state)) = crate::state::load_state(assets_dir) {
            app.apply_state(state);
        } else {
            // No prior state for this assets dir -- this is a fresh start, so persist
            // it immediately rather than waiting for the first navigation, so the
            // notes window's elapsed-time clock has a start time to read right away.
            app.persist_state();
        }
        Ok(app)
    }

    fn collect_state(&self) -> PresentationState {
        let panel_per_topic = self.topics.iter().map(|t| t.current_panel).collect();
        let mut visited: Vec<usize> = self.visited.iter().copied().collect();
        visited.sort();
        let started_at_epoch_secs =
            self.started_at.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs());
        PresentationState { current_topic: self.current_topic, panel_per_topic, visited, started_at_epoch_secs }
    }

    fn apply_state(&mut self, state: PresentationState) {
        self.current_topic = state.current_topic.min(self.topics.len().saturating_sub(1));
        for (i, &panel) in state.panel_per_topic.iter().enumerate() {
            if let Some(topic) = self.topics.get_mut(i) {
                topic.current_panel = panel.min(topic.panels.len().saturating_sub(1));
            }
        }
        self.visited = state.visited.into_iter().collect();
        if let Some(secs) = state.started_at_epoch_secs {
            self.started_at = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        }
        self.screen = Screen::Topic;
    }

    fn persist_state(&self) {
        if self.assets_dir.is_empty() {
            return;
        }
        let _ = crate::state::save_state(&self.assets_dir, &self.collect_state());
    }

    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        dbg(&format!("key={key:?} screen={:?} topic={} panel={}", self.screen, self.current_topic,
            self.topics.get(self.current_topic).map(|t| t.current_panel).unwrap_or(99)));
        match self.screen {
            Screen::Intro => match key {
                KeyCode::Char(' ') | KeyCode::Enter => {
                    self.screen = Screen::Topic;
                    self.enter_topic(0);
                }
                KeyCode::Char('q') => return true,
                _ => {}
            },
            Screen::Topic => {
                self.status_message = None;
                match key {
                    KeyCode::Char('q') => return true,
                    KeyCode::Char('?') => self.screen = Screen::Help,
                    KeyCode::Char('R') => self.reset(),
                    KeyCode::Char('c') => self.toggle_selected_line(),
                    KeyCode::Char('C') => self.copy_text_path_to_clipboard(),
                    KeyCode::Char('V') => self.open_in_editor(),
                    KeyCode::Char('H') => self.reorder_panel(-1),
                    KeyCode::Char('L') => self.reorder_panel(1),
                    KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Down => self.next_panel(),
                    KeyCode::Char('h') | KeyCode::Up => self.prev_panel(),
                    KeyCode::Right => self.next_topic(),
                    KeyCode::Left => self.prev_topic(),
                    KeyCode::Char('j') => {
                        let count = self.current_prompt_line_count();
                        if count > 0 {
                            self.selected_line = (self.selected_line + 1).min(count - 1);
                        }
                    }
                    KeyCode::Char('k') => {
                        self.selected_line = self.selected_line.saturating_sub(1);
                    }
                    KeyCode::Char('s') => self.stage_send_selected(),
                    KeyCode::Char('S') => self.stage_send_all(),
                    _ => {}
                }
            }
            Screen::Help => match key {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.screen = Screen::Topic;
                }
                _ => {}
            },
            Screen::Confirm => match key {
                KeyCode::Enter | KeyCode::Char('g') | KeyCode::Char(' ') => {
                    self.screen = Screen::Countdown;
                    self.countdown_start = Some(Instant::now());
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                    self.screen = Screen::Topic;
                    self.selected_lines.clear();
                    self.status_message = Some("Cancelled".to_string());
                }
                _ => {}
            },
            Screen::Countdown => match key {
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                    self.screen = Screen::Topic;
                    self.countdown_start = None;
                    self.selected_lines.clear();
                    self.status_message = Some("Cancelled".to_string());
                }
                _ => {}
            },
        }
        false
    }

    pub fn countdown_remaining(&self) -> u64 {
        match self.countdown_start {
            Some(start) => COUNTDOWN_SECS.saturating_sub(start.elapsed().as_secs()),
            None => 0,
        }
    }

    fn enter_topic(&mut self, idx: usize) {
        self.current_topic = idx;
        self.selected_line = 0;
        self.selected_lines.clear();
        if let Some(topic) = self.topics.get_mut(idx) {
            topic.current_panel = 0;
        }
        if !self.visited.contains(&idx) {
            self.visited.insert(idx);
            self.firework = Some(Firework::new());
        }
        self.persist_state();
    }

    fn next_topic(&mut self) {
        let next = self.current_topic + 1;
        if next < self.topics.len() {
            self.enter_topic(next);
        }
    }

    fn prev_topic(&mut self) {
        if self.current_topic > 0 {
            self.enter_topic(self.current_topic - 1);
        }
    }

    fn next_panel(&mut self) {
        let (panel_count, current_panel) = {
            let Some(topic) = self.topics.get(self.current_topic) else { return };
            (topic.panels.len(), topic.current_panel)
        };
        let next = current_panel + 1;
        dbg(&format!("next_panel: topic={} panel={} count={} next={}", self.current_topic, current_panel, panel_count, next));
        if next < panel_count {
            self.topics[self.current_topic].current_panel = next;
            self.selected_line = 0;
            self.selected_lines.clear();
            self.persist_state();
        } else {
            let next_topic = self.current_topic + 1;
            if next_topic < self.topics.len() {
                self.enter_topic(next_topic);
            }
        }
    }

    fn prev_panel(&mut self) {
        let current_panel = {
            let Some(topic) = self.topics.get(self.current_topic) else { return };
            topic.current_panel
        };
        if current_panel > 0 {
            self.topics[self.current_topic].current_panel = current_panel - 1;
            self.selected_line = 0;
            self.selected_lines.clear();
            self.persist_state();
        } else if self.current_topic > 0 {
            let prev_idx = self.current_topic - 1;
            let last_panel = self.topics[prev_idx].panels.len().saturating_sub(1);
            self.enter_topic(prev_idx);
            self.topics[prev_idx].current_panel = last_panel;
        }
    }

    fn current_prompt_line_count(&self) -> usize {
        self.topics.get(self.current_topic)
            .and_then(|t| t.current_panel())
            .and_then(|p| p.prompt())
            .map(|a| match &a.kind {
                AssetKind::Prompt { content, .. } => content.lines().filter(|l| !l.trim().is_empty()).count(),
                _ => 0,
            })
            .unwrap_or(0)
    }

    fn toggle_selected_line(&mut self) {
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let Some(panel) = topic.current_panel() else { return };
        let Some(asset) = panel.prompt() else {
            self.status_message = Some("No prompt on this panel".to_string());
            return;
        };
        let AssetKind::Prompt { content, .. } = &asset.kind else { return };
        let line_count = content.lines().filter(|l| !l.trim().is_empty()).count();
        if line_count == 0 { return; }
        let idx = self.selected_line.min(line_count - 1);
        if !self.selected_lines.remove(&idx) {
            self.selected_lines.insert(idx);
        }
    }

    fn stage_send_selected(&mut self) {
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let Some(panel) = topic.current_panel() else { return };
        let Some(asset) = panel.prompt() else {
            self.status_message = Some("No prompt on this panel".to_string());
            return;
        };
        let AssetKind::Prompt { label, content, .. } = &asset.kind else { return };
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() { return; }

        let selected_content = if self.selected_lines.is_empty() {
            let idx = self.selected_line.min(lines.len() - 1);
            lines[idx].to_string()
        } else {
            let mut indices: Vec<usize> = self.selected_lines.iter().copied().collect();
            indices.sort_unstable();
            indices
                .iter()
                .filter_map(|&i| lines.get(i).copied())
                .collect::<Vec<&str>>()
                .join("\n")
        };

        self.pending_label = label.clone();
        self.pending_content = selected_content;
        self.screen = Screen::Confirm;
    }

    fn stage_send_all(&mut self) {
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let Some(panel) = topic.current_panel() else { return };
        let Some(asset) = panel.prompt() else {
            self.status_message = Some("No prompt on this panel".to_string());
            return;
        };
        let AssetKind::Prompt { label, content, .. } = &asset.kind else { return };
        self.pending_label = label.clone();
        self.pending_content = content.clone();
        self.screen = Screen::Confirm;
    }

    fn execute_send(&mut self) {
        self.selected_lines.clear();
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let panel_idx = topic.current_panel;
        let topic_name = topic.name.clone();
        let content = self.pending_content.clone();

        match crate::claude::send_with(&content, &topic_name, panel_idx, self.sender.as_ref()) {
            Ok(msg) => {
                self.status_message = Some(msg);
                if let Some(topic) = self.topics.get_mut(self.current_topic) {
                    if let Some(panel) = topic.current_panel_mut() {
                        if let Some(asset) = panel.prompt_mut() {
                            if let AssetKind::Prompt { sent, .. } = &mut asset.kind {
                                *sent = true;
                            }
                        }
                    }
                }
            }
            Err(e) => self.status_message = Some(format!("Error: {e}")),
        }
    }

    fn copy_text_path_to_clipboard(&mut self) {
        let path = self.topics.get(self.current_topic)
            .and_then(|t| t.current_panel())
            .and_then(|p| p.assets.iter().find(|a| matches!(a.kind, AssetKind::Text { .. })))
            .map(|a| a.path.to_string_lossy().to_string());

        match path {
            Some(p) => {
                use std::io::Write;
                use std::process::{Command, Stdio};
                let ok = Command::new("pbcopy")
                    .stdin(Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        child.stdin.take().unwrap().write_all(p.as_bytes())?;
                        child.wait()
                    })
                    .is_ok();
                self.status_message = Some(if ok {
                    format!("Path copied: {p}")
                } else {
                    "Failed to copy path".to_string()
                });
            }
            None => {
                self.status_message = Some("No text file on this panel".to_string());
            }
        }
    }

    /// Queues an `nvim -o` launch for the current panel's markdown files (`text.md`
    /// first, then the rest alphabetically), cwd'd into the assets dir. The main loop
    /// drains `pending_editor` and actually runs it, since only it holds the terminal
    /// handle needed to suspend/restore the TUI around the external process.
    fn open_in_editor(&mut self) {
        self.pending_editor = self.nvim_launch();
        if self.pending_editor.is_none() {
            self.status_message = Some("No markdown files on this panel".to_string());
        }
    }

    fn nvim_launch(&self) -> Option<EditorLaunch> {
        let topic = self.topics.get(self.current_topic)?;
        let panel = topic.current_panel()?;
        let md_paths = panel.markdown_paths();
        if md_paths.is_empty() {
            return None;
        }
        let panel_dir = crate::assets::panel_dir_name(panel)?;
        let mut args = vec!["-o".to_string()];
        for path in &md_paths {
            let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            args.push(absolute.to_string_lossy().to_string());
        }
        let cwd = std::fs::canonicalize(&self.assets_dir).unwrap_or_else(|_| PathBuf::from(&self.assets_dir));
        Some(EditorLaunch { cwd, args, panel_rel_dir: format!("{}/{panel_dir}", topic.name) })
    }

    /// Commits whatever changes nvim left in the edited panel's directory, scoped so
    /// nothing else in the working tree is touched. No-op when `assets_dir` isn't a
    /// git repo, or when nvim didn't actually change anything.
    pub fn commit_editor_changes(&mut self, launch: &EditorLaunch) {
        if !self.git.is_repo(&self.assets_dir) {
            return;
        }
        let paths = vec![launch.panel_rel_dir.clone()];
        let message = format!("Edit panel {}", launch.panel_rel_dir);
        if let Err(e) = self.git.commit_paths(&self.assets_dir, &paths, &message) {
            self.status_message = Some(format!("Git commit failed: {e}"));
        }
    }

    /// Swaps the current panel's on-disk directory number with its previous
    /// (`direction: -1`, "H") or next (`direction: 1`, "L") neighbor, reordering it,
    /// and moves the cursor along with it so the presenter keeps viewing the same
    /// content. A no-op at either end of the panel list.
    fn reorder_panel(&mut self, direction: i32) {
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let current = topic.current_panel;
        let Some(target) = current.checked_add_signed(direction as isize) else { return };
        if target >= topic.panels.len() {
            return;
        }
        let topic_name = topic.name.clone();
        let Some(dir_a) = crate::assets::panel_dir_name(&topic.panels[current]) else { return };
        let Some(dir_b) = crate::assets::panel_dir_name(&topic.panels[target]) else { return };

        let Some(topic) = self.topics.get_mut(self.current_topic) else { return };
        match crate::assets::swap_panels(&self.assets_dir, topic, current, target) {
            Ok(()) => {
                topic.current_panel = target;
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to reorder panel: {e}"));
                return;
            }
        }
        self.persist_state();
        self.commit_reorder(&topic_name, &dir_a, &dir_b);
    }

    /// Commits the two panel directories swapped by `reorder_panel`, scoped so
    /// nothing else in the working tree is touched. No-op when `assets_dir` isn't a
    /// git repo. Directory *names* stay the same across the swap (only their content
    /// trades places), but git's own rename detection still recognizes it correctly
    /// per-file, as long as the two panels don't share an identical filename.
    fn commit_reorder(&mut self, topic_name: &str, dir_a: &str, dir_b: &str) {
        if !self.git.is_repo(&self.assets_dir) {
            return;
        }
        let paths = vec![format!("{topic_name}/{dir_a}"), format!("{topic_name}/{dir_b}")];
        let message = format!("Swap panels {dir_a} and {dir_b}");
        if let Err(e) = self.git.commit_paths(&self.assets_dir, &paths, &message) {
            self.status_message = Some(format!("Git commit failed: {e}"));
        }
    }

    fn reset(&mut self) {
        self.current_topic = 0;
        for topic in &mut self.topics {
            topic.current_panel = 0;
        }
        self.visited = HashSet::new();
        self.selected_line = 0;
        self.selected_lines.clear();
        self.screen = Screen::Topic;
        self.status_message = Some("State cleared — starting from the beginning".to_string());
        self.started_at = SystemTime::now();
        let _ = crate::state::clear_state(&self.assets_dir);
        self.persist_state();
    }

    pub fn tick(&mut self) {
        if self.screen == Screen::Intro && self.joke_timer.elapsed().as_secs() >= JOKE_SECS {
            self.joke_index = (self.joke_index + 1) % JOKES.len();
            self.joke_timer = Instant::now();
        }

        if let Some(fw) = &mut self.firework {
            fw.tick();
            if fw.done() {
                self.firework = None;
            }
        }

        if self.screen == Screen::Countdown {
            if let Some(start) = self.countdown_start {
                if start.elapsed().as_secs() >= COUNTDOWN_SECS {
                    self.countdown_start = None;
                    self.screen = Screen::Topic;
                    self.execute_send();
                }
            }
        }

        if self.screen == Screen::Topic && self.last_poll.elapsed().as_secs() >= POLL_SECS {
            self.last_poll = Instant::now();
            self.poll_for_changes();
        }
    }

    fn poll_for_changes(&mut self) {
        let Ok(signature) = crate::assets::dir_signature(&self.assets_dir) else { return };
        if signature != self.last_dir_signature {
            self.last_dir_signature = signature;
            self.reload_topics();
        }
    }

    fn reload_topics(&mut self) {
        let Ok(new_topics) = crate::assets::load_topics(&self.assets_dir) else { return };
        let panel_positions: Vec<usize> = self.topics.iter().map(|t| t.current_panel).collect();

        self.topics = new_topics;
        for (topic, panel) in self.topics.iter_mut().zip(panel_positions) {
            topic.current_panel = panel.min(topic.panels.len().saturating_sub(1));
        }
        self.current_topic = self.current_topic.min(self.topics.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Asset, AssetKind, Panel};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn make_app(panel_counts: &[usize]) -> App {
        let topics = panel_counts.iter().enumerate().map(|(i, &n)| Topic {
            name: format!("topic-{i}"),
            label: format!("Topic {i}"),
            panels: (0..n).map(|_| Panel { assets: vec![] }).collect(),
            current_panel: 0,
        }).collect();
        App {
            screen: Screen::Topic,
            topics,
            current_topic: 0,
            visited: HashSet::from([0]),
            firework: None,
            status_message: None,
            pending_label: String::new(),
            pending_content: String::new(),
            selected_line: 0,
            selected_lines: HashSet::new(),
            countdown_start: None,
            joke_index: 0,
            joke_timer: Instant::now(),
            assets_dir: String::new(),
            last_poll: Instant::now(),
            last_dir_signature: (0, SystemTime::UNIX_EPOCH),
        started_at: SystemTime::now(),
        pending_editor: None,
        sender: Box::new(crate::claude::NoopSender),
        git: Box::new(crate::git::NoopGit),
        }
    }

    fn make_app_with_prompt(content: &str) -> App {
        let prompt = Asset {
            path: PathBuf::from("prompt.md"),
            kind: AssetKind::Prompt {
                label: "Test Prompt".into(),
                content: content.to_string(),
                sent: false,
            },
        };
        let panel = Panel { assets: vec![prompt] };
        let topic = Topic {
            name: "topic-0".into(),
            label: "Topic 0".into(),
            panels: vec![panel],
            current_panel: 0,
        };
        App {
            screen: Screen::Topic,
            topics: vec![topic],
            current_topic: 0,
            visited: HashSet::from([0]),
            firework: None,
            status_message: None,
            pending_label: String::new(),
            pending_content: String::new(),
            selected_line: 0,
            selected_lines: HashSet::new(),
            countdown_start: None,
            joke_index: 0,
            joke_timer: Instant::now(),
            assets_dir: String::new(),
            last_poll: Instant::now(),
            last_dir_signature: (0, SystemTime::UNIX_EPOCH),
        started_at: SystemTime::now(),
        pending_editor: None,
        sender: Box::new(crate::claude::NoopSender),
        git: Box::new(crate::git::NoopGit),
        }
    }

    #[test]
    fn collects_state_from_current_position() {
        let mut app = make_app(&[3, 2]);
        app.current_topic = 1;
        app.topics[0].current_panel = 2;
        app.topics[1].current_panel = 1;
        app.visited = HashSet::from([0, 1]);

        let state = app.collect_state();
        assert_eq!(state.current_topic, 1);
        assert_eq!(state.panel_per_topic, vec![2, 1]);
        let mut visited = state.visited.clone();
        visited.sort();
        assert_eq!(visited, vec![0, 1]);
    }

    #[test]
    fn applies_state_to_restore_position() {
        let mut app = make_app(&[3, 2]);
        let state = PresentationState {
            current_topic: 1,
            panel_per_topic: vec![2, 1],
            visited: vec![0, 1],
            started_at_epoch_secs: Some(1_600_000_000),
        };
        app.apply_state(state);
        assert_eq!(app.current_topic, 1);
        assert_eq!(app.topics[0].current_panel, 2);
        assert_eq!(app.topics[1].current_panel, 1);
        assert!(app.visited.contains(&0));
        assert!(app.visited.contains(&1));
        assert_eq!(app.screen, Screen::Topic);
        assert_eq!(
            app.started_at.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
            1_600_000_000,
            "should adopt the persisted start time rather than resetting it"
        );
    }

    #[test]
    fn apply_state_clamps_panel_to_topic_length() {
        let mut app = make_app(&[2]);
        let state = PresentationState {
            current_topic: 0,
            panel_per_topic: vec![99],
            visited: vec![0],
            started_at_epoch_secs: None,
        };
        app.apply_state(state);
        assert_eq!(app.topics[0].current_panel, 1, "clamped to last panel index");
    }

    #[test]
    fn advances_panels_within_topic() {
        let mut app = make_app(&[3, 2]);
        assert_eq!(app.current_topic, 0);
        assert_eq!(app.topics[0].current_panel, 0);

        app.next_panel();
        assert_eq!(app.current_topic, 0, "should stay on topic 0");
        assert_eq!(app.topics[0].current_panel, 1, "should advance to panel 1");

        app.next_panel();
        assert_eq!(app.current_topic, 0, "should stay on topic 0");
        assert_eq!(app.topics[0].current_panel, 2, "should advance to panel 2");
    }

    #[test]
    fn advances_to_next_topic_at_last_panel() {
        let mut app = make_app(&[3, 2]);
        app.topics[0].current_panel = 2; // already at last panel

        app.next_panel();
        assert_eq!(app.current_topic, 1, "should advance to topic 1");
        assert_eq!(app.topics[1].current_panel, 0, "new topic starts at panel 0");
    }

    #[test]
    fn j_advances_selected_line_in_topic_with_prompt() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 0;
        app.handle_key(KeyCode::Char('j'));
        assert_eq!(app.selected_line, 1);
    }

    #[test]
    fn k_retreats_selected_line_in_topic_with_prompt() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 2;
        app.handle_key(KeyCode::Char('k'));
        assert_eq!(app.selected_line, 1);
    }

    #[test]
    fn j_clamps_at_last_prompt_line() {
        let mut app = make_app_with_prompt("line1\nline2");
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('j'));
        assert_eq!(app.selected_line, 1);
    }

    #[test]
    fn k_clamps_at_first_prompt_line() {
        let mut app = make_app_with_prompt("line1\nline2");
        app.selected_line = 0;
        app.handle_key(KeyCode::Char('k'));
        assert_eq!(app.selected_line, 0);
    }

    #[test]
    fn s_sends_selected_prompt_line_to_confirm() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Confirm);
        assert_eq!(app.pending_content, "line2");
    }

    #[test]
    fn s_ignores_blank_lines_when_selecting() {
        let mut app = make_app_with_prompt("line1\n\nline2\nline3");
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.pending_content, "line2");
    }

    #[test]
    fn selected_line_resets_on_panel_change() {
        let mut app = make_app_with_prompt("line1\nline2");
        app.topics[0].panels.push(Panel { assets: vec![] });
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('l'));
        assert_eq!(app.selected_line, 0);
    }

    #[test]
    fn shift_s_sends_all_lines_directly_to_confirm() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.handle_key(KeyCode::Char('S'));
        assert_eq!(app.screen, Screen::Confirm);
        assert_eq!(app.pending_content, "line1\nline2\nline3");
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = make_app(&[2]);
        app.handle_key(KeyCode::Char('?'));
        assert_eq!(app.screen, Screen::Help);
    }

    #[test]
    fn question_mark_closes_help() {
        let mut app = make_app(&[2]);
        app.screen = Screen::Help;
        app.handle_key(KeyCode::Char('?'));
        assert_eq!(app.screen, Screen::Topic);
    }

    #[test]
    fn esc_closes_help() {
        let mut app = make_app(&[2]);
        app.screen = Screen::Help;
        app.handle_key(KeyCode::Esc);
        assert_eq!(app.screen, Screen::Topic);
    }

    #[test]
    fn capital_r_resets_to_topic_zero_panel_zero() {
        let mut app = make_app(&[3, 2]);
        app.current_topic = 1;
        app.topics[0].current_panel = 2;
        app.topics[1].current_panel = 1;
        app.selected_line = 1;
        app.visited = HashSet::from([0, 1]);

        app.handle_key(KeyCode::Char('R'));

        assert_eq!(app.current_topic, 0);
        assert_eq!(app.topics[0].current_panel, 0);
        assert_eq!(app.topics[1].current_panel, 0);
        assert_eq!(app.selected_line, 0);
        assert!(app.visited.is_empty());
        assert_eq!(app.screen, Screen::Topic);
        assert!(app.status_message.is_some());
    }

    fn make_app_with_text_asset(path: &str) -> App {
        let text = Asset {
            path: PathBuf::from(path),
            kind: AssetKind::Text { content: "hello".into() },
        };
        let panel = Panel { assets: vec![text] };
        let topic = Topic {
            name: "topic-0".into(),
            label: "Topic 0".into(),
            panels: vec![panel],
            current_panel: 0,
        };
        App {
            screen: Screen::Topic,
            topics: vec![topic],
            current_topic: 0,
            visited: HashSet::from([0]),
            firework: None,
            status_message: None,
            pending_label: String::new(),
            pending_content: String::new(),
            selected_line: 0,
            selected_lines: HashSet::new(),
            countdown_start: None,
            joke_index: 0,
            joke_timer: Instant::now(),
            assets_dir: String::new(),
            last_poll: Instant::now(),
            last_dir_signature: (0, SystemTime::UNIX_EPOCH),
        started_at: SystemTime::now(),
        pending_editor: None,
        sender: Box::new(crate::claude::NoopSender),
        git: Box::new(crate::git::NoopGit),
        }
    }

    #[test]
    fn capital_v_opens_editor_with_markdown_files_text_md_first_then_alphabetical() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("editor-launch");
        let panel_dir = dir.join("01-topic").join("1");
        std::fs::create_dir_all(&panel_dir).unwrap();
        std::fs::write(panel_dir.join("text.md"), "body").unwrap();
        std::fs::write(panel_dir.join("prompt.md"), "# Label\n\ndo it").unwrap();
        std::fs::write(panel_dir.join("notes.md"), "remember this").unwrap();

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.handle_key(KeyCode::Char('V'));

        let launch = app.pending_editor.expect("should build an editor launch");
        assert_eq!(launch.cwd, std::fs::canonicalize(&dir).unwrap());
        let expected: Vec<String> = ["text.md", "notes.md", "prompt.md"]
            .iter()
            .map(|f| std::fs::canonicalize(panel_dir.join(f)).unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(launch.args, [vec!["-o".to_string()], expected].concat());
        cleanup(&dir);
    }

    #[test]
    fn capital_v_commits_the_edited_panel_when_assets_dir_is_a_git_repo() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("editor-commit-spy");
        let panel_dir = dir.join("01-topic").join("1");
        std::fs::create_dir_all(&panel_dir).unwrap();
        std::fs::write(panel_dir.join("text.md"), "body").unwrap();

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.handle_key(KeyCode::Char('V'));
        let launch = app.pending_editor.take().expect("should build a launch");

        let spy = std::rc::Rc::new(SpyGit::default());
        app.git = Box::new(SpyGitHandle(spy.clone()));
        app.commit_editor_changes(&launch);

        let calls = spy.calls.borrow();
        assert_eq!(calls.len(), 1, "expected exactly one git commit call, got: {calls:?}");
        let (call_dir, paths, message) = &calls[0];
        assert_eq!(call_dir, dir.to_str().unwrap());
        assert_eq!(paths, &vec!["01-topic/1".to_string()]);
        assert!(message.contains("01-topic/1"), "message should mention the panel dir: {message}");
        cleanup(&dir);
    }

    #[test]
    fn capital_v_creates_a_real_git_commit_scoped_to_the_edited_panel() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("editor-commit-real-git");
        let panel_dir = dir.join("01-topic").join("1");
        std::fs::create_dir_all(&panel_dir).unwrap();
        std::fs::write(panel_dir.join("text.md"), "body").unwrap();
        let other_dir = dir.join("01-topic").join("2");
        std::fs::create_dir_all(&other_dir).unwrap();
        std::fs::write(other_dir.join("text.md"), "untouched").unwrap();
        init_git_repo(&dir);
        git_commit_all(&dir, "Initial");

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.handle_key(KeyCode::Char('V'));
        let launch = app.pending_editor.take().expect("should build a launch");

        // Simulate the edit nvim would have made.
        std::fs::write(panel_dir.join("text.md"), "edited body").unwrap();
        app.commit_editor_changes(&launch);

        let subject = git_log_subject(&dir);
        assert!(subject.contains("01-topic/1"), "commit subject should mention the edited panel: {subject}");
        let changed = git_show_name_only(&dir);
        assert!(changed.contains("01-topic/1/text.md"), "commit should touch the edited panel: {changed}");
        assert!(!changed.contains("01-topic/2"), "commit should not touch the other panel: {changed}");
        cleanup(&dir);
    }

    #[test]
    fn capital_v_does_not_commit_when_nvim_made_no_changes() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("editor-commit-noop");
        let panel_dir = dir.join("01-topic").join("1");
        std::fs::create_dir_all(&panel_dir).unwrap();
        std::fs::write(panel_dir.join("text.md"), "body").unwrap();
        init_git_repo(&dir);
        git_commit_all(&dir, "Initial");
        let count_before = git_log_line_count(&dir);

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.handle_key(KeyCode::Char('V'));
        let launch = app.pending_editor.take().expect("should build a launch");

        app.commit_editor_changes(&launch);

        assert_eq!(git_log_line_count(&dir), count_before, "no commit should be made when nothing changed");
        cleanup(&dir);
    }

    #[test]
    fn capital_v_sets_status_when_panel_has_no_markdown_files() {
        let mut app = make_app(&[1]);
        app.handle_key(KeyCode::Char('V'));
        assert!(app.pending_editor.is_none());
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("No markdown files"), "expected 'No markdown files' in: {msg}");
    }

    fn tempdir_with_numbered_panels(suffix: &str, panel_dirs: &[&str]) -> std::path::PathBuf {
        let dir = tempdir(suffix);
        let topic_dir = dir.join("01-topic");
        for name in panel_dirs {
            let panel_dir = topic_dir.join(name);
            std::fs::create_dir_all(&panel_dir).unwrap();
            std::fs::write(panel_dir.join("text.md"), format!("content-{name}")).unwrap();
        }
        dir
    }

    fn panel_text_contents(app: &App) -> Vec<String> {
        app.topics[0]
            .panels
            .iter()
            .map(|p| {
                let asset = p.assets.iter().find(|a| matches!(a.kind, AssetKind::Text { .. })).unwrap();
                let AssetKind::Text { content } = &asset.kind else { unreachable!() };
                content.clone()
            })
            .collect()
    }

    #[test]
    fn capital_h_swaps_current_panel_with_previous_and_follows_it() {
        let _guard = crate::state::test_lock();
        let dir = tempdir_with_numbered_panels("reorder-h", &["10", "20", "30"]);

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.topics[0].current_panel = 1; // viewing "20"

        app.handle_key(KeyCode::Char('H'));

        assert_eq!(app.topics[0].current_panel, 0, "cursor should follow panel 20 to its new position");
        assert_eq!(panel_text_contents(&app), vec!["content-20", "content-10", "content-30"]);
        cleanup(&dir);
    }

    #[test]
    fn capital_h_commits_the_swap_when_assets_dir_is_a_git_repo() {
        let _guard = crate::state::test_lock();
        let dir = tempdir_with_numbered_panels("reorder-h-git-spy", &["10", "20", "30"]);

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.topics[0].current_panel = 1;
        let spy = std::rc::Rc::new(SpyGit::default());
        app.git = Box::new(SpyGitHandle(spy.clone()));

        app.handle_key(KeyCode::Char('H'));

        let calls = spy.calls.borrow();
        assert_eq!(calls.len(), 1, "expected exactly one git commit call, got: {calls:?}");
        let (call_dir, paths, message) = &calls[0];
        assert_eq!(call_dir, dir.to_str().unwrap());
        let mut sorted_paths = paths.clone();
        sorted_paths.sort();
        assert_eq!(sorted_paths, vec!["01-topic/10".to_string(), "01-topic/20".to_string()]);
        assert!(message.contains("10") && message.contains("20"), "message should mention both panels: {message}");
        cleanup(&dir);
    }

    #[test]
    fn capital_h_creates_a_real_git_commit_scoped_to_the_swapped_panels() {
        let _guard = crate::state::test_lock();
        let dir = tempdir_with_numbered_panels("reorder-h-real-git", &["10", "20", "30"]);
        init_git_repo(&dir);
        git_commit_all(&dir, "Initial");

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.topics[0].current_panel = 1;

        app.handle_key(KeyCode::Char('H'));

        let subject = git_log_subject(&dir);
        assert!(subject.contains("10") && subject.contains("20"), "commit subject should mention the swapped panels: {subject}");

        let changed = git_show_name_only(&dir);
        assert!(changed.contains("01-topic/10"), "commit should touch panel 10: {changed}");
        assert!(changed.contains("01-topic/20"), "commit should touch panel 20: {changed}");
        assert!(!changed.contains("01-topic/30"), "commit should not touch the untouched panel: {changed}");
        cleanup(&dir);
    }

    #[test]
    fn capital_h_is_a_noop_at_the_first_panel() {
        let _guard = crate::state::test_lock();
        let dir = tempdir_with_numbered_panels("reorder-h-noop", &["10", "20", "30"]);

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.topics[0].current_panel = 0;

        app.handle_key(KeyCode::Char('H'));

        assert_eq!(app.topics[0].current_panel, 0);
        assert_eq!(panel_text_contents(&app), vec!["content-10", "content-20", "content-30"]);
        cleanup(&dir);
    }

    #[test]
    fn capital_l_swaps_current_panel_with_next_and_follows_it() {
        let _guard = crate::state::test_lock();
        let dir = tempdir_with_numbered_panels("reorder-l", &["10", "20", "30"]);

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.topics[0].current_panel = 1; // viewing "20"

        app.handle_key(KeyCode::Char('L'));

        assert_eq!(app.topics[0].current_panel, 2, "cursor should follow panel 20 to its new position");
        assert_eq!(panel_text_contents(&app), vec!["content-10", "content-30", "content-20"]);
        cleanup(&dir);
    }

    #[test]
    fn capital_l_is_a_noop_at_the_last_panel() {
        let _guard = crate::state::test_lock();
        let dir = tempdir_with_numbered_panels("reorder-l-noop", &["10", "20", "30"]);

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.topics[0].current_panel = 2;

        app.handle_key(KeyCode::Char('L'));

        assert_eq!(app.topics[0].current_panel, 2);
        assert_eq!(panel_text_contents(&app), vec!["content-10", "content-20", "content-30"]);
        cleanup(&dir);
    }

    #[test]
    fn capital_c_sets_copied_status_when_text_asset_present() {
        let mut app = make_app_with_text_asset("/some/path/text.md");
        app.handle_key(KeyCode::Char('C'));
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("Path copied"), "expected 'Path copied' in: {msg}");
    }

    #[test]
    fn capital_c_sets_no_text_file_status_when_no_text_asset() {
        let mut app = make_app(&[1]);
        app.handle_key(KeyCode::Char('C'));
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("No text file"), "expected 'No text file' in: {msg}");
    }

    #[test]
    fn lowercase_c_toggles_line_into_selected_lines() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('c'));
        assert!(app.selected_lines.contains(&1));
        assert_eq!(app.screen, Screen::Topic);
    }

    #[test]
    fn lowercase_c_toggles_line_out_of_selected_lines() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('c'));
        app.handle_key(KeyCode::Char('c'));
        assert!(!app.selected_lines.contains(&1));
    }

    #[test]
    fn lowercase_c_with_no_prompt_sets_status_and_no_selection() {
        let mut app = make_app(&[1]);
        app.handle_key(KeyCode::Char('c'));
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("No prompt on this panel"), "expected 'No prompt' in: {msg}");
        assert!(app.selected_lines.is_empty());
    }

    #[test]
    fn s_sends_joined_selected_lines_in_document_order() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 2;
        app.handle_key(KeyCode::Char('c')); // select line3 first
        app.selected_line = 0;
        app.handle_key(KeyCode::Char('c')); // then select line1
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Confirm);
        assert_eq!(app.pending_content, "line1\nline3");
    }

    #[test]
    fn s_falls_back_to_cursor_line_when_no_selection() {
        let mut app = make_app_with_prompt("line1\nline2\nline3");
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Confirm);
        assert_eq!(app.pending_content, "line2");
    }

    #[test]
    fn selected_lines_resets_on_panel_change() {
        let mut app = make_app_with_prompt("line1\nline2");
        app.topics[0].panels.push(Panel { assets: vec![] });
        app.selected_line = 1;
        app.handle_key(KeyCode::Char('c'));
        assert!(!app.selected_lines.is_empty());
        app.handle_key(KeyCode::Char('l'));
        assert!(app.selected_lines.is_empty());
    }

    #[test]
    fn selected_lines_resets_after_cancel_from_confirm() {
        let mut app = make_app_with_prompt("line1\nline2");
        app.selected_line = 0;
        app.handle_key(KeyCode::Char('c'));
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Confirm);
        app.handle_key(KeyCode::Char('n'));
        assert_eq!(app.screen, Screen::Topic);
        assert!(app.selected_lines.is_empty());
    }

    #[test]
    fn selected_lines_resets_after_successful_send() {
        let mut app = make_app_with_prompt("line1\nline2");
        app.selected_line = 0;
        app.handle_key(KeyCode::Char('c'));
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Enter);
        app.countdown_start = Some(Instant::now() - std::time::Duration::from_secs(10));
        app.tick();
        assert_eq!(app.screen, Screen::Topic);
        assert!(app.selected_lines.is_empty());
    }

    #[test]
    fn execute_send_goes_through_injected_sender_not_real_commands() {
        let mut app = make_app_with_prompt("line1\nline2");
        let spy = std::rc::Rc::new(SpySender { osascript_calls: RefCell::new(0) });
        app.sender = Box::new(SpySenderHandle(spy.clone()));

        app.selected_line = 0;
        app.handle_key(KeyCode::Char('c'));
        app.handle_key(KeyCode::Char('s'));
        app.handle_key(KeyCode::Enter);
        app.countdown_start = Some(Instant::now() - std::time::Duration::from_secs(10));
        app.tick();

        assert_eq!(*spy.osascript_calls.borrow(), 1);
    }

    struct SpySender {
        osascript_calls: RefCell<usize>,
    }

    struct SpySenderHandle(std::rc::Rc<SpySender>);

    impl crate::claude::ClaudeSender for SpySenderHandle {
        fn send_via_osascript(&self, _content: &str) -> anyhow::Result<()> {
            *self.0.osascript_calls.borrow_mut() += 1;
            Ok(())
        }

        fn copy_to_clipboard(&self, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn tempdir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("present-app-test-{suffix}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    fn init_git_repo(dir: &std::path::Path) {
        assert!(std::process::Command::new("git").arg("init").arg("-q").current_dir(dir).status().unwrap().success());
        assert!(std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }

    fn git_commit_all(dir: &std::path::Path, message: &str) {
        assert!(std::process::Command::new("git").arg("add").arg("-A").current_dir(dir).status().unwrap().success());
        assert!(std::process::Command::new("git")
            .arg("commit")
            .arg("-q")
            .arg("-m")
            .arg(message)
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }

    fn git_log_line_count(dir: &std::path::Path) -> usize {
        let output = std::process::Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(dir)
            .output()
            .unwrap();
        if !output.status.success() {
            return 0;
        }
        String::from_utf8_lossy(&output.stdout).lines().count()
    }

    fn git_log_subject(dir: &std::path::Path) -> String {
        let output = std::process::Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn git_show_name_only(dir: &std::path::Path) -> String {
        let output = std::process::Command::new("git")
            .args(["show", "--stat", "--name-only", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    #[derive(Default)]
    struct SpyGit {
        calls: RefCell<Vec<(String, Vec<String>, String)>>,
    }

    struct SpyGitHandle(std::rc::Rc<SpyGit>);

    impl crate::git::GitCommitter for SpyGitHandle {
        fn is_repo(&self, _dir: &str) -> bool {
            true
        }

        fn commit_paths(&self, dir: &str, paths: &[String], message: &str) -> anyhow::Result<bool> {
            self.0.calls.borrow_mut().push((dir.to_string(), paths.to_vec(), message.to_string()));
            Ok(true)
        }
    }

    fn write_panel(topic_dir: &std::path::Path, panel: &str, text: &str) {
        let panel_dir = topic_dir.join(panel);
        std::fs::create_dir_all(&panel_dir).unwrap();
        std::fs::write(panel_dir.join("text.md"), text).unwrap();
    }

    #[test]
    fn tick_reloads_topics_when_assets_dir_changes_after_poll_interval() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("reload-on-change");
        let topic_dir = dir.join("01-topic");
        write_panel(&topic_dir, "1", "before");

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        assert_eq!(app.topics[0].panels.len(), 1, "should start with one panel");

        write_panel(&topic_dir, "2", "after");
        app.last_poll = Instant::now() - std::time::Duration::from_secs(6);
        app.tick();

        assert_eq!(app.topics[0].panels.len(), 2, "should reload to pick up the new panel");
        cleanup(&dir);
    }

    #[test]
    fn tick_does_not_reload_before_poll_interval_elapses() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("reload-too-soon");
        let topic_dir = dir.join("01-topic");
        write_panel(&topic_dir, "1", "before");

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;

        write_panel(&topic_dir, "2", "after");
        app.last_poll = Instant::now();
        app.tick();

        assert_eq!(app.topics[0].panels.len(), 1, "should not reload before the poll interval elapses");
        cleanup(&dir);
    }

    #[test]
    fn tick_does_not_reload_when_screen_is_not_topic() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("reload-wrong-screen");
        let topic_dir = dir.join("01-topic");
        write_panel(&topic_dir, "1", "before");

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Intro;

        write_panel(&topic_dir, "2", "after");
        app.last_poll = Instant::now() - std::time::Duration::from_secs(6);
        app.tick();

        assert_eq!(app.topics[0].panels.len(), 1, "should not reload while off the Topic screen");
        cleanup(&dir);
    }

    #[test]
    fn tick_reload_preserves_current_topic_and_panel_position() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("reload-preserves-position");
        let topic_dir_0 = dir.join("01-topic");
        write_panel(&topic_dir_0, "1", "a");
        write_panel(&topic_dir_0, "2", "b");
        let topic_dir_1 = dir.join("02-topic");
        write_panel(&topic_dir_1, "1", "c");

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        app.screen = Screen::Topic;
        app.current_topic = 1;
        app.topics[0].current_panel = 1;

        write_panel(&topic_dir_1, "2", "d");
        app.last_poll = Instant::now() - std::time::Duration::from_secs(6);
        app.tick();

        assert_eq!(app.current_topic, 1, "current topic should be preserved");
        assert_eq!(app.topics[0].current_panel, 1, "other topics' panel position should be preserved");
        assert_eq!(app.topics[1].panels.len(), 2, "should pick up the new panel");
        cleanup(&dir);
    }

    #[test]
    fn new_persists_a_fresh_started_at_when_no_prior_state_exists() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("fresh-started-at");
        write_panel(&dir.join("01-topic"), "1", "a");
        let _ = crate::state::clear_state(dir.to_str().unwrap());

        let before = SystemTime::now();
        let app = App::new(dir.to_str().unwrap()).unwrap();
        let after = SystemTime::now();

        assert!(
            app.started_at >= before && app.started_at <= after,
            "started_at should be set to roughly now on a fresh start"
        );
        let persisted = crate::state::load_state(dir.to_str().unwrap())
            .unwrap()
            .expect("a fresh state should be persisted immediately, not only on first navigation");
        assert!(
            persisted.started_at_epoch_secs.is_some(),
            "persisted state should capture the presentation start time"
        );
        cleanup(&dir);
    }

    #[test]
    fn new_keeps_the_persisted_started_at_instead_of_resetting_it() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("keeps-started-at");
        write_panel(&dir.join("01-topic"), "1", "a");
        crate::state::save_state(
            dir.to_str().unwrap(),
            &PresentationState {
                current_topic: 0,
                panel_per_topic: vec![0],
                visited: vec![0],
                started_at_epoch_secs: Some(1_600_000_000),
            },
        )
        .unwrap();

        let app = App::new(dir.to_str().unwrap()).unwrap();

        assert_eq!(
            app.started_at.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
            1_600_000_000,
            "resuming a presentation should not reset its start time"
        );
        cleanup(&dir);
    }

    #[test]
    fn reset_sets_a_new_started_at_and_persists_it_immediately() {
        let _guard = crate::state::test_lock();
        let dir = tempdir("reset-started-at");
        write_panel(&dir.join("01-topic"), "1", "a");
        crate::state::save_state(
            dir.to_str().unwrap(),
            &PresentationState {
                current_topic: 0,
                panel_per_topic: vec![0],
                visited: vec![0],
                started_at_epoch_secs: Some(1_600_000_000),
            },
        )
        .unwrap();

        let mut app = App::new(dir.to_str().unwrap()).unwrap();
        let before = SystemTime::now();
        app.handle_key(KeyCode::Char('R'));
        let after = SystemTime::now();

        assert!(
            app.started_at >= before && app.started_at <= after,
            "reset should set started_at to roughly now, not keep the old value"
        );
        let persisted = crate::state::load_state(dir.to_str().unwrap())
            .unwrap()
            .expect("reset should persist the new started_at immediately");
        assert_eq!(
            persisted.started_at_epoch_secs,
            app.started_at.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
        );
        cleanup(&dir);
    }
}
