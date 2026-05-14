use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Topic {
    pub name: String,
    pub label: String,
    pub panels: Vec<Panel>,
    pub current_panel: usize,
}

impl Topic {
    pub fn current_panel(&self) -> Option<&Panel> {
        self.panels.get(self.current_panel)
    }

    pub fn current_panel_mut(&mut self) -> Option<&mut Panel> {
        self.panels.get_mut(self.current_panel)
    }
}

#[derive(Debug, Clone)]
pub struct Panel {
    pub assets: Vec<Asset>,
}

impl Panel {
    pub fn prompt(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| matches!(a.kind, AssetKind::Prompt { .. }))
    }

    pub fn prompt_mut(&mut self) -> Option<&mut Asset> {
        self.assets.iter_mut().find(|a| matches!(a.kind, AssetKind::Prompt { .. }))
    }

    pub fn has_prompt(&self) -> bool {
        self.assets.iter().any(|a| matches!(a.kind, AssetKind::Prompt { .. }))
    }
}

#[derive(Debug, Clone)]
pub struct Asset {
    #[allow(dead_code)]
    pub path: PathBuf,
    pub kind: AssetKind,
}

#[derive(Debug, Clone)]
pub enum AssetKind {
    Prompt {
        label: String,
        content: String,
        sent: bool,
    },
    Diagram {
        content: String,
    },
    Text {
        content: String,
    },
}

pub fn load_topics(assets_dir: &str) -> Result<Vec<Topic>> {
    let path = Path::new(assets_dir);
    if !path.exists() {
        return Ok(vec![]);
    }

    let mut entries: Vec<_> = fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_dir()
                && e.file_name()
                    .to_string_lossy()
                    .starts_with(|c: char| c.is_ascii_digit())
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut topics = Vec::new();
    for entry in entries {
        let topic_path = entry.path();
        let name = topic_path.file_name().unwrap().to_string_lossy().to_string();
        let label = dir_name_to_label(&name);
        let panels = load_panels(&topic_path)?;
        topics.push(Topic { name, label, panels, current_panel: 0 });
    }

    Ok(topics)
}

fn load_panels(topic_path: &Path) -> Result<Vec<Panel>> {
    let mut entries: Vec<_> = fs::read_dir(topic_path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| {
        e.file_name().to_string_lossy().parse::<u32>().unwrap_or(u32::MAX)
    });

    let mut panels = Vec::new();
    for entry in entries {
        let dir = entry.path();
        let mut assets = Vec::new();

        let prompt_file  = dir.join("prompt.md");
        let diagram_file = dir.join("diagram.md");
        let text_file    = dir.join("text.md");

        if prompt_file.exists() {
            let raw = fs::read_to_string(&prompt_file)?;
            let label = extract_label(&raw);
            let content = strip_heading(&raw);
            assets.push(Asset {
                path: prompt_file,
                kind: AssetKind::Prompt { label, content, sent: false },
            });
        }
        if diagram_file.exists() {
            let content = fs::read_to_string(&diagram_file)?;
            assets.push(Asset { path: diagram_file, kind: AssetKind::Diagram { content } });
        }
        if text_file.exists() {
            let content = fs::read_to_string(&text_file)?;
            assets.push(Asset { path: text_file, kind: AssetKind::Text { content } });
        }

        if !assets.is_empty() {
            panels.push(Panel { assets });
        }
    }

    Ok(panels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_heading_with_hash_heading() {
        let content = "# Build Hello World\n\n/workflow:plan";
        assert_eq!(strip_heading(content), "/workflow:plan");
    }

    #[test]
    fn strip_heading_with_plain_text_title() {
        let content = "Build Hello World\n\n/workflow:plan";
        assert_eq!(strip_heading(content), "/workflow:plan");
    }

    #[test]
    fn extract_label_from_hash_heading() {
        assert_eq!(extract_label("# Build Hello World\n\n/workflow:plan"), "Build Hello World");
    }

    #[test]
    fn extract_label_from_plain_text() {
        assert_eq!(extract_label("Build Hello World\n\n/workflow:plan"), "Build Hello World");
    }
}

fn dir_name_to_label(name: &str) -> String {
    let stripped = name.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '_');
    stripped.replace(['-', '_'], " ")
}

fn extract_label(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim_start_matches('#').trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "untitled".to_string()
}

fn strip_heading(content: &str) -> String {
    let mut lines = content.lines();
    for line in lines.by_ref() {
        if !line.trim().is_empty() {
            break;
        }
    }
    lines
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
