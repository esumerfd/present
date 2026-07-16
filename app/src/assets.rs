use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

    pub fn word_cloud(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| matches!(a.kind, AssetKind::WordCloud { .. }))
    }

    pub fn has_word_cloud(&self) -> bool {
        self.assets.iter().any(|a| matches!(a.kind, AssetKind::WordCloud { .. }))
    }

    pub fn image(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| matches!(a.kind, AssetKind::Image { .. }))
    }

    pub fn has_image(&self) -> bool {
        self.assets.iter().any(|a| matches!(a.kind, AssetKind::Image { .. }))
    }
}

#[derive(Debug, Clone)]
pub struct Asset {
    #[allow(dead_code)]
    pub path: PathBuf,
    pub kind: AssetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordCloudSize {
    Small,
    #[default]
    Medium,
    Large,
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
    WordCloud {
        title: String,
        words: Vec<String>,
        size: WordCloudSize,
    },
    Image {
        image: image::DynamicImage,
    },
}

/// A cheap fingerprint of an assets directory: total file count and the
/// most recent modification time, walked recursively. Used to detect when
/// the directory has changed on disk without re-parsing its contents.
pub fn dir_signature(assets_dir: &str) -> Result<(usize, SystemTime)> {
    let path = Path::new(assets_dir);
    if !path.exists() {
        return Ok((0, SystemTime::UNIX_EPOCH));
    }
    let mut count = 0usize;
    let mut latest = SystemTime::UNIX_EPOCH;
    walk_dir_signature(path, &mut count, &mut latest)?;
    Ok((count, latest))
}

fn walk_dir_signature(dir: &Path, count: &mut usize, latest: &mut SystemTime) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            walk_dir_signature(&path, count, latest)?;
        } else {
            *count += 1;
            if let Ok(modified) = metadata.modified() {
                if modified > *latest {
                    *latest = modified;
                }
            }
        }
    }
    Ok(())
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
        let word_cloud_file = dir.join("word-cloud.md");
        if word_cloud_file.exists() {
            let raw = fs::read_to_string(&word_cloud_file)?;
            let (front_matter, body) = split_front_matter(&raw);
            let title = extract_label(body);
            let words = parse_word_cloud_words(body);
            let size = parse_word_cloud_size(front_matter);
            assets.push(Asset { path: word_cloud_file, kind: AssetKind::WordCloud { title, words, size } });
        }

        for ext in &["jpg", "jpeg", "png"] {
            let image_file = dir.join(format!("image.{ext}"));
            if image_file.exists() {
                match image::open(&image_file) {
                    Ok(img) => {
                        // Pre-scale to a terminal-appropriate maximum so render-time resize is fast.
                        // A 300-column terminal needs at most ~600×400 pixels via halfblock.
                        let img = img.resize(600, 400, image::imageops::FilterType::Triangle);
                        assets.push(Asset { path: image_file, kind: AssetKind::Image { image: img } });
                    }
                    Err(e) => eprintln!("Failed to load image {}: {e}", image_file.display()),
                }
                break;
            }
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

    #[test]
    fn parse_word_cloud_words_strips_headings_and_blanks() {
        let content = "# My Cloud\n\nownership\nborrowing\n\nmemory safety\n";
        assert_eq!(
            parse_word_cloud_words(content),
            vec!["ownership", "borrowing", "memory safety"]
        );
    }

    #[test]
    fn parse_word_cloud_words_no_heading() {
        let content = "async\ntraits\nlifetimes\n";
        assert_eq!(
            parse_word_cloud_words(content),
            vec!["async", "traits", "lifetimes"]
        );
    }

    #[test]
    fn parse_word_cloud_words_trims_whitespace() {
        let content = "  ownership  \n  borrowing  \n";
        assert_eq!(
            parse_word_cloud_words(content),
            vec!["ownership", "borrowing"]
        );
    }

    #[test]
    fn has_image_false_when_absent() {
        let text = Asset {
            path: PathBuf::from("text.md"),
            kind: AssetKind::Text { content: "hello".into() },
        };
        let panel = Panel { assets: vec![text] };
        assert!(!panel.has_image());
    }

    #[test]
    fn has_image_true_when_present() {
        use image::{DynamicImage, ImageBuffer, Rgb};
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(1, 1, |_, _| Rgb([0u8, 0, 0])));
        let asset = Asset {
            path: PathBuf::from("image.jpg"),
            kind: AssetKind::Image { image: img },
        };
        let panel = Panel { assets: vec![asset] };
        assert!(panel.has_image());
    }

    #[test]
    fn image_returns_the_asset() {
        use image::{DynamicImage, ImageBuffer, Rgb};
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(1, 1, |_, _| Rgb([0u8, 0, 0])));
        let asset = Asset {
            path: PathBuf::from("image.jpg"),
            kind: AssetKind::Image { image: img },
        };
        let panel = Panel { assets: vec![asset] };
        let found = panel.image().expect("should find image asset");
        assert!(matches!(found.kind, AssetKind::Image { .. }));
    }

    #[test]
    fn has_word_cloud_true_when_present() {
        let cloud = Asset {
            path: PathBuf::from("word-cloud.md"),
            kind: AssetKind::WordCloud {
                title: "Test".into(),
                words: vec!["foo".into()],
                size: WordCloudSize::default(),
            },
        };
        let panel = Panel { assets: vec![cloud] };
        assert!(panel.has_word_cloud());
    }

    #[test]
    fn has_word_cloud_false_when_absent() {
        let text = Asset {
            path: PathBuf::from("text.md"),
            kind: AssetKind::Text { content: "hello".into() },
        };
        let panel = Panel { assets: vec![text] };
        assert!(!panel.has_word_cloud());
    }

    #[test]
    fn word_cloud_returns_the_asset() {
        let cloud = Asset {
            path: PathBuf::from("word-cloud.md"),
            kind: AssetKind::WordCloud {
                title: "Cloud".into(),
                words: vec!["rust".into()],
                size: WordCloudSize::default(),
            },
        };
        let panel = Panel { assets: vec![cloud] };
        let asset = panel.word_cloud().expect("should find word cloud");
        assert!(matches!(asset.kind, AssetKind::WordCloud { .. }));
    }

    fn tempdir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("present-assets-test-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dir_signature_same_for_unchanged_directory() {
        let dir = tempdir("sig-unchanged");
        fs::write(dir.join("text.md"), "hello").unwrap();

        let first = dir_signature(dir.to_str().unwrap()).unwrap();
        let second = dir_signature(dir.to_str().unwrap()).unwrap();
        assert_eq!(first, second);
        cleanup(&dir);
    }

    #[test]
    fn dir_signature_changes_when_file_added() {
        let dir = tempdir("sig-file-added");
        fs::write(dir.join("text.md"), "hello").unwrap();
        let before = dir_signature(dir.to_str().unwrap()).unwrap();

        fs::write(dir.join("prompt.md"), "a new file").unwrap();
        let after = dir_signature(dir.to_str().unwrap()).unwrap();

        assert_ne!(before, after, "signature should change when a file is added");
        cleanup(&dir);
    }

    #[test]
    fn dir_signature_sees_files_in_nested_topic_and_panel_dirs() {
        let dir = tempdir("sig-nested");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("text.md"), "hello").unwrap();
        let before = dir_signature(dir.to_str().unwrap()).unwrap();

        fs::write(panel_dir.join("prompt.md"), "a new nested file").unwrap();
        let after = dir_signature(dir.to_str().unwrap()).unwrap();

        assert_ne!(before, after, "signature should detect changes in nested topic/panel dirs");
        cleanup(&dir);
    }

    #[test]
    fn dir_signature_is_zero_for_missing_directory() {
        let sig = dir_signature("/nonexistent-assets-dir-xyz").unwrap();
        assert_eq!(sig, (0, std::time::SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn split_front_matter_returns_none_when_absent() {
        let content = "# My Cloud\n\nownership\nborrowing\n";
        let (front, body) = split_front_matter(content);
        assert!(front.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn split_front_matter_extracts_block_and_strips_it_from_body() {
        let content = "---\nsize: large\n---\n# My Cloud\n\nownership\n";
        let (front, body) = split_front_matter(content);
        assert_eq!(front, Some("size: large"));
        assert!(!body.contains("size: large"));
        assert!(body.trim_start().starts_with("# My Cloud"), "body was: {body:?}");
    }

    #[test]
    fn parse_word_cloud_size_defaults_to_medium_when_front_matter_absent() {
        assert_eq!(parse_word_cloud_size(None), WordCloudSize::Medium);
    }

    #[test]
    fn parse_word_cloud_size_parses_large() {
        assert_eq!(parse_word_cloud_size(Some("size: large")), WordCloudSize::Large);
    }

    #[test]
    fn parse_word_cloud_size_parses_small_case_insensitively() {
        assert_eq!(parse_word_cloud_size(Some("SIZE: Small")), WordCloudSize::Small);
    }

    #[test]
    fn parse_word_cloud_size_falls_back_to_medium_for_unknown_value() {
        assert_eq!(parse_word_cloud_size(Some("size: gigantic")), WordCloudSize::Medium);
    }

    #[test]
    fn load_topics_parses_word_cloud_size_from_front_matter() {
        let dir = tempdir("wordcloud-size");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(
            panel_dir.join("word-cloud.md"),
            "---\nsize: large\n---\n# My Cloud\n\nownership\nborrowing\n",
        )
        .unwrap();

        let topics = load_topics(dir.to_str().unwrap()).unwrap();
        let asset = topics[0].panels[0].word_cloud().expect("should find word cloud");
        let AssetKind::WordCloud { title, words, size } = &asset.kind else {
            panic!("expected WordCloud asset");
        };
        assert_eq!(title, "My Cloud");
        assert_eq!(words, &vec!["ownership".to_string(), "borrowing".to_string()]);
        assert_eq!(*size, WordCloudSize::Large);
        cleanup(&dir);
    }

    #[test]
    fn load_topics_defaults_word_cloud_size_to_medium_without_front_matter() {
        let dir = tempdir("wordcloud-size-default");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("word-cloud.md"), "# My Cloud\n\nownership\n").unwrap();

        let topics = load_topics(dir.to_str().unwrap()).unwrap();
        let asset = topics[0].panels[0].word_cloud().expect("should find word cloud");
        let AssetKind::WordCloud { size, .. } = &asset.kind else {
            panic!("expected WordCloud asset");
        };
        assert_eq!(*size, WordCloudSize::Medium);
        cleanup(&dir);
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

/// Splits a leading `---`-delimited front matter block off the given content,
/// returning the front matter's inner text (without the delimiters) and the
/// remaining body. Returns `None` for the front matter when no such block is present.
fn split_front_matter(content: &str) -> (Option<&str>, &str) {
    let Some(rest) = content.strip_prefix("---\n") else {
        return (None, content);
    };
    let Some(end) = rest.find("\n---") else {
        return (None, content);
    };
    let front_matter = &rest[..end];
    let after = &rest[end + "\n---".len()..];
    let body = after.strip_prefix('\n').unwrap_or(after);
    (Some(front_matter), body)
}

fn parse_word_cloud_size(front_matter: Option<&str>) -> WordCloudSize {
    let Some(front_matter) = front_matter else { return WordCloudSize::default() };
    for line in front_matter.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        if !key.trim().eq_ignore_ascii_case("size") {
            continue;
        }
        return match value.trim().to_ascii_lowercase().as_str() {
            "small" => WordCloudSize::Small,
            "large" => WordCloudSize::Large,
            _ => WordCloudSize::default(),
        };
    }
    WordCloudSize::default()
}

fn parse_word_cloud_words(content: &str) -> Vec<String> {
    content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.trim().to_string())
        .collect()
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
