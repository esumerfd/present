use crate::assets::{AssetKind, Topic};
use crate::firework::Firework;
use crate::state::PresentationState;
use crossterm::event::KeyCode;
use std::collections::HashSet;
use std::time::{Instant, SystemTime};

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
        };
        if let Ok(Some(state)) = crate::state::load_state(assets_dir) {
            app.apply_state(state);
        }
        Ok(app)
    }

    fn collect_state(&self) -> PresentationState {
        let panel_per_topic = self.topics.iter().map(|t| t.current_panel).collect();
        let mut visited: Vec<usize> = self.visited.iter().copied().collect();
        visited.sort();
        PresentationState { current_topic: self.current_topic, panel_per_topic, visited }
    }

    fn apply_state(&mut self, state: PresentationState) {
        self.current_topic = state.current_topic.min(self.topics.len().saturating_sub(1));
        for (i, &panel) in state.panel_per_topic.iter().enumerate() {
            if let Some(topic) = self.topics.get_mut(i) {
                topic.current_panel = panel.min(topic.panels.len().saturating_sub(1));
            }
        }
        self.visited = state.visited.into_iter().collect();
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

        match crate::claude::send(&content, &topic_name, panel_idx) {
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
        let _ = crate::state::clear_state(&self.assets_dir);
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
        };
        app.apply_state(state);
        assert_eq!(app.current_topic, 1);
        assert_eq!(app.topics[0].current_panel, 2);
        assert_eq!(app.topics[1].current_panel, 1);
        assert!(app.visited.contains(&0));
        assert!(app.visited.contains(&1));
        assert_eq!(app.screen, Screen::Topic);
    }

    #[test]
    fn apply_state_clamps_panel_to_topic_length() {
        let mut app = make_app(&[2]);
        let state = PresentationState {
            current_topic: 0,
            panel_per_topic: vec![99],
            visited: vec![0],
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
        }
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

    fn tempdir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("present-app-test-{suffix}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    fn write_panel(topic_dir: &std::path::Path, panel: &str, text: &str) {
        let panel_dir = topic_dir.join(panel);
        std::fs::create_dir_all(&panel_dir).unwrap();
        std::fs::write(panel_dir.join("text.md"), text).unwrap();
    }

    #[test]
    fn tick_reloads_topics_when_assets_dir_changes_after_poll_interval() {
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
}
