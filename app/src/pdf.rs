use crate::assets::{load_topics, Asset, AssetKind, Panel, Topic};
use crate::markdown::{escape_typst, render_to_typst};
use crate::mermaid;
use anyhow::{anyhow, Result};
use std::path::Path;
use typst_as_lib::typst_kit_options::TypstKitFontOptions;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;

const CLOUD_COLORS: &[&str] = &["blue", "red", "green", "purple", "orange", "teal"];

/// Renders an assets directory to a PDF leave-behind, one page per panel in the
/// same order the TUI presents them. Notes are presenter-only and excluded unless
/// `include_notes` is set, in which case they're appended as a final section.
pub fn export_to_pdf(assets_dir: &str, output_path: &str, include_notes: bool) -> Result<()> {
    let topics = load_topics(assets_dir)?;
    let doc = build_typst_document(&topics, assets_dir, include_notes);
    let pdf_bytes = compile_typst(&doc.source, assets_dir, &doc.diagram_files)?;
    std::fs::write(output_path, pdf_bytes)?;
    Ok(())
}

/// A generated Typst source string plus the in-memory diagram SVGs it references,
/// which need registering with the compiler's static file resolver since they
/// don't exist on disk.
struct TypstDocument {
    source: String,
    diagram_files: Vec<(String, Vec<u8>)>,
}

fn build_typst_document(topics: &[Topic], assets_dir: &str, include_notes: bool) -> TypstDocument {
    let assets_root = Path::new(assets_dir);
    let mut source = String::from(PAGE_PREAMBLE);
    let mut diagram_files = Vec::new();

    for topic in topics {
        source.push_str(&topic_divider(topic));
        for panel in &topic.panels {
            source.push_str(&render_panel_block(panel, assets_root, &mut diagram_files));
        }
    }

    if include_notes {
        source.push_str(&notes_appendix(topics));
    }

    TypstDocument { source, diagram_files }
}

fn compile_typst(source: &str, assets_dir: &str, diagram_files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let static_files: Vec<(&str, Vec<u8>)> =
        diagram_files.iter().map(|(path, bytes)| (path.as_str(), bytes.clone())).collect();

    let engine = TypstEngine::builder()
        .main_file(source.to_string())
        .search_fonts_with(TypstKitFontOptions::default().include_system_fonts(false))
        .with_file_system_resolver(assets_dir)
        .with_static_file_resolver(static_files)
        .build();

    let doc: PagedDocument =
        engine.compile().output.map_err(|e| anyhow!("typst compile failed: {e}"))?;

    typst_pdf::pdf(&doc, &Default::default()).map_err(|e| anyhow!("pdf generation failed: {e:?}"))
}

const PAGE_PREAMBLE: &str = "#set page(width: 11in, height: 8.5in, margin: 0.75in)\n\
#set text(font: \"New Computer Modern\", size: 13pt)\n\
#set heading(numbering: none)\n\n";

fn topic_divider(topic: &Topic) -> String {
    let label = escape_typst(&topic.label);
    format!(
        "#pagebreak(weak: true)\n\
         #set page(header: align(right)[#text(size: 9pt, fill: gray)[{label}]])\n\
         #align(center + horizon)[#text(size: 28pt, weight: \"bold\")[{label}]]\n\n"
    )
}

fn render_panel_block(panel: &Panel, assets_root: &Path, diagram_files: &mut Vec<(String, Vec<u8>)>) -> String {
    let mut out = String::from("#pagebreak(weak: true)\n");
    for asset in &panel.assets {
        match &asset.kind {
            AssetKind::Text { content } | AssetKind::TextCentered { content } => {
                let is_centered = matches!(asset.kind, AssetKind::TextCentered { .. });
                let body = render_to_typst(content);
                if is_centered {
                    out.push_str("#align(center)[\n");
                    out.push_str(&body);
                    out.push_str("\n]\n\n");
                } else {
                    out.push_str(&body);
                    out.push('\n');
                }
            }
            AssetKind::Prompt { label, content, .. } => {
                out.push_str(&format!("== {}\n\n", escape_typst(label)));
                out.push_str(&render_to_typst(content));
                out.push('\n');
            }
            AssetKind::Diagram { content } => {
                out.push_str(&render_diagram(content, diagram_files));
            }
            AssetKind::WordCloud { title, words, .. } => {
                out.push_str(&render_word_cloud(title, words));
            }
            AssetKind::Image { .. } => {
                out.push_str(&render_image(asset, assets_root));
            }
            AssetKind::Notes { .. } => {} // presenter-only; see notes_appendix
        }
    }
    out
}

fn render_diagram(content: &str, diagram_files: &mut Vec<(String, Vec<u8>)>) -> String {
    let (diagram_src, _description) = mermaid::parse(content);
    let Some(src) = diagram_src else { return String::new() };
    let Some(svg) = mermaid::render_to_svg(src) else { return String::new() };

    let file_name = format!("__diagram_{}.svg", diagram_files.len());
    diagram_files.push((file_name.clone(), svg.into_bytes()));
    format!("#align(center)[#image(\"{file_name}\", width: 80%)]\n\n")
}

fn render_image(asset: &Asset, assets_root: &Path) -> String {
    let Some(rel) = relative_to(&asset.path, assets_root) else { return String::new() };
    format!("#align(center)[#image(\"{rel}\", width: 70%)]\n\n")
}

/// Path to `path` relative to `root`, using forward slashes for Typst's virtual
/// path syntax regardless of host path separator conventions.
fn relative_to(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<&str> = rel.components().map(|c| c.as_os_str().to_str().unwrap_or("")).collect();
    Some(parts.join("/"))
}

fn render_word_cloud(title: &str, words: &[String]) -> String {
    let mut out = format!("#align(center)[#text(size: 16pt, weight: \"bold\")[{}]]\n", escape_typst(title));
    out.push_str("#box(width: 100%, height: 3in)[\n");
    let seed = cloud_seed(words);
    for (i, word) in words.iter().enumerate() {
        let (dx, dy) = word_position(seed, word, i);
        let color = CLOUD_COLORS[(word_hash(word) as usize) % CLOUD_COLORS.len()];
        out.push_str(&format!(
            "  #place(dx: {dx}%, dy: {dy}%)[#text(size: 14pt, fill: {color})[{}]]\n",
            escape_typst(word)
        ));
    }
    out.push_str("]\n\n");
    out
}

/// Deterministic, hash-derived cloud layout -- same algorithm shape as the
/// terminal word-cloud renderer (see design-word-cloud.md), just resolved in
/// page-percentage coordinates instead of terminal cells.
fn cloud_seed(words: &[String]) -> u64 {
    words.iter().fold(0x517cc1b727220a95u64, |acc, w| acc.wrapping_add(word_hash(w)).rotate_left(7))
}

fn word_hash(word: &str) -> u64 {
    word.bytes().fold(0xcbf29ce484222325u64, |acc, b| acc.wrapping_mul(0x100000001b3).wrapping_add(b as u64))
}

/// Returns `(dx%, dy%)` for a word, clamped well inside the container so the
/// text doesn't overflow past the right/bottom edge.
fn word_position(seed: u64, word: &str, index: usize) -> (u64, u64) {
    let wh = word_hash(word);
    let dx = (seed ^ wh ^ (index as u64).wrapping_mul(2654435761)) % 85;
    let dy = (seed ^ wh.rotate_left(32) ^ (index as u64).wrapping_mul(6364136223846793005)) % 80;
    (dx, dy)
}

fn notes_appendix(topics: &[Topic]) -> String {
    let mut out = String::from("#pagebreak(weak: true)\n= Speaker Notes\n\n");
    for topic in topics {
        for (i, panel) in topic.panels.iter().enumerate() {
            let Some(notes) = panel.notes() else { continue };
            let AssetKind::Notes { content } = &notes.kind else { continue };
            out.push_str(&format!(
                "== {} -- Panel {}\n\n",
                escape_typst(&topic.label),
                i + 1
            ));
            out.push_str(&render_to_typst(content));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tempdir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("present-pdf-test-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn word_position_is_deterministic_for_the_same_inputs() {
        let seed = cloud_seed(&["rust".to_string(), "async".to_string()]);
        let a = word_position(seed, "rust", 0);
        let b = word_position(seed, "rust", 0);
        assert_eq!(a, b);
    }

    #[test]
    fn word_position_stays_within_bounds() {
        let words = vec!["ownership".to_string(), "borrowing".to_string(), "lifetimes".to_string()];
        let seed = cloud_seed(&words);
        for (i, word) in words.iter().enumerate() {
            let (dx, dy) = word_position(seed, word, i);
            assert!(dx < 85, "dx {dx} out of bounds");
            assert!(dy < 80, "dy {dy} out of bounds");
        }
    }

    #[test]
    fn relative_to_strips_the_assets_root_prefix() {
        let root = Path::new("assets/demo");
        let path = Path::new("assets/demo/01-topic/02/image.jpg");
        assert_eq!(relative_to(path, root), Some("01-topic/02/image.jpg".to_string()));
    }

    #[test]
    fn build_typst_document_includes_topic_label_and_prompt_heading() {
        let dir = tempdir("source-basics");
        let panel_dir = dir.join("01-intro").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("prompt.md"), "# Ask Me\n\nWhat is your question?").unwrap();

        let topics = load_topics(dir.to_str().unwrap()).unwrap();
        let doc = build_typst_document(&topics, dir.to_str().unwrap(), false);

        assert!(doc.source.contains("intro"), "expected topic label in: {}", doc.source);
        assert!(doc.source.contains("== Ask Me"), "expected prompt heading in: {}", doc.source);
        assert!(doc.source.contains("What is your question?"));
        cleanup(&dir);
    }

    #[test]
    fn build_typst_document_wraps_a_valid_diagram_in_an_image_call() {
        let dir = tempdir("source-diagram");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("diagram.md"), "```mermaid\ngraph TD\n    A-->B\n```").unwrap();

        let topics = load_topics(dir.to_str().unwrap()).unwrap();
        let doc = build_typst_document(&topics, dir.to_str().unwrap(), false);

        assert_eq!(doc.diagram_files.len(), 1, "expected one registered diagram SVG");
        assert!(doc.source.contains("__diagram_0.svg"));
        cleanup(&dir);
    }

    #[test]
    fn build_typst_document_excludes_notes_by_default() {
        let dir = tempdir("source-notes-excluded");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("text.md"), "# Title\n\nBody").unwrap();
        fs::write(panel_dir.join("notes.md"), "Slow down here.").unwrap();

        let topics = load_topics(dir.to_str().unwrap()).unwrap();
        let doc = build_typst_document(&topics, dir.to_str().unwrap(), false);

        assert!(!doc.source.contains("Slow down here"));
        cleanup(&dir);
    }

    #[test]
    fn build_typst_document_appends_notes_when_included() {
        let dir = tempdir("source-notes-included");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("text.md"), "# Title\n\nBody").unwrap();
        fs::write(panel_dir.join("notes.md"), "Slow down here.").unwrap();

        let topics = load_topics(dir.to_str().unwrap()).unwrap();
        let doc = build_typst_document(&topics, dir.to_str().unwrap(), true);

        assert!(doc.source.contains("Speaker Notes"));
        assert!(doc.source.contains("Slow down here"));
        cleanup(&dir);
    }

    #[test]
    fn compiled_output_is_a_valid_pdf() {
        let dir = tempdir("compile-smoke");
        let panel_dir = dir.join("01-topic").join("1");
        fs::create_dir_all(&panel_dir).unwrap();
        fs::write(panel_dir.join("text.md"), "# Hello\n\nA short line of body text.").unwrap();

        let output = dir.join("out.pdf");
        export_to_pdf(dir.to_str().unwrap(), output.to_str().unwrap(), false).unwrap();

        let bytes = fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "expected a valid PDF header");
        cleanup(&dir);
    }
}
