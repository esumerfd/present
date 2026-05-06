use mmdflux::{render_diagram, OutputFormat, RenderConfig};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Extract the mermaid source and optional description from a diagram.md file.
/// The file may contain a ```mermaid ... ``` fence plus free text below it.
pub fn parse(file_content: &str) -> (Option<&str>, &str) {
    if let Some(start) = file_content.find("```mermaid") {
        let after_fence = &file_content[start + "```mermaid".len()..];
        if let Some(end) = after_fence.find("```") {
            let diagram_src = after_fence[..end].trim();
            let description = after_fence[end + 3..].trim();
            return (Some(diagram_src), description);
        }
    }
    // No fence — treat entire content as raw diagram source
    (Some(file_content.trim()), "")
}

/// Render a mermaid diagram source to lines of styled ratatui text.
/// Falls back to displaying the raw source if mmdflux can't render it.
pub fn render_to_lines(diagram_src: &str) -> Vec<Line<'static>> {
    match render_diagram(diagram_src, OutputFormat::Text, &RenderConfig::default()) {
        Ok(rendered) => rendered
            .lines()
            .map(|l| Line::from(colorize(l)))
            .collect(),
        Err(_) => diagram_src
            .lines()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::DarkGray))))
            .collect(),
    }
}

fn colorize(line: &str) -> Vec<Span<'static>> {
    let keyword = Style::default().fg(Color::Cyan);
    let arrow   = Style::default().fg(Color::Yellow);
    let label   = Style::default().fg(Color::Green);
    let default = Style::default().fg(Color::White);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = line;

    while !rest.is_empty() {
        // Pipe-delimited labels: |text|
        if let Some(pos) = rest.find('|') {
            let before = &rest[..pos];
            if !before.is_empty() {
                tokenize(before, keyword, arrow, default, &mut spans);
            }
            let after = &rest[pos + 1..];
            if let Some(end) = after.find('|') {
                spans.push(Span::styled(format!("|{}|", &after[..end]), label));
                rest = &after[end + 1..];
            } else {
                spans.push(Span::styled("|".to_string(), default));
                rest = after;
            }
            continue;
        }
        tokenize(rest, keyword, arrow, default, &mut spans);
        break;
    }

    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), default));
    }
    spans
}

fn tokenize(segment: &str, keyword: Style, arrow: Style, default: Style, out: &mut Vec<Span<'static>>) {
    let mut i = 0;
    let bytes = segment.as_bytes();

    while i < bytes.len() {
        if let Some((s, len)) = match_arrow(&segment[i..]) {
            out.push(Span::styled(s, arrow));
            i += len;
            continue;
        }

        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
            {
                i += 1;
            }
            let word = &segment[start..i];
            let style = if is_keyword(word) { keyword } else { default };
            out.push(Span::styled(word.to_string(), style));
            continue;
        }

        let start = i;
        while i < segment.len() {
            let b = bytes[i];
            if b.is_ascii_alphabetic() || b == b'_' || b == b'|' {
                break;
            }
            if match_arrow(&segment[i..]).is_some() {
                break;
            }
            i += if b < 0x80 { 1 } else { segment[i..].chars().next().unwrap().len_utf8() };
        }
        if i > start {
            out.push(Span::styled(segment[start..i].to_string(), default));
        }
    }
}

fn match_arrow(s: &str) -> Option<(String, usize)> {
    for pat in &["-.->", "==>", "-->", "---", "-.-", "-..", "->", "--"] {
        if s.starts_with(pat) {
            return Some((pat.to_string(), pat.len()));
        }
    }
    None
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "flowchart" | "graph" | "sequenceDiagram" | "classDiagram" | "stateDiagram"
            | "stateDiagram-v2" | "erDiagram" | "gantt" | "pie" | "journey" | "gitGraph"
            | "mindmap" | "timeline" | "TB" | "TD" | "BT" | "LR" | "RL"
            | "subgraph" | "end" | "section" | "title" | "participant" | "actor"
            | "loop" | "alt" | "else" | "opt" | "note" | "class" | "state"
    )
}
