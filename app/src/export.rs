use crate::assets::{AssetKind, load_topics};
use anyhow::Result;
use std::fs;

pub fn export_to_file(assets_dir: &str, output_path: &str) -> Result<()> {
    let topics = load_topics(assets_dir)?;
    let mut out = String::new();

    for (i, topic) in topics.iter().enumerate() {
        if i > 0 {
            out.push_str("\n---\n\n");
        }
        out.push_str(&format!("# {}\n", topic.label));

        for panel in &topic.panels {
            for asset in &panel.assets {
                match &asset.kind {
                    AssetKind::Text { content } => {
                        out.push('\n');
                        out.push_str(content.trim_end());
                        out.push('\n');
                    }
                    AssetKind::Prompt { label, content, .. } => {
                        out.push_str(&format!("\n## {label}\n\n"));
                        out.push_str(content.trim_end());
                        out.push('\n');
                    }
                    _ => {}
                }
            }
        }
    }

    fs::write(output_path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn setup_topic(base: &Path, dir: &str, panels: &[(&str, Option<&str>, Option<&str>)]) {
        let topic_path = base.join(dir);
        fs::create_dir_all(&topic_path).unwrap();
        for (i, (_, text, prompt)) in panels.iter().enumerate() {
            let panel_path = topic_path.join(format!("{}", i + 1));
            fs::create_dir_all(&panel_path).unwrap();
            if let Some(content) = text {
                fs::write(panel_path.join("text.md"), content).unwrap();
            }
            if let Some(content) = prompt {
                fs::write(panel_path.join("prompt.md"), content).unwrap();
            }
        }
    }

    #[test]
    fn exports_topic_titles() {
        let dir = tempdir("export-titles");
        setup_topic(&dir, "01-intro", &[("", Some("# Intro\n\nHello world"), None)]);

        let out = dir.join("out.txt");
        export_to_file(dir.to_str().unwrap(), out.to_str().unwrap()).unwrap();

        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("# intro"), "expected topic title in: {content}");
        cleanup(&dir);
    }

    #[test]
    fn exports_text_content() {
        let dir = tempdir("export-text");
        setup_topic(&dir, "01-topic", &[("", Some("# Title\n\nHello from text"), None)]);

        let out = dir.join("out.txt");
        export_to_file(dir.to_str().unwrap(), out.to_str().unwrap()).unwrap();

        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("Hello from text"), "expected text content in: {content}");
        cleanup(&dir);
    }

    #[test]
    fn exports_prompt_label_and_content() {
        let dir = tempdir("export-prompt");
        setup_topic(&dir, "01-topic", &[("", None, Some("# Ask Me\n\nWhat is your question?"))]);

        let out = dir.join("out.txt");
        export_to_file(dir.to_str().unwrap(), out.to_str().unwrap()).unwrap();

        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("## Ask Me"), "expected prompt label in: {content}");
        assert!(content.contains("What is your question?"), "expected prompt content in: {content}");
        cleanup(&dir);
    }

    #[test]
    fn separates_multiple_topics_with_divider() {
        let dir = tempdir("export-multi");
        setup_topic(&dir, "01-first", &[("", Some("# First\n\nFirst topic"), None)]);
        setup_topic(&dir, "02-second", &[("", Some("# Second\n\nSecond topic"), None)]);

        let out = dir.join("out.txt");
        export_to_file(dir.to_str().unwrap(), out.to_str().unwrap()).unwrap();

        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("---"), "expected divider between topics in: {content}");
        assert!(content.contains("# first"), "expected first topic in: {content}");
        assert!(content.contains("# second"), "expected second topic in: {content}");
        cleanup(&dir);
    }

    #[test]
    fn exports_multiple_panels_per_topic() {
        let dir = tempdir("export-panels");
        setup_topic(&dir, "01-topic", &[
            ("", Some("# Panel\n\nPanel one text"), None),
            ("", None, Some("# Prompt Two\n\nPrompt two content")),
        ]);

        let out = dir.join("out.txt");
        export_to_file(dir.to_str().unwrap(), out.to_str().unwrap()).unwrap();

        let content = fs::read_to_string(&out).unwrap();
        assert!(content.contains("Panel one text"), "expected panel 1 text in: {content}");
        assert!(content.contains("Prompt two content"), "expected panel 2 prompt in: {content}");
        cleanup(&dir);
    }

    fn tempdir(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("present-export-test-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }
}
