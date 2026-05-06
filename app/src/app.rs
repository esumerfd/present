use crate::assets::{AssetKind, Topic};
use crate::firework::Firework;
use crossterm::event::KeyCode;
use std::collections::HashSet;
use std::time::Instant;

fn dbg(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/present.log") {
        let _ = writeln!(f, "{msg}");
    }
}

const COUNTDOWN_SECS: u64 = 3;
const JOKE_SECS: u64 = 5;

pub const JOKES: &[&str] = &[
    "Tonight's presenter... probably.",
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
    "He pair programs. Claude is the other half.",
    "Debugs with Claude. Ships with confidence.",
    "Stack Overflow? He hasn't visited in months.",
    "His IDE autocompletes his thoughts before he has them.",
    "Background: .NET developer. Foreground: AI enthusiast.",
    "Charges by the token. Tonight is free.",
    "He doesn't Google anymore. He prompts.",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Intro,
    Topic,
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
    pub countdown_start: Option<Instant>,
    pub joke_index: usize,
    pub joke_timer: Instant,
}

impl App {
    pub fn new(assets_dir: &str) -> anyhow::Result<Self> {
        let topics = crate::assets::load_topics(assets_dir)?;
        for (i, t) in topics.iter().enumerate() {
            dbg(&format!("topic[{}] {} panels={}", i, t.name, t.panels.len()));
        }
        Ok(Self {
            screen: Screen::Intro,
            topics,
            current_topic: 0,
            visited: HashSet::new(),
            firework: None,
            status_message: None,
            pending_label: String::new(),
            countdown_start: None,
            joke_index: 0,
            joke_timer: Instant::now(),
        })
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
                    KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Down => self.next_panel(),
                    KeyCode::Char('h') | KeyCode::Up => self.prev_panel(),
                    KeyCode::Right => self.next_topic(),
                    KeyCode::Left => self.prev_topic(),
                    KeyCode::Char('s') => self.stage_send(),
                    _ => {}
                }
            }
            Screen::Confirm => match key {
                KeyCode::Enter | KeyCode::Char('g') | KeyCode::Char(' ') => {
                    self.screen = Screen::Countdown;
                    self.countdown_start = Some(Instant::now());
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                    self.screen = Screen::Topic;
                    self.status_message = Some("Cancelled".to_string());
                }
                _ => {}
            },
            Screen::Countdown => match key {
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                    self.screen = Screen::Topic;
                    self.countdown_start = None;
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
        if let Some(topic) = self.topics.get_mut(idx) {
            topic.current_panel = 0;
        }
        if !self.visited.contains(&idx) {
            self.visited.insert(idx);
            self.firework = Some(Firework::new());
        }
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
        } else if self.current_topic > 0 {
            let prev_idx = self.current_topic - 1;
            let last_panel = self.topics[prev_idx].panels.len().saturating_sub(1);
            self.enter_topic(prev_idx);
            self.topics[prev_idx].current_panel = last_panel;
        }
    }

    fn stage_send(&mut self) {
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let Some(panel) = topic.current_panel() else { return };
        let Some(asset) = panel.prompt() else {
            self.status_message = Some("No prompt on this panel".to_string());
            return;
        };
        let AssetKind::Prompt { label, .. } = &asset.kind else { return };
        self.pending_label = label.clone();
        self.screen = Screen::Confirm;
    }

    fn execute_send(&mut self) {
        let Some(topic) = self.topics.get(self.current_topic) else { return };
        let panel_idx = topic.current_panel;
        let topic_name = topic.name.clone();
        let Some(panel) = topic.current_panel() else { return };
        let Some(asset) = panel.prompt() else { return };
        let AssetKind::Prompt { content, .. } = &asset.kind else { return };
        let content = content.clone();

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Panel;
    use std::collections::HashSet;

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
            countdown_start: None,
            joke_index: 0,
            joke_timer: Instant::now(),
        }
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
}
